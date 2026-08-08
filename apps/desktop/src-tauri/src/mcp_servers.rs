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

use lochor_mcp::{build_client, McpClient, McpServerEntry, McpState, Transport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::State;

use crate::Core;

#[derive(Debug, Clone, Serialize)]
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

fn entry_target(e: &McpServerEntry) -> String {
    match e.transport {
        Transport::Stdio => {
            let mut parts = vec![e.command.clone().unwrap_or_default()];
            parts.extend(e.args.clone());
            parts.join(" ")
        }
        Transport::Http => e.url.clone().unwrap_or_default(),
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
    let entry = {
        let cfg = core.mcp.config.lock().unwrap();
        cfg.mcp_servers.get(&name).cloned()
    }
    .ok_or_else(|| format!("« {name} » n'est pas enregistré."))?;

    let client: Arc<dyn McpClient> = Arc::from(build_client(&entry));

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
        let client: Arc<dyn McpClient> = Arc::from(build_client(&entry));
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
        let client: Arc<dyn McpClient> = Arc::from(build_client(&entry));
        match client.discover().await {
            Ok(caps) => {
                tracing::info!(server = %name, tools = caps.tools.len(), "serveur MCP démarré automatiquement");
                state.running.write().await.insert(name, client);
            }
            Err(e) => tracing::warn!(server = %name, error = %e, "démarrage automatique échoué"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SnapMcpCheck {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: String,
    pub value: Option<String>,
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapMcpDiagnostics {
    pub checked_at: String,
    pub checks: Vec<SnapMcpCheck>,
}

fn check(
    checks: &mut Vec<SnapMcpCheck>,
    id: impl Into<String>,
    label: impl Into<String>,
    status: &str,
    detail: impl Into<String>,
    value: Option<String>,
    fix: Option<String>,
) {
    checks.push(SnapMcpCheck {
        id: id.into(),
        label: label.into(),
        status: status.into(),
        detail: detail.into(),
        value,
        fix,
    });
}

fn probe_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(if stderr.is_empty() { stdout } else { stderr });
    }
    Ok(if stdout.is_empty() { stderr } else { stdout })
}

fn first_line(value: &str) -> String {
    value.lines().next().unwrap_or_default().trim().to_string()
}

fn has_playwright_chromium(root: &Path) -> bool {
    std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| {
            entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && entry.file_name().to_string_lossy().starts_with("chromium-")
        })
}

fn playwright_browser_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        roots.push(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(path).join("ms-playwright"));
    }
    if let Some(path) = std::env::var_os("USERPROFILE") {
        roots.push(PathBuf::from(path).join("AppData/Local/ms-playwright"));
    }
    if let Some(path) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(path).join(".cache/ms-playwright"));
    }
    roots
}

fn extension_roots(records: &[lochor_storage::repos::ExtensionRecord]) -> Vec<PathBuf> {
    records
        .iter()
        .filter(|r| r.name.to_lowercase().contains("snap"))
        .filter_map(|r| Path::new(&r.manifest_path).parent().map(PathBuf::from))
        .collect()
}

fn configured_value(roots: &[PathBuf], key: &str) -> Option<String> {
    for root in roots {
        let path = root.join(".data/config.json");
        let Ok(raw) = std::fs::read_to_string(path) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };
        let value = json.get(key).and_then(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_i64().map(|number| number.to_string()))
        });
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn telegram_session_path(roots: &[PathBuf]) -> Option<PathBuf> {
    let configured = std::env::var("TELEGRAM_SESSION_FILE")
        .ok()
        .or_else(|| std::env::var("SNAP_ASTREINTE_TELEGRAM_SESSION_FILE").ok())
        .or_else(|| configured_value(roots, "transport.telegram_session_file"));
    if let Some(value) = configured {
        let path = PathBuf::from(value);
        if path.is_absolute() { Some(path) } else { Some(std::env::current_dir().unwrap_or_default().join(path)) }
    } else {
        let mut candidates = roots.iter().map(|r| r.join(".telegram/session.txt")).collect::<Vec<_>>();
        candidates.push(std::env::current_dir().unwrap_or_default().join(".telegram/session.txt"));
        candidates.into_iter().find(|p| p.is_file())
    }
}

