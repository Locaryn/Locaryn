//! Locaryn MCP — wrapper around the Model Context Protocol (spec 2026-07-28).
//!
//! Supports:
//! - **stdio** transport: spawn a local MCP server subprocess, communicate
//!   via JSON-RPC 2.0 over stdin/stdout.
//! - **stateless HTTP** transport: call a remote MCP server over HTTP SSE.
//! - `server/discover` to enumerate tools/resources/prompts.
//! - tool invocation with permission gating (the `mcp` permission).
//!
//! The client abstraction wraps the JSON-RPC protocol so the rest of the
//! codebase never touches protocol details.

use locaryn_shared_types::ExtensionScope;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Registry (.mcp.json)
// ============================================================================

/// `.locaryn/mcp.json` / `~/.locaryn/mcp.json` — compatible with the
/// Claude Code / Cursor `mcpServers` format.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    /// For stdio transport: the command to spawn.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// For HTTP transport: the server URL (stateless, MCP 2026-07-28).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_transport")]
    pub transport: Transport,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub scope: Option<ExtensionScope>,
    /// Name of the plugin that contributed this server, when it did not come
    /// from the user.
    ///
    /// Plugin servers live in memory only. They are re-derived from the
    /// plugin's manifest on every start, so persisting them into the user's
    /// `mcp.json` would leave orphans behind after an uninstall — and would
    /// make the daemon try to spawn a command from a directory that no longer
    /// exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

fn default_transport() -> Transport {
    Transport::Stdio
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Stdio,
    Http,
}

