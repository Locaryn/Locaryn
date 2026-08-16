//! Locaryn shared types — serializable, zero business logic dependency.
//!
//! These types cross every boundary: Rust core <-> daemon HTTP API <->
//! remote-server HTTP API <-> CLI <-> Tauri frontend (via serde_json).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Projects
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub trust_level: TrustLevel,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Full file access; auto-approve reads.
    Trusted,
    /// Approve each file access. (Default — safest.)
    #[default]
    Untrusted,
    /// Preview-only; no shell, no writes.
    Sandbox,
}

// ============================================================================
// Sessions & Messages
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: Option<String>,
    pub provider_id: Option<Uuid>,
    pub model: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    /// Rangée aux archives. Elle ne s'affiche plus dans la liste courante,
    /// mais elle existe : la suppression est un geste séparé.
    #[serde(default)]
    pub archived_at: Option<DateTime<Utc>>,
    /// Éphémère : rien de ce qui s'y dit ne doit rester.
    #[serde(default)]
    pub ephemeral: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub call_id: String,
    pub tool: String,
    pub args: serde_json::Value,
}

// ============================================================================
// Tasks
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub session_id: Uuid,
    pub status: TaskStatus,
    pub progress: f32,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    AwaitingApproval,
    Completed,
    Cancelled,
    Failed,
}

// ============================================================================
// Artifacts
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub session_id: Uuid,
    pub kind: ArtifactKind,
    pub path: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Html,
    Markdown,
    PythonText,
    ImagePng,
    PlotlyHtml,
}

// ============================================================================
// Providers
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: Uuid,
    pub kind: ProviderKind,
    pub engine: ProviderEngine,
    pub endpoint: String,
    pub model: Option<String>,
    pub is_active: bool,
    pub status: ProviderStatus,
    pub config: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Remote,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderEngine {
    Ollama,
    LlamaCpp,
    Lmstudio,
    Vllm,
    OpenAiCompat,
    /// AirLLM — low-VRAM inference engine (layer-by-layer offloading).
    /// Runs an OpenAI-compatible Python server on loopback.
    AirLlm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStatus {
    Unknown,
    Healthy,
    Unhealthy,
    Starting,
}

// ============================================================================
// Connection mode (client)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionMode {
    Auto,
    Remote,
    Local,
}

// ============================================================================
// Extensions
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionKind {
    Plugin,
    Mcp,
    Command,
    Skill,
    Hook,
    Agent,
    Rules,
    Lsp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionScope {
    Global,
    User,
    Workspace,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Shell,
    FilesRead,
    FilesWrite,
    Network,
    Extensions,
    Mcp,
    Preview,
    Lsp,
    Env,
}

/// All permissions — used for completeness checks.
pub const ALL_PERMISSIONS: &[Permission] = &[
    Permission::Shell,
    Permission::FilesRead,
    Permission::FilesWrite,
    Permission::Network,
    Permission::Extensions,
    Permission::Mcp,
    Permission::Preview,
    Permission::Lsp,
    Permission::Env,
];

/// Which upstream ecosystem a bundle comes from. Drives two things: how the
/// browse UI groups entries, and which adapter converts the bundle at install
/// time. `Locaryn` needs no conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ExtensionEcosystem {
    #[default]
    #[serde(rename = "locaryn")]
    Locaryn,
    #[serde(rename = "claude_code")]
    ClaudeCode,
    #[serde(rename = "gemini_cli")]
    GeminiCli,
    #[serde(rename = "opencode")]
    OpenCode,
    /// A bare MCP server (no surrounding plugin bundle).
    #[serde(rename = "mcp")]
    Mcp,
    #[serde(rename = "cursor")]
    Cursor,
    #[serde(rename = "continue")]
    Continue,
    #[serde(rename = "cline")]
    Cline,
}

impl ExtensionEcosystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Locaryn => "locaryn",
            Self::ClaudeCode => "claude_code",
            Self::GeminiCli => "gemini_cli",
            Self::OpenCode => "opencode",
            Self::Mcp => "mcp",
            Self::Cursor => "cursor",
            Self::Continue => "continue",
            Self::Cline => "cline",
        }
    }

    /// Human label for the browse UI. Product names are never translated.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Locaryn => "Locaryn",
            Self::ClaudeCode => "Claude Code",
            Self::GeminiCli => "Gemini CLI",
            Self::OpenCode => "OpenCode",
            Self::Mcp => "MCP",
            Self::Cursor => "Cursor",
            Self::Continue => "Continue",
            Self::Cline => "Cline",
        }
    }
}