/// Check every local prerequisite without invoking a shell or changing files.
/// MCP discovery is performed through the same client implementation used by
/// the agent, so a green server check is meaningful rather than cosmetic.
#[tauri::command]
pub async fn diagnose_snapmcp(core: State<'_, Core>) -> Result<SnapMcpDiagnostics, String> {
    let mut checks = Vec::new();
    let records = core.storage.extensions.list().await.map_err(|e| e.to_string())?;
    let roots = extension_roots(&records);

    match probe_command("node", &["--version"]) {
        Ok(version) => check(&mut checks, "node", "Node.js", "ok", "Node.js est disponible.", Some(first_line(&version)), None),
        Err(error) => check(&mut checks, "node", "Node.js", "error", format!("Node.js introuvable : {error}"), None, Some("Installer Node.js 20 ou plus récent.".into())),
    }
    match probe_command("ffmpeg", &["-version"]) {
        Ok(version) => check(&mut checks, "ffmpeg", "ffmpeg", "ok", "Conversion audio disponible.", Some(first_line(&version)), None),
        Err(error) => check(&mut checks, "ffmpeg", "ffmpeg", "error", format!("ffmpeg introuvable : {error}"), None, Some("Installer ffmpeg et l'ajouter au PATH.".into())),
    }

    let adb_available = match probe_command("adb", &["version"]) {
        Ok(version) => {
            check(&mut checks, "adb", "Android Debug Bridge", "ok", "adb est disponible.", Some(first_line(&version)), None);
            true
        }
        Err(error) => {
            check(&mut checks, "adb", "Android Debug Bridge", "error", format!("adb introuvable : {error}"), None, Some("Installer Android Platform Tools.".into()));
            false
        }
    };
    if adb_available {
        match probe_command("adb", &["devices"]) {
            Ok(output) => {
                let devices: Vec<&str> = output.lines().skip(1).filter(|line| !line.trim().is_empty()).collect();
                let unauthorized = devices.iter().any(|line| line.contains("unauthorized"));
                let online = devices.iter().filter(|line| line.ends_with("device")).count();
                if unauthorized {
                    check(&mut checks, "android_device", "Téléphone Android", "warning", "Téléphone détecté mais autorisation USB manquante.", None, Some("Déverrouiller le téléphone et accepter la clé RSA ADB.".into()));
                } else if online > 0 {
                    check(&mut checks, "android_device", "Téléphone Android", "ok", format!("{online} téléphone(s) prêt(s)."), Some(devices.join(" | ")), None);
                } else {
                    check(&mut checks, "android_device", "Téléphone Android", "warning", "Aucun téléphone Android prêt.", None, Some("Activer le débogage USB puis relancer adb devices.".into()));
                }
            }
            Err(error) => check(&mut checks, "android_device", "Téléphone Android", "error", error, None, Some("Reconnecter le téléphone et relancer adb devices.".into())),
        }
    }

    let mut browser_roots = playwright_browser_roots();
    for root in &roots {
        browser_roots.push(root.join("node_modules/playwright-core/.local-browsers"));
        browser_roots.push(root.join("node_modules/playwright/.local-browsers"));
    }
    let chromium_root = browser_roots.into_iter().find(|root| has_playwright_chromium(root));
    if let Some(root) = chromium_root {
        check(&mut checks, "chromium", "Chromium Playwright", "ok", "Chromium Playwright est installé.", Some(root.display().to_string()), None);
    } else {
        check(&mut checks, "chromium", "Chromium Playwright", "error", "Chromium Playwright introuvable.", None, Some("Exécuter npx playwright install chromium dans l'extension.".into()));
    }

    let session = telegram_session_path(&roots);
    let api_id = std::env::var("TELEGRAM_API_ID").ok().or_else(|| configured_value(&roots, "transport.telegram_api_id"));
    let api_hash = std::env::var("TELEGRAM_API_HASH").ok().or_else(|| configured_value(&roots, "transport.telegram_api_hash"));
    match session {
        Some(path) => check(&mut checks, "telegram_session", "Session Telegram", "ok", "Fichier de session Telegram trouvé.", Some(path.display().to_string()), None),
        None => check(&mut checks, "telegram_session", "Session Telegram", "error", "Aucune session Telegram trouvée.", None, Some("Lancer npm run telegram:login après avoir configuré api_id et api_hash.".into())),
    }
    if api_id.as_deref().is_some_and(|v| !v.trim().is_empty()) && api_hash.as_deref().is_some_and(|v| !v.trim().is_empty()) {
        check(&mut checks, "telegram_credentials", "Identifiants Telegram", "ok", "api_id et api_hash sont configurés.", None, None);
    } else {
        check(&mut checks, "telegram_credentials", "Identifiants Telegram", "warning", "api_id ou api_hash manque.", None, Some("Les créer dans API development tools sur my.telegram.org.".into()));
    }

    let entries: Vec<(String, McpServerEntry)> = {
        let cfg = core.mcp.config.lock().unwrap();
        cfg.mcp_servers.iter().map(|(name, entry)| (name.clone(), entry.clone())).collect()
    };
    if entries.is_empty() {
        check(&mut checks, "mcp_servers", "Serveurs MCP", "warning", "Aucun serveur MCP enregistré.", None, Some("Installer ou enregistrer l'extension SnapMCP.".into()));
    } else {
        for (name, entry) in entries {
            let running = core.mcp.running.read().await.get(&name).cloned();
            let (client, temporary) = match running {
                Some(client) => (client, false),
                None => (Arc::from(build_client(&entry)), true),
            };
            match client.discover().await {
                Ok(caps) => check(&mut checks, format!("mcp:{name}"), format!("Serveur MCP {name}"), "ok", format!("Serveur joignable, {} outil(s) découvert(s).", caps.tools.len()), Some(entry_target(&entry)), None),
                Err(error) => check(&mut checks, format!("mcp:{name}"), format!("Serveur MCP {name}"), "error", format!("Serveur non joignable : {error}"), Some(entry_target(&entry)), Some("Vérifier la commande, l'URL et les permissions de l'extension.".into())),
            }
            if temporary { let _ = client.shutdown().await; }
        }
    }

    Ok(SnapMcpDiagnostics {
        checked_at: chrono::Utc::now().to_rfc3339(),
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_path_with_spaces_stays_one_argument() {
        let (cmd, args) =
            split_command(r#""C:/Program Files/graphify/serve.exe" --graph mon-projet"#);
        assert_eq!(cmd, "C:/Program Files/graphify/serve.exe");
        assert_eq!(args, vec!["--graph", "mon-projet"]);
    }

    #[test]
    fn the_usual_npx_line_splits_as_expected() {
        let (cmd, args) =
            split_command("npx -y @modelcontextprotocol/server-filesystem D:/Documents");
        assert_eq!(cmd, "npx");
        assert_eq!(
            args,
            vec![
                "-y",
                "@modelcontextprotocol/server-filesystem",
                "D:/Documents"
            ]
        );
    }

    #[test]
    fn extra_spaces_do_not_become_empty_arguments() {
        // An empty argument reaches the server as a real, meaningless one.
        let (cmd, args) = split_command("  uvx    graphify-mcp   ");
        assert_eq!(cmd, "uvx");
        assert_eq!(args, vec!["graphify-mcp"]);
        let (cmd, args) = split_command("");
        assert_eq!(cmd, "");
        assert!(args.is_empty());
    }

    #[test]
    fn the_target_shown_back_is_what_was_typed() {
        // The settings list must echo the command, not a reconstruction that
        // silently differs from it.
        let (command, args) = split_command("npx -y @scope/server --flag");
        let e = McpServerEntry {
            command: Some(command),
            args,
            env: HashMap::new(),
            url: None,
            headers: HashMap::new(),
            transport: Transport::Stdio,
            auto_start: false,
            scope: None,
            owner: None,
        };
        assert_eq!(entry_target(&e), "npx -y @scope/server --flag");
    }
}
