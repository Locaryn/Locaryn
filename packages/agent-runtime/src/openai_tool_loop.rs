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

/// Source de l'avis émis quand un tour finit sans réponse finale (limite
/// d'actions atteinte, ou modèle arrêté). L'interface la reconnaît.
pub const SOURCE_TOUR_INACHEVE: &str = "tour_inacheve";

/// Rappel envoyé quand un tour n'a produit que de la réflexion. Mécanique des
/// outils seulement : il ne dit pas au modèle quoi faire de la tâche.
const RELANCE_REFLEXION_SEULE: &str = "Ta réponse ne contenait que ta réflexion : aucun outil \
n'a été appelé et aucune réponse n'a été donnée. Un appel écrit dans la réflexion n'est pas \
exécuté. Appelle l'outil maintenant si tu voulais le faire, sinon donne ta réponse.";

/// Relance d'une réponse qui annonce une action sans la faire.
const RELANCE_ANNONCE: &str = "Tu as annoncé ce que tu allais faire, sans appeler d'outil : rien n'a été fait. Fais-le maintenant en appelant l'outil qu'il faut (image_search pour des images, web_search pour une recherche, browser_open pour voir une page). Si tout est déjà fait, dis-le.";

/// La réponse se termine-t-elle sur une action annoncée et pas faite
/// (« Laissez-moi commencer par trouver des images… ») ? Un petit modèle
/// s'arrêtait là, et la personne attendait un travail qui ne venait pas.
/// La relance quand le modèle a écrit son appel d'outil au lieu de le faire.
const RELANCE_APPEL_EN_TEXTE: &str = "Tu as écrit l'appel d'outil dans ta réponse, en texte : il n'a pas été exécuté. Fais un vrai appel d'outil (pas de texte « [Outil appelé : …] » ni « outil({…}) » dans la réponse).";

/// Le modèle a-t-il écrit un appel d'outil en texte au lieu de le faire ?
/// « [Outil appelé : wsl_run({…})] », ou le nom d'un outil offert suivi de
/// `({` — ce qu'un petit modèle finit par imiter.
fn appel_ecrit_en_texte(texte: &str, outils: &[crate::tools::ToolSpec]) -> bool {
    let t = texte.trim();
    if t.contains("[Outil appelé :") || t.contains("[Outil appelé:") {
        return true;
    }
    outils.iter().any(|o| {
        let court = o.name.rsplit("__").next().unwrap_or(&o.name);
        t.contains(&format!("{court}({{")) || t.contains(&format!("{}({{", o.name))
    })
}

