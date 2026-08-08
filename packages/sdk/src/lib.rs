//! Locaryn client SDK — talks to the local daemon or the remote-server
//! over HTTP/1.1 + SSE. Used by the CLI and (optionally) the desktop app
//! when it prefers the daemon over the in-process core.

use futures::TryStreamExt as _;
use locaryn_events::{SseError, StreamEvent};
use locaryn_shared_types::{
    ApiError, ConnectionMode, Health, Message, Project, Provider, Session, Task, TaskStatus,
};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A Locaryn client. The same client works against the loopback daemon
/// (`http://127.0.0.1:7474`) or a remote server (`https://host:7473`).
#[derive(Debug, Clone)]
pub struct LocarynClient {
    base_url: String,
    token: Option<String>,
    http: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {code} {message}")]
    Api {
        code: String,
        message: String,
        status: u16,
    },
    #[error("invalid URL: {0}")]
    BadUrl(String),
    #[error("SSE stream ended unexpectedly")]
    StreamEnded,
    #[error("SSE stream error: {0}")]
    Sse(#[from] SseError),
}

impl LocarynClient {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Result<Self, SdkError> {
        let base_url = base_url.into();
        // Sanity check.
        if base_url.is_empty() {
            return Err(SdkError::BadUrl(base_url));
        }
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            http,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn add_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(t) = &self.token {
            req.bearer_auth(t)
        } else {
            req
        }
    }

    async fn decode_error(resp: reqwest::Response) -> SdkError {
        let status = resp.status().as_u16();
        match resp.json::<ApiError>().await {
            Ok(body) => SdkError::Api {
                code: body.error.code,
                message: body.error.message,
                status,
            },
            Err(_) => SdkError::Api {
                code: format!("http_{status}"),
                message: format!("HTTP {status}"),
                status,
            },
        }
    }

    // ---- MCP --------------------------------------------------------------

    /// Ask the running daemon to start a registered MCP server.
    ///
    /// Starting has to happen there: a server is a child process, and a CLI
    /// that spawned one would take it down on exit.
    pub async fn start_mcp(&self, name: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.post(self.url(&format!("/v1/mcp/servers/{name}/start"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn stop_mcp(&self, name: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.post(self.url(&format!("/v1/mcp/servers/{name}/stop"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Travel mode ------------------------------------------------------

    pub async fn travel_status(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self.add_auth(self.http.get(self.url("/v1/travel"))).send().await?;
        if resp.status().is_success() { Ok(resp.json().await?) } else { Err(Self::decode_error(resp).await) }
    }

    /// `provider` of `None` switches travel mode off.
    pub async fn set_travel(&self, provider: Option<&str>) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({ "provider": provider });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/travel")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() { Ok(resp.json().await?) } else { Err(Self::decode_error(resp).await) }
    }

    pub async fn travel_home(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self.add_auth(self.http.get(self.url("/v1/travel/home"))).send().await?;
        if resp.status().is_success() { Ok(resp.json().await?) } else { Err(Self::decode_error(resp).await) }
    }

    // ---- Health & info ----------------------------------------------------

    pub async fn health(&self) -> Result<Health, SdkError> {
        let resp = self.http.get(self.url("/health")).send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    /// Fetch server info (version, capabilities). Returns a flexible JSON
    /// value because the daemon and remote-server may expose different
    /// capability sets; use `health()` for the typed `Health` shape.
    pub async fn info(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/info")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Projects ---------------------------------------------------------

    pub async fn list_projects(&self) -> Result<Vec<Project>, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/projects")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn create_project(
        &self,
        path: &str,
        name: &str,
        trust: locaryn_shared_types::TrustLevel,
    ) -> Result<Project, SdkError> {
        let body = serde_json::json!({ "path": path, "name": name, "trust_level": trust });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/projects")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Sessions ---------------------------------------------------------

    pub async fn list_sessions(&self, project_id: &str) -> Result<Vec<Session>, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/projects/{project_id}/sessions"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn create_session(&self, project_id: &str) -> Result<Session, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/projects/{project_id}/sessions"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Session, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/sessions/{session_id}"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<Message>, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/sessions/{session_id}"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            let v: SessionDetail = resp.json().await?;
            Ok(v.messages)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    /// Send a user message and return a streaming event source.
    /// The stream yields `StreamEvent`s (tokens, tool_calls, artifacts, ...).
    /// `+ Send` is declared explicitly so a future refactor that swaps the
    /// concrete stream type is caught at the SDK boundary.
    pub async fn send_message(
        &self,
        session_id: &str,
        content: &str,
    ) -> Result<impl futures::Stream<Item = Result<StreamEvent, SdkError>> + Send, SdkError> {
        let body = serde_json::json!({ "content": content });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/sessions/{session_id}/messages")))
                    .json(&body),
            )
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::decode_error(resp).await);
        }
        // Convert the stream's SseError into SdkError via the `From<SseError>`
        // impl on SdkError (the `Sse` variant). `TryStreamExt::map_err` does
        // this without consuming the stream.
        Ok(locaryn_events::sse_stream(resp.bytes_stream()).map_err(SdkError::from))
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<Task, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/tasks/{task_id}/cancel"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn approve_tool_call(
        &self,
        task_id: &str,
        call_id: &str,
        decision: ApprovalDecision,
        scope: ApprovalScope,
    ) -> Result<Task, SdkError> {
        let body = serde_json::json!({
            "call_id": call_id,
            "decision": decision,
            "scope": scope,
        });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/tasks/{task_id}/approve")))
                    .json(&body),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Providers --------------------------------------------------------

    pub async fn list_providers(&self) -> Result<Vec<Provider>, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/providers")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn switch_provider(&self, mode: ConnectionMode) -> Result<Provider, SdkError> {
        let body = serde_json::json!({ "mode": mode });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/providers/active")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn start_local(
        &self,
        engine: locaryn_shared_types::ProviderEngine,
        model: Option<&str>,
    ) -> Result<Provider, SdkError> {
        let mut body = serde_json::json!({ "engine": engine });
        if let Some(m) = model {
            body["model"] = serde_json::Value::String(m.to_string());
        }
        let resp = self
            .add_auth(
                self.http
                    .post(self.url("/v1/providers/local/start"))
                    .json(&body),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }
}

// ============================================================================
// Helpers / sub-types
// ============================================================================

#[derive(Debug, Clone, serde::Deserialize)]
struct SessionDetail {
    #[serde(default)]
    messages: Vec<Message>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalScope {
    Once,
    Session,
    Project,
    Always,
}

/// Probe a remote server's health with a short timeout. Used by the `auto`
/// mode fallback logic.
pub async fn remote_healthy(base_url: &str, timeout: Duration) -> bool {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .unwrap_or_default();
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    client
        .get(url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Pick the active connection in `auto` mode: try remote first, fall back to local.
pub async fn resolve_auto(
    remote_url: Option<&str>,
    _local_url: &str,
    timeout: Duration,
) -> ConnectionMode {
    if let Some(remote) = remote_url {
        if remote_healthy(remote, timeout).await {
            return ConnectionMode::Remote;
        }
    }
    // Fallback to local. (V1 also healthchecks _local_url here; the skeleton
    // defaults to Local regardless, since a missing daemon can be auto-started.)
    ConnectionMode::Local
}

/// Helper to mark a task as completed/failed locally (for in-process core use).
pub fn terminal_status(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed
    )
}
