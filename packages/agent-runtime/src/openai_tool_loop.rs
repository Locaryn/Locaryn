//! Tool-use loop for OpenAI-compatible providers (llama-server, LM Studio, vLLM).
//!
//! Speaks `/v1/chat/completions` with `stream: true` on EVERY round, so text
//! deltas reach the UI live while tool calls are assembled from streamed
//! fragments. Key wire facts:
//!
//! - Stream frames: `data: {"choices":[{"delta":{"content"|"tool_calls"},"finish_reason"}]}`,
//!   terminated by `data: [DONE]`.
//! - Streamed tool calls arrive fragmented: `delta.tool_calls[{index, id?, function:{name?, arguments-fragment}}]`
//!   and are re-assembled by `index`.
//! - `usage` rides the final frame when `stream_options.include_usage` is set.
//! - Tool result feedback: `{role:"tool", tool_call_id, content}`; the
//!   assistant message we echo back carries the assembled `tool_calls` with
//!   `arguments` as a JSON **string** (OpenAI convention).

use crate::tools::{builtin_tools, ollama_tools_json, ToolContext};
use crate::{AgentError, AgentInput, EventStream};
use futures::StreamExt as _;
use locaryn_events::{LogLevel, StreamEvent};
use locaryn_shared_types::TrustLevel;
use std::time::Instant;

/// Dix tours ne suffisaient pas à une tâche menée par étapes : construire six
/// éléments, plus lire l'état et vérifier, en prend déjà huit.
const MAX_TOOL_ROUNDS: u32 = 20;

/// Rappel envoyé quand un tour n'a produit que de la réflexion. Mécanique des
/// outils seulement : il ne dit pas au modèle quoi faire de la tâche.
const RELANCE_REFLEXION_SEULE: &str = "Ta réponse ne contenait que ta réflexion : aucun outil \
n'a été appelé et aucune réponse n'a été donnée. Un appel écrit dans la réflexion n'est pas \
exécuté. Appelle l'outil maintenant si tu voulais le faire, sinon donne ta réponse.";

/// A fully assembled tool call, reconstructed from streamed fragments.
#[derive(Debug, Clone)]
struct AssembledCall {
    id: String,
    name: String,
    /// Raw JSON string as streamed (kept verbatim for the echo-back message).
    arguments_raw: String,
}

/// Everything one streamed round produced.
#[derive(Debug, Default)]
struct RoundResult {
    content: String,
    /// Le modèle a réfléchi pendant ce tour (`reasoning_content`).
    reasoned: bool,
    calls: Vec<AssembledCall>,
    tokens_in: u64,
    tokens_out: u64,
    timings: RoundTimings,
}

/// Ce que llama-server mesure lui-même dans le dernier cadre d'un flux
/// (`timings`) : jetons et millisecondes du prompt, puis de la génération.
#[derive(Debug, Default, Clone, Copy)]
struct RoundTimings {
    prompt_n: u64,
    prompt_ms: f64,
    predicted_n: u64,
    predicted_ms: f64,
}

impl RoundTimings {
    fn add(&mut self, autre: RoundTimings) {
        self.prompt_n += autre.prompt_n;
        self.prompt_ms += autre.prompt_ms;
        self.predicted_n += autre.predicted_n;
        self.predicted_ms += autre.predicted_ms;
    }

    /// L'événement à envoyer, ou `None` si le moteur n'a rien mesuré.
    fn event(self) -> Option<StreamEvent> {
        if self.predicted_n == 0 || self.predicted_ms <= 0.0 {
            return None;
        }
        let par_seconde = |n: u64, ms: f64| {
            if ms > 0.0 {
                (n as f64 / ms * 1000.0) as f32
            } else {
                0.0
            }
        };
        Some(StreamEvent::Timings {
            prompt_tokens: self.prompt_n,
            generated_tokens: self.predicted_n,
            prompt_tokens_per_sec: par_seconde(self.prompt_n, self.prompt_ms),
            generation_tokens_per_sec: par_seconde(self.predicted_n, self.predicted_ms),
        })
    }
}