fn annonce_sans_agir(texte: &str) -> bool {
    let t = texte.trim();
    if t.is_empty() || t.ends_with('?') {
        return false;
    }
    let fin: String = t
        .chars()
        .rev()
        .take(320)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>()
        .to_lowercase();
    // La dernière phrase seulement : une réponse complète qui finit par « je
    // vais vous expliquer » au milieu ne compte pas.
    let derniere = fin
        .trim_end_matches(['.', '!', '…', ':', ' ', '\n'])
        .rsplit(['.', '!', '\n'])
        .next()
        .unwrap_or("");
    [
        "je vais ",
        "laissez-moi",
        "laisse-moi",
        "commençons",
        "je commence",
        "allons-y",
        "let me ",
        "i'll ",
        "i will ",
        "let's ",
    ]
    .iter()
    .any(|m| derniere.contains(m))
}

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
    // Chercher sur le web ne demande ni projet ni navigateur : ces deux outils
    // sont offerts partout.
    let all_tools: Vec<_> = tools
        .into_iter()
        .chain(crate::web::web_tools())
        .chain(host_specs)
        .chain(extension_tools)
        .chain(mcp_tools.clone())
        // Demander un secret n'a de sens que si un outil d'extension sait s'en servir.
        .chain(crate::coffre::outil_si_utile(&mcp_tools))
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
    // Roblox Studio). Ce qui ne tient pas est différé, jamais retiré.
    // Les outils qui ne tenaient pas dans la fenêtre, chargeables à la demande.
    let mut differes: Vec<crate::tools::ToolSpec> = Vec::new();
    let contexte_serveur = crate::tool_budget::server_context(client, endpoint).await;
    // Toute la conversation départage les outils, pas le seul dernier
    // message : « continue » ne nomme rien, et l'outil dont la tâche
    // dépendait disparaissait au tour suivant (le modèle essayait alors de
    // l'appeler dans un shell).
    let demande: String = input
        .history
        .iter()
        // Les outils appelés comptent aussi : « continue » après un wsl_run
        // doit garder les outils WSL offerts.
        .map(|turn| {
            let noms: Vec<&str> = turn.tool_calls.iter().map(|a| a.name.as_str()).collect();
            format!("{} {}", turn.content, noms.join(" "))
        })
        .chain(std::iter::once(input.message.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    // Le navigateur intégré et le contrôle du bureau ne sont offerts que si la
    // personne les demande (ou les désigne par « @ ») : sinon un modèle les
    // prenait pour une simple recherche ou pour écrire du code, à grand renfort
    // de captures d'écran. Ils restent chargeables à la demande
    // (`charger_outils`), et leurs définitions — des milliers de jetons — ne
    // pèsent plus sur chaque message.
    let ecran = crate::tool_budget::outils_ecran(&mcp_tools, &all_tools);
    // Le message en cours décide d'abord ; sinon les deux derniers de la
    // personne ; sinon ce qui était déjà offert dans la conversation (les
    // retirer changerait le début du prompt, relu alors en entier).
    let recents: Vec<crate::tool_budget::DemandeEcran> =
        std::iter::once(crate::tool_budget::demande_ecran(&input.message))
            .chain(
                input
                    .history
                    .iter()
                    .rev()
                    .filter(|t| t.role == "user")
                    .take(2)
                    .map(|t| crate::tool_budget::demande_ecran(&t.content)),
            )
            .collect();
    let (nav_avant, bureau_avant) = crate::outils_stables::ecran_deja_offert(input.session_id);
    let impose = |noms: &std::collections::HashSet<String>| {
        input.forced_tools.iter().any(|f| noms.contains(f))
    };
    let veut_navigateur =
        recents.iter().any(|d| d.navigateur) || nav_avant || impose(&ecran.navigateur);
    // Le bureau : le dernier avis exprimé l'emporte (« n'utilise pas desktop »
    // le retire), et une page web ne l'appelle pas.
    let veut_bureau = impose(&ecran.bureau)
        || recents
            .iter()
            .find_map(|d| d.bureau)
            .unwrap_or(bureau_avant);
    // Le navigateur de la personne (extension morph-browser) ne passe
    // qu'après le navigateur intégré : offert seulement si elle le nomme
    // (Chrome, Opera, « mon navigateur »), sinon chargeable à la demande.
    let veut_externe = recents.iter().any(|d| d.navigateur_externe)
        || impose(&ecran.navigateur_externe)
        || !ecran.navigateur_integre;
    let mut caches = std::collections::HashSet::new();
    if !veut_navigateur {
        caches.extend(ecran.navigateur.iter().cloned());
    } else if !veut_externe {
        caches.extend(ecran.navigateur_externe.iter().cloned());
    }
    // Le contrôle du bureau, dernier recours : seulement quand la personne le
    // demande. Il n'est même plus chargeable sans elle — un modèle qui ne
    // trouvait pas son navigateur prenait la souris et le clavier du PC.
    let retires: std::collections::HashSet<String> = if veut_bureau {
        std::collections::HashSet::new()
    } else {
        ecran.bureau.clone()
    };
    let all_tools: Vec<_> = all_tools
        .into_iter()
        .filter(|t| !retires.contains(&t.name))
        .collect();
    let (all_tools, ecran_differes): (Vec<_>, Vec<_>) = if caches.is_empty() {
        (all_tools, Vec::new())
    } else {
        all_tools
            .into_iter()
            .partition(|t| !caches.contains(&t.name))
    };
    // Gardée pour recaler l'allègement si le moteur refuse la requête.
    let outils_complets = all_tools.clone();
    let mut all_tools = match contexte_serveur {
        Some(ctx) if !all_tools.is_empty() => {
            let before = all_tools.len();
            // L'échelle mesurée sur un refus précédent : l'estimation en octets
            // sous-évalue ce que le gabarit du modèle fait des outils.
            let ctx_estime = (ctx as f64 / echelle_connue()) as usize;
            // Le coût des outils ne se mesure pas qu'en place : chaque jeton de
            // définition est relu à chaque nouvelle conversation. Avec un grand
            // contexte, 50 outils passaient tous — 18 000 jetons, plus de quatre
            // minutes de lecture sur une RX 570 avant le premier mot pour
            // « quelle est la capitale de la France ? ». Au-delà d'un plafond,
            // les plus pertinents restent et les autres se chargent à la
            // demande (`charger_outils`).
            let ctx_estime = crate::tool_budget::plafonner_pour_outils(ctx_estime, &all_tools);
            let deja = crate::outils_stables::ensemble(&crate::outils_stables::precedents(
                input.session_id,
            ));
            let fit = crate::tool_budget::fit_selon_avec(
                all_tools,
                ctx_estime,
                &demande,
                input.trim_tools,
                &input.forced_tools,
                &deja,
            );
            // Rien n'est écarté : ce qui ne tient pas est différé, et le modèle
            // le charge quand il en a besoin (`charger_outils`). L'ancien avis
            // « N outils laissés de côté » annonçait une décision que la
            // personne n'avait pas prise.
            if fit.dropped > 0 || fit.compacted {
                tracing::info!(
                    contexte = ctx,
                    avant = before,
                    offerts = fit.specs.len(),
                    differes = fit.dropped,
                    "outils ajustés au contexte du modèle"
                );
            }
            differes = fit.deferred;
            fit.specs
        }
        _ => all_tools,
    };
    if !ecran_differes.is_empty() {
        let (dedoublonnes, _) = crate::tool_budget::one_name_per_tool(ecran_differes);
        differes.extend(dedoublonnes);
        all_tools.retain(|t| t.name != crate::tool_budget::CHARGER_OUTILS);
        all_tools.push(crate::tool_budget::outil_de_chargement(&differes));
    }
    // Même ordre qu'au tour précédent : le moteur reprend son cache au lieu
    // de relire tout le prompt.
    crate::outils_stables::ordonner_et_retenir(
        input.session_id,
        &mut all_tools,
        veut_navigateur,
        veut_bureau,
    );
    // Le premier tour doit appeler un outil quand des outils sont imposés et
    // effectivement offerts : une consigne dans le message ne suffisait pas à
    // un petit modèle. Les tours suivants restent libres, pour qu'il puisse
    // répondre une fois l'outil revenu.
    // Un outil imposé se retrouve sous son nom préfixé ou sous son nom court.
    let exige_outil = input.require_tool_call
        && !input.native_chat_api
        && all_tools.iter().any(|t| {
            input
                .forced_tools
                .iter()
                .any(|f| f == &t.name || f.ends_with(&format!("__{}", t.name)))
        });
    let mut tools_json = if all_tools.is_empty() {
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
        input.git_contributeur,
    );
    tracing::info!(
        octets = system_prompt.len(),
        outils = all_tools.len(),
        jetons_outils = all_tools
            .iter()
            .map(crate::tool_budget::estimate_tokens)
            .sum::<usize>(),
        noms = %all_tools
            .iter()
            .map(|t| format!("{}~{}", t.name, crate::tool_budget::estimate_tokens(t)))
            .collect::<Vec<_>>()
            .join(","),
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
        arr.extend(crate::compaction::messages_api(&input.history));
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
                    // Une réponse JSON imposée et des outils ne vont pas
                    // ensemble : llama-server fusionne les deux grammaires et
                    // refuse tout (« failed to parse grammar »). Avec des
                    // outils, le modèle écrit son JSON sans contrainte.
                    if k == "response_format" && tools_json.is_some() {
                        continue;
                    }
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

    // Ce que la conversation peut occuper une fois les outils posés. Inconnu
    // quand le serveur ne dit pas sa fenêtre : rien n'est alors retiré.
    let budget_conversation = contexte_serveur.map(|ctx| {
        let outils: usize = all_tools
            .iter()
            .map(crate::tool_budget::estimate_tokens)
            .sum();
        crate::context_window::budget((ctx as f64 / echelle_connue()) as usize, outils)
    });
    let mut annonce_contexte: Option<String> = None;
    if let Some(b) = budget_conversation {
        let fait = crate::context_window::fit(&mut messages, b);
        if !fait.rien() {
            tracing::warn!(budget = b, ?fait, "conversation ajustée à la fenêtre");
            annonce_contexte = Some(fait.annonce());
        }
    }

    // First round runs BEFORE we return the stream so connection errors are
    // reported synchronously (the caller falls back to a helpful message).
    let bearer = input.bearer_token.clone();
    let native_url = format!("{}/api/chat", endpoint.trim_end_matches('/'));
    let first_body = make_body(&messages, &tools_json);
    let (mut openai_body, native_body) = first_body;
    if exige_outil {
        openai_body["tool_choice"] = serde_json::json!("required");
    }
    let (first_url, first_payload) = if native_ollama {
        (native_url.clone(), native_body)
    } else {
        (chat_url.clone(), openai_body)
    };
    let mut first_resp = post_json(client, &first_url, &first_payload, bearer.as_deref())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "openai-compat connection failed");
            AgentError::ProviderUnavailable
        })?;
    // Fenêtre pleine malgré l'estimation : on resserre et on renvoie une fois,
    // plutôt que d'annoncer un moteur indisponible.
    if !first_resp.status().is_success() {
        let status = first_resp.status();
        let body_text = first_resp.text().await.unwrap_or_default();
        // Le moteur n'a pas su traduire un schéma d'outil en grammaire : un
        // seul connecteur mal décrit suffisait à empêcher toute réponse. On
        // renvoie une fois, les arguments des outils MCP réduits à leurs noms.
        let grammaire = erreur_de_grammaire(&body_text) && !all_tools.is_empty();
        if grammaire {
            tracing::warn!(%status, body = %body_text, "grammaire des outils refusée par le moteur — nouvel essai avec des schémas MCP simplifiés");
            for t in all_tools.iter_mut().filter(|t| {
                t.required_permissions
                    .contains(&locaryn_shared_types::Permission::Mcp)
            }) {
                t.input_schema = crate::mcp_tools::schema_permissif(&t.input_schema);
            }
            tools_json = Some(ollama_tools_json(&all_tools));
        } else if !fenetre_pleine(&body_text) {
            tracing::warn!(%status, body = %body_text, "openai-compat returned non-2xx");
            return Err(refus(status.as_u16(), &body_text));
        }
        let budget_refus = if grammaire { None } else { budget_conversation };
        if let Some(b) = budget_refus {
            // Le moteur dit la taille réelle de la requête. L'estimation (octets
            // de JSON) la sous-évalue quand le gabarit du modèle enrobe chaque
            // outil : 92 outils comptés sous 80 % d'une fenêtre de 8 192 en
            // faisaient 22 558. On recale l'échelle sur ce chiffre, on allège les
            // outils à ce qui tient vraiment, puis la conversation.
            let reelle = jetons_annonces(&body_text);
            match (reelle, contexte_serveur) {
                (Some(reelle), Some(ctx)) if !outils_complets.is_empty() => {
                    let estimee: usize = all_tools
                        .iter()
                        .map(crate::tool_budget::estimate_tokens)
                        .sum::<usize>()
                        + crate::context_window::estimate_tokens(&messages);
                    // Estimation brute (octets) contre taille réelle : le rapport
                    // est l'échelle, retenue pour les messages suivants.
                    let echelle = (reelle as f64 / estimee.max(1) as f64).max(1.0);
                    retenir_echelle(echelle);
                    let ctx_recale = (ctx as f64 / echelle) as usize;
                    let refit = crate::tool_budget::fit(
                        outils_complets.clone(),
                        ctx_recale,
                        &demande,
                        &input.forced_tools,
                    );
                    let outils: usize = refit
                        .specs
                        .iter()
                        .map(crate::tool_budget::estimate_tokens)
                        .sum();
                    let fait = crate::context_window::fit(
                        &mut messages,
                        crate::context_window::budget(ctx_recale, outils),
                    );
                    tracing::warn!(
                        reelle,
                        estimee,
                        echelle,
                        outils_avant = all_tools.len(),
                        outils_apres = refit.specs.len(),
                        ?fait,
                        "fenêtre pleine au premier envoi : outils allégés selon la taille réelle"
                    );
                    if !fait.rien() {
                        annonce_contexte = Some(fait.annonce());
                    }
                    differes = refit.deferred;
                    all_tools = refit.specs;
                    tools_json = if all_tools.is_empty() {
                        None
                    } else {
                        Some(ollama_tools_json(&all_tools))
                    };
                }
                _ => {
                    let fait = crate::context_window::fit(&mut messages, b * 6 / 10);
                    tracing::warn!(
                        ?fait,
                        "fenêtre pleine au premier envoi : conversation resserrée"
                    );
                    if !fait.rien() {
                        annonce_contexte = Some(fait.annonce());
                    }
                }
            }
        } else if !grammaire {
            return Err(refus(status.as_u16(), &body_text));
        }
        let (mut openai_body, native_body) = make_body(&messages, &tools_json);
        if exige_outil {
            openai_body["tool_choice"] = serde_json::json!("required");
        }
        let payload = if native_ollama {
            native_body
        } else {
            openai_body
        };
        first_resp = post_json(client, &first_url, &payload, bearer.as_deref())
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "openai-compat connection failed");
                AgentError::ProviderUnavailable
            })?;
    }
    // La grammaire refusée encore, schémas MCP simplifiés : le schéma fautif
    // n'est pas celui d'un connecteur. Tous les outils réduits aux noms de
    // leurs arguments, puis, en dernier recours, une réponse sans outils —
    // plutôt qu'une conversation arrêtée sur « failed to parse grammar ».
    let mut repli_grammaire = 0u8;
    while !first_resp.status().is_success() {
        let status = first_resp.status();
        let body_text = first_resp.text().await.unwrap_or_default();
        if !(erreur_de_grammaire(&body_text) && tools_json.is_some() && repli_grammaire < 2) {
            tracing::warn!(%status, body = %body_text, "openai-compat returned non-2xx");
            return Err(refus(status.as_u16(), &body_text));
        }
        repli_grammaire += 1;
        if repli_grammaire == 1 {
            tracing::warn!(body = %body_text, "grammaire encore refusée — tous les schémas d'outils simplifiés");
            for t in all_tools.iter_mut() {
                t.input_schema = crate::mcp_tools::schema_permissif(&t.input_schema);
            }
            tools_json = Some(ollama_tools_json(&all_tools));
        } else {
            tracing::warn!(body = %body_text, "grammaire toujours refusée — réponse sans outils");
            all_tools.clear();
            differes.clear();
            tools_json = None;
            annonce_contexte = Some(
                "Le moteur refusait la description des outils (« failed to parse grammar ») : \
                 cette réponse est écrite sans outils."
                    .into(),
            );
        }
        let (mut openai_body, native_body) = make_body(&messages, &tools_json);
        if exige_outil && tools_json.is_some() {
            openai_body["tool_choice"] = serde_json::json!("required");
        }
        let payload = if native_ollama {
            native_body
        } else {
            openai_body
        };
        first_resp = post_json(client, &first_url, &payload, bearer.as_deref())
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "openai-compat connection failed");
                AgentError::ProviderUnavailable
            })?;
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
    if let Some(msg) = annonce_contexte {
        let _ = tx
            .send(StreamEvent::Log {
                level: locaryn_events::LogLevel::Warn,
                msg,
                source: "contexte".into(),
            })
            .await;
    }

    let input = input.clone();
    let client = client.clone();
    let chat_url = chat_url.clone();
    let native_ollama_loop = native_ollama;
    let native_url_loop = native_url;
    let message_id_loop = message_id.clone();
    let mut tools_for_dispatch = all_tools.clone();
    // Pour la jauge : quels outils viennent d'un connecteur.
    let noms_mcp: std::collections::HashSet<String> =
        mcp_tools.iter().map(|t| t.name.clone()).collect();
    let mcp_state_for_dispatch = input.mcp_state.clone();
    let approval = input.approval.clone();
    let question = input.question.clone();
    let host_tools = input.host_tools.clone();
    // Le poids estimé des outils offerts ; il grandit quand le modèle charge
    // un outil différé.
    let mut outils_estimes: usize = all_tools
        .iter()
        .map(crate::tool_budget::estimate_tokens)
        .sum();
    let estimation_premiere = outils_estimes + crate::context_window::estimate_tokens(&messages);

    // Personne n'écoute plus (Stop, conversation fermée) : la boucle s'arrête
    // au lieu de continuer en arrière-plan. Elle relançait le modèle et
    // exécutait encore des outils après un Stop — le GPU tournait toujours,
    // et la réponse suivante attendait derrière.
    let garde = tx.clone();
    tokio::spawn(async move {
        let travail = async move {
            let mut ctx = ToolContext {
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
            let mut limite_atteinte = false;
            // Une seule nouvelle tentative par tour après un refus pour fenêtre
            // pleine : l'estimation a pu être trop optimiste.
            let mut serre = false;
            let mut contexte_annonce = false;
            // Jetons réels par jeton estimé, mesurés à chaque tour sur ce que le
            // moteur annonce. L'estimation (octets / 3) sous-évaluait la requête
            // d'un modèle à gabarit bavard : à 8 192 de fenêtre, la requête
            // atteignait 7 900 jetons, la réponse était coupée net au milieu de
            // la réflexion, puis le moteur refusait la suite.
            let mut echelle = echelle_connue();
            let mut estimation_envoyee = estimation_premiere;

            for round in 0..MAX_TOOL_ROUNDS {
                let resp = match pending_resp.take() {
                    Some(r) => Some(r),
                    None => loop {
                        // Un morph a pu décharger le modèle de conversation pour
                        // travailler : il revient avant qu'on lui parle.
                        if let Some(arbitre) = &input.gpu {
                            if let Err(e) = arbitre
                                .0
                                .restore(&crate::gpu::Annonce::dans_le_journal(&tx))
                                .await
                            {
                                let _ = tx
                                    .send(StreamEvent::Log {
                                        level: LogLevel::Warn,
                                        msg: format!(
                                        "Le modèle de conversation n'a pas pu être rechargé : {e}"
                                    ),
                                        source: "gpu".into(),
                                    })
                                    .await;
                                break None;
                            }
                        }
                        let budget_tour = contexte_serveur.map(|c| {
                            crate::context_window::budget(
                                (c as f64 / echelle) as usize,
                                outils_estimes,
                            )
                        });
                        if let Some(b) = budget_tour {
                            // Après un refus, on vise plus bas que l'estimation.
                            let cible = if serre { b * 6 / 10 } else { b };
                            let fait = crate::context_window::fit(&mut messages, cible);
                            // Une seule annonce par message : la répéter à chaque
                            // tour noyait la conversation sous le même avis.
                            if !fait.rien() && !contexte_annonce {
                                contexte_annonce = true;
                                let _ = tx
                                    .send(StreamEvent::Log {
                                        level: LogLevel::Warn,
                                        msg: fait.annonce(),
                                        source: "contexte".into(),
                                    })
                                    .await;
                            }
                        }
                        estimation_envoyee =
                            outils_estimes + crate::context_window::estimate_tokens(&messages);
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
                                break None;
                            }
                        };
                        let resp = if native_ollama_loop {
                            convert_native_to_openai_stream(resp)
                        } else {
                            resp
                        };
                        match resp {
                            r if r.status().is_success() => {
                                serre = false;
                                break Some(r);
                            }
                            r => {
                                let status = r.status();
                                let body = r.text().await.unwrap_or_default();
                                if !serre && contexte_serveur.is_some() && fenetre_pleine(&body) {
                                    // La taille réelle annoncée recale l'échelle
                                    // avant de resserrer.
                                    if let Some(reelle) = jetons_annonces(&body) {
                                        echelle = (reelle as f64
                                            / estimation_envoyee.max(1) as f64)
                                            .max(echelle)
                                            .clamp(1.0, 8.0);
                                        retenir_echelle(echelle);
                                        tracing::warn!(
                                            reelle,
                                            estimation_envoyee,
                                            echelle,
                                            "fenêtre pleine en cours de tâche : échelle recalée"
                                        );
                                    }
                                    serre = true;
                                    continue;
                                }
                                let _ = tx
                                    .send(StreamEvent::Log {
                                        level: LogLevel::Warn,
                                        msg: server_error_message(status.as_u16(), &body),
                                        source: "openai_tool_loop".into(),
                                    })
                                    .await;
                                break None;
                            }
                        }
                    },
                };
                let Some(resp) = resp else {
                    break;
                };

                let round_result = match stream_one_round(resp, &tx).await {
                    Ok(r) => r,
                    Err(e) => {
                        let e_lower = e.to_lowercase();
                        let is_tdr_or_crash = e_lower.contains("error decoding response body")
                            || e_lower.contains("connection closed")
                            || e_lower.contains("broken pipe")
                            || e_lower.contains("reset by peer")
                            || e_lower.contains("errordevicelost");
                        let user_msg = if is_tdr_or_crash {
                            "Le moteur d'inférence (llama-server) s'est arrêté en pleine réponse — le pilote graphique a sans doute été réinitialisé par Windows, ou la mémoire a manqué. Il redémarre au prochain message avec des réglages plus légers : micro-lots plus petits, puis le NPU laissé de côté sur Snapdragon, puis le processeur si cela se reproduit.".to_string()
                        } else {
                            format!("stream error: {e}")
                        };
                        let _ = tx
                            .send(StreamEvent::Log {
                                level: LogLevel::Warn,
                                msg: user_msg,
                                source: "openai_tool_loop".into(),
                            })
                            .await;
                        break;
                    }
                };
                // Le moteur a compté la requête : l'échelle suit la mesure, dans
                // les deux sens. Bornée à 1 par le bas, une estimation trop
                // pessimiste n'était jamais corrigée : à 14 336 de fenêtre, la
                // conversation était raccourcie vers 10 000 jetons réels.
                if round_result.tokens_in > 0 && estimation_envoyee > 0 {
                    let mesuree = (round_result.tokens_in as f64 / estimation_envoyee as f64)
                        .clamp(ECHELLE_MIN, 8.0);
                    if (mesuree - echelle).abs() > 0.05 {
                        tracing::debug!(echelle = mesuree, "échelle des jetons mesurée");
                        echelle = mesuree;
                        retenir_echelle(echelle);
                    }
                }
                tokens_in += round_result.tokens_in;
                tokens_out += round_result.tokens_out;
                let _ = tx
                    .send(bilan_du_contexte(
                        &messages,
                        &tools_for_dispatch,
                        &noms_mcp,
                        &differes,
                        contexte_serveur.unwrap_or(0) as u64,
                        round_result.tokens_in,
                        round_result.tokens_out,
                    ))
                    .await;
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
                    // L'appel écrit en texte au lieu d'être fait : on le dit au
                    // modèle tout de suite, qui le refait pour de vrai.
                    if !relance_faite
                        && !tools_for_dispatch.is_empty()
                        && appel_ecrit_en_texte(&round_result.content, &tools_for_dispatch)
                        && round + 1 < MAX_TOOL_ROUNDS
                    {
                        relance_faite = true;
                        let liste = messages.as_array_mut().unwrap();
                        liste.push(serde_json::json!({
                            "role": "assistant",
                            "content": round_result.content,
                        }));
                        liste.push(serde_json::json!({
                            "role": "user",
                            "content": RELANCE_APPEL_EN_TEXTE,
                        }));
                        continue;
                    }
                    // Une action annoncée et pas faite : une relance aussi, la
                    // réponse déjà écrite gardée dans l'échange.
                    if !relance_faite
                        && !tools_for_dispatch.is_empty()
                        && annonce_sans_agir(&round_result.content)
                        && round + 1 < MAX_TOOL_ROUNDS
                    {
                        relance_faite = true;
                        let liste = messages.as_array_mut().unwrap();
                        liste.push(serde_json::json!({
                            "role": "assistant",
                            "content": round_result.content,
                        }));
                        liste.push(serde_json::json!({
                            "role": "user",
                            "content": RELANCE_ANNONCE,
                        }));
                        continue;
                    }
                    // Une correction remise pendant la réponse finale n'attend pas
                    // le message suivant : le modèle la lit et reprend.
                    if round + 1 < MAX_TOOL_ROUNDS {
                        if let Some(boite) = &input.mailbox {
                            let mut suite = serde_json::json!([]);
                            let lus = boite.verser(&mut suite).await;
                            if !lus.is_empty() {
                                let _ = tx.send(StreamEvent::MailRead { ids: lus }).await;
                                let liste = messages.as_array_mut().unwrap();
                                liste.push(serde_json::json!({
                                    "role": "assistant",
                                    "content": round_result.content,
                                }));
                                liste.extend(suite.as_array().cloned().unwrap_or_default());
                                continue;
                            }
                        }
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
                    // Charger des outils différés : traité ici, rien ne part vers
                    // un connecteur. Ils sont offerts dès le tour suivant.
                    if call.name == crate::tool_budget::CHARGER_OUTILS {
                        let args: serde_json::Value = serde_json::from_str(&call.arguments_raw)
                            .unwrap_or(serde_json::json!({}));
                        let resultat = charger_differes(
                            &mut differes,
                            &mut tools_for_dispatch,
                            &mut tools_json,
                            &mut outils_estimes,
                            &args,
                        );
                        let _ = tx
                            .send(StreamEvent::ToolCall {
                                call_id: call.id.clone(),
                                tool: call.name.clone(),
                                args,
                            })
                            .await;
                        let _ = tx
                            .send(StreamEvent::ToolResult {
                                call_id: call.id.clone(),
                                ok: true,
                                output: resultat.clone(),
                            })
                            .await;
                        messages.as_array_mut().unwrap().push(serde_json::json!({
                            "role": "tool",
                            "tool_call_id": call.id,
                            "content": resultat,
                        }));
                        continue;
                    }
                    // La permission a pu changer pendant que le modèle écrivait :
                    // c'est celle de maintenant qui décide, pas celle de l'envoi.
                    if let Some(source) = &input.trust_source {
                        let maintenant = source.refresh(ctx.trust).await;
                        if maintenant != ctx.trust {
                            tracing::info!(avant = ?ctx.trust, apres = ?maintenant, "permission changée en cours de tâche");
                            ctx.trust = maintenant;
                        }
                    }
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
                            trust: input.trust_source.as_ref(),
                            gpu: input.gpu.as_ref(),
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

                // Ce que la personne a remis pendant ces appels arrive maintenant,
                // avant que le modèle décide de la suite.
                if let Some(boite) = &input.mailbox {
                    let lus = boite.verser(&mut messages).await;
                    if !lus.is_empty() {
                        tracing::info!(lus = lus.len(), "message(s) remis en cours de tâche");
                        let _ = tx.send(StreamEvent::MailRead { ids: lus }).await;
                    }
                }

                if round + 1 == MAX_TOOL_ROUNDS {
                    limite_atteinte = true;
                }
            }

            // La tâche s'arrête sur un outil qui avait libéré la carte : le modèle
            // de conversation revient quand même, prêt pour le message suivant.
            if let Some(arbitre) = &input.gpu {
                if let Err(e) = arbitre
                    .0
                    .restore(&crate::gpu::Annonce::dans_le_journal(&tx))
                    .await
                {
                    tracing::warn!(error = %e, "modèle de conversation non rechargé en fin de tâche");
                }
            }

            // Dit en clair, et une seule fois : les deux avis anglais
            // (« tool-round limit reached », « loop ended without a final
            // response ») se suivaient sous la dernière action, sans dire
            // quoi faire. `source` reste stable : /goal s'en sert pour
            // relancer le travail au lieu de juger un tour coupé.
            if !got_final {
                let msg = if limite_atteinte {
                    format!(
                        "Limite de {MAX_TOOL_ROUNDS} actions atteinte pour ce message : la tâche \
                         s'arrête ici, sans réponse finale. Écrivez « continue » pour reprendre."
                    )
                } else {
                    "Le modèle s'est arrêté sans réponse finale.".to_string()
                };
                let _ = tx
                    .send(StreamEvent::Log {
                        level: LogLevel::Warn,
                        msg,
                        source: SOURCE_TOUR_INACHEVE.into(),
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
        };
        tokio::select! {
            () = travail => {}
            () = garde.closed() => {
                tracing::info!("réponse abandonnée : la boucle d'outils s'arrête");
            }
        }
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

/// Le moteur a refusé de traduire un schéma d'outil en grammaire
/// (llama-server : « failed to parse grammar », « JSON schema error … »).
fn erreur_de_grammaire(corps: &str) -> bool {
    let bas = corps.to_ascii_lowercase();
    bas.contains("parse grammar")
        || bas.contains("json schema error")
        || bas.contains("failed to initialize samplers")
}

/// Ce que la personne lit quand le moteur refuse une requête. Le statut seul
/// (« model server returned 500 ») ne disait rien. Sur un petit modèle local,
/// la cause la plus courante est une fenêtre de contexte pleine : la réponse
/// est coupée, souvent au milieu du JSON d'un appel d'outil.
/// Rapport entre jetons réels et jetons estimés, mesuré au dernier refus
/// « fenêtre pleine » (1,0 tant qu'aucun refus). Il évite de payer un refus à
/// chaque message : les suivants partent déjà allégés.
static ECHELLE_CONNUE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Plancher de l'échelle mesurée : l'estimation (octets / 3) surévalue le
/// texte courant, mais jamais de moitié ; en deçà, la mesure est suspecte.
const ECHELLE_MIN: f64 = 0.6;

fn echelle_connue() -> f64 {
    let bits = ECHELLE_CONNUE.load(std::sync::atomic::Ordering::Relaxed);
    if bits == 0 {
        1.0
    } else {
        f64::from_bits(bits).clamp(ECHELLE_MIN, 8.0)
    }
}

fn retenir_echelle(echelle: f64) {
    ECHELLE_CONNUE.store(
        echelle.clamp(ECHELLE_MIN, 8.0).to_bits(),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// Ce qui occupe la fenêtre à ce tour, poste par poste. Les postes sont
/// estimés (octets / 3) puis recalés sur `tokens_in`, le compte du moteur,
/// quand il le donne : la somme est alors exacte, la répartition approchée.
fn bilan_du_contexte(
    messages: &serde_json::Value,
    offerts: &[crate::tools::ToolSpec],
    noms_mcp: &std::collections::HashSet<String>,
    differes: &[crate::tools::ToolSpec],
    window: u64,
    tokens_in: u64,
    // La réponse de ce tour occupera la fenêtre au suivant : elle compte avec
    // les messages.
    tokens_out: u64,
) -> StreamEvent {
    let liste = messages.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let (systeme, conversation): (Vec<_>, Vec<_>) = liste
        .iter()
        .cloned()
        .partition(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"));
    let systeme = crate::context_window::estimate_tokens(&serde_json::Value::Array(systeme));
    let conversation =
        crate::context_window::estimate_tokens(&serde_json::Value::Array(conversation));
    let (mcp, integres): (Vec<_>, Vec<_>) =
        offerts.iter().partition(|t| noms_mcp.contains(&t.name));
    let poids = |l: &[&crate::tools::ToolSpec]| -> usize {
        l.iter()
            .map(|t| crate::tool_budget::estimate_tokens(t))
            .sum()
    };
    let estimes = [
        ("system", systeme),
        ("tools", poids(&integres)),
        ("mcp_tools", poids(&mcp)),
        ("messages", conversation),
    ];
    let somme: usize = estimes.iter().map(|(_, n)| n).sum();
    let echelle = if tokens_in > 0 && somme > 0 {
        tokens_in as f64 / somme as f64
    } else {
        1.0
    };
    StreamEvent::ContextUsage {
        window,
        parts: estimes
            .iter()
            .map(|(cle, n)| locaryn_events::ContextPart {
                key: (*cle).to_string(),
                tokens: (*n as f64 * echelle).round() as u64
                    + if *cle == "messages" { tokens_out } else { 0 },
            })
            .collect(),
        deferred_tokens: (differes
            .iter()
            .map(crate::tool_budget::estimate_tokens)
            .sum::<usize>() as f64
            * echelle)
            .round() as u64,
        deferred_count: differes.len() as u32,
    }
}

/// Charger les outils différés que le modèle nomme : ils rejoignent les outils
/// offerts (et la liste de dispatch), l'outil de chargement ne liste plus que
/// ce qui reste. Rend le texte que le modèle lit en résultat.
fn charger_differes(
    differes: &mut Vec<crate::tools::ToolSpec>,
    offerts: &mut Vec<crate::tools::ToolSpec>,
    tools_json: &mut Option<serde_json::Value>,
    outils_estimes: &mut usize,
    args: &serde_json::Value,
) -> String {
    let (charges, inconnus) = crate::tool_budget::charger(differes, args);
    offerts.retain(|s| s.name != crate::tool_budget::CHARGER_OUTILS);
    let noms: Vec<String> = charges.iter().map(|s| s.name.clone()).collect();
    offerts.extend(charges);
    if !differes.is_empty() {
        offerts.push(crate::tool_budget::outil_de_chargement(differes));
    }
    *tools_json = Some(ollama_tools_json(offerts));
    *outils_estimes = offerts
        .iter()
        .map(crate::tool_budget::estimate_tokens)
        .sum();
    let mut texte = if noms.is_empty() {
        "Aucun outil chargé.".to_string()
    } else {
        format!(
            "Outils chargés : {}. Tu peux les appeler maintenant.",
            noms.join(", ")
        )
    };
    if !inconnus.is_empty() {
        texte.push_str(&format!(
            " Inconnus (vérifie l'orthographe dans la liste de charger_outils) : {}.",
            inconnus.join(", ")
        ));
    }
    texte
}

/// La taille réelle d'une requête refusée, telle que llama-server l'annonce :
/// « request (22558 tokens) exceeds the available context size ».
fn jetons_annonces(texte: &str) -> Option<usize> {
    let debut = texte.find("request (")? + "request (".len();
    let reste = &texte[debut..];
    let fin = reste.find(" tokens)")?;
    reste[..fin].trim().parse().ok()
}

/// Le moteur refuse-t-il la requête parce qu'elle dépasse sa fenêtre ?
fn fenetre_pleine(texte: &str) -> bool {
    let bas = texte.to_lowercase();
    bas.contains("context") && (bas.contains("exceed") || bas.contains("size"))
}

/// Un refus du moteur : une requête qu'il a lue et rejetée se dit avec ses
/// mots. Seule une réponse vide, sans rien d'exploitable, reste « indisponible ».
fn refus(status: u16, body: &str) -> AgentError {
    if body.trim().is_empty() && status >= 500 {
        return AgentError::ProviderUnavailable;
    }
    AgentError::Refused(server_error_message(status, body))
}

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
    if bas.contains("image input is not supported") || bas.contains("mmproj") {
        return "Le modèle chargé ne sait pas analyser les images : il lui faut un projecteur                 « mmproj » à côté de ses poids. Choisissez un modèle multimodal, ou envoyez le                 message sans l'image."
            .into();
    }
    if fenetre_pleine(&detail) {
        if let Some(n) = jetons_annonces(body) {
            return format!(
                "Le message dépasse la fenêtre de contexte du modèle ({n} jetons). Un document                  joint trop long en est la cause la plus fréquente : joignez-en une partie, ou                  augmentez la fenêtre dans Paramètres du modèle."
            );
        }
        return "La fenêtre de contexte du modèle est pleine. Augmentez-la dans Paramètres du \
                modèle, ou repartez d'une nouvelle conversation."
            .into();
    }
    if detail.to_lowercase().contains("errordevicelost")
        || detail.to_lowercase().contains("vk::queue::submit")
    {
        return "Le pilote graphique de votre carte (Vulkan) a dépassé le délai de réponse Windows (TDR) \
                et a été réinitialisé par le système. Un micro-batch trop lourd ou un contexte très étendu \
                en est la cause. Redémarrez le modèle pour réinitialiser le moteur."
            .into();
    }
    if detail
        .to_lowercase()
        .contains("failed to parse tool call arguments")
    {
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

    // Dernier envoi d'un brouillon, par appel : quelques nouvelles par
    // seconde suffisent, le flux de fragments est bien plus dense.
    let mut dernier_brouillon: std::collections::HashMap<u64, std::time::Instant> =
        std::collections::HashMap::new();

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
                    let maintenant = std::time::Instant::now();
                    let du_nouveau = dernier_brouillon.get(&idx).is_none_or(|t| {
                        maintenant.duration_since(*t) >= std::time::Duration::from_millis(250)
                    });
                    if du_nouveau && !entry.1.is_empty() {
                        dernier_brouillon.insert(idx, maintenant);
                        let b = crate::brouillon::lire(&entry.2);
                        // L'interface peut être partie : le brouillon n'est
                        // qu'un aperçu, son échec n'arrête pas la réponse.
                        let _ = tx
                            .send(StreamEvent::ToolDraft {
                                index: idx as u32,
                                tool: entry.1.clone(),
                                path: b.chemin,
                                chars: b.caracteres,
                                tail: b.fin,
                            })
                            .await;
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
mod tests_annonce {
    #[test]
    fn une_action_annoncee_sans_appel_est_reconnue() {
        let outils = vec![crate::tools::ToolSpec {
            name: "wsl__wsl_run".into(),
            description: String::new(),
            input_schema: serde_json::json!({}),
            risk: crate::tools::Risk::High,
            required_permissions: Vec::new(),
        }];
        assert!(super::appel_ecrit_en_texte(
            "[Outil appelé : wsl_run({\"command\":\"tshark -r capture.pcap\"})]",
            &outils
        ));
        assert!(super::appel_ecrit_en_texte(
            "Je lance wsl_run({\"command\":\"ls\"})",
            &outils
        ));
        assert!(!super::appel_ecrit_en_texte(
            "La commande wsl_run a listé trois fichiers.",
            &outils
        ));
        assert!(super::annonce_sans_agir(
            "Vous avez tout à fait raison ! Je vais utiliser le navigateur intégré. Laissez-moi commencer par trouver des images de haute qualité sur Internet pour enrichir le site."
        ));
        assert!(super::annonce_sans_agir(
            "Parfait. Let me search for that now."
        ));
        assert!(!super::annonce_sans_agir(
            "La capitale de la France est Paris."
        ));
        assert!(!super::annonce_sans_agir(
            "Je vais le faire : veux-tu des photos ou des dessins ?"
        ));
        assert!(!super::annonce_sans_agir(
            "Je vais vous expliquer. Le site contient trois sections et tout fonctionne."
        ));
    }
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
        assert_eq!(
            jetons_annonces(
                r#"{"error":{"code":400,"message":"request (22558 tokens) exceeds the available context size (8192 tokens), try increasing it"}}"#
            ),
            Some(22558)
        );
        assert_eq!(jetons_annonces("autre erreur"), None);
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
        let image = r#"{"error":{"code":500,"message":"image input is not supported - hint: if this is unexpected, you may need to provide the mmproj"}}"#;
        assert!(server_error_message(500, image)
            .starts_with("Le modèle chargé ne sait pas analyser les images"));
        let pdf = r#"{"error":{"code":400,"message":"request (32151 tokens) exceeds the available context size (8192 tokens), try increasing it"}}"#;
        assert!(server_error_message(400, pdf).contains("32151 jetons"));
        assert!(matches!(refus(500, image), AgentError::Refused(_)));
        assert!(matches!(refus(502, ""), AgentError::ProviderUnavailable));
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
