//! Typed repositories over SQLite. Each repo owns a subset of tables.
//!
//! Conventions
//! -----------
//! * UUIDs are stored as `TEXT` (canonical hyphenated form).
//! * Timestamps are stored as `TEXT` (RFC 3339 / ISO 8601, UTC).
//! * Enums are stored as `TEXT` using the same token names `serde` emits
//!   (so the wire format and the on-disk format are identical).
//! * Booleans are stored as `INTEGER` (0/1).
//! * JSON blobs (`tool_calls`, provider `config`) are stored as `TEXT`.
//!
//! Every `create`/`upsert_*` uses `INSERT ... RETURNING *` (SQLite ≥ 3.35,
//! which the bundled sqlx sqlite-sys satisfies).

use crate::error::StorageError;
use locaryn_shared_types::{
    Artifact, ArtifactKind, ExtensionEcosystem, ExtensionKind, ExtensionScope, Message,
    MessageRole, Permission, Project, Provider, ProviderEngine, ProviderKind, ProviderStatus,
    Session, Task, TaskStatus, ToolCall, TrustLevel,
};
#[cfg(feature = "ssh-connector")]
use locaryn_shared_types::{SshAiAccess, SshAuthMethod, SshJump, SshServer, SshStatus};
use sqlx::SqlitePool;
use uuid::Uuid;

// ============================================================================
// Row structs (sqlx::FromRow) — map 1:1 to columns. Conversions from these
// to the shared `locaryn_shared_types::*` happen in small `try_into` helpers.
// ============================================================================

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    path: String,
    name: String,
    trust_level: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    project_id: String,
    title: Option<String>,
    provider_id: Option<String>,
    model: Option<String>,
    created_at: String,
    last_message_at: Option<String>,
    closed_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    session_id: String,
    role: String,
    content: String,
    tool_calls: Option<String>,
    tool_call_id: Option<String>,
    tokens_in: i64,
    tokens_out: i64,
    parent_id: Option<String>,
    created_at: String,
}