impl McpConfig {
    pub fn load(path: &std::path::Path) -> Result<Self, std::io::Error> {
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    /// Write the user's servers back to disk. Plugin-owned entries are skipped:
    /// they belong to the plugin's manifest, not to this file.
    pub fn save(&self, path: &std::path::Path) -> Result<(), std::io::Error> {
        let user_only = McpConfig {
            mcp_servers: self
                .mcp_servers
                .iter()
                .filter(|(_, e)| e.owner.is_none())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };
        let raw = serde_json::to_string_pretty(&user_only).unwrap_or_default();
        std::fs::write(path, raw)
    }
}

// ============================================================================
// Discovered capabilities
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub tools: Vec<ToolDescriptor>,
    pub resources: Vec<ResourceDescriptor>,
    pub prompts: Vec<PromptDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDescriptor {
    pub uri: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDescriptor {
    pub name: String,
    pub description: Option<String>,
}

// ============================================================================
// Client trait (abstraction over transport)
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("permission denied for tool {0}")]
    PermissionDenied(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-RPC error: code={code}, message={message}")]
    JsonRpc { code: i64, message: String },
}

#[async_trait::async_trait]
pub trait McpClient: Send + Sync {
    /// `server/discover` — enumerate capabilities.
    async fn discover(&self) -> Result<ServerCapabilities, McpError>;
    /// Invoke a tool by name with JSON arguments.
    async fn invoke_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError>;
    /// Stop the underlying transport (subprocess or HTTP pool).
    async fn shutdown(&self) -> Result<(), McpError>;
}

// ============================================================================
// JSON-RPC helpers
// ============================================================================

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// Stdio transport uses `tokio::process::Command` and reads/writes JSON lines.
struct StdioTransport {
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
    next_id: std::sync::atomic::AtomicU64,
    _child: tokio::process::Child,
}

impl StdioTransport {
    async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, McpError> {
        let mut cmd = tokio::process::Command::new(locaryn_config::resolve_program(command));
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // Add to the environment, never replace it. Configurations set one
        // API key; clearing the rest would take PATH with it, and the server
        // would fail to find the very interpreter that is running it.
        cmd.envs(env);

        let mut child = cmd.spawn().map_err(|e| {
            McpError::Transport(format!(
                "spawn {command}: {e}. Vérifiez que « {command} » se lance depuis un terminal."
            ))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Transport("stdin not available".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Transport("stdout not available".into()))?;

        Ok(Self {
            stdin,
            stdout: tokio::io::BufReader::new(stdout),
            next_id: std::sync::atomic::AtomicU64::new(1),
            _child: child,
        })
    }

    async fn call(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, McpError> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };

        let mut line = serde_json::to_string(&req)
            .map_err(|e| McpError::Protocol(format!("serialize: {e}")))?;
        line.push('\n');

        use tokio::io::AsyncWriteExt;
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| McpError::Transport(format!("write: {e}")))?;

        // Read response line(s). MCP returns one JSON-RPC response per request.
        use tokio::io::AsyncBufReadExt;
        let mut raw = String::new();
        self.stdout
            .read_line(&mut raw)
            .await
            .map_err(|e| McpError::Transport(format!("read: {e}")))?;

        if raw.is_empty() {
            return Err(McpError::Transport("connection closed by server".into()));
        }

        let resp: JsonRpcResponse = serde_json::from_str(&raw)
            .map_err(|e| McpError::Protocol(format!("parse response: {e}")))?;

        if let Some(err) = resp.error {
            return Err(McpError::JsonRpc {
                code: err.code,
                message: err.message,
            });
        }

        resp.result
            .ok_or_else(|| McpError::Protocol("no result in response".into()))
    }
}

// ============================================================================
// HTTP transport for MCP protocol — simple stateless HTTP POST
// ============================================================================

/// Simple stateless HTTP JSON-RPC transport. Sends POST requests and reads
/// the JSON-RPC response from the HTTP response body. Used when the server
/// doesn't support SSE streaming (or we don't need it).
struct HttpTransport {
    url: String,
    client: reqwest::Client,
    headers: HashMap<String, String>,
    next_id: std::sync::atomic::AtomicU64,
}

impl HttpTransport {
    fn new(url: &str, headers: &HashMap<String, String>) -> Self {
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
            headers: headers.clone(),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    async fn call(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, McpError> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let mut req = self.client.post(&self.url).json(&body);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| McpError::Transport(format!("HTTP POST: {e}")))?;

        let status = resp.status();
        let raw = resp
            .text()
            .await
            .map_err(|e| McpError::Transport(format!("read body: {e}")))?;

        if !status.is_success() {
            return Err(McpError::Transport(format!("HTTP {status}: {raw}")));
        }

        let rpc_resp: JsonRpcResponse =
            serde_json::from_str(&raw).map_err(|e| McpError::Protocol(format!("parse: {e}")))?;

        if let Some(err) = rpc_resp.error {
            return Err(McpError::JsonRpc {
                code: err.code,
                message: err.message,
            });
        }

        rpc_resp
            .result
            .ok_or_else(|| McpError::Protocol("no result".into()))
    }
}

// ============================================================================
// MCP HTTP + SSE transport (spec 2026-07-28)
// ============================================================================

/// In-flight JSON-RPC calls, keyed by request `id`. The SSE reader task
/// delivers each response through the matching sender.
type PendingRequests = std::sync::Arc<
    tokio::sync::Mutex<
        HashMap<String, tokio::sync::oneshot::Sender<Result<serde_json::Value, McpError>>>,
    >,
>;

/// MCP HTTP+SSE transport client.
///
/// Protocol:
/// 1. Connect to the server via SSE (GET with `Accept: text/event-stream`)
/// 2. Server immediately sends `event: endpoint` with `data: <post_uri>`
/// 3. Client sends JSON-RPC requests via HTTP POST to the discovered URI
/// 4. Server sends JSON-RPC responses as SSE `message` events
///
/// The SSE connection is kept alive in a background task. Requests are
/// correlated by JSON-RPC `id` field. Each method call waits for the
/// matching `message` event or a timeout.
struct SseClient {
    /// The URL for sending HTTP POST requests (discovered via SSE `endpoint` event).
    post_url: String,
    /// Reusable reqwest client for connection pooling.
    client: reqwest::Client,
    /// Custom HTTP headers from the server entry.
    headers: HashMap<String, String>,
    /// Monotonic JSON-RPC request ID counter.
    next_id: std::sync::atomic::AtomicU64,
    /// Pending requests keyed by JSON-RPC `id` as string. When a matching
    /// response arrives via SSE, the sender delivers it to the waiting caller.
    pending: PendingRequests,
    /// Signal the background SSE reader to stop.
    shutdown: tokio_util::sync::CancellationToken,
    /// Tracks whether the background SSE task is still alive. Set to `false`
    /// when the stream ends or errors, allowing `call_rpc` to detect
    /// connection loss and return promptly instead of timing out after 60s.
    /// Shared with the background task via `Arc`.
    alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SseClient {
    /// Establish an SSE connection to `sse_url`, discover the POST endpoint,
    /// and start reading SSE events in a background task.
    pub async fn connect(
        sse_url: &str,
        headers: &HashMap<String, String>,
    ) -> Result<Self, McpError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| McpError::Transport(format!("build client: {e}")))?;