/// Run the OpenAI-compat loop. Tools are enabled only when the input carries
/// project context (path + trust); otherwise it's a plain streamed chat.
pub async fn run_openai_tool_loop(
    endpoint: &str,
    client: &reqwest::Client,
    input: &AgentInput,
) -> Result<EventStream, AgentError> {
    let model = input.model.clone().unwrap_or_else(|| "default".into());
    let chat_url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));

    // Les outils intégrés touchent des fichiers : ils n'ont de sens que dans un
    // projet. Les extensions restent disponibles dans une conversation libre
    // et gèrent elles-mêmes leur espace de stockage.
    let in_project = input.project_path.is_some();
    let extension_tools = crate::tools::capability_tools(&input.capabilities);
    let trust = input.trust.unwrap_or(TrustLevel::Sandbox);
    let tools = if in_project {
        builtin_tools()
    } else {
        // Hors d'un projet, les outils de fichiers n'ont pas de racine ou
        // travailler. Demander, si : une conversation libre souleve une
        // question aussi bien qu'un projet, et l'en priver forcerait le
        // modele a deviner la ou il pouvait poser la question.
        vec![crate::tools::question_tool()]
    };
    // MCP extensions are valid in a free conversation too (for example an
    // image plugin writes only to its own storage). Only the host's built-in
    // file tools require a project path.
    let mcp_tools = if let Some(ref mcp) = input.mcp_state {
        crate::mcp_tools::collect_mcp_tools(mcp).await
    } else {
        Vec::new()
    };
    // MCP tools are merged into the main list so approval gating works
    // uniformly. The `all_tools` vec must stay alive for the spawned task.
    // Outils intégrés + ceux apportés par les extensions actives + ceux des
    // serveurs MCP. Tous passent par la même liste, donc par la même
    // demande d'accord.
    // Les outils de l'application elle-même valent dans un projet comme dans
    // une conversation libre : ajouter un connecteur n'a pas besoin de dossier.
    let host_specs = input
        .host_tools
        .as_ref()
        .map(|h| h.0.specs())
        .unwrap_or_default();
    let all_tools: Vec<_> = tools
        .into_iter()
        .chain(host_specs)
        .chain(extension_tools)
        .chain(mcp_tools.clone())
        .collect();
    // Une figure peut limiter les outils : seuls ceux qu'elle nomme restent
    // dans la liste offerte au modèle. Vide ou absent, tout passe.
    let all_tools = match &input.tools {
        Some(autorises) if !autorises.is_empty() => all_tools
            .into_iter()
            .filter(|t| autorises.iter().any(|a| a == &t.name))
            .collect(),
        _ => all_tools,
    };
    // Les définitions d'outils doivent tenir dans le contexte du serveur : quelques
    // connecteurs MCP suffisent à le dépasser, et le serveur refuse alors la
    // requête en bloc (41 009 jetons pour un contexte de 8 192, mesuré avec
    // Roblox Studio). On réduit seulement si ça déborde, et on le dit.
    let mut tools_notice: Option<String> = None;
    let all_tools = match crate::tool_budget::server_context(client, endpoint).await {
        Some(ctx) if !all_tools.is_empty() => {
            let before = all_tools.len();
            // Toute la conversation départage les outils, pas le seul dernier
            // message : « continue » ne nomme rien, et l'outil dont la tâche
            // dépendait disparaissait au tour suivant (le modèle essayait
            // alors de l'appeler dans un shell).
            let demande: String = input
                .history
                .iter()
                .map(|turn| turn.content.as_str())
                .chain(std::iter::once(input.message.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            let fit = crate::tool_budget::fit(all_tools, ctx, &demande);
            if fit.dropped > 0 || fit.compacted {
                tracing::warn!(
                    contexte = ctx,
                    avant = before,
                    apres = fit.specs.len(),
                    retires = fit.dropped,
                    "outils ajustés au contexte du modèle"
                );
                tools_notice = Some(if fit.dropped > 0 {
                    format!(
                        "Le contexte du modèle ({ctx} jetons) ne contient pas tous les outils : {} ont été laissés de côté, d'après votre demande. Décochez ceux dont vous n'avez pas besoin dans Réglages → Connecteurs MCP → Configurer, ou augmentez le contexte.",
                        fit.dropped
                    )
                } else {
                    format!(
                        "Le contexte du modèle ({ctx} jetons) est serré : les descriptions des outils ont été raccourcies."
                    )
                });
            }
            fit.specs
        }
        _ => all_tools,
    };
    let tools_json = if all_tools.is_empty() {
        None
    } else {
        Some(ollama_tools_json(&all_tools))
    };

    // OpenAI vision: images go in `content` as an array of parts.
    let user_content: serde_json::Value = if input.images.is_empty() {
        serde_json::json!(input.message)
    } else {
        let mut parts: Vec<serde_json::Value> =
            vec![serde_json::json!({ "type": "text", "text": input.message })];
        for img_b64 in &input.images {
            let url = if img_b64.starts_with("data:") {
                img_b64.clone()
            } else {
                format!("data:image/jpeg;base64,{img_b64}")
            };
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": url }
            }));
        }
        serde_json::json!(parts)
    };

    // Rien n'est posé devant le modèle sauf ce que la personne a écrit et ce
    // que la mécanique des outils exige. Sans consigne et sans outil, aucun
    // message système n'est envoyé du tout : le modèle répond exactement comme
    // lancé hors de l'application.
    let system_prompt = crate::assemble_system_prompt_pour(
        input.system_override.as_deref(),
        !all_tools.is_empty(),
        input.extra_system.as_ref(),
        // Écrire du code puis annoncer « c'est corrigé » sans compiler affirme
        // ce qu'on n'a pas constaté. La machine est là, la commande existe :
        // le projet dit laquelle.
        input.project_path.as_deref(),
        input.project_context.as_deref(),
    );
    tracing::info!(
        octets = system_prompt.len(),
        outils = all_tools.len(),
        "message système posé devant le modèle"
    );

    // system → prior turns (conversation memory) → the new user message.
    let mut messages = if system_prompt.trim().is_empty() {
        serde_json::json!([])
    } else {
        serde_json::json!([{ "role": "system", "content": system_prompt }])
    };
    {
        let arr = messages.as_array_mut().expect("messages is an array");
        for turn in &input.history {
            if turn.content.trim().is_empty() {
                continue;
            }
            arr.push(serde_json::json!({ "role": turn.role, "content": turn.content }));
        }
        arr.push(serde_json::json!({ "role": "user", "content": user_content }));
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let task_id = uuid::Uuid::new_v4().to_string();
    let start = Instant::now();

    let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);

    // Build the request body once per round from shared parts. Ollama's
    // native API gets its own shape: /api/chat wants options.{...} and a
    // flat tools list, and it is the only endpoint of its that honours
    // num_ctx — so the context setting only lands when this branch runs.
    let params = input.params.clone();
    let native_ollama = input.native_chat_api;
    let make_body = {
        let model = model.clone();
        move |messages: &serde_json::Value, tools_json: &Option<serde_json::Value>| {
            let mut body = serde_json::json!({
                "model": model,
                "messages": messages,
                "stream": true,
                "stream_options": { "include_usage": true },
            });
            if let Some(t) = tools_json {
                body["tools"] = t.clone();
            }
            let mut native_body = serde_json::json!({
                "model": model,
                "messages": messages,
                "stream": true,
            });
            if let Some(serde_json::Value::Object(p)) = &params {
                let mut options = serde_json::Map::new();
                for (k, v) in p {
                    body[k.clone()] = v.clone();
                    if k == "num_ctx" {
                        options.insert("num_ctx".into(), v.clone());
                    } else if k == "max_tokens" {
                        options.insert("num_predict".into(), v.clone());
                    } else if k == "repeat_penalty" {
                        options.insert("repeat_penalty".into(), v.clone());
                    } else if k == "seed" {
                        options.insert("seed".into(), v.clone());
                    } else {
                        // temperature / top_p / top_k : mêmes noms des deux
                        // côtés, au niveau options pour l'API native.
                        options.insert(k.clone(), v.clone());
                    }
                }
                if !options.is_empty() {
                    native_body["options"] = serde_json::Value::Object(options);
                }
            }
            if let Some(t) = tools_json {
                native_body["tools"] = t.clone();
            }
            (body, native_body)
        }
    };

    // First round runs BEFORE we return the stream so connection errors are
    // reported synchronously (the caller falls back to a helpful message).
    let bearer = input.bearer_token.clone();
    let native_url = format!("{}/api/chat", endpoint.trim_end_matches('/'));
    let first_body = make_body(&messages, &tools_json);
    let (openai_body, native_body) = first_body;
    let (first_url, first_payload) = if native_ollama {
        (native_url.clone(), native_body)
    } else {
        (chat_url.clone(), openai_body)
    };
    let first_resp = post_json(client, &first_url, &first_payload, bearer.as_deref())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "openai-compat connection failed");
            AgentError::ProviderUnavailable
        })?;
    if !first_resp.status().is_success() {
        let status = first_resp.status();
        let body_text = first_resp.text().await.unwrap_or_default();
        tracing::warn!(%status, body = %body_text, "openai-compat returned non-2xx");
        return Err(AgentError::ProviderUnavailable);
    }
    let first_resp = if native_ollama {
        convert_native_to_openai_stream(first_resp)
    } else {
        first_resp
    };

    let _ = tx
        .send(StreamEvent::MessageStart {
            message_id: message_id.clone(),
            task_id,
        })
        .await;
    if let Some(msg) = tools_notice {
        let _ = tx
            .send(StreamEvent::Log {
                level: locaryn_events::LogLevel::Warn,
                msg,
                source: "outils".into(),
            })
            .await;
    }

    let input = input.clone();
    let client = client.clone();
    let chat_url = chat_url.clone();
    let native_ollama_loop = native_ollama;
    let native_url_loop = native_url;
    let message_id_loop = message_id.clone();
    let tools_for_dispatch = all_tools.clone();
    let mcp_state_for_dispatch = input.mcp_state.clone();
    let approval = input.approval.clone();
    let question = input.question.clone();
    let host_tools = input.host_tools.clone();

    tokio::spawn(async move {
        let ctx = ToolContext {
            project_id: input.project_id.unwrap_or_default(),
            project_path: input.project_path.clone().unwrap_or_default(),
            trust,
            session_id: input.session_id,
            // TODO: populate remote_target for SSH/MCP calls so
            // approval_decision() can escalate to Critical.
            remote_target: None,
        };

        let mut tokens_in = 0u64;
        let mut tokens_out = 0u64;
        let mut timings = RoundTimings::default();

        // Consume the first (already-sent) response, then loop.
        let mut pending_resp = Some(first_resp);
        let mut got_final = false;
        let mut relance_faite = false;

        for round in 0..MAX_TOOL_ROUNDS {
            let resp = match pending_resp.take() {
                Some(r) => r,
                None => {
                    let (openai_body, native_body) = make_body(&messages, &tools_json);
                    let (url, body) = if native_ollama_loop {
                        (native_url_loop.clone(), native_body)
                    } else {
                        (chat_url.clone(), openai_body)
                    };
                    let posted = post_json(&client, &url, &body, bearer.as_deref()).await;
                    let resp = match posted {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = tx
                                .send(StreamEvent::Log {
                                    level: LogLevel::Warn,
                                    msg: format!("model server connection failed: {e}"),
                                    source: "openai_tool_loop".into(),
                                })
                                .await;
                            break;
                        }
                    };
                    let resp = if native_ollama_loop {
                        convert_native_to_openai_stream(resp)
                    } else {
                        resp
                    };
                    match resp {
                        r if r.status().is_success() => r,
                        r => {
                            let status = r.status();
                            let body = r.text().await.unwrap_or_default();
                            let _ = tx
                                .send(StreamEvent::Log {
                                    level: LogLevel::Warn,
                                    msg: server_error_message(status.as_u16(), &body),
                                    source: "openai_tool_loop".into(),
                                })
                                .await;
                            break;
                        }
                    }
                }
            };

            let round_result = match stream_one_round(resp, &tx).await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Log {
                            level: LogLevel::Warn,
                            msg: format!("stream error: {e}"),
                            source: "openai_tool_loop".into(),
                        })
                        .await;
                    break;
                }
            };
            tokens_in += round_result.tokens_in;
            tokens_out += round_result.tokens_out;
            timings.add(round_result.timings);

            if round_result.calls.is_empty() {
                // Un tour fait de réflexion seule, sans appel ni réponse : le
                // modèle a écrit son appel d'outil dans sa réflexion (Bonsai y
                // pose des balises `<parameter>`), où le serveur ne le lit pas.
                // La tâche s'arrêtait là, muette. Une relance, jamais deux de
                // suite : un modèle qui recommence n'est pas relancé en boucle.
                if !relance_faite
                    && round_result.reasoned
                    && round_result.content.trim().is_empty()
                    && round + 1 < MAX_TOOL_ROUNDS
                {
                    relance_faite = true;
                    messages.as_array_mut().unwrap().push(serde_json::json!({
                        "role": "user",
                        "content": RELANCE_REFLEXION_SEULE,
                    }));
                    continue;
                }
                got_final = true;
                break;
            }

            relance_faite = false;

            // Echo the assistant tool-call message back into the transcript.
            let tc_json: Vec<serde_json::Value> = round_result
                .calls
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "type": "function",
                        "function": { "name": c.name, "arguments": c.arguments_raw }
                    })
                })
                .collect();
            messages.as_array_mut().unwrap().push(serde_json::json!({
                "role": "assistant",
                "content": round_result.content,
                "tool_calls": tc_json,
            }));

            // Dispatch each call (with approval gating), feed results back.
            // Le chemin d'exécution (décision, refus, dispatch, événements)
            // est partagé avec le pont de noyaux alternatifs : une seule
            // implémentation de la politique d'approbation, où que l'appel
            // vienne.
            for call in &round_result.calls {
                let args: serde_json::Value =
                    serde_json::from_str(&call.arguments_raw).unwrap_or(serde_json::json!({}));

                let result_content = match crate::execute_tool_call(
                    &tx,
                    &call.id,
                    &call.name,
                    args,
                    &crate::exec::ToolDispatchContext {
                        tools: &tools_for_dispatch,
                        ctx: &ctx,
                        mcp: mcp_state_for_dispatch.as_deref(),
                        approval: approval.as_ref(),
                        question: question.as_ref(),
                        host: host_tools.as_ref(),
                    },
                )
                .await
                {
                    Some(text) => text,
                    // Client gone — nobody is listening, stop the loop.
                    None => return,
                };

                messages.as_array_mut().unwrap().push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": call.id,
                    "content": result_content,
                }));
            }

            if round + 1 == MAX_TOOL_ROUNDS {
                let _ = tx
                    .send(StreamEvent::Log {
                        level: LogLevel::Warn,
                        msg: "tool-round limit reached".into(),
                        source: "openai_tool_loop".into(),
                    })
                    .await;
            }
        }

        if !got_final {
            let _ = tx
                .send(StreamEvent::Log {
                    level: LogLevel::Warn,
                    msg: "loop ended without a final response".into(),
                    source: "openai_tool_loop".into(),
                })
                .await;
        }

        if let Some(evenement) = timings.event() {
            let _ = tx.send(evenement).await;
        }
        let _ = tx
            .send(StreamEvent::MessageEnd {
                message_id: message_id_loop,
                tokens_in,
                tokens_out,
                duration_ms: start.elapsed().as_millis() as u64,
            })
            .await;
    });

    Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
}

