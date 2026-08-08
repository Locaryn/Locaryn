//! MCP route handlers — manage MCP server registration, lifecycle,
//! capability discovery, and tool invocation.
//!
//! Routes (API contract 06-api-contract.md):
//!   GET    /v1/mcp/servers                     — list registered servers
//!   POST   /v1/mcp/servers                     — register {name, command, ...}
//!   DELETE /v1/mcp/servers/{name}              — unregister
//!   POST   /v1/mcp/servers/{name}/start        — start (build client)
//!   POST   /v1/mcp/servers/{name}/stop         — stop (shutdown client)
//!   GET    /v1/mcp/servers/{name}/discover     — discover capabilities
//!   POST   /v1/mcp/servers/{name}/tools/{tool} — invoke a tool

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use lochor_mcp::{build_client, McpClient, McpServerEntry, McpState, Transport};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::DaemonState;

// ============================================================================
// Request / response helpers
// ============================================================================

#[derive(Deserialize)]
pub struct RegisterBody {
    pub name: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_transport_str")]
    pub transport: String,
    #[serde(default)]
    pub auto_start: bool,
}

fn default_transport_str() -> String {
    "stdio".to_string()
}

#[derive(Deserialize)]
pub struct InvokeBody {
    pub args: serde_json::Value,
}

fn parse_transport(s: &str) -> Result<Transport, &'static str> {
    match s {
        "stdio" => Ok(Transport::Stdio),
        "http" => Ok(Transport::Http),
        _ => Err("transport must be 'stdio' or 'http'"),
    }
}

fn server_to_json(name: &str, entry: &McpServerEntry) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "command": entry.command,
        "args": entry.args,
        "env": entry.env,
        "url": entry.url,
        "headers": entry.headers,
        "transport": entry.transport,
        "auto_start": entry.auto_start,
        "scope": entry.scope,
    })
}

fn capabilities_to_json(caps: &lochor_mcp::ServerCapabilities) -> serde_json::Value {
    serde_json::json!({
        "tools": caps.tools,
        "resources": caps.resources,
        "prompts": caps.prompts,
    })
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /v1/mcp/servers — list all registered MCP servers.
pub async fn list_servers(State(s): State<Arc<DaemonState>>) -> Response {
    let cfg = s.mcp_state.config.lock().unwrap();
    let entries: Vec<serde_json::Value> = cfg
        .mcp_servers
        .iter()
        .map(|(name, entry)| server_to_json(name, entry))
        .collect();
    (StatusCode::OK, Json(entries)).into_response()
}

/// POST /v1/mcp/servers — register a new MCP server.
pub async fn register_server(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<RegisterBody>,
) -> Response {
    let transport = match parse_transport(&body.transport) {
        Ok(t) => t,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };

    let mut cfg = s.mcp_state.config.lock().unwrap();
    if cfg.mcp_servers.contains_key(&body.name) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": { "code": "conflict", "message": format!("server already registered: {}", body.name) }
            })),
        )
            .into_response();
    }

    let entry = McpServerEntry {
        command: Some(body.command.unwrap_or_default()).filter(|c| !c.is_empty()),
        args: body.args,
        env: body.env,
        url: Some(body.url.unwrap_or_default()).filter(|u| !u.is_empty()),
        headers: body.headers,
        transport,
        auto_start: body.auto_start,
        scope: None,
        owner: None,
    };

    cfg.mcp_servers.insert(body.name.clone(), entry.clone());
    drop(cfg);
    s.mcp_state.save();

    (
        StatusCode::CREATED,
        Json(server_to_json(&body.name, &entry)),
    )
        .into_response()
}