        // Connect to the SSE endpoint.
        let mut req = client.get(sse_url).header("Accept", "text/event-stream");
        for (k, v) in headers {
            req = req.header(k, v);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| McpError::Transport(format!("SSE connect: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(McpError::Transport(format!(
                "SSE connect HTTP {status}: {body}"
            )));
        }

        // Read the byte stream for SSE parsing.
        let byte_stream = resp.bytes_stream();
        let mut sse_parser = SseParser::new(byte_stream);

        // Wait for the first `endpoint` event which tells us the POST URL.
        let post_url = loop {
            match sse_parser.next_event().await {
                Ok(Some(event)) if event.event_type == "endpoint" => {
                    let url = event.data.trim().to_string();
                    if url.is_empty() {
                        return Err(McpError::Protocol("empty endpoint event data".into()));
                    }
                    // Resolve relative URLs against the SSE URL.
                    let resolved = if url.starts_with("http://") || url.starts_with("https://") {
                        url
                    } else {
                        let base = sse_url.trim_end_matches('/');
                        let path = url.trim_start_matches('/');
                        format!("{base}/{path}")
                    };
                    break resolved;
                }
                Ok(Some(_)) => continue, // skip non-endpoint events
                Ok(None) => {
                    return Err(McpError::Transport(
                        "SSE connection closed without endpoint event".into(),
                    ))
                }
                Err(e) => return Err(e),
            }
        };