#[derive(sqlx::FromRow)]
struct TaskRow {
    id: String,
    session_id: String,
    status: String,
    progress: f64,
    started_at: Option<String>,
    ended_at: Option<String>,
    error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ArtifactRow {
    id: String,
    session_id: String,
    kind: String,
    path: String,
    title: Option<String>,
    created_at: String,
}

#[derive(sqlx::FromRow)]
struct ProviderRow {
    id: String,
    kind: String,
    engine: String,
    endpoint: String,
    model: Option<String>,
    is_active: i64,
    status: String,
    config: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct ExtensionRow {
    id: String,
    name: String,
    version: String,
    api_version: String,
    kind: String,
    scope: String,
    ecosystem: String,
    source: Option<String>,
    manifest_path: String,
    enabled: i64,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct ExtensionPermissionRow {
    permission: String,
    granted: i64,
}

#[cfg(feature = "ssh-connector")]
#[derive(sqlx::FromRow)]
struct SshServerRow {
    id: String,
    name: String,
    description: String,
    host: String,
    port: i64,
    username: String,
    auth_method: String,
    secret_ref: Option<String>,
    key_path: Option<String>,
    jump_json: Option<String>,
    host_key_algo: Option<String>,
    host_key_sha256: Option<String>,
    host_key_verified: i64,
    ai_access: String,
    capabilities: Option<String>,
    scope: String,
    status: String,
    enabled: i64,
    last_connected_at: Option<String>,
    created_at: String,
    updated_at: String,
}

// ============================================================================
// Helpers
// ============================================================================

fn uid(s: &str) -> Result<Uuid, StorageError> {
    Uuid::parse_str(s).map_err(|e| StorageError::Decode(format!("uuid: {e}")))
}

fn opt_uid(s: Option<&str>) -> Result<Option<Uuid>, StorageError> {
    match s {
        Some(v) => Ok(Some(uid(v)?)),
        None => Ok(None),
    }
}

fn dt(s: &str) -> Result<chrono::DateTime<chrono::Utc>, StorageError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .map_err(|e| StorageError::Decode(format!("datetime: {e}")))
}

fn opt_dt(s: Option<&str>) -> Result<Option<chrono::DateTime<chrono::Utc>>, StorageError> {
    match s {
        Some(v) => Ok(Some(dt(v)?)),
        None => Ok(None),
    }
}

fn bool_from_i64(v: i64) -> bool {
    v != 0
}

// ============================================================================
// Conversions: row → shared type
// ============================================================================

impl TryFrom<ProjectRow> for Project {
    type Error = StorageError;
    fn try_from(r: ProjectRow) -> Result<Self, Self::Error> {
        Ok(Project {
            id: uid(&r.id)?,
            path: r.path,
            name: r.name,
            trust_level: TrustLevel::from_token(&r.trust_level),
            created_at: dt(&r.created_at)?,
            updated_at: dt(&r.updated_at)?,
            deleted_at: opt_dt(r.deleted_at.as_deref())?,
        })
    }
}

impl TryFrom<SessionRow> for Session {
    type Error = StorageError;
    fn try_from(r: SessionRow) -> Result<Self, Self::Error> {
        Ok(Session {
            id: uid(&r.id)?,
            project_id: uid(&r.project_id)?,
            title: r.title,
            provider_id: opt_uid(r.provider_id.as_deref())?,
            model: r.model,
            created_at: dt(&r.created_at)?,
            last_message_at: opt_dt(r.last_message_at.as_deref())?,
            closed_at: opt_dt(r.closed_at.as_deref())?,
        })
    }
}

impl TryFrom<MessageRow> for Message {
    type Error = StorageError;
    fn try_from(r: MessageRow) -> Result<Self, Self::Error> {
        let tool_calls = match r.tool_calls.as_deref() {
            Some(s) if !s.is_empty() => Some(
                serde_json::from_str::<Vec<ToolCall>>(s)
                    .map_err(|e| StorageError::Decode(format!("tool_calls: {e}")))?,
            ),
            _ => None,
        };
        Ok(Message {
            id: uid(&r.id)?,
            session_id: uid(&r.session_id)?,
            role: MessageRole::from_token(&r.role),
            content: r.content,
            tool_calls,
            tool_call_id: r.tool_call_id,
            tokens_in: r.tokens_in as u64,
            tokens_out: r.tokens_out as u64,
            parent_id: opt_uid(r.parent_id.as_deref())?,
            created_at: dt(&r.created_at)?,
        })
    }
}

impl TryFrom<TaskRow> for Task {
    type Error = StorageError;
    fn try_from(r: TaskRow) -> Result<Self, Self::Error> {
        Ok(Task {
            id: uid(&r.id)?,
            session_id: uid(&r.session_id)?,
            status: TaskStatus::from_token(&r.status),
            progress: r.progress as f32,
            started_at: opt_dt(r.started_at.as_deref())?,
            ended_at: opt_dt(r.ended_at.as_deref())?,
            error: r.error,
        })
    }
}

impl TryFrom<ArtifactRow> for Artifact {
    type Error = StorageError;
    fn try_from(r: ArtifactRow) -> Result<Self, Self::Error> {
        Ok(Artifact {
            id: uid(&r.id)?,
            session_id: uid(&r.session_id)?,
            kind: ArtifactKind::from_token(&r.kind),
            path: r.path,
            title: r.title,
            created_at: dt(&r.created_at)?,
        })
    }
}

impl TryFrom<ProviderRow> for Provider {
    type Error = StorageError;
    fn try_from(r: ProviderRow) -> Result<Self, Self::Error> {
        let config = match r.config.as_deref() {
            Some(s) if !s.is_empty() => Some(
                serde_json::from_str(s)
                    .map_err(|e| StorageError::Decode(format!("config: {e}")))?,
            ),
            _ => None,
        };
        Ok(Provider {
            id: uid(&r.id)?,
            kind: ProviderKind::from_token(&r.kind),
            engine: ProviderEngine::from_token(&r.engine),
            endpoint: r.endpoint,
            model: r.model,
            is_active: bool_from_i64(r.is_active),
            status: ProviderStatus::from_token(&r.status),
            config,
            created_at: dt(&r.created_at)?,
            updated_at: dt(&r.updated_at)?,
        })
    }
}

#[cfg(feature = "ssh-connector")]
impl TryFrom<SshServerRow> for SshServer {
    type Error = StorageError;
    fn try_from(r: SshServerRow) -> Result<Self, Self::Error> {
        let jump = match r.jump_json.as_deref() {
            Some(s) if !s.is_empty() => Some(
                serde_json::from_str::<SshJump>(s)
                    .map_err(|e| StorageError::Decode(format!("jump_json: {e}")))?,
            ),
            _ => None,
        };
        let capabilities = match r.capabilities.as_deref() {
            Some(s) if !s.is_empty() => Some(
                serde_json::from_str(s)
                    .map_err(|e| StorageError::Decode(format!("capabilities: {e}")))?,
            ),
            _ => None,
        };
        Ok(SshServer {
            id: uid(&r.id)?,
            name: r.name,
            description: r.description,
            host: r.host,
            port: r.port as u16,
            username: r.username,
            auth_method: SshAuthMethod::from_token(&r.auth_method),
            key_path: r.key_path,
            jump,
            host_key_algo: r.host_key_algo,
            host_key_sha256: r.host_key_sha256,
            host_key_verified: bool_from_i64(r.host_key_verified),
            ai_access: SshAiAccess::from_token(&r.ai_access),
            capabilities,
            scope: ExtensionScope::from_token(&r.scope),
            status: SshStatus::from_token(&r.status),
            enabled: bool_from_i64(r.enabled),
            last_connected_at: opt_dt(r.last_connected_at.as_deref())?,
            created_at: dt(&r.created_at)?,
            updated_at: dt(&r.updated_at)?,
        })
    }
}

// ============================================================================
// Small enum-token helpers (kept local to avoid a serde round-trip in the
// hot path; they mirror the serde `rename_all` rules in shared-types).
// ============================================================================

trait FromToken: Sized {
    fn from_token(s: &str) -> Self;
}

impl FromToken for TrustLevel {
    fn from_token(s: &str) -> Self {
        match s {
            "trusted" => TrustLevel::Trusted,
            "sandbox" => TrustLevel::Sandbox,
            _ => TrustLevel::Untrusted,
        }
    }
}

impl FromToken for MessageRole {
    fn from_token(s: &str) -> Self {
        match s {
            "user" => MessageRole::User,
            "tool" => MessageRole::Tool,
            "system" => MessageRole::System,
            _ => MessageRole::Assistant,
        }
    }
}

impl FromToken for TaskStatus {
    fn from_token(s: &str) -> Self {
        match s {
            "running" => TaskStatus::Running,
            "awaiting_approval" => TaskStatus::AwaitingApproval,
            "completed" => TaskStatus::Completed,
            "cancelled" => TaskStatus::Cancelled,
            "failed" => TaskStatus::Failed,
            _ => TaskStatus::Pending,
        }
    }
}

impl FromToken for ArtifactKind {
    fn from_token(s: &str) -> Self {
        match s {
            "html" => ArtifactKind::Html,
            "markdown" => ArtifactKind::Markdown,
            "python_text" => ArtifactKind::PythonText,
            "image_png" => ArtifactKind::ImagePng,
            "plotly_html" => ArtifactKind::PlotlyHtml,
            _ => ArtifactKind::Html,
        }
    }
}

impl FromToken for ProviderKind {
    fn from_token(s: &str) -> Self {
        match s {
            "remote" => ProviderKind::Remote,
            _ => ProviderKind::Local,
        }
    }
}

impl FromToken for ProviderEngine {
    fn from_token(s: &str) -> Self {
        match s {
            "ollama" => ProviderEngine::Ollama,
            "llama_cpp" => ProviderEngine::LlamaCpp,
            "lmstudio" => ProviderEngine::Lmstudio,
            "vllm" => ProviderEngine::Vllm,
            "open_ai_compat" => ProviderEngine::OpenAiCompat,
            _ => ProviderEngine::Ollama,
        }
    }
}

impl FromToken for ProviderStatus {
    fn from_token(s: &str) -> Self {
        match s {
            "healthy" => ProviderStatus::Healthy,
            "unhealthy" => ProviderStatus::Unhealthy,
            "starting" => ProviderStatus::Starting,
            _ => ProviderStatus::Unknown,
        }
    }
}

impl FromToken for ExtensionScope {
    fn from_token(s: &str) -> Self {
        match s {
            "global" => ExtensionScope::Global,
            "workspace" => ExtensionScope::Workspace,
            "session" => ExtensionScope::Session,
            _ => ExtensionScope::User,
        }
    }
}

impl FromToken for ExtensionKind {
    fn from_token(s: &str) -> Self {
        match s {
            "mcp" => ExtensionKind::Mcp,
            "command" => ExtensionKind::Command,
            "skill" => ExtensionKind::Skill,
            "hook" => ExtensionKind::Hook,
            "agent" => ExtensionKind::Agent,
            "rules" => ExtensionKind::Rules,
            "lsp" => ExtensionKind::Lsp,
            _ => ExtensionKind::Plugin,
        }
    }
}

impl FromToken for ExtensionEcosystem {
    fn from_token(s: &str) -> Self {
        match s {
            "claude_code" => ExtensionEcosystem::ClaudeCode,
            "gemini_cli" => ExtensionEcosystem::GeminiCli,
            "opencode" => ExtensionEcosystem::OpenCode,
            "mcp" => ExtensionEcosystem::Mcp,
            "cursor" => ExtensionEcosystem::Cursor,
            "continue" => ExtensionEcosystem::Continue,
            "cline" => ExtensionEcosystem::Cline,
            _ => ExtensionEcosystem::Locaryn,
        }
    }
}

impl FromToken for Permission {
    fn from_token(s: &str) -> Self {
        match s {
            "files_read" => Permission::FilesRead,
            "files_write" => Permission::FilesWrite,
            "network" => Permission::Network,
            "extensions" => Permission::Extensions,
            "mcp" => Permission::Mcp,
            "preview" => Permission::Preview,
            "lsp" => Permission::Lsp,
            "env" => Permission::Env,
            _ => Permission::Shell,
        }
    }
}

/// `Permission` → on-disk token. Mirrors the `snake_case` serde rule, so the
/// column and the wire format read the same.
fn permission_token(p: &Permission) -> &'static str {
    match p {
        Permission::Shell => "shell",
        Permission::FilesRead => "files_read",
        Permission::FilesWrite => "files_write",
        Permission::Network => "network",
        Permission::Extensions => "extensions",
        Permission::Mcp => "mcp",
        Permission::Preview => "preview",
        Permission::Lsp => "lsp",
        Permission::Env => "env",
    }
}

fn extension_kind_token(k: &ExtensionKind) -> &'static str {
    match k {
        ExtensionKind::Plugin => "plugin",
        ExtensionKind::Mcp => "mcp",
        ExtensionKind::Command => "command",
        ExtensionKind::Skill => "skill",
        ExtensionKind::Hook => "hook",
        ExtensionKind::Agent => "agent",
        ExtensionKind::Rules => "rules",
        ExtensionKind::Lsp => "lsp",
    }
}

#[cfg(feature = "ssh-connector")]
impl FromToken for SshAuthMethod {
    fn from_token(s: &str) -> Self {
        match s {
            "key" => SshAuthMethod::Key,
            "agent" => SshAuthMethod::Agent,
            _ => SshAuthMethod::Password,
        }
    }
}

#[cfg(feature = "ssh-connector")]
impl FromToken for SshAiAccess {
    fn from_token(s: &str) -> Self {
        match s {
            "read_only" => SshAiAccess::ReadOnly,
            "approval" => SshAiAccess::Approval,
            "trusted" => SshAiAccess::Trusted,
            _ => SshAiAccess::None,
        }
    }
}

