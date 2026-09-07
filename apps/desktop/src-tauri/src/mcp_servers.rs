//! MCP servers, from the application rather than from a text editor.
//!
//! The protocol client already existed and the daemon already used it; the
//! application did not. Anything registered here lands in the same
//! `mcp.json` the daemon reads, in the format Claude Code and Cursor use, so
//! a server added on one side is visible from the other.
//!
//! Starting a server also *discovers* it immediately. The transport is lazy —
//! the subprocess only spawns on first use — which would otherwise turn a
//! mistyped command into a chat that silently has no tools, half an hour
//! later, with nothing pointing at the cause.

use locaryn_mcp::{McpClient, McpServerEntry, McpState, Transport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

use crate::Core;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpServerInfo {
    pub name: String,
    /// "stdio" or "http".
    pub transport: String,
    /// The command line or the URL, whichever applies — what the user typed.
    pub target: String,
    pub running: bool,
    pub auto_start: bool,
    /// Tools the server announced, once it has been started.
    pub tools: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMcpServer {
    pub name: String,
    /// "stdio" or "http". Anything else is refused.
    pub transport: String,
    /// Full command line for stdio (`npx -y @scope/server /path`), or the URL
    /// for HTTP. One field because that is how the user thinks about it.
    pub target: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub auto_start: bool,
}

fn entry_target(entry: &McpServerEntry) -> String {
    match entry.transport {
        Transport::Stdio => {
            let mut parts = Vec::new();
            if let Some(cmd) = &entry.command {
                parts.push(cmd.clone());
            }
            parts.extend(entry.args.clone());
            parts.join(" ")
        }
        Transport::Http => entry.url.clone().unwrap_or_default(),
    }
}

/// Split a command line into program and arguments.
///
/// Quotes are honoured because paths contain spaces on Windows far more often
/// than not, and `"C:/Program Files/x/server.exe"` must not become two words.
fn split_command(line: &str) -> (String, Vec<String>) {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => cur.push(c),
            (None, '"') | (None, '\'') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    let mut it = out.into_iter();
    (it.next().unwrap_or_default(), it.collect())
}

#[tauri::command]
pub async fn list_mcp_servers(core: State<'_, Core>) -> Result<Vec<McpServerInfo>, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(val) = client.list_mcp_servers().await {
            if let Ok(infos) = serde_json::from_value::<Vec<McpServerInfo>>(val) {
                return Ok(infos);
            }
        }
    }
    let entries: Vec<(String, McpServerEntry)> = {
        let cfg = core.mcp.config.lock().unwrap();
        let mut v: Vec<_> = cfg
            .mcp_servers
            .iter()
            .map(|(n, e)| (n.clone(), e.clone()))
            .collect();
        // Stable order: the list is a settings screen, not a log.
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };

    let running = core.mcp.running.read().await;
    let mut out = Vec::with_capacity(entries.len());
    for (name, e) in entries {
        let client = running.get(&name).cloned();
        let tools = match client {
            Some(c) => c
                .discover()
                .await
                .map(|caps| caps.tools.into_iter().map(|t| t.name).collect())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        out.push(McpServerInfo {
            transport: match e.transport {
                Transport::Stdio => "stdio".into(),
                Transport::Http => "http".into(),
            },
            target: entry_target(&e),
            running: running.contains_key(&name),
            auto_start: e.auto_start,
            tools,
            name,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn add_mcp_server(
    core: State<'_, Core>,
    args: AddMcpServer,
) -> Result<Vec<McpServerInfo>, String> {
    if let Some(client) = core.remote_client() {
        let env_obj = serde_json::to_value(&args.env).unwrap_or_default();
        let _ = client
            .register_mcp_server(
                &args.name,
                &args.transport,
                &args.target,
                env_obj,
                args.auto_start,
            )
            .await;
        return list_mcp_servers(core).await;
    }
    let name = args.name.trim().to_string();
    if name.is_empty() {
        return Err("Donnez un nom à ce serveur.".into());
    }
    // The name becomes part of every tool name the model sees
    // (`mcp__<serveur>__<outil>`), so a space or a separator there would
    // produce tools nobody can call.
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Le nom ne peut contenir que des lettres, des chiffres, « - » et « _ ».".into(),
        );
    }
    let target = args.target.trim();
    if target.is_empty() {
        return Err("Indiquez la commande à lancer, ou l'adresse du serveur.".into());
    }

    let entry = match args.transport.as_str() {
        "stdio" => {
            let (command, cmd_args) = split_command(target);
            McpServerEntry {
                command: Some(command),
                args: cmd_args,
                env: args.env,
                url: None,
                headers: HashMap::new(),
                transport: Transport::Stdio,
                auto_start: args.auto_start,
                scope: None,
                owner: None,
            }
        }
        "http" => {
            if !target.starts_with("http://") && !target.starts_with("https://") {
                return Err("L'adresse doit commencer par http:// ou https://.".into());
            }
            McpServerEntry {
                command: None,
                args: Vec::new(),
                env: args.env,
                url: Some(target.to_string()),
                headers: HashMap::new(),
                transport: Transport::Http,
                auto_start: args.auto_start,
                scope: None,
                owner: None,
            }
        }
        other => return Err(format!("Transport inconnu : {other}")),
    };

    {
        let mut cfg = core.mcp.config.lock().unwrap();
        if cfg.mcp_servers.contains_key(&name) {
            return Err(format!("« {name} » existe déjà."));
        }
        cfg.mcp_servers.insert(name.clone(), entry);
    }
    core.mcp.save();
    tracing::info!(server = %name, "serveur MCP enregistré");

    list_mcp_servers(core).await
}

#[tauri::command]
pub async fn remove_mcp_server(
    core: State<'_, Core>,
    name: String,
) -> Result<Vec<McpServerInfo>, String> {
    if let Some(client) = core.remote_client() {
        let _ = client.unregister_mcp_server(&name).await;
        return list_mcp_servers(core).await;
    }
    if let Some(client) = core.mcp.running.write().await.remove(&name) {
        let _ = client.shutdown().await;
    }
    {
        let mut cfg = core.mcp.config.lock().unwrap();
        cfg.mcp_servers.remove(&name);
    }
    core.mcp.save();
    list_mcp_servers(core).await
}

/// Start a server and confirm it answers.
///
/// The returned tool list is the proof: a server that starts but announces
/// nothing is indistinguishable from one that failed, unless we say so.
#[tauri::command]
pub async fn start_mcp_server(core: State<'_, Core>, name: String) -> Result<Vec<String>, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(val) = client.start_mcp(&name).await {
            if let Some(tools) = val.get("tools").and_then(|t| t.as_array()) {
                let list: Vec<String> = tools
                    .iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect();
                return Ok(list);
            }
        }
    }
    let entry = {
        let cfg = core.mcp.config.lock().unwrap();
        cfg.mcp_servers.get(&name).cloned()
    }
    .ok_or_else(|| format!("« {name} » n'est pas enregistré."))?;

    let client: Arc<dyn McpClient> = Arc::from(core.mcp.build_client(&entry));

    // Discover before publishing it: a client left in the running map after a
    // failed handshake would be retried on every message of every chat.
    let caps = client.discover().await.map_err(|e| {
        let hint = match entry.transport {
            Transport::Stdio => "Vérifiez que la commande existe et se lance depuis un terminal.",
            Transport::Http => "Vérifiez que l'adresse est joignable.",
        };
        format!("{name} n'a pas répondu : {e}. {hint}")
    })?;

    let tools: Vec<String> = caps.tools.into_iter().map(|t| t.name).collect();
    core.mcp.running.write().await.insert(name.clone(), client);
    tracing::info!(server = %name, tools = tools.len(), "serveur MCP démarré");
    Ok(tools)
}

#[tauri::command]
pub async fn stop_mcp_server(core: State<'_, Core>, name: String) -> Result<(), String> {
    if let Some(client) = core.remote_client() {
        let _ = client.stop_mcp(&name).await;
        return Ok(());
    }
    if let Some(client) = core.mcp.running.write().await.remove(&name) {
        client.shutdown().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Invoke one tool through a registered MCP server. The test bench uses the
/// same client/runtime path as the agent, so a green result proves the actual
/// endpoint and not a second mock implementation.
#[tauri::command]
pub async fn invoke_mcp_tool(
    core: State<'_, Core>,
    name: String,
    tool: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let client = if let Some(client) = core.mcp.running.read().await.get(&name).cloned() {
        client
    } else {
        let entry = {
            let cfg = core.mcp.config.lock().unwrap();
            cfg.mcp_servers.get(&name).cloned()
        }
        .ok_or_else(|| format!("« {name} » n'est pas enregistré."))?;
        let client: Arc<dyn McpClient> = Arc::from(core.mcp.build_client(&entry));
        client
            .discover()
            .await
            .map_err(|e| format!("{name} n'a pas répondu : {e}. Vérifiez la commande ou l'URL."))?;
        core.mcp
            .running
            .write()
            .await
            .insert(name.clone(), client.clone());
        client
    };

    client
        .invoke_tool(&tool, &args)
        .await
        .map_err(|e| format!("outil {tool} sur {name} : {e}"))
}

/// Start every server the user marked as automatic.
///
/// Failures are logged, never fatal: a laptop that cannot reach one server
/// must still open.
pub async fn start_automatic(state: &McpState) {
    let entries: Vec<(String, McpServerEntry)> = {
        let cfg = state.config.lock().unwrap();
        cfg.mcp_servers
            .iter()
            .filter(|(_, e)| e.auto_start)
            .map(|(n, e)| (n.clone(), e.clone()))
            .collect()
    };
    for (name, entry) in entries {
        let client: Arc<dyn McpClient> = Arc::from(state.build_client(&entry));
        match client.discover().await {
            Ok(caps) => {
                tracing::info!(server = %name, tools = caps.tools.len(), "serveur MCP démarré automatiquement");
                state.running.write().await.insert(name, client);
            }
            Err(e) => tracing::warn!(server = %name, error = %e, "démarrage automatique échoué"),
        }
    }
}
