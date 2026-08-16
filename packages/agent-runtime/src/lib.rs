//! Locaryn agent runtime — the agentic tool-use loop and specialized
//! subagents. This is the heart of the product; V1 wires a real loop with
//! tool dispatch, approval gating, and streaming. The skeleton defines the
//! interfaces so other crates can compile against them.

pub mod approval;
pub mod reasoning;

pub mod mcp_tools;
pub mod ollama;
pub mod openai_compat;
pub mod openai_tool_loop;
pub mod profile;
pub mod titling;
pub mod tool_loop;
pub mod tools;

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
    /// Ce que les extensions actives apportent (`image-gen`, `voice-tts`…).
    ///
    /// Décide des outils offerts au modèle : sans l'extension de génération
    /// d'images, `generate_image` n'existe pas dans sa liste, et il répond
    /// qu'il ne sait pas le faire — au lieu de l'appeler puis d'échouer.
    pub capabilities: Vec<String>,
    /// Comment demander son accord à l'utilisateur avant un appel d'outil
    /// sensible. `None` — le cas d'un hôte sans interface — vaut refus : un
    /// service qui tourne sans personne devant ne doit pas s'autoriser une
    /// opération que l'on aurait voulu arbitrer.
    pub approval: Option<approval::ApprovalHandle>,
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