#[cfg(feature = "ssh-connector")]
impl FromToken for SshStatus {
    fn from_token(s: &str) -> Self {
        match s {
            "ok" => SshStatus::Ok,
            "error" => SshStatus::Error,
            _ => SshStatus::Unknown,
        }
    }
}

// Enum → on-disk token (mirrors the serde rename rules).
fn scope_token(s: ExtensionScope) -> &'static str {
    match s {
        ExtensionScope::Global => "global",
        ExtensionScope::User => "user",
        ExtensionScope::Workspace => "workspace",
        ExtensionScope::Session => "session",
    }
}
#[cfg(feature = "ssh-connector")]
fn auth_method_token(a: SshAuthMethod) -> &'static str {
    match a {
        SshAuthMethod::Password => "password",
        SshAuthMethod::Key => "key",
        SshAuthMethod::Agent => "agent",
    }
}
#[cfg(feature = "ssh-connector")]
fn ai_access_token(a: SshAiAccess) -> &'static str {
    match a {
        SshAiAccess::None => "none",
        SshAiAccess::ReadOnly => "read_only",
        SshAiAccess::Approval => "approval",
        SshAiAccess::Trusted => "trusted",
    }
}
#[cfg(feature = "ssh-connector")]
fn ssh_status_token(s: SshStatus) -> &'static str {
    match s {
        SshStatus::Unknown => "unknown",
        SshStatus::Ok => "ok",
        SshStatus::Error => "error",
    }
}

// ============================================================================
// Repos
// ============================================================================

#[derive(Clone)]
pub struct ProjectRepo {
    pool: SqlitePool,
}

impl ProjectRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// All non-deleted projects, oldest first.
    pub async fn list(&self) -> Result<Vec<Project>, StorageError> {
        let rows = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, path, name, trust_level, created_at, updated_at, deleted_at \
             FROM projects WHERE deleted_at IS NULL ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Project::try_from).collect()
    }

    pub async fn create(
        &self,
        path: &str,
        name: &str,
        trust: TrustLevel,
    ) -> Result<Project, StorageError> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        let trust_token = match trust {
            TrustLevel::Trusted => "trusted",
            TrustLevel::Untrusted => "untrusted",
            TrustLevel::Sandbox => "sandbox",
        };

        // Archiving is a soft delete, but the UNIQUE index on `path` still
        // covers the archived row — so adding a folder back reported "already
        // registered" forever, with no way to recover it from the UI. Restore
        // it instead, which also brings its sessions back.
        if let Some(restored) = sqlx::query_as::<_, ProjectRow>(
            "UPDATE projects SET deleted_at = NULL, name = ?, trust_level = ?, updated_at = ? \
             WHERE path = ? AND deleted_at IS NOT NULL \
             RETURNING id, path, name, trust_level, created_at, updated_at, deleted_at",
        )
        .bind(name)
        .bind(trust_token)
        .bind(&now)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?
        {
            tracing::info!(%path, "projet archivé restauré au lieu d'être recréé");
            return Project::try_from(restored);
        }

        let row = sqlx::query_as::<_, ProjectRow>(
            "INSERT INTO projects (id, path, name, trust_level, created_at, updated_at, deleted_at) \
             VALUES (?, ?, ?, ?, ?, ?, NULL) \
             RETURNING id, path, name, trust_level, created_at, updated_at, deleted_at",
        )
        .bind(id.to_string())
        .bind(path)
        .bind(name)
        .bind(trust_token)
        .bind(&now)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db) if db.is_unique_violation() => {
                StorageError::Conflict(format!("project path already registered: {path}"))
            }
            other => StorageError::Sqlx(other),
        })?;
        row.try_into()
    }

    pub async fn get(&self, id: Uuid) -> Result<Project, StorageError> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, path, name, trust_level, created_at, updated_at, deleted_at \
             FROM projects WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => r.try_into(),
            None => Err(StorageError::NotFound(format!("project {id}"))),
        }
    }

    /// Soft-delete (set `deleted_at`). Idempotent if already deleted.
    /// Update a project's editable fields. `None` leaves a field untouched.
    /// Needed by the project-settings and chat-governance dialogs, which could
    /// previously display a trust level but never persist a change to it.
    pub async fn update_project(
        &self,
        id: Uuid,
        name: Option<&str>,
        trust: Option<TrustLevel>,
    ) -> Result<Project, StorageError> {
        if let Some(n) = name {
            sqlx::query("UPDATE projects SET name = ?, updated_at = ? WHERE id = ?")
                .bind(n)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
        }
        if let Some(t) = trust {
            let token = match t {
                TrustLevel::Trusted => "trusted",
                TrustLevel::Untrusted => "untrusted",
                TrustLevel::Sandbox => "sandbox",
            };
            sqlx::query("UPDATE projects SET trust_level = ?, updated_at = ? WHERE id = ?")
                .bind(token)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
        }
        self.get(id).await
    }

    pub async fn soft_delete(&self, id: Uuid) -> Result<(), StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query("UPDATE projects SET deleted_at = ?, updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("project {id}")));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct SessionRepo {
    pool: SqlitePool,
}

impl SessionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_for_project(&self, project_id: Uuid) -> Result<Vec<Session>, StorageError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT id, project_id, title, provider_id, model, created_at, last_message_at, closed_at \
             FROM sessions WHERE project_id = ? AND closed_at IS NULL \
             ORDER BY COALESCE(last_message_at, created_at) DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Session::try_from).collect()
    }

    pub async fn create(
        &self,
        project_id: Uuid,
        title: Option<String>,
    ) -> Result<Session, StorageError> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, SessionRow>(
            "INSERT INTO sessions (id, project_id, title, provider_id, model, created_at, last_message_at, closed_at) \
             VALUES (?, ?, ?, NULL, NULL, ?, NULL, NULL) \
             RETURNING id, project_id, title, provider_id, model, created_at, last_message_at, closed_at",
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(title.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await?;
        row.try_into()
    }

    pub async fn get(&self, id: Uuid) -> Result<Session, StorageError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, project_id, title, provider_id, model, created_at, last_message_at, closed_at \
             FROM sessions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => r.try_into(),
            None => Err(StorageError::NotFound(format!("session {id}"))),
        }
    }

    /// Bump `last_message_at` whenever a message is appended.
    pub async fn touch(&self, id: Uuid) -> Result<(), StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE sessions SET last_message_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Hard-delete a session and its messages.
    pub async fn delete(&self, id: Uuid) -> Result<(), StorageError> {
        let res = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("session {id}")));
        }
        Ok(())
    }

    /// Mark a session closed (no further messages expected).
    pub async fn close(&self, id: Uuid) -> Result<(), StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let res = sqlx::query("UPDATE sessions SET closed_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("session {id}")));
        }
        Ok(())
    }

    /// Update the session's display title.
    pub async fn update_title(&self, id: Uuid, title: &str) -> Result<(), StorageError> {
        let res = sqlx::query("UPDATE sessions SET title = ? WHERE id = ?")
            .bind(title)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("session {id}")));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct MessageRepo {
    pool: SqlitePool,
}