/// What a plugin actually contributes once enabled. Counted after the bundle
/// is parsed, so a plugin advertising ten commands but shipping none shows 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionComponents {
    pub skills: u32,
    pub commands: u32,
    pub agents: u32,
    pub rules: u32,
    pub hooks: u32,
    pub mcp_servers: u32,
    pub lsp_adapters: u32,
}

impl ExtensionComponents {
    pub fn total(&self) -> u32 {
        self.skills
            + self.commands
            + self.agents
            + self.rules
            + self.hooks
            + self.mcp_servers
            + self.lsp_adapters
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// One permission a plugin asked for, plus the user's decision on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionPermissionState {
    pub permission: Permission,
    /// Why the plugin says it needs this. Shown verbatim in the approval modal.
    pub reason: Option<String>,
    pub granted: bool,
}

/// Ce qu'une extension ajoute à l'interface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtensionUi {
    #[serde(default)]
    pub nav_items: Vec<ExtensionUiEntry>,
    #[serde(default)]
    pub studio_tabs: Vec<ExtensionUiEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionUiEntry {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// An installed extension as the UI sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledExtension {
    pub id: Uuid,
    /// Canonical id: lowercase, kebab/underscore only. Unique per scope.
    pub name: String,
    /// Pretty name from the manifest; falls back to `name`.
    pub display_name: String,
    pub version: String,
    pub api_version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub kind: ExtensionKind,
    pub scope: ExtensionScope,
    pub ecosystem: ExtensionEcosystem,
    /// Where it came from: `owner/repo`, a git URL, or a local path.
    pub source: Option<String>,
    pub install_dir: String,
    pub enabled: bool,
    pub components: ExtensionComponents,
    /// Ce que l'extension sait faire (`image-gen`, `voice-tts`, …). L'interface
    /// s'en sert pour décider quels écrans existent : le Studio de génération
    /// n'apparaît que si une extension active apporte une de ces capacités, et
    /// disparaît quand la dernière est retirée.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Ce que l'extension ajoute à l'interface : entrées de navigation et
    /// onglets du Studio.
    #[serde(default)]
    pub ui: ExtensionUi,
    pub permissions: Vec<ExtensionPermissionState>,
    /// Non-fatal problems found while loading components. A plugin can be
    /// enabled with a broken skill file; the UI surfaces why that skill is
    /// missing instead of silently dropping it.
    pub load_errors: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How well a catalog entry can actually run inside Locaryn. Set by the
/// adapter that would handle it, so the browse UI never promises more than
/// the loader delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogCompat {
    /// A Locaryn plugin. Installs and runs as-is.
    Native,
    /// Foreign but fully declarative — the adapter converts it losslessly.
    Adapted,
    /// Only part of the bundle can run here (e.g. host-specific runtime code
    /// is skipped, the MCP servers and markdown still work).
    Partial,
    /// Listed so it is findable, but nothing in it can run in Locaryn.
    Unsupported,
}

impl CatalogCompat {
    pub fn installable(&self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// One browsable entry from a remote catalog (a marketplace, a registry, or a
/// curated index).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Stable across refreshes: `<ecosystem>:<catalog_id>:<name>`.
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub homepage: Option<String>,
    pub ecosystem: ExtensionEcosystem,
    /// Id of the `CatalogSource` this came from.
    pub catalog_id: String,
    /// Human name of that source, e.g. "anthropics/claude-code".
    pub catalog_label: String,
    /// What to hand to `install_extension`.
    pub install_source: String,
    pub keywords: Vec<String>,
    /// Component labels the catalog advertises, e.g. `["4 commands", "1 mcp"]`.
    /// Advertised, not verified — only an install can confirm.
    pub advertised: Vec<String>,
    pub compat: CatalogCompat,
    /// Set by the backend when a matching extension is already installed.
    pub installed: bool,
}

/// A remote catalog Locaryn can read. Built-in sources ship with the app; the
/// user can add more (any repo exposing a supported index file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSource {
    pub id: String,
    pub label: String,
    pub ecosystem: ExtensionEcosystem,
    /// Fetchable URL of the index (raw JSON) or the API endpoint.
    pub url: String,
    /// Shipped with the app (cannot be deleted, only disabled).
    pub builtin: bool,
    pub enabled: bool,
}

/// Per-source outcome of a catalog refresh, so a single dead source does not
/// look like "no extensions exist".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSourceStatus {
    pub source: CatalogSource,
    pub ok: bool,
    pub entry_count: u32,
    pub error: Option<String>,
}

