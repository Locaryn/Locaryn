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

use base64::Engine as _;
use locaryn_mcp::{build_client, McpClient, McpServerEntry, McpState, Transport};
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
#[serde(rename_all = "camelCase")]
pub struct AndroidVmStatus {
    pub sdk_root: Option<String>,
    pub sdkmanager: Option<String>,
    pub avdmanager: Option<String>,
    pub emulator: Option<String>,
    pub avds: Vec<String>,
    pub running_emulators: Vec<String>,
    pub recommended_avd: String,
    pub detail: String,
}

/// Lance une commande courte et rend sa sortie. Utilisé par les sondes
/// d'environnement : ce qui compte est ce que l'outil répond, pas son code
/// de retour seul — certains écrivent leur diagnostic sur stderr en
/// réussissant.
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

fn android_sdk_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT", "LOCARYN_ANDROID_SDK"] {
        if let Some(value) = std::env::var_os(key) {
            roots.push(PathBuf::from(value));
        }
    }
    if let Some(value) = std::env::var_os("USERPROFILE") {
        roots.push(PathBuf::from(value).join(".locaryn/android-sdk"));
    }
    if let Some(value) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(value).join(".locaryn/android-sdk"));
    }
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(value).join("Android/Sdk"));
    }
    if let Some(value) = std::env::var_os("USERPROFILE") {
        roots.push(PathBuf::from(value).join("AppData/Local/Android/Sdk"));
    }
    roots.push(PathBuf::from("C:/Android/sdk"));
    roots.push(PathBuf::from("/opt/android-sdk"));
    roots.push(PathBuf::from("/usr/local/android-sdk"));
    roots
}

fn find_android_tool(root: Option<&Path>, name: &str) -> Option<PathBuf> {
    let executable = if cfg!(windows) {
        if matches!(name, "adb" | "emulator") {
            format!("{name}.exe")
        } else {
            format!("{name}.bat")
        }
    } else {
        name.to_string()
    };
    if let Some(root) = root {
        let candidates = if name == "adb" {
            vec![root.join("platform-tools").join(&executable)]
        } else if name == "emulator" {
            vec![root.join("emulator").join(&executable)]
        } else {
            vec![
                root.join("cmdline-tools/latest/bin").join(&executable),
                root.join("cmdline-tools/bin").join(&executable),
                root.join("tools/bin").join(&executable),
            ]
        };
        if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
            return Some(path);
        }
    }
    Command::new(name)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|_| PathBuf::from(name))
}

