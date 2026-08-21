//! Locaryn agent runtime — the agentic tool-use loop and specialized
//! subagents. This is the heart of the product; V1 wires a real loop with
//! tool dispatch, approval gating, and streaming. The skeleton defines the
//! interfaces so other crates can compile against them.

pub mod approval;
pub mod embeddings;
pub mod exec;
pub mod reasoning;

pub mod mcp_tools;
pub mod ollama;
pub mod openai_compat;
pub mod openai_tool_loop;
pub mod profile;
pub mod titling;
pub mod tool_loop;
pub mod tools;

pub use exec::execute_tool_call;
pub use ollama::OllamaAgent;
pub use openai_compat::OpenAiCompatAgent;
pub use profile::{AgentProfile, AgentRegistry};
pub use tools::{ToolContext, ToolError, ToolResult, ToolSpec};

use futures::Stream;
use locaryn_events::StreamEvent;
use locaryn_shared_types::ConnectionMode;

/// Input to an agent run.
#[derive(Debug, Clone)]
pub struct AgentInput {
    pub session_id: uuid::Uuid,
    pub message: String,
    pub mode: ConnectionMode,
    pub model: Option<String>,
    pub agent: Option<String>,
    /// S4: project context for the tool-use loop. When all three are `Some`,
    /// `OllamaAgent` runs the agentic tool-use loop instead of simple streaming.
    pub project_id: Option<uuid::Uuid>,
    pub project_path: Option<std::path::PathBuf>,
    pub trust: Option<locaryn_shared_types::TrustLevel>,
    /// Base64-encoded images attached to the user message (no data-URL prefix).
    /// Passed through to vision-capable models via Ollama's `images` field.
    pub images: Vec<String>,
    /// Sampling parameters merged verbatim into the request body
    /// (temperature, top_p, top_k, max_tokens, repeat_penalty, seed…).
    pub params: Option<serde_json::Value>,
    /// Prior turns of this conversation, oldest first. Without these the model
    /// has no memory: every message used to be sent standalone.
    pub history: Vec<ChatTurn>,
    /// Optional MCP server state. When set, the tool loop discovers tools from
    /// running MCP servers and makes them available to the model alongside
    /// built-in tools. Tool names are prefixed `mcp__{server}__{tool}` to avoid
    /// collisions across servers.
    pub mcp_state: Option<std::sync::Arc<locaryn_mcp::McpState>>,
    /// Extra instructions appended to the system prompt: workspace rules and
    /// the skill index contributed by enabled extensions. Empty when no
    /// extension is enabled, in which case the prompt is byte-for-byte what it
    /// was before extensions existed.
    #[allow(clippy::doc_markdown)]
    pub extra_system: Option<String>,
    /// La consigne écrite par la personne, qui **remplace** celle de
    /// l'application.
    ///
    /// `None` : celle par défaut. `Some(texte)` : celui-ci à la place.
    /// `Some("")` : aucune consigne du tout — le modèle répond avec son
    /// caractère propre, comme lancé hors de l'application. C'est un logiciel
    /// local : ce que le modèle installé accepte de faire regarde son auteur
    /// et la personne qui l'a choisi, pas le programme qui le lance.
    pub system_override: Option<String>,
    /// Ce que les extensions actives apportent (`image-gen`, `voice-tts`…).
    ///
    /// Décide des outils offerts au modèle : sans l'extension de génération
    /// d'images, `generate_image` n'existe pas dans sa liste, et il répond
    /// qu'il ne sait pas le faire — au lieu de l'appeler puis d'échouer.
    pub capabilities: Vec<String>,
    /// Les outils que la figure de cette conversation a le droit d'appeler,
    /// par nom (`generate_image`, `generate_speech`, `read_file`,
    /// `mcp__serveur__outil`…). `None` ou vide : tout ce que l'application
    /// propose.
    pub tools: Option<Vec<String>>,
    /// Comment demander son accord à l'utilisateur avant un appel d'outil
    /// sensible. `None` — le cas d'un hôte sans interface — vaut refus : un
    /// service qui tourne sans personne devant ne doit pas s'autoriser une
    /// opération que l'on aurait voulu arbitrer.
    pub approval: Option<approval::ApprovalHandle>,
    /// Jeton Bearer envoyé à l'endpoint (noyaux alternatifs : OpenClaw,
    /// Hermes…). `None` = pas d'en-tête d'authentification.
    pub bearer_token: Option<String>,
}