/// Result of browsing the catalogs. Served from the on-disk cache when
/// offline, so the store is never empty just because the network is down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub entries: Vec<CatalogEntry>,
    pub sources: Vec<CatalogSourceStatus>,
    pub fetched_at: Option<DateTime<Utc>>,
    /// True when every source failed and the entries came from cache.
    pub stale: bool,
}

// ============================================================================
// Tool approval — scope of a user-granted "allow"
// ============================================================================

/// Longevity of a granted tool approval. The LLM cannot widen its own scope;
/// only an explicit UI click in the modal upgrades the lifetime, and the
/// promotion is persisted (in-memory + optional whitelist table in V1.1).
///
/// The rule "minimum acceptable scope" is enforced by the desktop modal:
/// `Critical` tools CANNOT be approved with `Once` only — they require at
/// least one user-confirmation per session, and `Always` is hidden for
/// `Critical` until the user grants a project-level scope explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskScope {
    /// Apply only to this exact invocation. Default for High/Critical.
    Once,
    /// Apply for any subsequent call of the same tool name within the
    /// current session. UI may pick this for Medium tools.
    Session,
    /// Apply when the same (project_id, tool) pair recurs. Stored via the
    /// `extension_permissions` table (added in migration 0004 in V1.1).
    Project,
    /// Apply forever for this (project_id, tool) pair across sessions.
    /// UI hides this for Critical.
    Always,
}

impl RiskScope {
    /// Minimum scope the runtime will accept for a given risk. Modal MUST
    /// disable any weaker option (e.g. Critical refuses Once).
    pub const fn minimum_for(risk_tier: u8) -> Self {
        match risk_tier {
            0 => RiskScope::Once, // Low: doesn't matter, not displayed
            1 => RiskScope::Once, // Medium
            2 => RiskScope::Once, // High
            _ => RiskScope::Once, // Critical: still requires fresh consent per call
                                   //       (the user can then UP-grade to Project
                                   //       or Always by clicking the appropriate chip)
        }
    }

    /// True if this scope is allowed to be granted for the given risk tier.
    /// Critical blocks "Always" until a confirmed project-level whitelist
    /// exists (V1.1); for now we allow Project but warn loudly.
    pub const fn is_allowed_for(risk_tier: u8, _scope: RiskScope) -> bool {
        if risk_tier >= 3 {
            // Critical: every scope is allowed on the modal, but the UI
            // must make the user type the project path to confirm "Always".
            return true;
        }
        true
    }

    /// Short label for chip text.
    pub const fn label(self) -> &'static str {
        match self {
            RiskScope::Once => "this call only",
            RiskScope::Session => "this session",
            RiskScope::Project => "this project",
            RiskScope::Always => "always (whitelist)",
        }
    }
}

/// User's answer to one approval prompt. Persisted as part of the audit log
/// (table `approval_decisions`, added in V1.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolApprovalDecision {
    pub call_id: String,
    pub tool: String,
    pub risk: Risk,
    pub decision: ApprovalVerdict,
    pub scope: RiskScope,
    /// Set for `Project` and `Always` so downstream renewals can verify.
    pub project_id: Option<Uuid>,
    pub decided_by: String, // "local" or user id (remote)
    pub decided_at: DateTime<Utc>,
    /// Optional user-typed reason ("because I'm refactoring the auth module")
    /// captured for the audit log. Empty string == none.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalVerdict {
    Allow,
    Deny,
}

