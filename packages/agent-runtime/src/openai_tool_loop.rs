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

const MAX_TOOL_ROUNDS: u32 = 10;

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
    calls: Vec<AssembledCall>,
    tokens_in: u64,
    tokens_out: u64,
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
        Vec::new()
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
    let all_tools: Vec<_> = tools
        .into_iter()
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
    let _ = in_project;
    let system_prompt = crate::assemble_system_prompt(
        input.system_override.as_deref(),
        !all_tools.is_empty(),
        input.extra_system.as_ref(),
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

    // Build the request body once per round from shared parts.
    let params = input.params.clone();
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
            // Sampling parameters (temperature, top_p, top_k, max_tokens,
            // repeat_penalty, seed) merged in from the provider config.
            if let Some(serde_json::Value::Object(p)) = &params {
                for (k, v) in p {
                    body[k] = v.clone();
                }
            }
            body
        }
    };

    // First round runs BEFORE we return the stream so connection errors are
    // reported synchronously (the caller falls back to a helpful message).
    let bearer = input.bearer_token.clone();
    let first_body = make_body(&messages, &tools_json);
    let first_resp = post_json(client, &chat_url, &first_body, bearer.as_deref())
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

    let _ = tx
        .send(StreamEvent::MessageStart {
            message_id: message_id.clone(),
            task_id,
        })
        .await;

    let input = input.clone();
    let client = client.clone();
    let chat_url = chat_url.clone();
    let message_id_loop = message_id.clone();
    let tools_for_dispatch = all_tools.clone();
    let mcp_state_for_dispatch = input.mcp_state.clone();
    let approval = input.approval.clone();

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

        // Consume the first (already-sent) response, then loop.
        let mut pending_resp = Some(first_resp);
        let mut got_final = false;

        for round in 0..MAX_TOOL_ROUNDS {
            let resp = match pending_resp.take() {
                Some(r) => r,
                None => {
                    let body = make_body(&messages, &tools_json);
                    match post_json(&client, &chat_url, &body, bearer.as_deref()).await {
                        Ok(r) if r.status().is_success() => r,
                        Ok(r) => {
                            let _ = tx
                                .send(StreamEvent::Log {
                                    level: LogLevel::Warn,
                                    msg: format!("model server returned {}", r.status()),
                                    source: "openai_tool_loop".into(),
                                })
                                .await;
                            break;
                        }
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

            if round_result.calls.is_empty() {
                got_final = true;
                break;
            }

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

/// Consume one streamed response: emit `Token` events live for text deltas,
/// assemble fragmented tool calls, and pick up `usage` from the final frame.
async fn stream_one_round(
    resp: reqwest::Response,
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
) -> Result<RoundResult, String> {
    let mut out = RoundResult::default();
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

            if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                if !text.is_empty() {
                    out.content.push_str(text);
                    if tx
                        .send(StreamEvent::Token {
                            text: text.to_string(),
                        })
                        .await
                        .is_err()
                    {
                        return Err("client gone".into());
                    }
                }
            }

            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
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