/// Ce qu'il faut dire au modèle pour qu'il se serve correctement des outils.
///
/// Uniquement de la mécanique : aller lire un fichier plutôt qu'en deviner le
/// contenu, ne pas affirmer qu'on ne sait pas faire ce dont l'outil existe.
/// Rien sur ce dont il peut parler, ni sur qui il est.
///
/// L'application ne pose **aucune** consigne de comportement. Une version
/// antérieure ouvrait chaque conversation par « You are Locaryn, an AI coding
/// assistant » : le modèle se présentait comme « conçu pour aider à la
/// programmation » et refusait le reste, là où le même modèle lancé hors de
/// l'application répondait. Ce que fait un modèle installé sur une machine
/// regarde son auteur et la personne qui l'a choisi — le logiciel qui le lance
/// n'a pas à trancher à leur place. Qui veut un caractère l'écrit lui-même
/// dans son profil.
pub fn tool_discipline_prompt() -> String {
    "You have tools for reading and changing files in the project that is open.\n\n\
     How to use them — this is about accuracy, not about subject matter:\n\
     1. Never guess or invent file contents. To know what a file holds, call `read_file`.\n\
     2. Never assume the directory layout. When unsure where something lives, call `search`.\n\
     3. Use paths relative to the project root (`src/main.rs`, `Cargo.toml`). If a file is \
        missing, the tool says so — do not invent it.\n\
     4. Say briefly what you are about to check before calling a tool.\n\
     5. Call tools as many times as needed to actually know.\n\
     6. Once you know enough, answer directly and stop calling tools.\n\
     7. Never say you are unable to do something a listed tool does — call it."
        .to_string()
}

/// Append `extra` to a base system prompt, under a heading that tells the
/// model where the text came from. Kept here so both tool loops compose the
/// prompt identically.
pub fn compose_system_prompt(base: &str, extra: Option<&String>) -> String {
    match extra.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(extra) => format!("{base}\n\n# Extensions\n\n{extra}"),
        None => base.to_string(),
    }
}

#[cfg(test)]
mod prompt_tests {
    /// Sans consigne écrite et sans outil, l'application ne pose rien : le
    /// message système est vide, et l'appelant n'en envoie alors aucun. C'est
    /// ce qui distingue ce logiciel d'un service qui impose un caractère.
    #[test]
    fn rien_de_pose_quand_rien_n_est_demande() {
        assert!(super::compose_system_prompt("", None).is_empty());
    }

    /// La mécanique des outils ne parle que de mécanique : elle n'a pas à
    /// restreindre les sujets ni à donner une identité au modèle.
    #[test]
    fn la_discipline_des_outils_ne_donne_pas_de_caractere() {
        let regles = super::tool_discipline_prompt().to_lowercase();
        for interdit in [
            "coding assistant",
            "you are locaryn",
            "refuse",
            "inappropriate",
        ] {
            assert!(
                !regles.contains(interdit),
                "« {interdit} » n'a rien à faire dans les règles d'outillage"
            );
        }
        assert!(regles.contains("read_file"));
    }
}

/// One prior turn replayed to the model.
#[derive(Debug, Clone)]
pub struct ChatTurn {
    /// "user" | "assistant" | "system"
    pub role: String,
    pub content: String,
}

/// An agent run produces a stream of `StreamEvent`s (tokens, tool calls,
/// tool results, artifacts, message end).
pub type EventStream = std::pin::Pin<Box<dyn Stream<Item = StreamEvent> + Send>>;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("tool: {0}")]
    Tool(#[from] ToolError),
    #[error("provider unavailable")]
    ProviderUnavailable,
    #[error("no agent profile: {0}")]
    NoProfile(String),
    #[error("cancelled")]
    Cancelled,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Le driver déclaré par le manifeste d'un noyau n'existe pas
    /// (`responses`, `runs`, `chat_completions` sont les trois connus).
    #[error("unknown core driver: {0}")]
    UnknownDriver(String),
}

/// The trait every provider-facing agent implements.
#[async_trait::async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    /// Run the agent, returning a stream of events.
    async fn run(&self, input: AgentInput) -> Result<EventStream, AgentError>;
}

/// A stub agent that emits a single end event (for the MVP bootstrap).
pub struct StubAgent;

#[async_trait::async_trait]
impl Agent for StubAgent {
    fn name(&self) -> &str {
        "stub"
    }
    async fn run(&self, input: AgentInput) -> Result<EventStream, AgentError> {
        use futures::stream;
        let msg_id = uuid::Uuid::new_v4().to_string();
        let task_id = uuid::Uuid::new_v4().to_string();
        let events = vec![
            StreamEvent::MessageStart {
                message_id: msg_id.clone(),
                task_id: task_id.clone(),
            },
            StreamEvent::Token {
                text: format!("(stub agent) echo: {}", input.message),
            },
            StreamEvent::MessageEnd {
                message_id: msg_id,
                tokens_in: 0,
                tokens_out: 8,
                duration_ms: 1,
            },
        ];
        Ok(Box::pin(stream::iter(events)))
    }
}