impl MessageRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_for_session(&self, session_id: Uuid) -> Result<Vec<Message>, StorageError> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, session_id, role, content, tool_calls, tool_call_id, \
                    tokens_in, tokens_out, parent_id, created_at \
             FROM messages WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Message::try_from).collect()
    }

    pub async fn append(
        &self,
        session_id: Uuid,
        role: MessageRole,
        content: &str,
    ) -> Result<Message, StorageError> {
        self.append_full(session_id, role, content, None, None, 0, 0, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn append_full(
        &self,
        session_id: Uuid,
        role: MessageRole,
        content: &str,
        tool_calls: Option<&[ToolCall]>,
        tool_call_id: Option<&str>,
        tokens_in: u64,
        tokens_out: u64,
        parent_id: Option<Uuid>,
    ) -> Result<Message, StorageError> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        let role_token = match role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
        };
        let tool_calls_json = match tool_calls {
            Some(tc) => Some(
                serde_json::to_string(tc)
                    .map_err(|e| StorageError::Decode(format!("tool_calls: {e}")))?,
            ),
            None => None,
        };
        let row = sqlx::query_as::<_, MessageRow>(
            "INSERT INTO messages \
             (id, session_id, role, content, tool_calls, tool_call_id, tokens_in, tokens_out, parent_id, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             RETURNING id, session_id, role, content, tool_calls, tool_call_id, \
                       tokens_in, tokens_out, parent_id, created_at",
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(role_token)
        .bind(content)
        .bind(tool_calls_json.as_deref())
        .bind(tool_call_id)
        .bind(tokens_in as i64)
        .bind(tokens_out as i64)
        .bind(parent_id.map(|u| u.to_string()))
        .bind(&now)
        .fetch_one(&self.pool)
        .await?;

        // Keep sessions.last_message_at in sync (best-effort; log on error).
        if let Err(e) = sqlx::query("UPDATE sessions SET last_message_at = ? WHERE id = ?")
            .bind(&now)
            .bind(session_id.to_string())
            .execute(&self.pool)
            .await
        {
            tracing::warn!(error = %e, "failed to bump session last_message_at");
        }

        row.try_into()
    }
}

#[derive(Clone)]
pub struct TaskRepo {
    pool: SqlitePool,
}

impl TaskRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, session_id: Uuid) -> Result<Task, StorageError> {
        let id = Uuid::new_v4();
        let row = sqlx::query_as::<_, TaskRow>(
            "INSERT INTO tasks (id, session_id, status, progress, started_at, ended_at, error) \
             VALUES (?, ?, 'pending', 0.0, NULL, NULL, NULL) \
             RETURNING id, session_id, status, progress, started_at, ended_at, error",
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        row.try_into()
    }

    pub async fn update_status(&self, id: Uuid, status: TaskStatus) -> Result<(), StorageError> {
        let token = match status {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
            TaskStatus::AwaitingApproval => "awaiting_approval",
            TaskStatus::Completed => "completed",
            TaskStatus::Cancelled => "cancelled",
            TaskStatus::Failed => "failed",
        };
        let now = chrono::Utc::now().to_rfc3339();
        // Set boundary timestamps depending on the transition.
        let (started, ended) = match status {
            TaskStatus::Running => (Some(now.as_str()), None),
            TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed => {
                (None, Some(now.as_str()))
            }
            _ => (None, None),
        };
        // Preserve the FIRST start/end timestamp: only set when currently
        // NULL, so a Running → AwaitingApproval → Running cycle doesn't
        // overwrite the original started_at.
        let res = sqlx::query(
            "UPDATE tasks SET status = ?, started_at = COALESCE(started_at, ?), \
             ended_at = COALESCE(ended_at, ?) WHERE id = ?",
        )
        .bind(token)
        .bind(started)
        .bind(ended)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("task {id}")));
        }
        Ok(())
    }

    pub async fn set_error(&self, id: Uuid, error: &str) -> Result<(), StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let res =
            sqlx::query("UPDATE tasks SET status = 'failed', error = ?, ended_at = ? WHERE id = ?")
                .bind(error)
                .bind(&now)
                .bind(id.to_string())
                .execute(&self.pool)
                .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("task {id}")));
        }
        Ok(())
    }

    pub async fn set_progress(&self, id: Uuid, progress: f32) -> Result<(), StorageError> {
        let res = sqlx::query("UPDATE tasks SET progress = ? WHERE id = ?")
            .bind(progress as f64)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("task {id}")));
        }
        Ok(())
    }

    pub async fn list_for_session(&self, session_id: Uuid) -> Result<Vec<Task>, StorageError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, session_id, status, progress, started_at, ended_at, error \
             FROM tasks WHERE session_id = ? ORDER BY started_at ASC, id ASC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Task::try_from).collect()
    }

    pub async fn get(&self, id: Uuid) -> Result<Task, StorageError> {
        let row = sqlx::query_as::<_, TaskRow>(
            "SELECT id, session_id, status, progress, started_at, ended_at, error \
             FROM tasks WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => r.try_into(),
            None => Err(StorageError::NotFound(format!("task {id}"))),
        }
    }
}

#[derive(Clone)]
pub struct ArtifactRepo {
    pool: SqlitePool,
}

impl ArtifactRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create an artifact with an auto-generated UUID.
    pub async fn create(
        &self,
        session_id: Uuid,
        kind: ArtifactKind,
        path: &str,
        title: Option<String>,
    ) -> Result<Artifact, StorageError> {
        self.create_with_id(Uuid::new_v4(), session_id, kind, path, title)
            .await
    }

    /// Create an artifact with an explicit ID (used when the agent runtime
    /// generates the artifact and emits `StreamEvent::Artifact` with a
    /// pre-assigned UUID so the path and metadata stay in sync).
    pub async fn create_with_id(
        &self,
        id: Uuid,
        session_id: Uuid,
        kind: ArtifactKind,
        path: &str,
        title: Option<String>,
    ) -> Result<Artifact, StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let kind_token = match kind {
            ArtifactKind::Html => "html",
            ArtifactKind::Markdown => "markdown",
            ArtifactKind::PythonText => "python_text",
            ArtifactKind::ImagePng => "image_png",
            ArtifactKind::PlotlyHtml => "plotly_html",
        };
        let row = sqlx::query_as::<_, ArtifactRow>(
            "INSERT INTO artifacts (id, session_id, kind, path, title, created_at) \
             VALUES (?, ?, ?, ?, ?, ?) \
             RETURNING id, session_id, kind, path, title, created_at",
        )
        .bind(id.to_string())
        .bind(session_id.to_string())
        .bind(kind_token)
        .bind(path)
        .bind(title.as_deref())
        .bind(&now)
        .fetch_one(&self.pool)
        .await?;
        row.try_into()
    }

    pub async fn list_for_session(&self, session_id: Uuid) -> Result<Vec<Artifact>, StorageError> {
        let rows = sqlx::query_as::<_, ArtifactRow>(
            "SELECT id, session_id, kind, path, title, created_at \
             FROM artifacts WHERE session_id = ? ORDER BY created_at ASC",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Artifact::try_from).collect()
    }

    pub async fn get(&self, id: Uuid) -> Result<Artifact, StorageError> {
        let row = sqlx::query_as::<_, ArtifactRow>(
            "SELECT id, session_id, kind, path, title, created_at FROM artifacts WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => r.try_into(),
            None => Err(StorageError::NotFound(format!("artifact {id}"))),
        }
    }
}

#[derive(Clone)]
pub struct ProviderRepo {
    pool: SqlitePool,
}

impl ProviderRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Provider>, StorageError> {
        let rows = sqlx::query_as::<_, ProviderRow>(
            "SELECT id, kind, engine, endpoint, model, is_active, status, config, created_at, updated_at \
             FROM providers ORDER BY is_active DESC, created_at ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Provider::try_from).collect()
    }

    /// Return the currently active provider (is_active = 1), if any.
    pub async fn active(&self) -> Result<Option<Provider>, StorageError> {
        let row = sqlx::query_as::<_, ProviderRow>(
            "SELECT id, kind, engine, endpoint, model, is_active, status, config, created_at, updated_at \
             FROM providers WHERE is_active = 1 ORDER BY created_at ASC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(r.try_into()?)),
            None => Ok(None),
        }
    }