/// Réécrire la réponse NDJSON d'Ollama (`/api/chat`) dans la forme SSE
/// OpenAI que `stream_one_round` sait lire : chaque objet natif devient une
/// ligne `data: {...}` avec `choices[0].delta.content`, les fragments
/// d'appel d'outil deviennent des fragments `tool_calls`, l'objet final
/// (`done: true`) fournit l'usage. Le flux, lui, ne change pas — on filtre
/// les octets au vol.
fn convert_native_to_openai_stream(resp: reqwest::Response) -> reqwest::Response {
    use futures::StreamExt;

    let status = resp.status();
    let headers = resp.headers().clone();
    let translated = resp
        .bytes_stream()
        .map(|chunk| match chunk {
            Ok(bytes) => {
                let mut out = Vec::with_capacity(bytes.len() + 16);
                for line in bytes.split(|b| *b == 10) {
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(line) {
                        let message = val.get("message");
                        let content = message
                            .and_then(|m| m.get("content"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let tool_calls = message.and_then(|m| m.get("tool_calls"));
                        let mut delta = serde_json::Map::new();
                        if !content.is_empty() {
                            delta.insert("content".into(), serde_json::json!(content));
                        }
                        if let Some(tcs) = tool_calls {
                            let arr: Vec<serde_json::Value> = tcs
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .enumerate()
                                        .map(|(i, tc)| {
                                            serde_json::json!({
                                                "index": i,
                                                "id": format!("call_{}", i),
                                                "type": "function",
                                                "function": {
                                                    "name": tc.pointer("/function/name").cloned().unwrap_or_default(),
                                                    "arguments": tc.pointer("/function/arguments").cloned().unwrap_or_else(|| serde_json::json!("{}")),
                                                }
                                            })
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            if !arr.is_empty() {
                                delta.insert("tool_calls".into(), serde_json::json!(arr));
                            }
                        }
                        let mut frame = serde_json::Map::new();
                        frame.insert("id".into(), serde_json::json!("ollama-native"));
                        frame.insert("choices".into(), serde_json::json!([{ "delta": delta }]));
                        if val.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                            let pi = val
                                .get("prompt_eval_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let co = val.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
                            frame.insert(
                                "usage".into(),
                                serde_json::json!({
                                    "prompt_tokens": pi,
                                    "completion_tokens": co,
                                }),
                            );
                        }
                        out.extend_from_slice(
                            format!("data: {}

", serde_json::Value::Object(frame)).as_bytes(),
                        );
                    }
                }
                if out.is_empty() {
                    None
                } else {
                    Some(Ok(bytes::Bytes::from(out)))
                }
            }
            // Une erreur de transport traverse telle quelle : le consommateur
            // la verra au meme endroit qu'un flux OpenAI natif.
            Err(e) => Some(Err(e)),
        })
        .filter_map(|res| async move { res });

    let mut builder = http::Response::builder().status(status);
    for (k, v) in headers.iter() {
        builder = builder.header(k, v);
    }
    let body = reqwest::Body::wrap_stream(translated);
    let rebuilt = builder.body(body).expect("valid http response");
    reqwest::Response::from(rebuilt)
}

/// POST JSON, with the optional Bearer header used by alternate cores
/// (OpenClaw, Hermes…). `None` keeps the exact behaviour of a plain provider
/// call — no header at all.
async fn post_json(
    client: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
    bearer: Option<&str>,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut req = client.post(url);
    if let Some(t) = bearer {
        req = req.bearer_auth(t);
    }
    req.json(body).send().await
}

/// Ce que la personne lit quand le moteur refuse une requête. Le statut seul
/// (« model server returned 500 ») ne disait rien. Sur un petit modèle local,
/// la cause la plus courante est une fenêtre de contexte pleine : la réponse
/// est coupée, souvent au milieu du JSON d'un appel d'outil.
fn server_error_message(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string());
    let bas = detail.to_lowercase();
    if bas.contains("context") && (bas.contains("exceed") || bas.contains("size")) {
        return "La fenêtre de contexte du modèle est pleine. Augmentez-la dans Paramètres du \
                modèle, ou repartez d'une nouvelle conversation."
            .into();
    }
    if bas.contains("failed to parse tool call arguments") {
        return "L'appel d'outil du modèle est arrivé incomplet, le plus souvent parce que la \
                fenêtre de contexte s'est remplie pendant qu'il l'écrivait. Augmentez-la dans \
                Paramètres du modèle, ou demandez une tâche plus courte."
            .into();
    }
    let detail: String = detail.chars().take(300).collect();
    if detail.is_empty() {
        format!("Le moteur a refusé la requête (HTTP {status}).")
    } else {
        format!("Le moteur a refusé la requête (HTTP {status}) : {detail}")
    }
}

/// Balises du bloc de réflexion que l'interface replie (`reasoning.ts`).
pub const REFLEXION_DEBUT: &str = "<think>";
pub const REFLEXION_FIN: &str = "</think>";

/// Une réponse sans ses blocs de réflexion, y compris un bloc resté ouvert
/// (réponse interrompue). La réflexion se montre à la personne mais ne repart
/// pas au modèle : rejouée dans l'historique, elle mangeait la fenêtre de
/// contexte d'un petit modèle en deux ou trois tours.
pub fn sans_reflexion(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut reste = texte;
    while let Some(debut) = reste.find(REFLEXION_DEBUT) {
        sortie.push_str(&reste[..debut]);
        let apres = &reste[debut + REFLEXION_DEBUT.len()..];
        match apres.find(REFLEXION_FIN) {
            Some(fin) => reste = &apres[fin + REFLEXION_FIN.len()..],
            None => {
                reste = "";
                break;
            }
        }
    }
    sortie.push_str(reste);
    sortie
}

/// Le texte de réflexion d'un fragment, quel que soit le champ où le serveur
/// le range : `reasoning_content` (llama-server, DeepSeek) ou `reasoning`
/// (Ollama, OpenRouter).
fn reasoning_delta(delta: &serde_json::Value) -> Option<&str> {
    ["reasoning_content", "reasoning"]
        .into_iter()
        .find_map(|cle| delta.get(cle).and_then(|v| v.as_str()))
        .filter(|t| !t.is_empty())
}

async fn send_token(
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
    text: String,
) -> Result<(), String> {
    tx.send(StreamEvent::Token { text })
        .await
        .map_err(|_| "client gone".to_string())
}

/// Consume one streamed response: emit `Token` events live for text deltas,
/// assemble fragmented tool calls, and pick up `usage` from the final frame.
///
/// La réflexion qu'un serveur range à part (`reasoning_content`) part dans un
/// bloc `<think>` du même flux. Ignorée, elle laissait l'interface sans rien
/// recevoir pendant des minutes — « Chargement du modèle » affiché alors que
/// le modèle réfléchissait déjà. Elle ne rejoint pas `out.content` : ce texte
/// repart au modèle au tour suivant.
async fn stream_one_round(
    resp: reqwest::Response,
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
) -> Result<RoundResult, String> {
    let mut out = RoundResult::default();
    let mut reflexion_ouverte = false;
    // index → (id, name, args buffer)
    let mut partial: std::collections::BTreeMap<u64, (String, String, String)> =
        std::collections::BTreeMap::new();

    let mut byte_stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk_res) = byte_stream.next().await {
        let chunk = chunk_res.map_err(|e| e.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer.drain(..=pos);

            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }
            let json_str = match line.strip_prefix("data: ") {
                Some(s) => s,
                None => continue,
            };
            let val: serde_json::Value = match serde_json::from_str(json_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Une erreur peut arriver dans le flux, après des jetons déjà
            // montrés : l'ignorer finissait le tour sans réponse ni motif.
            if val.get("error").is_some() {
                let code = val
                    .pointer("/error/code")
                    .and_then(|c| c.as_u64())
                    .and_then(|c| u16::try_from(c).ok())
                    .unwrap_or(500);
                return Err(server_error_message(code, json_str));
            }

            if let Some(t) = val.get("timings") {
                let nombre = |cle: &str| t.get(cle).and_then(|v| v.as_u64()).unwrap_or(0);
                let duree = |cle: &str| t.get(cle).and_then(|v| v.as_f64()).unwrap_or(0.0);
                out.timings = RoundTimings {
                    prompt_n: nombre("prompt_n"),
                    prompt_ms: duree("prompt_ms"),
                    predicted_n: nombre("predicted_n"),
                    predicted_ms: duree("predicted_ms"),
                };
            }

            if let Some(usage) = val.get("usage") {
                if let Some(pi) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                    out.tokens_in = pi;
                }
                if let Some(co) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                    out.tokens_out = co;
                }
            }

            let delta = val
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|c| c.get("delta"));
            let Some(delta) = delta else { continue };

            if let Some(pensee) = reasoning_delta(delta) {
                let texte = if reflexion_ouverte {
                    pensee.to_string()
                } else {
                    format!("{REFLEXION_DEBUT}{pensee}")
                };
                reflexion_ouverte = true;
                out.reasoned = true;
                send_token(tx, texte).await?;
            }

            let text = delta
                .get("content")
                .and_then(|c| c.as_str())
                .filter(|t| !t.is_empty());
            let tcs = delta.get("tool_calls").and_then(|t| t.as_array());
            if reflexion_ouverte && (text.is_some() || tcs.is_some()) {
                reflexion_ouverte = false;
                send_token(tx, format!("{REFLEXION_FIN}\n\n")).await?;
            }

            if let Some(text) = text {
                out.content.push_str(text);
                send_token(tx, text.to_string()).await?;
            }

            if let Some(tcs) = tcs {
                for frag in tcs {
                    let idx = frag.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                    let entry = partial.entry(idx).or_default();
                    if let Some(id) = frag.get("id").and_then(|v| v.as_str()) {
                        entry.0 = id.to_string();
                    }
                    if let Some(f) = frag.get("function") {
                        if let Some(name) = f.get("name").and_then(|v| v.as_str()) {
                            entry.1.push_str(name);
                        }
                        if let Some(args) = f.get("arguments").and_then(|v| v.as_str()) {
                            entry.2.push_str(args);
                        }
                    }
                }
            }
        }
    }
    if reflexion_ouverte {
        send_token(tx, format!("{REFLEXION_FIN}\n\n")).await?;
    }

    for (_, (id, name, args)) in partial {
        out.calls.push(AssembledCall {
            id: if id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                id
            },
            name,
            arguments_raw: if args.is_empty() { "{}".into() } else { args },
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_reflexion_ne_repart_pas_au_modele() {
        assert_eq!(
            sans_reflexion("<think>je pèse</think>\n\nRéponse"),
            "\n\nRéponse"
        );
        assert_eq!(
            sans_reflexion("a<think>x</think>b<think>y</think>c"),
            "abc",
            "chaque bloc, pas seulement le premier"
        );
        assert_eq!(sans_reflexion("Début<think>coupé net"), "Début");
        assert_eq!(sans_reflexion("rien à retirer"), "rien à retirer");
    }

    #[test]
    fn une_erreur_du_moteur_se_lit_en_clair() {
        let coupe = r#"{"error":{"code":500,"message":"Failed to parse tool call arguments as JSON: parse error at line 1, column 3515"}}"#;
        assert!(server_error_message(500, coupe).contains("fenêtre de contexte"));
        let plein =
            r#"{"error":{"code":400,"message":"the request exceeds the available context size"}}"#;
        assert!(server_error_message(400, plein).starts_with("La fenêtre de contexte"));
        assert_eq!(
            server_error_message(503, r#"{"error":{"message":"Loading model"}}"#),
            "Le moteur a refusé la requête (HTTP 503) : Loading model"
        );
        assert_eq!(
            server_error_message(502, ""),
            "Le moteur a refusé la requête (HTTP 502)."
        );
    }

    #[test]
    fn la_reflexion_est_lue_dans_les_deux_champs_usuels() {
        let llama = serde_json::json!({ "reasoning_content": "hmm" });
        let ollama = serde_json::json!({ "reasoning": "hmm" });
        let vide = serde_json::json!({ "reasoning_content": "", "content": "x" });
        assert_eq!(reasoning_delta(&llama), Some("hmm"));
        assert_eq!(reasoning_delta(&ollama), Some("hmm"));
        assert_eq!(reasoning_delta(&vide), None);
    }
}