        let pending = std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let shutdown = tokio_util::sync::CancellationToken::new();
        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        // Spawn background task to read SSE events.
        let pending_bg = pending.clone();
        let shutdown_bg = shutdown.clone();
        let alive_bg = alive.clone();
        tokio::spawn(async move {
            let result = loop {
                tokio::select! {
                    _ = shutdown_bg.cancelled() => break Ok(()),
                    result = sse_parser.next_event() => {
                        match result {
                            Ok(Some(event)) => {
                                if event.event_type == "message" || event.event_type == "event" {
                                    SseClient::dispatch_response(&pending_bg, &event.data).await;
                                }
                            }
                            Ok(None) => break Ok(()), // stream ended cleanly
                            Err(e) => break Err(e),
                        }
                    }
                }
            };
            // Mark client dead regardless of how we exited so call_rpc
            // can detect connection loss immediately.
            alive_bg.store(false, std::sync::atomic::Ordering::Release);
            if let Err(e) = result {
                tracing::warn!(error = %e, "SSE client stopped");
            }
            // Wake up any pending callers so they don't wait for the timeout.
            let mut map = pending_bg.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(McpError::Transport("SSE connection lost".into())));
            }
        });

        Ok(Self {
            post_url,
            client,
            headers: headers.clone(),
            next_id: std::sync::atomic::AtomicU64::new(1),
            pending,
            shutdown,
            alive,
        })
    }

    /// Send a JSON-RPC request via POST and wait for the matching SSE response.
    /// Returns immediately with an error if the SSE background task has died.
    async fn call_rpc(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, McpError> {
        // Fast-fail if the SSE task died.
        if !self.alive.load(std::sync::atomic::Ordering::Acquire) {
            return Err(McpError::Transport("SSE connection already lost".into()));
        }

        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id_str = id.to_string();

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        // Create a oneshot channel and register it in the pending map BEFORE
        // sending the request to avoid a race where the response arrives
        // before we start waiting.
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut map = self.pending.lock().await;
            map.insert(id_str.clone(), tx);
        }

        // Send the POST request.
        let mut req = self.client.post(&self.post_url).json(&body);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }

        let http_result = req.send().await;

        match http_result {
            Ok(resp) if resp.status().is_success() => {
                // Some servers respond synchronously in the HTTP body.
                // Check for a JSON-RPC response before waiting on SSE.
                let body_bytes = resp.bytes().await.unwrap_or_default();
                if !body_bytes.is_empty() {
                    if let Ok(rpc_resp) = serde_json::from_slice::<JsonRpcResponse>(&body_bytes) {
                        // Remove the pending entry before processing.
                        let mut map = self.pending.lock().await;
                        map.remove(&id_str);
                        drop(map);

                        if let Some(err) = rpc_resp.error {
                            return Err(McpError::JsonRpc {
                                code: err.code,
                                message: err.message,
                            });
                        }
                        if let Some(result) = rpc_resp.result {
                            return Ok(result);
                        }
                        return Err(McpError::Protocol(
                            "no result in synchronous response".into(),
                        ));
                    }
                }
                // No synchronous response — wait for SSE.
                match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
                    Ok(Ok(result)) => result,
                    Ok(Err(_)) => {
                        // The sender was dropped without sending — connection lost.
                        Err(McpError::Transport(
                            "SSE connection lost before response".into(),
                        ))
                    }
                    Err(_) => {
                        // Timeout waiting for response.
                        let mut map = self.pending.lock().await;
                        map.remove(&id_str);
                        Err(McpError::Transport(format!("RPC timeout for {method}")))
                    }
                }
            }
            Ok(resp) => {
                // Non-2xx response.
                let status = resp.status();
                let raw = resp.text().await.unwrap_or_default();
                let mut map = self.pending.lock().await;
                map.remove(&id_str);
                Err(McpError::Transport(format!("HTTP {status}: {raw}")))
            }
            Err(e) => {
                // Connection error.
                let mut map = self.pending.lock().await;
                map.remove(&id_str);
                Err(McpError::Transport(format!("HTTP POST: {e}")))
            }
        }
    }

    /// Parse a JSON-RPC response from an SSE `data` field and dispatch
    /// it to the matching pending request.
    async fn dispatch_response(
        pending: &tokio::sync::Mutex<
            HashMap<String, tokio::sync::oneshot::Sender<Result<serde_json::Value, McpError>>>,
        >,
        data: &str,
    ) {
        let val: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, raw = %data, "failed to parse SSE message data");
                return;
            }
        };

        let id = match val.get("id") {
            Some(v) => v.to_string(),
            None => {
                // Notifications or batch messages without an id are ignored.
                return;
            }
        };

        let mut map = pending.lock().await;
        if let Some(tx) = map.remove(&id) {
            if let Some(err) = val.get("error") {
                let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
                let msg = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                let _ = tx.send(Err(McpError::JsonRpc {
                    code,
                    message: msg.into(),
                }));
            } else if let Some(result) = val.get("result") {
                let _ = tx.send(Ok(result.clone()));
            } else {
                let _ = tx.send(Err(McpError::Protocol(
                    "response missing result and error".into(),
                )));
            }
        }
        // If no pending request matches, the response is stale or a notification.
    }
}