    /// Rewrite a provider's endpoint in place (used to repair values written by
    /// an older build). Matching `upsert_local` on the new endpoint would insert
    /// a duplicate instead of fixing the existing row.
    pub async fn set_endpoint(&self, id: Uuid, endpoint: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE providers SET endpoint = ?, updated_at = ? WHERE id = ?")
            .bind(endpoint)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Could this name plausibly be loaded by a text-generation server?
    ///
    /// A speech or image checkpoint stored as the chat model makes
    /// llama-server fail to start, and the failure surfaces far away — the
    /// daemon quietly answers with its stub agent instead. That is how a TTS
    /// repo URL ended up as the active chat model with no visible error, so
    /// the check lives at the write, not the read.
    fn is_plausible_chat_model(name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        // A URL is never a local weight path.
        if n.starts_with("http://") || n.starts_with("https://") {
            return false;
        }
        // Formats a text-generation server cannot load. A Piper voice is named
        // after a locale, not its purpose, so only the extension gives it away.
        const NOT_LOADABLE: &[&str] = &[".onnx", ".pt", ".pth", ".bin", ".ckpt"];
        if NOT_LOADABLE.iter().any(|e| n.ends_with(e)) {
            return false;
        }
        const NOT_CHAT: &[&str] = &[
            "-tts",
            "_tts",
            "tts-",
            "xtts",
            "piper",
            "kokoro",
            "parler",
            "bark",
            "musicgen",
            "audioldm",
            "whisper",
            "stable-diffusion",
            "sd_xl",
            "sdxl",
            "z_image",
            "z-image",
            "flux",
            "clipseg",
            "segformer",
            "vae",
            "clip_l",
            "t5xxl",
        ];
        !NOT_CHAT.iter().any(|p| n.contains(p))
    }

    pub async fn upsert_local(
        &self,
        engine: ProviderEngine,
        endpoint: &str,
        model: Option<String>,
    ) -> Result<Provider, StorageError> {
        if let Some(m) = model.as_deref() {
            if !m.is_empty() && !Self::is_plausible_chat_model(m) {
                return Err(StorageError::Conflict(format!(
                    "« {m} » n'est pas un modèle de conversation : il ne peut pas être \
                     chargé par le moteur de texte. Choisissez un modèle de chat."
                )));
            }
        }
        let engine_token = match engine {
            ProviderEngine::Ollama => "ollama",
            ProviderEngine::LlamaCpp => "llama_cpp",
            ProviderEngine::Lmstudio => "lmstudio",
            ProviderEngine::Vllm => "vllm",
            ProviderEngine::OpenAiCompat => "open_ai_compat",
        };
        // Upsert by (kind='local', endpoint): if a local provider with the
        // same endpoint exists, activate it and update the model; otherwise
        // insert a new one as active. We also deactivate other providers so
        // there is at most one active.
        let mut tx = self.pool.begin().await?;
        // Deactivate all existing providers (only one active at a time).
        sqlx::query("UPDATE providers SET is_active = 0")
            .execute(&mut *tx)
            .await?;
        let existing = sqlx::query_as::<_, ProviderRow>(
            "SELECT id, kind, engine, endpoint, model, is_active, status, config, created_at, updated_at \
             FROM providers WHERE kind = 'local' AND endpoint = ? LIMIT 1",
        )
        .bind(endpoint)
        .fetch_optional(&mut *tx)
        .await?;
        let row = if let Some(r) = existing {
            // Update model + activate.
            sqlx::query(
                "UPDATE providers SET engine = ?, model = ?, is_active = 1, updated_at = ? \
                 WHERE id = ?",
            )
            .bind(engine_token)
            .bind(model.as_deref())
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(&r.id)
            .execute(&mut *tx)
            .await?;
            // Re-read the updated row.
            sqlx::query_as::<_, ProviderRow>(
                "SELECT id, kind, engine, endpoint, model, is_active, status, config, created_at, updated_at \
                 FROM providers WHERE id = ?",
            )
            .bind(&r.id)
            .fetch_one(&mut *tx)
            .await?
        } else {
            let id = Uuid::new_v4();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query_as::<_, ProviderRow>(
                "INSERT INTO providers (id, kind, engine, endpoint, model, is_active, status, config, created_at, updated_at) \
                 VALUES (?, 'local', ?, ?, ?, 1, 'unknown', NULL, ?, ?) \
                 RETURNING id, kind, engine, endpoint, model, is_active, status, config, created_at, updated_at",
            )
            .bind(id.to_string())
            .bind(engine_token)
            .bind(endpoint)
            .bind(model.as_deref())
            .bind(&now)
            .bind(&now)
            .fetch_one(&mut *tx)
            .await?
        };
        tx.commit().await?;
        row.try_into()
    }

    /// Mark a provider as active, deactivating all others (single-active rule).
    pub async fn set_active(&self, id: Uuid) -> Result<Provider, StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE providers SET is_active = 0")
            .execute(&mut *tx)
            .await?;
        let res = sqlx::query("UPDATE providers SET is_active = 1 WHERE id = ?")
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        if res.rows_affected() == 0 {
            tx.rollback().await.ok();
            return Err(StorageError::NotFound(format!("provider {id}")));
        }
        let row = sqlx::query_as::<_, ProviderRow>(
            "SELECT id, kind, engine, endpoint, model, is_active, status, config, created_at, updated_at \
             FROM providers WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        row.try_into()
    }

    /// Update the health status of a provider by ID (called by healthcheck loops).
    pub async fn set_status(&self, id: Uuid, status: ProviderStatus) -> Result<(), StorageError> {
        let token = match status {
            ProviderStatus::Unknown => "unknown",
            ProviderStatus::Healthy => "healthy",
            ProviderStatus::Unhealthy => "unhealthy",
            ProviderStatus::Starting => "starting",
        };
        let res = sqlx::query("UPDATE providers SET status = ?, updated_at = ? WHERE id = ?")
            .bind(token)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }

    /// Update the health status of all local providers with the given engine.
    /// Called by the provider supervisor's healthcheck loop, which tracks
    /// engines (not individual provider UUIDs).
    pub async fn set_status_by_engine(
        &self,
        engine: ProviderEngine,
        status: ProviderStatus,
    ) -> Result<(), StorageError> {
        let engine_token = match engine {
            ProviderEngine::Ollama => "ollama",
            ProviderEngine::LlamaCpp => "llama_cpp",
            ProviderEngine::Lmstudio => "lmstudio",
            ProviderEngine::Vllm => "vllm",
            ProviderEngine::OpenAiCompat => "open_ai_compat",
        };
        let status_token = match status {
            ProviderStatus::Unknown => "unknown",
            ProviderStatus::Healthy => "healthy",
            ProviderStatus::Unhealthy => "unhealthy",
            ProviderStatus::Starting => "starting",
        };
        sqlx::query(
            "UPDATE providers SET status = ?, updated_at = ? WHERE engine = ? AND kind = 'local'",
        )
        .bind(status_token)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(engine_token)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Persist arbitrary JSON config for a provider (e.g. model sampling params).
    pub async fn set_config(
        &self,
        id: Uuid,
        config: serde_json::Value,
    ) -> Result<(), StorageError> {
        let config_str =
            serde_json::to_string(&config).map_err(|e| StorageError::Decode(e.to_string()))?;
        let res = sqlx::query("UPDATE providers SET config = ?, updated_at = ? WHERE id = ?")
            .bind(config_str)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("provider {id}")));
        }
        Ok(())
    }
}

// ============================================================================
// SSH servers (feature gate: `ssh-connector`)
// ============================================================================

#[cfg(feature = "ssh-connector")]
const SSH_COLS: &str = "id, name, description, host, port, username, auth_method, secret_ref, \
    key_path, jump_json, host_key_algo, host_key_sha256, host_key_verified, ai_access, \
    capabilities, scope, status, enabled, last_connected_at, created_at, updated_at";

#[cfg(feature = "ssh-connector")]
/// Fields needed to insert a new SSH server (secrets are keychain refs only).
pub struct NewSshServer {
    pub name: String,
    pub description: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub secret_ref: Option<String>,
    pub key_path: Option<String>,
    pub jump: Option<SshJump>,
    pub host_key_algo: Option<String>,
    pub host_key_sha256: Option<String>,
    pub host_key_verified: bool,
    pub capabilities: Option<serde_json::Value>,
    pub scope: ExtensionScope,
}

