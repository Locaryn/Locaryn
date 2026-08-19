//! Bridge between `locaryn-mcp` running clients and the agent tool loop.
//!
//! Two operations:
//! - `collect_mcp_tools`: scans all running MCP servers, calls `discover()`
//!   on each, and returns a `Vec<ToolSpec>` with prefixed names
//!   (`mcp__{server}__{tool}`).
//! - `dispatch_mcp_tool`: parses the prefixed name back to find the right
//!   server and forwards the call via `invoke_tool()`.

use crate::tools::{ToolArtifact, ToolResult, ToolSpec};
use locaryn_mcp::{McpClient, McpState};
use std::collections::HashMap;
use std::sync::Arc;

/// Prefix used to namespace MCP tools in the LLM's tool list.
pub const MCP_PREFIX: &str = "mcp__";

/// Prefix separator between server name and tool name.
const SEP: &str = "__";

/// Build the prefixed tool name that the LLM sees.
pub fn mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("{MCP_PREFIX}{server_name}{SEP}{tool_name}")
}

/// Split a prefixed name back into `(server_name, tool_name)`.
/// Returns `None` if the name doesn't look like an MCP tool.
pub fn parse_mcp_tool_name(prefixed: &str) -> Option<(String, String)> {
    let rest = prefixed.strip_prefix(MCP_PREFIX)?;
    let (server, tool) = rest.split_once(SEP)?;
    Some((server.to_string(), tool.to_string()))
}

/// Collect tool specs from all running MCP servers by calling `discover()`
/// on each client. Returns `ToolSpec` items with prefixed names (and clean aliases).
pub async fn collect_mcp_tools(state: &McpState) -> Vec<ToolSpec> {
    // Snapshot running servers under a short-lived read lock.
    let snapshot: HashMap<String, Arc<dyn McpClient>> = {
        let r = state.running.read().await;
        r.clone()
    };

    if snapshot.is_empty() {
        return Vec::new();
    }

    let mut specs = Vec::new();
    let mut clean_names = std::collections::HashSet::new();

    for (server_name, client) in &snapshot {
        match client.discover().await {
            Ok(caps) => {
                for t in &caps.tools {
                    let desc = t
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("MCP tool from server '{server_name}'"));
                    specs.push(ToolSpec {
                        name: mcp_tool_name(server_name, &t.name),
                        description: desc.clone(),
                        input_schema: t.input_schema.clone(),
                        risk: crate::tools::Risk::Medium,
                        required_permissions: Vec::new(),
                    });
                    if clean_names.insert(t.name.clone()) {
                        specs.push(ToolSpec {
                            name: t.name.clone(),
                            description: desc,
                            input_schema: t.input_schema.clone(),
                            risk: crate::tools::Risk::Medium,
                            required_permissions: Vec::new(),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!(server = %server_name, error = %e, "MCP discover failed");
            }
        }
    }

    specs
}

/// Dispatch a tool call to the correct MCP server.
/// Supports both `mcp__{server}__{tool}` and direct tool names like `generate_image`.
pub async fn dispatch_mcp_tool(
    state: &McpState,
    prefixed_or_clean_name: &str,
    args: &serde_json::Value,
) -> ToolResult {
    let (server_name, tool_name, client) = if let Some((server, tool)) = parse_mcp_tool_name(prefixed_or_clean_name) {
        let client = {
            let r = state.running.read().await;
            r.get(&server).cloned()
        };
        let Some(c) = client else {
            return ToolResult {
                ok: false,
                output: format!("MCP server '{server}' is not running. Call /v1/mcp/servers/{server}/start first."),
                artifact: None,
            };
        };
        (server, tool, c)
    } else {
        // Direct tool name lookup across running servers
        let snapshot: HashMap<String, Arc<dyn McpClient>> = {
            let r = state.running.read().await;
            r.clone()
        };
        let mut matched = None;
        for (s_name, c) in snapshot {
            if let Ok(caps) = c.discover().await {
                if caps.tools.iter().any(|t| t.name == prefixed_or_clean_name) {
                    matched = Some((s_name, prefixed_or_clean_name.to_string(), c));
                    break;
                }
            }
        }
        let Some((s_name, t_name, c)) = matched else {
            return ToolResult {
                ok: false,
                output: format!("not an MCP tool or server not running: {prefixed_or_clean_name}"),
                artifact: None,
            };
        };
        (s_name, t_name, c)
    };

    match client.invoke_tool(&tool_name, args).await {
        Ok(val) => {
            // Convert the JSON-RPC result to a display string.
            let output = serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
            ToolResult {
                ok: true,
                output,
                // MCP transports return a text content item for most tools.
                // The host stays capability-agnostic: an extension may opt in
                // to the generic artifact envelope without teaching Locaryn
                // what the file means.
                artifact: artifact_from_mcp_value(&val),
            }
        }
        Err(e) => ToolResult {
            ok: false,
            output: format!("MCP tool '{tool_name}' on '{server_name}' failed: {e}"),
            artifact: None,
        },
    }
}

/// Read the generic artifact envelope emitted by an MCP extension.
///
/// Both the direct JSON shape (`{\"artifacts\":[...]}`) and the MCP text
/// shape (a JSON string containing that object) are accepted. This keeps the
/// transport layer independent from image/audio/video plugins while still
/// allowing every client to render the produced file.
fn artifact_from_mcp_value(value: &serde_json::Value) -> Option<ToolArtifact> {
    let payload = value
        .as_str()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .unwrap_or_else(|| value.clone());
    let candidate = payload
        .get("artifacts")
        .and_then(|items| items.as_array())
        .and_then(|items| items.first())
        .or_else(|| payload.get("artifact"));
    let object = candidate?.as_object()?;
    let kind = object.get("kind")?.as_str()?;
    let path = object.get("path")?.as_str()?;
    let kind = match kind {
        "html" => locaryn_shared_types::ArtifactKind::Html,
        "markdown" => locaryn_shared_types::ArtifactKind::Markdown,
        "python_text" => locaryn_shared_types::ArtifactKind::PythonText,
        "image_png" => locaryn_shared_types::ArtifactKind::ImagePng,
        "plotly_html" => locaryn_shared_types::ArtifactKind::PlotlyHtml,
        "audio_wav" => locaryn_shared_types::ArtifactKind::AudioWav,
        _ => return None,
    };
    Some(ToolArtifact {
        kind,
        path: path.to_string(),
    })
}