/// Extract a `ServerCapabilities` from a `tools/list` JSON-RPC result.
fn parse_tools_from_result(result: &serde_json::Value) -> ServerCapabilities {
    let tools = result
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    Some(ToolDescriptor {
                        name: t.get("name")?.as_str()?.to_string(),
                        description: t
                            .get("description")
                            .and_then(|d| d.as_str())
                            .map(String::from),
                        input_schema: t
                            .get("inputSchema")
                            .cloned()
                            .or_else(|| t.get("input_schema").cloned())
                            .unwrap_or(serde_json::json!({})),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    ServerCapabilities {
        tools,
        resources: Vec::new(),
        prompts: Vec::new(),
    }
}

/// Extract text content from an MCP tool call response `content` array.
fn extract_text_content(result: &serde_json::Value) -> serde_json::Value {
    let content = result
        .get("content")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    serde_json::json!(content)
}

impl Drop for SseClient {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

#[async_trait::async_trait]
impl McpClient for SseClient {
    async fn discover(&self) -> Result<ServerCapabilities, McpError> {
        let result = self.call_rpc("tools/list", None).await?;

        let tools: Vec<ToolDescriptor> = result
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(ToolDescriptor {
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(String::from),
                            input_schema: t
                                .get("inputSchema")
                                .cloned()
                                .or_else(|| t.get("input_schema").cloned())
                                .unwrap_or(serde_json::json!({})),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ServerCapabilities {
            tools,
            resources: Vec::new(),
            prompts: Vec::new(),
        })
    }

    async fn invoke_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": args,
        });
        let result = self.call_rpc("tools/call", Some(params)).await?;

        // MCP spec: tool response `content` is an array of content items.
        // Extract text items joined together. Other content types (images,
        // resources) are returned as JSON for the caller to handle.
        let content = result
            .get("content")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        Ok(serde_json::json!(content))
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        self.shutdown.cancel();
        Ok(())
    }
}

// ============================================================================
// SSE event parser — reads a byte stream line-by-line, building SSE events
// ============================================================================

struct SseEvent {
    event_type: String,
    data: String,
}

struct SseParser<S> {
    stream: S,
    buffer: Vec<u8>,
    current_event: Option<String>,
    current_data: String,
}

impl<S> SseParser<S>
where
    S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    fn new(stream: S) -> Self {
        Self {
            stream,
            buffer: Vec::with_capacity(4096),
            current_event: None,
            current_data: String::new(),
        }
    }

    /// Read the next complete SSE event from the stream. Returns `None`
    /// when the stream ends cleanly.
    async fn next_event(&mut self) -> Result<Option<SseEvent>, McpError> {
        use futures::StreamExt;

        loop {
            // Process complete lines from the buffer.
            while let Some(nl_pos) = self.buffer.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = self.buffer.drain(..=nl_pos).collect();
                // Remove trailing \r if present (Windows line endings).
                let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1])
                    .trim_end_matches('\r')
                    .to_string();

                if line.is_empty() {
                    // Empty line = end of event. Dispatch if we have data.
                    if !self.current_data.is_empty() {
                        let event = SseEvent {
                            event_type: self
                                .current_event
                                .take()
                                .unwrap_or_else(|| "message".into()),
                            data: std::mem::take(&mut self.current_data),
                        };
                        return Ok(Some(event));
                    }
                    continue;
                }

                if let Some(value) = line.strip_prefix("event: ") {
                    self.current_event = Some(value.to_string());
                } else if let Some(value) = line.strip_prefix("data: ") {
                    if !self.current_data.is_empty() {
                        self.current_data.push('\n');
                    }
                    self.current_data.push_str(value);
                }
                // Ignore other SSE fields (id, retry, etc.).
            }

            // Need more data from the stream.
            match self.stream.next().await {
                Some(Ok(chunk)) => {
                    self.buffer.extend_from_slice(&chunk);
                }
                Some(Err(e)) => {
                    return Err(McpError::Transport(format!("SSE read: {e}")));
                }
                None => {
                    // Stream ended. Flush any pending event.
                    if !self.current_data.is_empty() {
                        let event = SseEvent {
                            event_type: self
                                .current_event
                                .take()
                                .unwrap_or_else(|| "message".into()),
                            data: std::mem::take(&mut self.current_data),
                        };
                        return Ok(Some(event));
                    }
                    return Ok(None);
                }
            }
        }
    }
}

// ============================================================================
// Real MCP client implementations
// ============================================================================

/// Stdio-based MCP client.
pub struct StdioClient {
    transport: tokio::sync::Mutex<StdioTransport>,
}

