//! Locaryn client SDK — talks to the local daemon or the remote-server
//! over HTTP/1.1 + SSE. Used by the CLI and the desktop app when acting
//! in client mode against a remote server (e.g. DGX Spark supercomputer).

use futures::TryStreamExt as _;
use locaryn_events::{SseError, StreamEvent};
use locaryn_shared_types::{
    ApiError, ConnectionMode, Health, InstalledExtension, Message, Project, Provider, Session,
    Task, TaskStatus,
};
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Stored session credentials on disk (`session-token.json`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredSession {
    pub server_url: String,
    pub username: String,
    pub token: String,
}

/// A Locaryn client. The same client works against the loopback daemon
/// (`http://127.0.0.1:7474`) or a remote server (`https://host:7474`).
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
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl LocarynClient {
    /// Create a client with default timeout and standard TLS settings.
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Result<Self, SdkError> {
        let base_url = base_url.into();
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

    /// Create a client using an externally configured `reqwest::Client` (e.g. for custom mTLS or TLS pins).
    pub fn with_client(
        base_url: impl Into<String>,
        token: Option<String>,
        http: reqwest::Client,
    ) -> Result<Self, SdkError> {
        let base_url = base_url.into();
        if base_url.is_empty() {
            return Err(SdkError::BadUrl(base_url));
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            http,
        })
    }

    /// Path to the stored session token file (`<data_dir>/session-token.json`).
    pub fn token_file_path() -> PathBuf {
        locaryn_config::default_data_dir().join("session-token.json")
    }

    /// Load stored session token from disk if it exists.
    pub fn stored_session() -> Option<StoredSession> {
        let path = Self::token_file_path();
        if !path.is_file() {
            return None;
        }
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Build client from the stored session if available, falling back to config.
    pub fn from_stored_session_or_config() -> Result<Self, SdkError> {
        if let Some(stored) = Self::stored_session() {
            if !stored.server_url.trim().is_empty() {
                return Self::new(stored.server_url, Some(stored.token));
            }
        }
        let cfg = locaryn_config::load(None).unwrap_or_default();
        let base_url = cfg.connection.local_url;
        Self::new(base_url, None)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
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

    // ---- Health & Info ----------------------------------------------------

    pub async fn health(&self) -> Result<Health, SdkError> {
        let resp = self.http.get(self.url("/health")).send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

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

    pub async fn create_session_with_core(
        &self,
        project_id: &str,
        core_id: Option<&str>,
    ) -> Result<Session, SdkError> {
        let body = serde_json::json!({ "core_id": core_id });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/projects/{project_id}/sessions")))
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
        Ok(locaryn_events::sse_stream(resp.bytes_stream()).map_err(SdkError::from))
    }

    pub async fn cancel_session(&self, session_id: &str) -> Result<(), SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/sessions/{session_id}/cancel"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn rename_session(&self, session_id: &str, title: &str) -> Result<Session, SdkError> {
        let body = serde_json::json!({ "title": title });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/sessions/{session_id}/title")))
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

    pub async fn archive_session(
        &self,
        session_id: &str,
        archived: bool,
    ) -> Result<Session, SdkError> {
        let body = serde_json::json!({ "archived": archived });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/sessions/{session_id}/archive")))
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

    pub async fn move_session(
        &self,
        session_id: &str,
        project_id: &str,
    ) -> Result<Session, SdkError> {
        let body = serde_json::json!({ "project_id": project_id });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/sessions/{session_id}/project")))
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

    pub async fn suggest_project(&self, session_id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/sessions/{session_id}/suggest-project"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn merge_sessions(
        &self,
        session_id: &str,
        source_id: &str,
    ) -> Result<Session, SdkError> {
        let body = serde_json::json!({ "source_id": source_id });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/sessions/{session_id}/merge")))
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

    pub async fn list_archived_sessions(&self, project_id: &str) -> Result<Vec<Session>, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/projects/{project_id}/archived"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Tasks & Approvals ------------------------------------------------

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

    // ---- Providers & Supervisor -------------------------------------------

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

    pub async fn supervisor_status(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/supervisor/status")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn supervisor_start(
        &self,
        engine: locaryn_shared_types::ProviderEngine,
        model: Option<&str>,
    ) -> Result<serde_json::Value, SdkError> {
        let engine_name = format!("{engine:?}").to_lowercase();
        let mut body = serde_json::json!({ "engine": engine_name });
        if let Some(m) = model {
            body["model"] = serde_json::Value::String(m.to_string());
        }
        let resp = self
            .add_auth(self.http.post(self.url("/v1/supervisor/start")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn supervisor_stop(
        &self,
        engine: locaryn_shared_types::ProviderEngine,
    ) -> Result<serde_json::Value, SdkError> {
        let engine_name = format!("{engine:?}").to_lowercase();
        let body = serde_json::json!({ "engine": engine_name });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/supervisor/stop")).json(&body))
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
    ) -> Result<serde_json::Value, SdkError> {
        self.supervisor_start(engine, model).await
    }

    // ---- Media & Models ---------------------------------------------------

    pub async fn list_media_models(
        &self,
        kind: Option<&str>,
    ) -> Result<serde_json::Value, SdkError> {
        let path = match kind {
            Some(k) => format!("/v1/media/models?kind={k}"),
            None => "/v1/media/models".to_string(),
        };
        let resp = self.add_auth(self.http.get(self.url(&path))).send().await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn delete_model(&self, name: &str) -> Result<(), SdkError> {
        let resp = self
            .add_auth(self.http.delete(self.url(&format!("/v1/models/{name}"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn pull_model(
        &self,
        endpoint: &str,
        model: &str,
        heretic: Option<bool>,
        consent: Option<bool>,
        selection: Option<&serde_json::Value>,
        companions: Option<&serde_json::Value>,
    ) -> Result<impl futures::Stream<Item = Result<bytes::Bytes, SdkError>> + Send, SdkError> {
        let body = serde_json::json!({
            "endpoint": endpoint,
            "name": model,
            "heretic": heretic.unwrap_or(false),
            "consent": consent.unwrap_or(false),
            "selection": selection,
            "companions": companions,
        });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/models/pull")).json(&body))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::decode_error(resp).await);
        }
        Ok(resp.bytes_stream().map_err(SdkError::from))
    }

    pub async fn generate_audio(
        &self,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.post(self.url("/v1/media/audio")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn get_model_metrics(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/metrics/models")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Extensions & Capabilities ----------------------------------------

    pub async fn list_extensions(&self) -> Result<Vec<InstalledExtension>, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/extensions")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn install_extension(
        &self,
        source: &str,
        scope: &str,
    ) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({ "source": source, "scope": scope });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url("/v1/extensions/install"))
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

    pub async fn set_extension_enabled(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<serde_json::Value, SdkError> {
        let action = if enabled { "enable" } else { "disable" };
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/extensions/{name}/{action}"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn remove_extension(&self, name: &str) -> Result<(), SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .delete(self.url(&format!("/v1/extensions/{name}"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn reload_extensions(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.post(self.url("/v1/extensions/reload")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn get_extension_config(&self, name: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/extensions/{name}/config"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn set_extension_config(
        &self,
        name: &str,
        values: serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/extensions/{name}/config")))
                    .json(&values),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn get_extension_permissions(
        &self,
        name: &str,
    ) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/extensions/{name}/permissions"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn set_extension_permission(
        &self,
        name: &str,
        permission: &str,
        granted: bool,
    ) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({
            "permission": permission,
            "granted": granted,
        });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/extensions/{name}/permissions")))
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

    pub async fn list_capabilities(&self) -> Result<Vec<serde_json::Value>, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/capabilities")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Alternate Cores --------------------------------------------------

    pub async fn list_cores(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/cores")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn core_status(&self, id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url(&format!("/v1/cores/{id}"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn core_start(&self, id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.post(self.url(&format!("/v1/cores/{id}/start"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn core_stop(&self, id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.post(self.url(&format!("/v1/cores/{id}/stop"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn core_skills(&self, id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url(&format!("/v1/cores/{id}/skills"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn core_install_skill(
        &self,
        id: &str,
        slug: &str,
    ) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({ "slug": slug });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/cores/{id}/skills/install")))
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

    // ---- MCP Servers ------------------------------------------------------

    pub async fn list_mcp_servers(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/mcp/servers")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn register_mcp_server(
        &self,
        name: &str,
        transport: &str,
        target: &str,
        env: serde_json::Value,
        auto_start: bool,
    ) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({
            "name": name,
            "transport": transport,
            "target": target,
            "env": env,
            "auto_start": auto_start,
        });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/mcp/servers")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn unregister_mcp_server(&self, name: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .delete(self.url(&format!("/v1/mcp/servers/{name}"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn start_mcp(&self, name: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/mcp/servers/{name}/start"))),
            )
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
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/mcp/servers/{name}/stop"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn discover_mcp(&self, name: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .get(self.url(&format!("/v1/mcp/servers/{name}/discover"))),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn invoke_mcp_tool(
        &self,
        name: &str,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(
                self.http
                    .post(self.url(&format!("/v1/mcp/servers/{name}/tools/{tool}")))
                    .json(&args),
            )
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Memory -----------------------------------------------------------

    pub async fn list_memories(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/memory")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn remember(
        &self,
        content: &str,
        category: Option<&str>,
        source: Option<&str>,
    ) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({
            "content": content,
            "category": category.unwrap_or("general"),
            "source": source.unwrap_or("user"),
        });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/memory")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn forget_memory(&self, id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.delete(self.url(&format!("/v1/memory/{id}"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn forget_all_memories(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.delete(self.url("/v1/memory")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Assistance / Micro Model -----------------------------------------

    pub async fn get_micro_model(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/assistance/micro-model")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn set_micro_model(
        &self,
        model: Option<&str>,
    ) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({ "model": model });
        let resp = self
            .add_auth(
                self.http
                    .post(self.url("/v1/assistance/micro-model"))
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

    // ---- Figures ----------------------------------------------------------

    pub async fn list_figures(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/figures")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn save_figure(
        &self,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.post(self.url("/v1/figures")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn remove_figure(&self, id: &str) -> Result<(), SdkError> {
        let resp = self
            .add_auth(self.http.delete(self.url(&format!("/v1/figures/{id}"))))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    // ---- Travel mode ------------------------------------------------------

    pub async fn travel_status(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/travel")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn set_travel(&self, provider: Option<&str>) -> Result<serde_json::Value, SdkError> {
        let body = serde_json::json!({ "provider": provider });
        let resp = self
            .add_auth(self.http.post(self.url("/v1/travel")).json(&body))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }

    pub async fn travel_home(&self) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .add_auth(self.http.get(self.url("/v1/travel/home")))
            .send()
            .await?;
        if resp.status().is_success() {
            Ok(resp.json().await?)
        } else {
            Err(Self::decode_error(resp).await)
        }
    }
}

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

/// Probe a remote server's health with a short timeout.
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

/// Helper to mark a task as completed/failed locally.
pub fn terminal_status(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed
    )
}
