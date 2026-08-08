//! Bridge between `locaryn-mcp` running clients and the agent tool loop.
//!
//! Two operations:
//! - `collect_mcp_tools`: scans all running MCP servers, calls `discover()`
//!   on each, and returns a `Vec<ToolSpec>` with prefixed names
//!   (`mcp__{server}__{tool}`).
//! - `dispatch_mcp_tool`: parses the prefixed name back to find the right
//!   server and forwards the call via `invoke_tool()`.

use crate::tools::{ToolResult, ToolSpec};
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
/// on each client. Returns `ToolSpec` items with prefixed names.
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
    for (server_name, client) in &snapshot {
        match client.discover().await {
            Ok(caps) => {
                for t in &caps.tools {
                    specs.push(ToolSpec {
                        name: mcp_tool_name(server_name, &t.name),
                        description: t
                            .description
                            .clone()
                            .unwrap_or_else(|| format!("MCP tool from server '{server_name}'")),
                        input_schema: t.input_schema.clone(),
                        risk: crate::tools::Risk::Medium,
                        required_permissions: Vec::new(),
                    });
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
/// Returns `None` if the tool name isn't an MCP-prefixed tool or the server
/// is no longer running.
pub async fn dispatch_mcp_tool(
    state: &McpState,
    prefixed_name: &str,
    args: &serde_json::Value,
) -> ToolResult {
    let (server_name, tool_name) = match parse_mcp_tool_name(prefixed_name) {
        Some(p) => p,
        None => {
            return ToolResult {
                ok: false,
                output: format!("not an MCP tool: {prefixed_name}"),
            };
        }
    };

    // Clone the client Arc under a short-lived read lock.
    let client = {
        let r = state.running.read().await;
        r.get(&server_name).cloned()
    };

    let client = match client {
        Some(c) => c,
        None => {
            return ToolResult {
                ok: false,
                output: format!("MCP server '{server_name}' is not running. Call /v1/mcp/servers/{server_name}/start first."),
            };
        }
    };

    match client.invoke_tool(&tool_name, args).await {
        Ok(val) => {
            // Convert the JSON-RPC result to a display string.
            let output = serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
            ToolResult { ok: true, output }
        }
        Err(e) => ToolResult {
            ok: false,
            output: format!("MCP tool '{tool_name}' on '{server_name}' failed: {e}"),
        },
    }
}