#[cfg(feature = "ssh-connector")]
/// Editable metadata for `update` (each `Some` field is applied).
#[derive(Default)]
pub struct SshServerPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub key_path: Option<String>,
}

#[cfg(feature = "ssh-connector")]
#[derive(Clone)]
pub struct SshServerRepo {
    pool: SqlitePool,
}

#[cfg(feature = "ssh-connector")]
impl SshServerRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<SshServer>, StorageError> {
        let rows = sqlx::query_as::<_, SshServerRow>(&format!(
            "SELECT {SSH_COLS} FROM ssh_servers ORDER BY name ASC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SshServer::try_from).collect()
    }

    pub async fn list_for_scope(
        &self,
        scope: ExtensionScope,
    ) -> Result<Vec<SshServer>, StorageError> {
        let rows = sqlx::query_as::<_, SshServerRow>(&format!(
            "SELECT {SSH_COLS} FROM ssh_servers WHERE scope = ? ORDER BY name ASC"
        ))
        .bind(scope_token(scope))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SshServer::try_from).collect()
    }

    pub async fn get(&self, id: Uuid) -> Result<SshServer, StorageError> {
        let row = sqlx::query_as::<_, SshServerRow>(&format!(
            "SELECT {SSH_COLS} FROM ssh_servers WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("ssh_server {id}")))?;
        SshServer::try_from(row)
    }

    /// Fetch the keychain reference for a server (used by the connect path to
    /// resolve the secret). Never returned across the serde boundary.
    pub async fn secret_ref(&self, id: Uuid) -> Result<Option<String>, StorageError> {
        let r: Option<(Option<String>,)> =
            sqlx::query_as("SELECT secret_ref FROM ssh_servers WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        Ok(r.and_then(|t| t.0))
    }

    pub async fn create(&self, new: NewSshServer) -> Result<SshServer, StorageError> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        let jump_json = match &new.jump {
            Some(j) => Some(
                serde_json::to_string(j).map_err(|e| StorageError::Decode(format!("jump: {e}")))?,
            ),
            None => None,
        };
        let caps_json = match &new.capabilities {
            Some(c) => Some(
                serde_json::to_string(c)
                    .map_err(|e| StorageError::Decode(format!("capabilities: {e}")))?,
            ),
            None => None,
        };
        let row = sqlx::query_as::<_, SshServerRow>(&format!(
            "INSERT INTO ssh_servers (id, name, description, host, port, username, auth_method, \
             secret_ref, key_path, jump_json, host_key_algo, host_key_sha256, host_key_verified, \
             ai_access, capabilities, scope, status, enabled, last_connected_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'none', ?, ?, 'ok', 1, NULL, ?, ?) \
             RETURNING {SSH_COLS}"
        ))
        .bind(id.to_string())
        .bind(&new.name)
        .bind(&new.description)
        .bind(&new.host)
        .bind(new.port as i64)
        .bind(&new.username)
        .bind(auth_method_token(new.auth_method))
        .bind(&new.secret_ref)
        .bind(&new.key_path)
        .bind(&jump_json)
        .bind(&new.host_key_algo)
        .bind(&new.host_key_sha256)
        .bind(new.host_key_verified as i64)
        .bind(&caps_json)
        .bind(scope_token(new.scope))
        .bind(&now)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .map_err(map_unique)?;
        SshServer::try_from(row)
    }

    pub async fn update(&self, id: Uuid, patch: SshServerPatch) -> Result<SshServer, StorageError> {
        let cur = self.get(id).await?;
        let name = patch.name.unwrap_or(cur.name);
        let description = patch.description.unwrap_or(cur.description);
        let host = patch.host.unwrap_or(cur.host);
        let port = patch.port.unwrap_or(cur.port);
        let username = patch.username.unwrap_or(cur.username);
        let key_path = patch.key_path.or(cur.key_path);
        let row = sqlx::query_as::<_, SshServerRow>(&format!(
            "UPDATE ssh_servers SET name = ?, description = ?, host = ?, port = ?, username = ?, \
             key_path = ?, updated_at = ? WHERE id = ? RETURNING {SSH_COLS}"
        ))
        .bind(&name)
        .bind(&description)
        .bind(&host)
        .bind(port as i64)
        .bind(&username)
        .bind(&key_path)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(map_unique)?;
        SshServer::try_from(row)
    }

    /// The AI-write path: replace a server's free-text description.
    pub async fn update_description(
        &self,
        id: Uuid,
        text: &str,
    ) -> Result<SshServer, StorageError> {
        self.simple_set(id, "description = ?", text).await
    }

    pub async fn set_ai_access(
        &self,
        id: Uuid,
        level: SshAiAccess,
    ) -> Result<SshServer, StorageError> {
        self.simple_set(id, "ai_access = ?", ai_access_token(level))
            .await
    }

    pub async fn set_status(&self, id: Uuid, status: SshStatus) -> Result<(), StorageError> {
        sqlx::query("UPDATE ssh_servers SET status = ?, updated_at = ? WHERE id = ?")
            .bind(ssh_status_token(status))
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Point a server at its keychain secret (or clear it). The value is a
    /// keychain reference, never the secret itself.
    pub async fn set_secret_ref(
        &self,
        id: Uuid,
        secret_ref: Option<String>,
    ) -> Result<(), StorageError> {
        sqlx::query("UPDATE ssh_servers SET secret_ref = ?, updated_at = ? WHERE id = ?")
            .bind(secret_ref)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_capabilities(
        &self,
        id: Uuid,
        caps: &serde_json::Value,
    ) -> Result<(), StorageError> {
        let json =
            serde_json::to_string(caps).map_err(|e| StorageError::Decode(format!("caps: {e}")))?;
        sqlx::query("UPDATE ssh_servers SET capabilities = ?, updated_at = ? WHERE id = ?")
            .bind(json)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_last_connected(&self, id: Uuid) -> Result<(), StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE ssh_servers SET last_connected_at = ?, status = 'ok', updated_at = ? WHERE id = ?",
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<(), StorageError> {
        sqlx::query("UPDATE ssh_servers SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled as i64)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Delete a server, returning its freed `secret_ref` (if any) so the caller
    /// can remove it from the keychain. Storage never touches the keychain.
    pub async fn delete(&self, id: Uuid) -> Result<Option<String>, StorageError> {
        let secret = self.secret_ref(id).await?;
        sqlx::query("DELETE FROM ssh_servers WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(secret)
    }

    async fn simple_set(
        &self,
        id: Uuid,
        set_clause: &str,
        value: impl Into<String>,
    ) -> Result<SshServer, StorageError> {
        let row = sqlx::query_as::<_, SshServerRow>(&format!(
            "UPDATE ssh_servers SET {set_clause}, updated_at = ? WHERE id = ? RETURNING {SSH_COLS}"
        ))
        .bind(value.into())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("ssh_server {id}")))?;
        SshServer::try_from(row)
    }
}

#[cfg(feature = "ssh-connector")]
/// Map a UNIQUE-constraint violation to a friendly Conflict error.
fn map_unique(e: sqlx::Error) -> StorageError {
    if let sqlx::Error::Database(db) = &e {
        if db.message().contains("UNIQUE") {
            return StorageError::Conflict("an SSH server with that name already exists".into());
        }
    }
    StorageError::from(e)
}

// ============================================================================
// Convenience bundle: one struct holding all repos over a shared pool.
// ============================================================================

// ============================================================================
// Extensions
// ============================================================================

/// A persisted extension: identity plus the state only the user can set
/// (scope, enabled, granted permissions). Descriptive metadata — description,
/// author, which components it ships — is deliberately *not* stored: it is
/// re-read from the manifest at `manifest_path`, which is the source of truth
/// and therefore cannot drift out of sync with the row.
#[derive(Debug, Clone)]
pub struct ExtensionRecord {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub kind: ExtensionKind,
    pub scope: ExtensionScope,
    pub ecosystem: ExtensionEcosystem,
    /// `owner/repo`, a git URL, or a local path — whatever the user installed from.
    pub source: Option<String>,
    pub manifest_path: String,
    pub enabled: bool,
    pub requested: Vec<Permission>,
    pub granted: Vec<Permission>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// What `upsert` needs to record an install.
#[derive(Debug, Clone)]
pub struct NewExtension {
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub kind: ExtensionKind,
    pub scope: ExtensionScope,
    pub ecosystem: ExtensionEcosystem,
    pub source: Option<String>,
    pub manifest_path: String,
    /// Permissions the manifest asks for. Stored as requested-but-not-granted;
    /// the user grants them separately via `set_granted`.
    pub requested: Vec<Permission>,
}

const EXT_COLS: &str = "id, name, version, api_version, kind, scope, ecosystem, source, \
                        manifest_path, enabled, created_at, updated_at";

#[derive(Clone)]
pub struct ExtensionRepo {
    pool: SqlitePool,
}

impl ExtensionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<ExtensionRecord>, StorageError> {
        let rows = sqlx::query_as::<_, ExtensionRow>(&format!(
            "SELECT {EXT_COLS} FROM extensions ORDER BY name ASC"
        ))
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            out.push(self.hydrate(r).await?);
        }
        Ok(out)
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<ExtensionRecord>, StorageError> {
        let row = sqlx::query_as::<_, ExtensionRow>(&format!(
            "SELECT {EXT_COLS} FROM extensions WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(self.hydrate(r).await?)),
            None => Ok(None),
        }
    }

    pub async fn get_by_name(
        &self,
        name: &str,
        scope: ExtensionScope,
    ) -> Result<Option<ExtensionRecord>, StorageError> {
        let row = sqlx::query_as::<_, ExtensionRow>(&format!(
            "SELECT {EXT_COLS} FROM extensions WHERE name = ? AND scope = ?"
        ))
        .bind(name)
        .bind(scope_token(scope))
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some(self.hydrate(r).await?)),
            None => Ok(None),
        }
    }

    /// Insert or update by `(name, scope)` — the table's unique key. Upsert
    /// rather than insert so re-installing over an existing copy (an update)
    /// keeps the same id, and with it the permissions the user already granted.
    pub async fn upsert(&self, new: NewExtension) -> Result<ExtensionRecord, StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;

        let existing = sqlx::query_as::<_, ExtensionRow>(&format!(
            "SELECT {EXT_COLS} FROM extensions WHERE name = ? AND scope = ?"
        ))
        .bind(&new.name)
        .bind(scope_token(new.scope))
        .fetch_optional(&mut *tx)
        .await?;

        let id = match &existing {
            Some(r) => uid(&r.id)?,
            None => Uuid::new_v4(),
        };

        if existing.is_some() {
            sqlx::query(
                "UPDATE extensions SET version = ?, api_version = ?, kind = ?, ecosystem = ?, \
                 source = ?, manifest_path = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&new.version)
            .bind(&new.api_version)
            .bind(extension_kind_token(&new.kind))
            .bind(new.ecosystem.as_str())
            .bind(new.source.as_deref())
            .bind(&new.manifest_path)
            .bind(&now)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO extensions \
                 (id, name, version, api_version, kind, scope, ecosystem, source, manifest_path, \
                  enabled, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)",
            )
            .bind(id.to_string())
            .bind(&new.name)
            .bind(&new.version)
            .bind(&new.api_version)
            .bind(extension_kind_token(&new.kind))
            .bind(scope_token(new.scope))
            .bind(new.ecosystem.as_str())
            .bind(new.source.as_deref())
            .bind(&new.manifest_path)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        // Re-declare the requested set. Rows the manifest no longer asks for
        // are dropped; rows it still asks for keep their `granted` value, so an
        // update does not silently re-prompt for permissions already approved.
        let requested: Vec<&'static str> = new.requested.iter().map(permission_token).collect();
        if requested.is_empty() {
            sqlx::query("DELETE FROM extension_permissions WHERE extension_id = ?")
                .bind(id.to_string())
                .execute(&mut *tx)
                .await?;
        } else {
            let placeholders = vec!["?"; requested.len()].join(", ");
            let prune_sql = format!(
                "DELETE FROM extension_permissions \
                 WHERE extension_id = ? AND permission NOT IN ({placeholders})"
            );
            let mut del = sqlx::query(&prune_sql).bind(id.to_string());
            for p in &requested {
                del = del.bind(*p);
            }
            del.execute(&mut *tx).await?;

            for p in &requested {
                sqlx::query(
                    "INSERT INTO extension_permissions (id, extension_id, permission, requested, granted) \
                     VALUES (?, ?, ?, 1, 0) \
                     ON CONFLICT(extension_id, permission) DO UPDATE SET requested = 1",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(id.to_string())
                .bind(*p)
                .execute(&mut *tx)
                .await?;
            }
        }

        // Install history — one row per install/update attempt.
        sqlx::query(
            "INSERT INTO extension_installs (id, extension_id, installed_by, scope, status, log, created_at, completed_at) \
             VALUES (?, ?, NULL, ?, 'installed', ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(id.to_string())
        .bind(scope_token(new.scope))
        .bind(new.source.as_deref())
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        self.get(id)
            .await?
            .ok_or_else(|| StorageError::Decode("extension vanished after upsert".into()))
    }

    pub async fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<(), StorageError> {
        sqlx::query("UPDATE extensions SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(i64::from(enabled))
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Replace the granted set. Anything not listed is revoked, so the modal
    /// can send the user's full decision in one call.
    pub async fn set_granted(&self, id: Uuid, granted: &[Permission]) -> Result<(), StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE extension_permissions SET granted = 0, decided_at = ? WHERE extension_id = ?",
        )
        .bind(&now)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
        for p in granted {
            sqlx::query(
                "INSERT INTO extension_permissions (id, extension_id, permission, requested, granted, decided_at) \
                 VALUES (?, ?, ?, 1, 1, ?) \
                 ON CONFLICT(extension_id, permission) DO UPDATE SET granted = 1, decided_at = excluded.decided_at",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(id.to_string())
            .bind(permission_token(p))
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Delete the row. `ON DELETE CASCADE` takes the permission and install
    /// history rows with it; removing files on disk is the caller's job.
    pub async fn delete(&self, id: Uuid) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM extensions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn hydrate(&self, r: ExtensionRow) -> Result<ExtensionRecord, StorageError> {
        let id = uid(&r.id)?;
        let perms = sqlx::query_as::<_, ExtensionPermissionRow>(
            "SELECT permission, granted FROM extension_permissions \
             WHERE extension_id = ? ORDER BY permission ASC",
        )
        .bind(&r.id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ExtensionRecord {
            id,
            name: r.name,
            version: r.version,
            api_version: r.api_version,
            kind: ExtensionKind::from_token(&r.kind),
            scope: ExtensionScope::from_token(&r.scope),
            ecosystem: ExtensionEcosystem::from_token(&r.ecosystem),
            source: r.source,
            manifest_path: r.manifest_path,
            enabled: bool_from_i64(r.enabled),
            requested: perms
                .iter()
                .map(|p| Permission::from_token(&p.permission))
                .collect(),
            granted: perms
                .iter()
                .filter(|p| bool_from_i64(p.granted))
                .map(|p| Permission::from_token(&p.permission))
                .collect(),
            created_at: dt(&r.created_at)?,
            updated_at: dt(&r.updated_at)?,
        })
    }
}

/// All repositories sharing one `SqlitePool`. Callers (daemon, remote-server,
/// desktop in-process core) typically hold one of these.
#[derive(Clone)]
pub struct Storage {
    pub projects: ProjectRepo,
    pub sessions: SessionRepo,
    pub messages: MessageRepo,
    pub tasks: TaskRepo,
    pub artifacts: ArtifactRepo,
    pub providers: ProviderRepo,
    pub extensions: ExtensionRepo,
    #[cfg(feature = "ssh-connector")]
    pub ssh_servers: SshServerRepo,
}

impl Storage {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            projects: ProjectRepo::new(pool.clone()),
            sessions: SessionRepo::new(pool.clone()),
            messages: MessageRepo::new(pool.clone()),
            tasks: TaskRepo::new(pool.clone()),
            artifacts: ArtifactRepo::new(pool.clone()),
            providers: ProviderRepo::new(pool.clone()),
            extensions: ExtensionRepo::new(pool.clone()),
            #[cfg(feature = "ssh-connector")]
            ssh_servers: SshServerRepo::new(pool),
        }
    }
}

#[cfg(all(test, feature = "ssh-connector"))]
mod ssh_tests {
    use super::*;
    use locaryn_shared_types::{ExtensionScope, SshAiAccess, SshAuthMethod};

    fn new_server(name: &str) -> NewSshServer {
        NewSshServer {
            name: name.into(),
            description: "seeded".into(),
            host: "10.0.0.4".into(),
            port: 22,
            username: "deploy".into(),
            auth_method: SshAuthMethod::Agent,
            secret_ref: None,
            key_path: None,
            jump: None,
            host_key_algo: Some("ssh-ed25519".into()),
            host_key_sha256: Some("abc123".into()),
            host_key_verified: true,
            capabilities: None,
            scope: ExtensionScope::User,
        }
    }

    #[tokio::test]
    async fn create_list_update_delete() {
        let pool = crate::open_in_memory().await.unwrap();
        let repo = SshServerRepo::new(pool);

        let s = repo.create(new_server("web-prod")).await.unwrap();
        assert_eq!(s.name, "web-prod");
        assert_eq!(s.ai_access, SshAiAccess::None);
        assert!(s.host_key_verified);

        let all = repo.list().await.unwrap();
        assert_eq!(all.len(), 1);

        let updated = repo
            .update_description(s.id, "Ubuntu 22.04, read+write, no sudo")
            .await
            .unwrap();
        assert_eq!(updated.description, "Ubuntu 22.04, read+write, no sudo");

        let widened = repo
            .set_ai_access(s.id, SshAiAccess::ReadOnly)
            .await
            .unwrap();
        assert_eq!(widened.ai_access, SshAiAccess::ReadOnly);

        // UNIQUE(name, scope) enforced.
        assert!(repo.create(new_server("web-prod")).await.is_err());

        let freed = repo.delete(s.id).await.unwrap();
        assert_eq!(freed, None); // agent auth → no secret_ref
        assert_eq!(repo.list().await.unwrap().len(), 0);
    }
}

#[cfg(test)]
mod extension_tests {
    use super::*;

    fn new_ext(name: &str) -> NewExtension {
        NewExtension {
            name: name.into(),
            version: "1.0.0".into(),
            api_version: "0.1".into(),
            kind: ExtensionKind::Plugin,
            scope: ExtensionScope::User,
            ecosystem: ExtensionEcosystem::ClaudeCode,
            source: Some(format!("github:acme/{name}")),
            manifest_path: format!("/plugins/{name}/plugin.json"),
            requested: vec![Permission::Mcp, Permission::FilesRead],
        }
    }

    #[tokio::test]
    async fn install_enable_grant_remove() {
        let pool = crate::open_in_memory().await.unwrap();
        let repo = ExtensionRepo::new(pool);

        let e = repo.upsert(new_ext("code-review")).await.unwrap();
        assert_eq!(e.name, "code-review");
        assert_eq!(e.ecosystem, ExtensionEcosystem::ClaudeCode);
        // An install never enables itself: permissions come first.
        assert!(!e.enabled);
        assert_eq!(e.requested.len(), 2);
        assert!(e.granted.is_empty());

        repo.set_granted(e.id, &[Permission::Mcp]).await.unwrap();
        let after = repo.get(e.id).await.unwrap().unwrap();
        assert_eq!(after.granted, vec![Permission::Mcp]);

        repo.set_enabled(e.id, true).await.unwrap();
        assert!(repo.get(e.id).await.unwrap().unwrap().enabled);

        assert_eq!(repo.list().await.unwrap().len(), 1);
        repo.delete(e.id).await.unwrap();
        assert!(repo.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reinstalling_keeps_the_permissions_already_granted() {
        let pool = crate::open_in_memory().await.unwrap();
        let repo = ExtensionRepo::new(pool);

        let e = repo.upsert(new_ext("tool")).await.unwrap();
        repo.set_granted(e.id, &[Permission::Mcp]).await.unwrap();
        repo.set_enabled(e.id, true).await.unwrap();

        // An update: same name and scope, newer version.
        let mut next = new_ext("tool");
        next.version = "2.0.0".into();
        let updated = repo.upsert(next).await.unwrap();

        // Same row, so the user is not asked to approve the same thing twice.
        assert_eq!(updated.id, e.id);
        assert_eq!(updated.version, "2.0.0");
        assert_eq!(updated.granted, vec![Permission::Mcp]);
        assert!(updated.enabled);
        assert_eq!(repo.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_permission_the_manifest_dropped_is_forgotten() {
        let pool = crate::open_in_memory().await.unwrap();
        let repo = ExtensionRepo::new(pool);

        let e = repo.upsert(new_ext("shrinking")).await.unwrap();
        repo.set_granted(e.id, &[Permission::Mcp, Permission::FilesRead])
            .await
            .unwrap();

        let mut next = new_ext("shrinking");
        next.requested = vec![Permission::FilesRead];
        let updated = repo.upsert(next).await.unwrap();

        assert_eq!(updated.requested, vec![Permission::FilesRead]);
        assert_eq!(updated.granted, vec![Permission::FilesRead]);
    }

    #[tokio::test]
    async fn the_same_name_may_live_in_two_scopes() {
        let pool = crate::open_in_memory().await.unwrap();
        let repo = ExtensionRepo::new(pool);

        repo.upsert(new_ext("shared")).await.unwrap();
        let mut ws = new_ext("shared");
        ws.scope = ExtensionScope::Workspace;
        repo.upsert(ws).await.unwrap();

        assert_eq!(repo.list().await.unwrap().len(), 2);
        let user = repo
            .get_by_name("shared", ExtensionScope::User)
            .await
            .unwrap()
            .unwrap();
        let workspace = repo
            .get_by_name("shared", ExtensionScope::Workspace)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(user.id, workspace.id);
    }
}

#[cfg(test)]
mod chat_model_guard {
    use super::ProviderRepo;

    /// Built from the value that actually broke the daemon: a TTS repo URL
    /// stored as the chat model, which made llama-server fail to start and the
    /// agent silently degrade to its stub.
    #[test]
    fn speech_image_and_url_models_are_refused_as_chat_models() {
        for bad in [
            "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-Base",
            "Qwen__Qwen3-TTS-12Hz-0.6B-Base",
            "fr_FR-siwis-medium.onnx",
            "xtts_v2",
            "hexgrad__Kokoro-82M",
            "z_image_turbo-Q8_0.gguf",
            "sd_xl_turbo_1.0.q8_0.gguf",
            "flux1-schnell-Q4_0.gguf",
            "Systran--faster-whisper-small",
        ] {
            assert!(
                !ProviderRepo::is_plausible_chat_model(bad),
                "{bad} devrait être refusé comme modèle de chat"
            );
        }
    }

    #[test]
    fn real_chat_models_are_accepted() {
        for good in [
            "qwen2.5:3b",
            "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
            "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
            "mistral-7b-instruct-v0.2.Q5_K_M.gguf",
            "phi-3-mini-4k-instruct.gguf",
        ] {
            assert!(
                ProviderRepo::is_plausible_chat_model(good),
                "{good} devrait être accepté"
            );
        }
    }
}