impl StdioClient {
    pub async fn new(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, McpError> {
        let transport = StdioTransport::spawn(command, args, env).await?;
        Ok(Self {
            transport: tokio::sync::Mutex::new(transport),
        })
    }
}

#[async_trait::async_trait]
impl McpClient for StdioClient {
    async fn discover(&self) -> Result<ServerCapabilities, McpError> {
        let mut t = self.transport.lock().await;

        // First send `tools/list` to get available tools.
        let result = t.call("tools/list", None).await?;

        let tools: Vec<ToolDescriptor> = result
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(ToolDescriptor {
                            name: t.get("name")?.as_str()?.to_string(),
                            description: t
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(String::from),
                            input_schema: t
                                .get("inputSchema")
                                .cloned()
                                .or_else(|| t.get("input_schema").cloned())
                                .unwrap_or(serde_json::json!({})),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ServerCapabilities {
            tools,
            resources: Vec::new(),
            prompts: Vec::new(),
        })
    }

    async fn invoke_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let mut t = self.transport.lock().await;
        let params = serde_json::json!({
            "name": name,
            "arguments": args,
        });
        let result = t.call("tools/call", Some(params)).await?;

        // Extract text content from the response `content` array.
        let content = result
            .get("content")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        Ok(serde_json::json!(content))
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        Ok(())
    }
}

/// A no-op client used as a fallback when connection fails.
pub struct StubClient;

#[async_trait::async_trait]
impl McpClient for StubClient {
    async fn discover(&self) -> Result<ServerCapabilities, McpError> {
        Ok(ServerCapabilities {
            tools: vec![],
            resources: vec![],
            prompts: vec![],
        })
    }
    async fn invoke_tool(
        &self,
        name: &str,
        _args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        Err(McpError::Protocol(format!(
            "stub client cannot invoke tool {name}"
        )))
    }
    async fn shutdown(&self) -> Result<(), McpError> {
        Ok(())
    }
}

// ============================================================================
// Builder
// ============================================================================

/// Build a client for a registered server. Dispatches to `LazyStdio` for
/// stdio transport and `LazySseHttp` for HTTP transport (SSE with HTTP POST
/// fallback). Falls back to `StubClient` if required fields are missing.
pub fn build_client(entry: &McpServerEntry) -> Box<dyn McpClient> {
    match entry.transport {
        Transport::Stdio => {
            let command = match &entry.command {
                Some(c) if !c.is_empty() => c.clone(),
                _ => {
                    tracing::warn!("MCP stdio entry missing 'command'");
                    return Box::new(StubClient);
                }
            };
            Box::new(LazyStdio {
                command,
                args: entry.args.clone(),
                env: entry.env.clone(),
                spawned: tokio::sync::Mutex::new(None),
            })
        }
        Transport::Http => {
            let url = match &entry.url {
                Some(u) if !u.is_empty() => u.clone(),
                _ => {
                    tracing::warn!("MCP HTTP entry missing 'url'");
                    return Box::new(StubClient);
                }
            };
            Box::new(LazySseHttp {
                sse_url: url,
                headers: entry.headers.clone(),
                inner: tokio::sync::Mutex::new(None),
            })
        }
    }
}

// Lazy initialization wrappers — build_client returns immediately without
// blocking. The real client is created on first discover/invoke call.

struct LazyStdio {
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    spawned: tokio::sync::Mutex<Option<StdioTransport>>,
}

#[async_trait::async_trait]
impl McpClient for LazyStdio {
    async fn discover(&self) -> Result<ServerCapabilities, McpError> {
        let mut guard = self.spawned.lock().await;
        if guard.is_none() {
            *guard = Some(StdioTransport::spawn(&self.command, &self.args, &self.env).await?);
        }
        guard
            .as_mut()
            .unwrap()
            .call("tools/list", None)
            .await
            .map(|result| {
                let tools = result
                    .get("tools")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| {
                                Some(ToolDescriptor {
                                    name: t.get("name")?.as_str()?.to_string(),
                                    description: t
                                        .get("description")
                                        .and_then(|d| d.as_str())
                                        .map(String::from),
                                    input_schema: t
                                        .get("inputSchema")
                                        .cloned()
                                        .or_else(|| t.get("input_schema").cloned())
                                        .unwrap_or(serde_json::json!({})),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                ServerCapabilities {
                    tools,
                    resources: Vec::new(),
                    prompts: Vec::new(),
                }
            })
    }

    async fn invoke_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let mut guard = self.spawned.lock().await;
        if guard.is_none() {
            *guard = Some(StdioTransport::spawn(&self.command, &self.args, &self.env).await?);
        }
        let params = serde_json::json!({
            "name": name,
            "arguments": args,
        });
        let result = guard
            .as_mut()
            .unwrap()
            .call("tools/call", Some(params))
            .await?;
        let content = result
            .get("content")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        Ok(serde_json::json!(content))
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        Ok(())
    }
}

/// Lazy SSE HTTP client. Connects to the server via SSE on first use.
/// Fallback: if SSE connection fails, uses simple HTTP POST instead.
struct LazySseHttp {
    sse_url: String,
    headers: HashMap<String, String>,
    inner: tokio::sync::Mutex<Option<ClientOrFallback>>,
}

enum ClientOrFallback {
    Sse(SseClient),
    Fallback(HttpTransport),
}

impl LazySseHttp {
    /// Ensure the underlying client is initialized. Tries SSE first, falls
    /// back to simple HTTP POST if the server doesn't support SSE.
    async fn ensure_initialized(&self, guard: &mut Option<ClientOrFallback>) {
        if guard.is_some() {
            return;
        }
        match SseClient::connect(&self.sse_url, &self.headers).await {
            Ok(client) => {
                tracing::info!(url = %self.sse_url, "MCP: SSE transport connected");
                *guard = Some(ClientOrFallback::Sse(client));
            }
            Err(e) => {
                tracing::warn!(url = %self.sse_url, error = %e, "MCP: SSE connect failed, falling back to HTTP POST");
                let transport = HttpTransport::new(&self.sse_url, &self.headers);
                *guard = Some(ClientOrFallback::Fallback(transport));
            }
        }
    }
}

#[async_trait::async_trait]
impl McpClient for LazySseHttp {
    async fn discover(&self) -> Result<ServerCapabilities, McpError> {
        let mut guard = self.inner.lock().await;
        self.ensure_initialized(&mut guard).await;

        match guard.as_ref().unwrap() {
            ClientOrFallback::Sse(client) => match client.discover().await {
                Ok(caps) => Ok(caps),
                Err(e @ McpError::Transport(_)) if is_sse_dead(&e) => {
                    tracing::warn!("MCP: SSE lost, switching to HTTP POST");
                    let fallback = HttpTransport::new(&self.sse_url, &self.headers);
                    let r = fallback.call("tools/list", None).await?;
                    *guard = Some(ClientOrFallback::Fallback(HttpTransport::new(
                        &self.sse_url,
                        &self.headers,
                    )));
                    Ok(parse_tools_from_result(&r))
                }
                Err(e) => Err(e),
            },
            ClientOrFallback::Fallback(transport) => {
                let result = transport.call("tools/list", None).await?;
                Ok(parse_tools_from_result(&result))
            }
        }
    }

    async fn invoke_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let mut guard = self.inner.lock().await;
        self.ensure_initialized(&mut guard).await;

        match guard.as_ref().unwrap() {
            ClientOrFallback::Sse(client) => match client.invoke_tool(name, args).await {
                Ok(val) => Ok(val),
                Err(e @ McpError::Transport(_)) if is_sse_dead(&e) => {
                    tracing::warn!("MCP: SSE lost, switching to HTTP POST");
                    let fallback = HttpTransport::new(&self.sse_url, &self.headers);
                    let params = serde_json::json!({ "name": name, "arguments": args });
                    let r = fallback.call("tools/call", Some(params)).await?;
                    *guard = Some(ClientOrFallback::Fallback(HttpTransport::new(
                        &self.sse_url,
                        &self.headers,
                    )));
                    Ok(extract_text_content(&r))
                }
                Err(e) => Err(e),
            },
            ClientOrFallback::Fallback(transport) => {
                let params = serde_json::json!({ "name": name, "arguments": args });
                let result = transport.call("tools/call", Some(params)).await?;
                Ok(extract_text_content(&result))
            }
        }
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        let mut guard = self.inner.lock().await;
        match guard.as_mut() {
            Some(ClientOrFallback::Sse(client)) => client.shutdown().await,
            _ => Ok(()),
        }
    }
}

/// Check if the error message indicates the SSE connection is dead.
fn is_sse_dead(e: &McpError) -> bool {
    match e {
        McpError::Transport(msg) => {
            matches!(
                msg.as_str(),
                "SSE connection already lost"
                    | "SSE connection lost"
                    | "SSE connection lost before response"
            )
        }
        _ => false,
    }
}

// ============================================================================
// Config path resolution
// ============================================================================

// ============================================================================
// McpState — shared state container for the daemon
// ============================================================================

/// Env vars Locaryn injecte dans chaque serveur stdio qu'il démarre, pour
/// qu'une extension puisse joindre le moteur actif sans le coder en dur.
pub const ACTIVE_MODEL_ENV: &str = "LOCARYN_ACTIVE_MODEL";
pub const LLM_ENDPOINT_ENV: &str = "LOCARYN_LLM_ENDPOINT";

/// Holds the MCP server configuration (loaded from / saved to `.mcp.json`)
/// and a map of currently running clients. Used by the daemon and injected
/// into the agent-runtime so MCP tools are available in the tool loop.
pub struct McpState {
    /// In-memory config mirror. Mutated by register / unregister, persisted
    /// on each write via save().
    pub config: std::sync::Mutex<McpConfig>,
    /// Config file path (resolved from the global MCP config path on init).
    pub config_path: PathBuf,
    /// Running clients keyed by server name. Uses tokio::sync::RwLock so
    /// handlers can hold a read lock across .await points.
    pub running: RwLock<HashMap<String, Arc<dyn McpClient>>>,
    /// Env Locaryn injecte dans chaque serveur stdio qu'il démarre
    /// (`LOCARYN_ACTIVE_MODEL`, `LOCARYN_LLM_ENDPOINT`). Renseigné depuis le
    /// provider actif ; jamais persisté dans `mcp.json`.
    pub runtime_env: std::sync::RwLock<HashMap<String, String>>,
}

impl std::fmt::Debug for McpState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpState")
            .field("config_path", &self.config_path)
            .finish_non_exhaustive()
    }
}