/// DELETE /v1/mcp/servers/{name} — unregister a server.
pub async fn unregister_server(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    // Remove client from the running map under write lock, then shut down
    // outside the lock. tokio::sync::RwLockWriteGuard is Send.
    let stopped = {
        let mut running = s.mcp_state.running.write().await;
        running.remove(&name)
    };
    if let Some(client) = stopped {
        if let Err(e) = client.shutdown().await {
            tracing::warn!(server = %name, error = %e, "error shutting down MCP client");
        }
    }

    let mut cfg = s.mcp_state.config.lock().unwrap();
    if cfg.mcp_servers.remove(&name).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("server not found: {name}") }
            })),
        )
            .into_response();
    }
    drop(cfg);
    s.mcp_state.save();

    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "unregistered", "name": name })),
    )
        .into_response()
}

/// POST /v1/mcp/servers/{name}/start — start a server (build + store client).
pub async fn start_server(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    let entry = {
        let cfg = s.mcp_state.config.lock().unwrap();
        cfg.mcp_servers.get(&name).cloned()
    };

    let entry = match entry {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": { "code": "not_found", "message": format!("server not found: {name}") }
                })),
            )
                .into_response();
        }
    };

    // Check if already running.
    {
        let running = s.mcp_state.running.read().await;
        if running.contains_key(&name) {
            return (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "already_running", "name": name })),
            )
                .into_response();
        }
    }

    let client: Arc<dyn McpClient> = Arc::from(build_client(&entry));
    let mut running = s.mcp_state.running.write().await;
    running.insert(name.clone(), client);

    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "started", "name": name })),
    )
        .into_response()
}

/// POST /v1/mcp/servers/{name}/stop — stop a running server.
pub async fn stop_server(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    // Remove and shut down client outside the lock.
    let client = {
        let mut running = s.mcp_state.running.write().await;
        running.remove(&name)
    };
    match client {
        Some(client) => {
            if let Err(e) = client.shutdown().await {
                tracing::warn!(server = %name, error = %e, "MCP shutdown error");
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "stopped", "name": name })),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("server not running: {name}") }
            })),
        )
            .into_response(),
    }
}

/// GET /v1/mcp/servers/{name}/discover — discover capabilities of a server.
pub async fn discover_server(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    // Clone the Arc<dyn McpClient> under read lock, then call discover()
    // outside the lock (tokio::sync::RwLockReadGuard is Send).
    let client = {
        let running = s.mcp_state.running.read().await;
        running.get(&name).cloned()
    };

    let client = match client {
        Some(c) => c,
        None => return start_and_discover(&s.mcp_state, &name).await,
    };

    match client.discover().await {
        Ok(caps) => (StatusCode::OK, Json(capabilities_to_json(&caps))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "mcp_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

/// Helper: auto-start a server, discover, then stop.
async fn start_and_discover(
    state: &McpState,
    name: &str,
) -> Response {
    let entry = {
        let cfg = state.config.lock().unwrap();
        cfg.mcp_servers.get(name).cloned()
    };
    let entry = match entry {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": { "code": "not_found", "message": format!("server not found: {name}") }
                })),
            )
                .into_response();
        }
    };

    let client = build_client(&entry);
    let caps = match client.discover().await {
        Ok(caps) => caps,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": { "code": "mcp_error", "message": e.to_string() }
                })),
            )
                .into_response();
        }
    };

    // Don't keep the client running after discover.
    let _ = client.shutdown().await;

    (
        StatusCode::OK,
        Json(capabilities_to_json(&caps)),
    )
        .into_response()
}

/// POST /v1/mcp/servers/{name}/tools/{tool} — invoke a tool on a server.
pub async fn invoke_tool(
    State(s): State<Arc<DaemonState>>,
    Path((name, tool)): Path<(String, String)>,
    Json(body): Json<InvokeBody>,
) -> Response {
    // Clone the Arc<dyn McpClient> under read lock so we can call
    // invoke_tool() outside the lock.
    let client = {
        let running = s.mcp_state.running.read().await;
        running.get(&name).cloned()
    };

    let client = match client {
        Some(c) => c,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "not_running", "message": format!("server not running: {name}. Call /start first.") }
                })),
            )
                .into_response();
        }
    };

    match client.invoke_tool(&tool, &body.args).await {
        Ok(val) => (StatusCode::OK, Json(val)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "mcp_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