fn android_vm_status() -> AndroidVmStatus {
    let sdk_root = android_sdk_roots().into_iter().find(|root| root.is_dir());
    let sdkmanager = find_android_tool(sdk_root.as_deref(), "sdkmanager");
    let avdmanager = find_android_tool(sdk_root.as_deref(), "avdmanager");
    let emulator = find_android_tool(sdk_root.as_deref(), "emulator");
    let avds = avdmanager
        .as_ref()
        .and_then(|tool| Command::new(tool).args(["list", "avd"]).output().ok())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.trim().strip_prefix("Name: "))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let running_emulators = probe_command("adb", &["devices"])
        .ok()
        .map(|output| {
            output
                .lines()
                .filter_map(|line| line.split_whitespace().next())
                .filter(|serial| serial.starts_with("emulator-"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let detail = if emulator.is_none() || sdkmanager.is_none() || avdmanager.is_none() {
        "Android SDK command-line tools incomplets.".to_string()
    } else if avds.is_empty() {
        "Émulateur disponible, aucune AVD Locaryn configurée.".to_string()
    } else {
        format!("{} AVD disponible(s).", avds.len())
    };
    AndroidVmStatus {
        sdk_root: sdk_root.map(|path| path.display().to_string()),
        sdkmanager: sdkmanager.map(|path| path.display().to_string()),
        avdmanager: avdmanager.map(|path| path.display().to_string()),
        emulator: emulator.map(|path| path.display().to_string()),
        avds,
        running_emulators,
        recommended_avd: "LocarynVM".into(),
        detail,
    }
}

#[tauri::command]
pub fn diagnose_android_vm() -> Result<AndroidVmStatus, String> {
    Ok(android_vm_status())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidVmSetupArgs {
    pub avd_name: Option<String>,
    pub api_level: Option<u32>,
    pub install_components: Option<bool>,
}

#[tauri::command]
pub fn setup_android_vm(args: AndroidVmSetupArgs) -> Result<AndroidVmStatus, String> {
    let before = android_vm_status();
    let sdk = before.sdk_root.as_ref().map(PathBuf::from).ok_or_else(|| {
        "Android SDK introuvable. Installe Android Studio ou les command-line tools.".to_string()
    })?;
    let sdkmanager = before
        .sdkmanager
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "sdkmanager introuvable dans Android SDK.".to_string())?;
    let avdmanager = before
        .avdmanager
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "avdmanager introuvable dans Android SDK.".to_string())?;
    let api = args.api_level.unwrap_or(35);
    let image = format!("system-images;android-{api};google_apis;x86_64");
    if args.install_components.unwrap_or(true) {
        let status = Command::new(&sdkmanager)
            .arg(format!("--sdk_root={}", sdk.display()))
            .args([
                "platform-tools",
                "emulator",
                &format!("platforms;android-{api}"),
                &image,
            ])
            .status()
            .map_err(|e| format!("Impossible de lancer sdkmanager : {e}"))?;
        if !status.success() {
            return Err(
                "sdkmanager a échoué. Accepte les licences Android puis relance l'installation."
                    .into(),
            );
        }
    }
    let avd = args.avd_name.unwrap_or_else(|| "LocarynVM".into());
    if !before.avds.iter().any(|name| name == &avd) {
        let mut child = Command::new(&avdmanager)
            .args(["create", "avd", "-n", &avd, "-k", &image, "-d", "pixel_2"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Impossible de lancer avdmanager : {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(b"no\n");
        }
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "Création AVD échouée : {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Ok(android_vm_status())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidVmStartArgs {
    pub avd_name: String,
    pub memory_mb: Option<u32>,
    pub camera: Option<String>,
    pub microphone: Option<String>,
}

#[tauri::command]
pub fn start_android_vm(args: AndroidVmStartArgs) -> Result<AndroidVmStatus, String> {
    let status = android_vm_status();
    let emulator = status
        .emulator
        .as_ref()
        .ok_or_else(|| "emulator introuvable. Configure d'abord la VM.".to_string())?;
    let mut command = Command::new(emulator);
    if let Some(sdk_root) = status.sdk_root.as_deref() {
        command.env("ANDROID_HOME", sdk_root);
        command.env("ANDROID_SDK_ROOT", sdk_root);
        command.env("LOCARYN_ANDROID_SDK", sdk_root);
        let separator = if cfg!(windows) { ";" } else { ":" };
        let bin = [
            PathBuf::from(sdk_root).join("platform-tools"),
            PathBuf::from(sdk_root).join("emulator"),
            PathBuf::from(sdk_root).join("cmdline-tools/latest/bin"),
        ];
        let mut path = bin
            .iter()
            .map(|item| item.display().to_string())
            .collect::<Vec<_>>()
            .join(separator);
        if let Some(existing) = std::env::var_os("PATH") {
            path.push_str(separator);
            path.push_str(&existing.to_string_lossy());
        }
        command.env("PATH", path);
    }
    command.args([
        "-avd",
        &args.avd_name,
        "-no-boot-anim",
        "-no-snapshot",
        "-accel",
        "auto",
    ]);
    command.args(["-memory", &args.memory_mb.unwrap_or(2048).to_string()]);
    if args.camera.as_deref() == Some("webcam0") {
        command.args(["-camera-front", "webcam0"]);
    }
    if args.microphone.as_deref() == Some("none") {
        command.arg("-no-audio");
    }
    command
        .spawn()
        .map_err(|e| format!("Impossible de démarrer la VM : {e}"))?;
    Ok(status)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidVmStopArgs {
    pub console_port: Option<u16>,
}

/// Stop through the Android Emulator console, not through the physical-phone ADB workflow.
#[tauri::command]
pub fn stop_android_vm(args: AndroidVmStopArgs) -> Result<AndroidVmStatus, String> {
    use std::io::Write;
    use std::net::TcpStream;
    let port = args.console_port.unwrap_or(5554);
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("Console de l'émulateur inaccessible sur {port} : {e}"))?;
    let token_path = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|path| path.join(".emulator_console_auth_token"));
    if let Some(path) = token_path {
        if let Ok(token) = std::fs::read_to_string(path) {
            let _ = stream.write_all(format!("auth {}\n", token.trim()).as_bytes());
        }
    }
    stream
        .write_all(b"kill\n")
        .map_err(|e| format!("Impossible d'arrêter l'émulateur : {e}"))?;
    Ok(android_vm_status())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidScreenProbe {
    pub serial: String,
    pub state: String,
    pub boot_completed: bool,
    pub display_size: Option<String>,
    pub screenshot_base64: String,
    pub ui_xml: String,
    pub ui_text: Vec<String>,
    pub ocr_text: Option<String>,
    pub ocr_available: bool,
    pub ocr_detail: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidScreenArgs {
    pub serial: Option<String>,
    #[serde(default)]
    pub ocr: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidScreenActionArgs {
    pub serial: Option<String>,
    pub action: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub x2: Option<i32>,
    pub y2: Option<i32>,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub ocr: bool,
}

fn android_adb() -> Result<PathBuf, String> {
    let root = android_sdk_roots().into_iter().find(|root| root.is_dir());
    find_android_tool(root.as_deref(), "adb").ok_or_else(|| {
        "ADB introuvable. Installe Platform Tools ou branche un appareil Android.".into()
    })
}

fn android_devices(adb: &Path) -> Result<Vec<(String, String)>, String> {
    let output = Command::new(adb)
        .args(["devices"])
        .output()
        .map_err(|e| format!("ADB inaccessible : {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect())
}

fn choose_android_serial(adb: &Path, requested: Option<&str>) -> Result<(String, String), String> {
    let devices = android_devices(adb)?;
    if let Some(serial) = requested.filter(|value| !value.trim().is_empty()) {
        if let Some((_, state)) = devices.iter().find(|(name, _)| name == serial) {
            if state == "device" {
                return Ok((serial.to_string(), state.clone()));
            }
            return Err(format!(
                "L'appareil Android {serial} est {state}, pas prêt."
            ));
        }
        return Err(format!("Appareil Android {serial} introuvable."));
    }
    devices
        .into_iter()
        .find(|(_, state)| state == "device")
        .ok_or_else(|| {
            "Aucun appareil Android prêt. Démarre une AVD ou autorise le téléphone.".into()
        })
}

fn adb_output(adb: &Path, serial: &str, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(adb)
        .args(["-s", serial])
        .args(args)
        .output()
        .map_err(|e| format!("ADB inaccessible : {e}"))
}

fn xml_texts(xml: &str) -> Vec<String> {
    let mut values = Vec::new();
    for key in ["text", "content-desc"] {
        let marker = format!("{key}=\"");
        let mut rest = xml;
        while let Some(start) = rest.find(&marker) {
            let value_start = start + marker.len();
            let tail = &rest[value_start..];
            let Some(end) = tail.find('\"') else { break };
            let value = tail[..end].trim();
            if !value.is_empty() && !values.iter().any(|item| item == value) {
                values.push(value.to_string());
            }
            rest = &tail[end + 1..];
        }
    }
    values
}

fn screen_coordinate(value: Option<i32>, label: &str) -> Result<String, String> {
    let value = value.ok_or_else(|| format!("Coordonnée {label} manquante."))?;
    if !(0..=10000).contains(&value) {
        return Err(format!("Coordonnée {label} hors limites."));
    }
    Ok(value.to_string())
}

fn screen_probe(args: AndroidScreenArgs) -> Result<AndroidScreenProbe, String> {
    let adb = android_adb()?;
    let (serial, state) = choose_android_serial(&adb, args.serial.as_deref())?;
    let screenshot = adb_output(&adb, &serial, &["exec-out", "screencap", "-p"])?;
    if !screenshot.status.success() || screenshot.stdout.is_empty() {
        return Err(format!(
            "Capture écran impossible : {}",
            String::from_utf8_lossy(&screenshot.stderr)
        ));
    }
    let dump = adb_output(
        &adb,
        &serial,
        &["shell", "uiautomator", "dump", "/sdcard/locaryn-window.xml"],
    )?;
    let ui_xml = if dump.status.success() {
        let xml_output = adb_output(
            &adb,
            &serial,
            &["shell", "cat", "/sdcard/locaryn-window.xml"],
        )?;
        String::from_utf8_lossy(&xml_output.stdout).to_string()
    } else {
        format!(
            "[UI XML indisponible : {}]",
            String::from_utf8_lossy(&dump.stderr).trim()
        )
    };
    let boot = adb_output(&adb, &serial, &["shell", "getprop", "sys.boot_completed"])?;
    let size = adb_output(&adb, &serial, &["shell", "wm", "size"])?;
    let size_text = String::from_utf8_lossy(&size.stdout).trim().to_string();
    let ocr_result = if args.ocr {
        let path =
            std::env::temp_dir().join(format!("locaryn-screen-{}.png", uuid::Uuid::new_v4()));
        let result = (|| {
            std::fs::write(&path, &screenshot.stdout).map_err(|e| e.to_string())?;
            let output = Command::new("tesseract")
                .args([path.to_string_lossy().as_ref(), "stdout"])
                .output();
            let value = match output {
                Ok(output) if output.status.success() => {
                    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
                }
                Ok(output) => {
                    return Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
                }
                Err(error) => return Err(error.to_string()),
            };
            Ok(value)
        })();
        let _ = std::fs::remove_file(&path);
        match result {
            Ok(text) => (text, true, "Tesseract disponible.".to_string()),
            Err(error) => (None, false, format!("OCR indisponible : {error}")),
        }
    } else {
        (
            None,
            Command::new("tesseract").arg("--version").output().is_ok(),
            "OCR non lancé ; l'analyse UI sémantique reste disponible.".to_string(),
        )
    };
    Ok(AndroidScreenProbe {
        serial,
        state,
        boot_completed: String::from_utf8_lossy(&boot.stdout).trim() == "1",
        display_size: (!size_text.is_empty()).then_some(size_text),
        screenshot_base64: base64::engine::general_purpose::STANDARD.encode(screenshot.stdout),
        ui_text: xml_texts(&ui_xml),
        ui_xml,
        ocr_text: ocr_result.0,
        ocr_available: ocr_result.1,
        ocr_detail: ocr_result.2,
    })
}

#[tauri::command]
pub fn android_screen_probe(args: AndroidScreenArgs) -> Result<AndroidScreenProbe, String> {
    screen_probe(args)
}

#[tauri::command]
pub fn android_screen_action(args: AndroidScreenActionArgs) -> Result<AndroidScreenProbe, String> {
    let adb = android_adb()?;
    let (serial, _) = choose_android_serial(&adb, args.serial.as_deref())?;
    let action = args.action.to_ascii_lowercase();
    let input_args: Vec<String> = match action.as_str() {
        "back" => vec!["keyevent".into(), "4".into()],
        "home" => vec!["keyevent".into(), "3".into()],
        "refresh" => {
            return screen_probe(AndroidScreenArgs {
                serial: Some(serial),
                ocr: args.ocr,
            })
        }
        "tap" => vec![
            "tap".into(),
            screen_coordinate(args.x, "X")?,
            screen_coordinate(args.y, "Y")?,
        ],
        "swipe" => vec![
            "swipe".into(),
            screen_coordinate(args.x, "X")?,
            screen_coordinate(args.y, "Y")?,
            screen_coordinate(args.x2, "X2")?,
            screen_coordinate(args.y2, "Y2")?,
            args.duration_ms.unwrap_or(400).min(10000).to_string(),
        ],
        _ => {
            return Err(
                "Action écran refusée. Actions disponibles : tap, swipe, back, home, refresh."
                    .into(),
            )
        }
    };
    let refs: Vec<&str> = input_args.iter().map(String::as_str).collect();
    let status = Command::new(&adb)
        .args(["-s", &serial, "shell", "input"])
        .args(&refs)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Commande de contrôle écran refusée par Android.".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    screen_probe(AndroidScreenArgs {
        serial: Some(serial),
        ocr: args.ocr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_xml_extracts_unique_text_and_content_descriptions() {
        let xml =
            r#"<node text="Accueil" content-desc="Accueil"/><node text="" content-desc="Retour"/>"#;
        assert_eq!(xml_texts(xml), vec!["Accueil", "Retour"]);
    }

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