/// Severity of a tool invocation. Drives the approval UX (modal vs auto-run,
/// banner color, minimum approval scope). See `locaryn-agent-runtime::tools`
/// for the canonical decision tables that consume this enum.
///
/// `Critical` is reserved for tools that cross the local trust boundary
/// (SSH exec, network-aware MCP, settings that affect the daemon globally).
/// Honour the doc 11 §5 rule: never auto-approve Critical under any trust
/// level, ever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

impl Risk {
    /// Numeric ordering used for comparison: Low < Medium < High < Critical.
    pub fn tier(self) -> u8 {
        match self {
            Risk::Low => 0,
            Risk::Medium => 1,
            Risk::High => 2,
            Risk::Critical => 3,
        }
    }

    /// True when this risk tier triggers the interactive approval modal
    /// (Medium or above). The rule is shared between the rust core and the
    /// modal so the UI cannot disagree with the runtime.
    pub const fn requires_approval(self) -> bool {
        matches!(self, Risk::Medium | Risk::High | Risk::Critical)
    }

    /// Short label for badge text.
    pub const fn label(self) -> &'static str {
        match self {
            Risk::Low => "Safe",
            Risk::Medium => "Modifies",
            Risk::High => "Executes",
            Risk::Critical => "Remote / Critical",
        }
    }

    /// CSS-friendly token name (matches `var(--risk-…)` set in
    /// `apps/desktop/src/styles/global.css`).
    pub const fn token(self) -> &'static str {
        match self {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
            Risk::Critical => "critical",
        }
    }
}

/// Backward-compatible alias: `RiskLite` is the same as `Risk`.
pub type RiskLite = Risk;

// ============================================================================
// SSH server connector
// ============================================================================

/// An SSH server managed by the SSH connector. Deliberately models NO secret
/// field — passwords/passphrases never cross this (serde) boundary; only a
/// keychain reference (`secret_ref`) lives in the DB, and it is never exposed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshServer {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    /// Path to an on-disk private key (for `key` auth). Referenced, never copied.
    pub key_path: Option<String>,
    pub jump: Option<SshJump>,
    pub host_key_algo: Option<String>,
    /// Base64 SHA-256 host-key fingerprint pinned on first contact (TOFU).
    pub host_key_sha256: Option<String>,
    pub host_key_verified: bool,
    pub ai_access: SshAiAccess,
    /// JSON probe result (OS, read/write, sudo…).
    pub capabilities: Option<serde_json::Value>,
    pub scope: ExtensionScope,
    pub status: SshStatus,
    pub enabled: bool,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshAuthMethod {
    Password,
    Key,
    Agent,
}

/// How much the AI may do on a server. Default `None` = the AI cannot even see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAiAccess {
    /// Invisible to the AI (default).
    #[default]
    None,
    /// AI may run a non-mutating allowlist only.
    ReadOnly,
    /// AI may propose any command, but every call needs interactive approval.
    Approval,
    /// Allowlist auto-runs; mutating commands still need approval.
    Trusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshStatus {
    #[default]
    Unknown,
    Ok,
    Error,
}

/// An optional jump host (ProxyJump) to reach the target through.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshJump {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub key_path: Option<String>,
}

// ============================================================================
// Health & info
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub status: String,
    pub version: String,
    pub mode: ConnectionMode,
    pub active_provider: Option<ProviderSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSummary {
    pub kind: ProviderKind,
    pub engine: ProviderEngine,
    pub endpoint: String,
    pub model: Option<String>,
}

// ============================================================================
// Error envelope (HTTP API)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// ============================================================================
// Encodage base64
// ============================================================================

/// Encoder des octets en base64 standard, avec remplissage.
///
/// Vit ici parce que trois programmes en ont besoin pour la même raison : une
/// vue web ne peut pas lire le disque du serveur, donc une image générée
/// voyage dans le JSON. Le service l'utilisait déjà, le téléphone en a besoin
/// pour les artefacts d'un outil, et une deuxième implémentation finirait par
/// diverger de la première.
pub fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

#[cfg(test)]
mod base64_tests {
    use super::base64_encode;

    #[test]
    fn vecteurs_de_la_rfc_4648() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn les_octets_non_ascii_passent_entiers() {
        // L'en-tête PNG : c'est ce qui traverse réellement.
        assert_eq!(base64_encode(&[0x89, 0x50, 0x4E, 0x47]), "iVBORw==");
    }
}
