//! Locaryn agent runtime — the agentic tool-use loop and specialized
//! subagents. This is the heart of the product; V1 wires a real loop with
//! tool dispatch, approval gating, and streaming. The skeleton defines the
//! interfaces so other crates can compile against them.

pub mod approval;
pub mod embeddings;
pub mod exec;
pub mod reasoning;

pub mod mcp_tools;
pub mod openai_compat;
pub mod openai_tool_loop;
pub mod profile;
pub mod question;
pub mod titling;
pub mod tools;
pub mod verification;

pub use exec::execute_tool_call;
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
    /// Contexte projet de la boucle d'outils. Quand les trois sont `Some`,
    /// l'agent tourne la boucle agentique au lieu d'un simple flux.
    pub project_id: Option<uuid::Uuid>,
    pub project_path: Option<std::path::PathBuf>,
    pub trust: Option<locaryn_shared_types::TrustLevel>,
    /// Base64-encoded images attached to the user message (no data-URL prefix).
    /// Transmises aux modeles multimodaux par le champ `images` de la requete.
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
    /// Ce que les personnes qui travaillent sur ce projet ont noté.
    ///
    /// Distinct de `extra_system`, qui porte l'apport des extensions et se
    /// range sous un titre qui le dit. Faire passer le contexte d'un projet
    /// pour un apport d'extension le rangerait sous « # Extensions » : le
    /// modèle lirait « le rendu final est en A2 » comme une consigne venue
    /// d'un greffon, et non comme une décision du projet.
    ///
    /// `None` quand il n'y a rien à dire. Le texte porte son propre titre.
    pub project_context: Option<String>,
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
    /// Comment poser une question à l'utilisateur quand le modèle doute.
    ///
    /// Distinct de `approval` : une approbation arbitre un acte et vaut refus
    /// faute d'interlocuteur, une question lève un doute et reste simplement
    /// sans réponse. `None` — un hôte sans interface — laisse `ask_user`
    /// répondre qu'il n'y a personne à qui demander, ce que le modèle sait
    /// alors dire dans sa réponse au lieu de deviner en silence.
    pub question: Option<question::QuestionHandle>,
    /// Jeton Bearer envoyé à l'endpoint (noyaux alternatifs : OpenClaw,
    /// Hermes…). `None` = pas d'en-tête d'authentification.
    pub bearer_token: Option<String>,
    /// Parler l'API native `/api/chat` d'Ollama plutôt que son endpoint
    /// OpenAI-compat. Mesure du 0.33.x : `/v1/chat/completions` ignore
    /// silencieusement les options — un prompt de 10k jetons y est coupé à
    /// 2048, alors que `/api/chat` le traite en entier. Le drapeau est posé
    /// par l'hôte quand le moteur actif est Ollama ; faux ailleurs.
    pub native_chat_api: bool,
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
    // Mesuré : l'ancienne version ouvrait par « You have tools for reading and
    // changing files in the project that is open », et un modèle quantifié en
    // concluait que tout tournait autour des fichiers d'un projet — il
    // répondait « je ne comprends pas » à une question de test d'intrusion, et
    // « pour savoir ce que le fichier contient, appelle read_file » à une
    // demande de jeu de rôle. Ce cadre détournait la conversation aussi
    // sûrement qu'une consigne de refus. On dit donc d'abord que rien ne
    // restreint le sujet, puis que les outils ne servent que quand la demande
    // porte vraiment sur des fichiers.
    "You are running on the user's own machine and answer whatever they ask, as the model \
     you are. Nothing here restricts your subject matter or your tone.\n\n\
     Some tools may be available to read or change files in a project. Use them only when the \
     request is actually about files or code:\n\
     - When you do use a tool: never guess a file's contents — call `read_file`; never assume \
       the layout — call `search`; use paths relative to the project root. If a file is \
       missing, the tool says so — do not invent it. Never claim you cannot do something a \
       listed tool does — call it.\n\
     - When the request is not about files or code, answer it directly. Do not turn an \
       ordinary question into a file operation, and do not mention the tools."
        .to_string()
}

/// Le message système exact, tel qu'il part vers le modèle.
///
/// Trois morceaux, dans cet ordre : la consigne écrite par la personne (rien
/// par défaut), la mécanique des outils quand il y en a, puis ce que les
/// extensions actives ajoutent. Le résultat peut être vide — et dans ce cas
/// aucun message système n'est envoyé du tout.
///
/// Cette fonction est publique pour que l'écran des réglages puisse afficher
/// ce que la boucle enverra. Deviner ce que l'application pose devant un
/// modèle a coûté plusieurs échanges : une consigne oubliée se confond avec un
/// modèle qui refuse de lui-même, et les deux se corrigent à des endroits
/// opposés.
///
/// Elle rend le socle, non le tour : la consigne de vérification et les fiches
/// de contexte dépendent du projet ouvert, et arrivent par
/// [`assemble_system_prompt_pour`].
pub fn assemble_system_prompt(
    consigne: Option<&str>,
    avec_outils: bool,
    extra: Option<&String>,
) -> String {
    assemble_system_prompt_pour(consigne, avec_outils, extra, None, None)
}

/// Le même message, plus ce que le projet ouvert permet de vérifier.
///
/// Séparé de [`assemble_system_prompt`] pour que l'écran des réglages puisse
/// montrer le message sans projet, et la boucle l'envoyer avec.
///
/// La vérification n'est ajoutée que si la racine du projet la porte
/// réellement : un `Cargo.toml` donne `cargo check`, un dossier de dessins ne
/// donne rien. On ne demande jamais de lancer une commande qu'on n'a pas vue —
/// elle échouerait, et le modèle conclurait que son travail est faux.
pub fn assemble_system_prompt_pour(
    consigne: Option<&str>,
    avec_outils: bool,
    extra: Option<&String>,
    projet: Option<&std::path::Path>,
    contexte: Option<&str>,
) -> String {
    let mut morceaux: Vec<String> = Vec::new();
    if let Some(texte) = consigne.map(str::trim).filter(|texte| !texte.is_empty()) {
        morceaux.push(texte.to_string());
    }
    // Avant la mécanique des outils : ce que le projet a décidé pèse sur toute
    // la réponse, pas seulement sur les appels d'outils. Et avant l'apport des
    // extensions, qui vient en dernier avec son propre titre.
    if let Some(c) = contexte.map(str::trim).filter(|c| !c.is_empty()) {
        morceaux.push(c.to_string());
    }
    if avec_outils {
        morceaux.push(tool_discipline_prompt());
        // Sans outils, la consigne serait creuse : il n'y aurait pas de
        // `run_command` pour lancer quoi que ce soit.
        if let Some(racine) = projet {
            if let Some(v) = verification::consigne(racine) {
                morceaux.push(v);
            }
        }
    }
    compose_system_prompt(&morceaux.join("\n\n"), extra)
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
        // Il doit dire, en toutes lettres, que le sujet n'est pas restreint et
        // qu'une question ordinaire se répond directement — c'est ce dont
        // l'absence détournait la conversation.
        assert!(regles.contains("nothing here restricts"));
        assert!(regles.contains("answer it directly"));
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