impl Default for McpState {
    fn default() -> Self {
        Self::new()
    }
}

impl McpState {
    pub fn new() -> Self {
        let path = config_path(ExtensionScope::Global, None);
        let cfg = McpConfig::load(&path).unwrap_or_default();
        Self {
            config: std::sync::Mutex::new(cfg),
            config_path: path,
            running: RwLock::new(HashMap::new()),
            runtime_env: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Save the current config to disk.
    pub fn save(&self) {
        let cfg = self.config.lock().unwrap();
        if let Err(e) = cfg.save(&self.config_path) {
            tracing::warn!(path = %self.config_path.display(), error = %e, "failed to save MCP config");
        }
    }

    /// Record the active LLM so every stdio server spawned afterwards can
    /// discover it. Called by the host (desktop/daemon) at startup and when
    /// the provider changes.
    pub fn set_runtime_env(&self, active_model: Option<String>, endpoint: Option<String>) {
        let mut env = HashMap::new();
        if let Some(m) = active_model.filter(|s| !s.is_empty()) {
            env.insert(ACTIVE_MODEL_ENV.to_string(), m);
        }
        if let Some(e) = endpoint.filter(|s| !s.is_empty()) {
            env.insert(LLM_ENDPOINT_ENV.to_string(), e);
        }
        if let Ok(mut guard) = self.runtime_env.write() {
            *guard = env;
        }
    }

    /// Like the free `build_client`, but merges the runtime LLM env into the
    /// entry first. The entry's own declared env wins — Locaryn fills only the
    /// gaps, never overrides what the user set explicitly.
    pub fn build_client(&self, entry: &McpServerEntry) -> Box<dyn McpClient> {
        let mut entry = entry.clone();
        if let Ok(runtime) = self.runtime_env.read() {
            for (k, v) in runtime.iter() {
                entry.env.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        build_client(&entry)
    }
}

// ============================================================================
// Config path resolution
// ============================================================================

/// Where `.mcp.json` lives for a given scope.
pub fn config_path(scope: ExtensionScope, workspace_root: Option<&std::path::Path>) -> PathBuf {
    match scope {
        ExtensionScope::Global | ExtensionScope::User => {
            locaryn_config::global_dir().join("mcp.json")
        }
        ExtensionScope::Workspace => workspace_root
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".locaryn")
            .join("mcp.json"),
        ExtensionScope::Session => std::env::temp_dir().join("locaryn-mcp-session.json"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mcp_json_cursor_format() {
        let raw = r#"{
            "mcpServers": {
                "narsil": { "command": "narsil-mcp", "args": ["--git"], "transport": "stdio" },
                "weather": { "url": "https://mcp.example.com/weather", "transport": "http" }
            }
        }"#;
        let cfg: McpConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.mcp_servers.len(), 2);
        assert_eq!(cfg.mcp_servers["narsil"].transport, Transport::Stdio);
        assert_eq!(cfg.mcp_servers["weather"].transport, Transport::Http);
    }
}
