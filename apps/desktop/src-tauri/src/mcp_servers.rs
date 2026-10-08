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
    /// Environment variables the server runs with. The settings screen shows
    /// them back: a key typed once must stay checkable, not disappear.
    /// Tolerant by default: an answer from the daemon that predates this
    /// field must still deserialize.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Tools the server announced, once it has been started.
    pub tools: Vec<String>,
    /// Outils que la personne a interdits au modèle. Tolérant : une réponse du
    /// démon d'avant ce champ doit se lire quand même.
    #[serde(default)]
    pub disabled_tools: Vec<String>,
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

/// The name becomes part of every tool name the model sees
/// (`mcp__<serveur>__<outil>`), so a space or a separator there would produce
/// tools nobody can call.
fn validate_server_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Donnez un nom à ce serveur.".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Le nom « {name} » ne peut contenir que des lettres, des chiffres, « - » et « _ »."
        ));
    }
    Ok(())
}

/// Ce qu'un bloc JSON collé va enregistrer, montré avant que rien ne soit écrit
/// ni lancé. Les valeurs des variables d'environnement et des en-têtes ne sont
/// jamais renvoyées : elles portent souvent une clé d'API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct McpServerPreview {
    pub name: String,
    pub transport: String,
    pub target: String,
    pub env_keys: Vec<String>,
    pub header_keys: Vec<String>,
    /// Un serveur du même nom est déjà enregistré : l'import serait refusé.
    pub exists: bool,
}

/// Résultat d'un import : les serveurs enregistrés, et pourquoi certains ne
/// démarrent pas quand on a demandé de les lancer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ImportMcpResult {
    /// Tous les connecteurs, pour rafraîchir l'écran d'un coup.
    pub servers: Vec<McpServerInfo>,
    /// Les noms de ceux que cet import vient d'ajouter.
    pub imported: Vec<String>,
    pub errors: Vec<String>,
}

/// Les clés d'un objet JSON dont toutes les valeurs sont du texte (ou un nombre
/// ou un booléen, que les fichiers d'exemple écrivent sans guillemets).
fn string_map(
    v: Option<&serde_json::Value>,
    serveur: &str,
    champ: &str,
) -> Result<HashMap<String, String>, String> {
    let Some(v) = v else {
        return Ok(HashMap::new());
    };
    let obj = v
        .as_object()
        .ok_or_else(|| format!("« {serveur} » : « {champ} » doit être un objet."))?;
    obj.iter()
        .map(|(k, val)| match val {
            serde_json::Value::String(s) => Ok((k.clone(), s.clone())),
            serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
                Ok((k.clone(), val.to_string()))
            }
            _ => Err(format!(
                "« {serveur} » : la valeur de « {k} » dans « {champ} » doit être du texte."
            )),
        })
        .collect()
}

fn string_list(v: Option<&serde_json::Value>, serveur: &str) -> Result<Vec<String>, String> {
    let Some(v) = v else {
        return Ok(Vec::new());
    };
    let arr = v
        .as_array()
        .ok_or_else(|| format!("« {serveur} » : « args » doit être une liste."))?;
    arr.iter()
        .map(|a| {
            a.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("« {serveur} » : chaque argument doit être du texte."))
        })
        .collect()
}

/// Une entrée `{ "command": …, "args": […] }` ou `{ "url": … }`, comme l'écrivent
/// Claude Code, Cursor, VS Code et Antigravity.
fn entry_from_json(name: &str, v: &serde_json::Value) -> Result<McpServerEntry, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| format!("« {name} » : une entrée doit être un objet."))?;
    let command = obj.get("command").and_then(|c| c.as_str()).map(str::trim);
    let url = obj
        .get("url")
        .or_else(|| obj.get("serverUrl"))
        .and_then(|u| u.as_str())
        .map(str::trim);
    let env = string_map(obj.get("env"), name, "env")?;
    match (
        command.filter(|c| !c.is_empty()),
        url.filter(|u| !u.is_empty()),
    ) {
        (Some(command), _) => Ok(McpServerEntry {
            command: Some(command.to_string()),
            args: string_list(obj.get("args"), name)?,
            env,
            url: None,
            headers: HashMap::new(),
            transport: Transport::Stdio,
            auto_start: false,
            scope: None,
            owner: None,
            disabled_tools: Vec::new(),
        }),
        (None, Some(url)) => {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(format!(
                    "« {name} » : l'adresse doit commencer par http:// ou https://."
                ));
            }
            Ok(McpServerEntry {
                command: None,
                args: Vec::new(),
                env,
                url: Some(url.to_string()),
                headers: string_map(obj.get("headers"), name, "headers")?,
                transport: Transport::Http,
                auto_start: false,
                scope: None,
                owner: None,
                disabled_tools: Vec::new(),
            })
        }
        (None, None) => Err(format!(
            "« {name} » : ni « command » ni « url ». Il faut l'un des deux."
        )),
    }
}

fn is_entry_like(v: &serde_json::Value) -> bool {
    v.as_object().is_some_and(|o| {
        ["command", "url", "serverUrl"]
            .iter()
            .any(|k| o.contains_key(*k))
    })
}

/// Lire un bloc collé depuis les instructions d'un serveur.
///
/// Trois formes courantes : `{ "mcpServers": { nom: entrée } }` (ou `servers`,
/// chez VS Code), une table `{ nom: entrée }` sans enveloppe, et une entrée
/// seule `{ "command": … }` — alors le nom vient du champ prévu pour lui.
fn parse_mcp_json(
    text: &str,
    fallback_name: &str,
) -> Result<Vec<(String, McpServerEntry)>, String> {
    let root: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|e| format!("Ce texte n'est pas du JSON valide : {e}"))?;
    let obj = root
        .as_object()
        .ok_or("Collez un objet JSON, par exemple { \"mcpServers\": { … } }.")?;
    let table = obj
        .get("mcpServers")
        .or_else(|| obj.get("servers"))
        .and_then(|t| t.as_object())
        .or_else(|| (!obj.is_empty() && obj.values().all(is_entry_like)).then_some(obj));
    let mut out = Vec::new();
    if let Some(table) = table {
        for (name, v) in table {
            let name = name.trim().to_string();
            validate_server_name(&name)?;
            out.push((name.clone(), entry_from_json(&name, v)?));
        }
    } else if is_entry_like(&root) {
        let name = fallback_name.trim().to_string();
        if name.is_empty() {
            return Err(
                "Ce JSON décrit un seul serveur sans nom : remplissez le champ « Nom ».".into(),
            );
        }
        validate_server_name(&name)?;
        out.push((name.clone(), entry_from_json(&name, &root)?));
    }
    if out.is_empty() {
        return Err("Aucun serveur trouvé dans ce JSON : cherchez un bloc « mcpServers ».".into());
    }
    Ok(out)
}

fn preview_of(name: &str, e: &McpServerEntry, exists: bool) -> McpServerPreview {
    let mut env_keys: Vec<String> = e.env.keys().cloned().collect();
    let mut header_keys: Vec<String> = e.headers.keys().cloned().collect();
    env_keys.sort();
    header_keys.sort();
    McpServerPreview {
        name: name.to_string(),
        transport: match e.transport {
            Transport::Stdio => "stdio".into(),
            Transport::Http => "http".into(),
        },
        target: entry_target(e),
        env_keys,
        header_keys,
        exists,
    }
}

/// Montrer ce qu'un JSON collé enregistrerait, sans rien écrire ni lancer.
///
/// C'est l'étape de relecture : un bloc copié depuis une page web contient une
/// commande qui s'exécutera sur cette machine, et la personne doit la voir telle
/// qu'elle sera lancée avant de l'approuver.
#[tauri::command]
pub async fn preview_mcp_json(
    core: State<'_, Core>,
    text: String,
    name: Option<String>,
) -> Result<Vec<McpServerPreview>, String> {
    let parsed = parse_mcp_json(&text, name.as_deref().unwrap_or(""))?;
    let known = known_server_names(&core).await;
    Ok(parsed
        .iter()
        .map(|(n, e)| preview_of(n, e, known.contains(n)))
        .collect())
}

async fn known_server_names(core: &Core) -> std::collections::HashSet<String> {
    if let Some(client) = core.remote_client() {
        if let Ok(val) = client.list_mcp_servers().await {
            if let Ok(infos) = serde_json::from_value::<Vec<McpServerInfo>>(val) {
                return infos.into_iter().map(|i| i.name).collect();
            }
        }
    }
    core.mcp
        .config
        .lock()
        .unwrap()
        .mcp_servers
        .keys()
        .cloned()
        .collect()
}

/// Une ligne de commande que `split_command` relira à l'identique, pour un
/// serveur distant qui ne reçoit qu'une chaîne.
fn join_command(command: &str, args: &[String]) -> Result<String, String> {
    let mut parts = vec![command.to_string()];
    for a in args {
        let quoted = if a.is_empty() {
            return Err("Un argument vide ne peut pas être transmis à un serveur distant.".into());
        } else if !a.contains(char::is_whitespace) && !a.contains(['"', '\'']) {
            a.clone()
        } else if !a.contains('"') {
            format!("\"{a}\"")
        } else if !a.contains('\'') {
            format!("'{a}'")
        } else {
            return Err(format!(
                "L'argument « {a} » ne peut pas être transmis tel quel."
            ));
        };
        parts.push(quoted);
    }
    Ok(parts.join(" "))
}

/// Enregistrer sur un serveur Locaryn distant, qui ne reçoit que
/// nom, transport, cible et environnement.
async fn register_remote(
    client: &locaryn_sdk::LocarynClient,
    name: &str,
    e: &McpServerEntry,
) -> Result<(), String> {
    let (transport, target) = match e.transport {
        Transport::Stdio => (
            "stdio",
            join_command(e.command.as_deref().unwrap_or(""), &e.args)?,
        ),
        Transport::Http => {
            if !e.headers.is_empty() {
                return Err(format!(
                    "« {name} » : un serveur Locaryn distant n'accepte pas d'en-têtes personnalisés."
                ));
            }
            ("http", e.url.clone().unwrap_or_default())
        }
    };
    let env = serde_json::to_value(&e.env).map_err(|x| x.to_string())?;
    client
        .register_mcp_server(name, transport, &target, env, false)
        .await
        .map(|_| ())
        .map_err(|x| format!("« {name} » : {x}"))
}

/// Enregistrer les serveurs d'un JSON collé, après relecture.
///
/// Tout ou rien : si un nom existe déjà, rien n'est écrit. Les serveurs ne
/// démarrent pas d'eux-mêmes ; `start` ne vaut que si la personne l'a coché
/// dans l'écran de relecture.
#[tauri::command]
pub async fn import_mcp_json(
    core: State<'_, Core>,
    text: String,
    name: Option<String>,
    start: bool,
) -> Result<ImportMcpResult, String> {
    let parsed = parse_mcp_json(&text, name.as_deref().unwrap_or(""))?;
    let known = known_server_names(&core).await;
    if let Some((n, _)) = parsed.iter().find(|(n, _)| known.contains(n)) {
        return Err(format!(
            "« {n} » existe déjà. Retirez-le d'abord, ou renommez-le dans le JSON."
        ));
    }
    if let Some(client) = core.remote_client() {
        for (n, e) in &parsed {
            register_remote(&client, n, e).await?;
        }
    } else {
        {
            let mut cfg = core.mcp.config.lock().unwrap();
            for (n, e) in &parsed {
                cfg.mcp_servers.insert(n.clone(), e.clone());
            }
        }
        core.mcp.save();
    }
    tracing::info!(
        count = parsed.len(),
        "serveurs MCP importés depuis un JSON collé"
    );

    let mut errors = Vec::new();
    if start {
        for (n, _) in &parsed {
            if let Err(e) = start_mcp_server(core.clone(), n.clone()).await {
                errors.push(e);
            }
        }
    }
    Ok(ImportMcpResult {
        servers: list_mcp_servers(core).await?,
        imported: parsed.into_iter().map(|(n, _)| n).collect(),
        errors,
    })
}

/// Un outil d'un connecteur, avec ce que la personne peut en décider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    /// Faux quand la personne l'a décoché : le modèle ne le voit pas.
    pub enabled: bool,
    /// Le serveur dit que l'outil ne modifie rien.
    pub read_only: bool,
    /// Le serveur dit que l'outil peut détruire ou écraser quelque chose.
    pub destructive: bool,
}

fn tool_hint(t: &locaryn_mcp::ToolDescriptor, key: &str) -> bool {
    t.annotations
        .as_ref()
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

const REMOTE_TOOLS_UNSUPPORTED: &str =
    "Le choix des outils se règle sur la machine qui héberge le connecteur : il n'est pas disponible depuis une connexion à un serveur distant.";

/// Les outils qu'un connecteur démarré annonce, avec leur état.
///
/// Il doit tourner : la liste vient du serveur lui-même, et la deviner serait la
/// donner fausse. L'écran propose de le démarrer quand il est arrêté.
#[tauri::command]
pub async fn list_mcp_tools(
    core: State<'_, Core>,
    name: String,
) -> Result<Vec<McpToolInfo>, String> {
    if core.remote_client().is_some() {
        return Err(REMOTE_TOOLS_UNSUPPORTED.into());
    }
    let client = core
        .mcp
        .running
        .read()
        .await
        .get(&name)
        .cloned()
        .ok_or_else(|| {
            format!("« {name} » n'est pas démarré : démarrez-le pour voir ses outils.")
        })?;
    let caps = client
        .discover()
        .await
        .map_err(|e| format!("{name} n'a pas répondu : {e}"))?;
    let mut tools: Vec<McpToolInfo> = caps
        .tools
        .iter()
        .filter(|t| !locaryn_agent_runtime::mcp_tools::is_ui_only_tool(&t.name))
        .map(|t| McpToolInfo {
            enabled: core.mcp.tool_allowed(&name, &t.name),
            read_only: tool_hint(t, "readOnlyHint"),
            destructive: tool_hint(t, "destructiveHint"),
            description: t.description.clone(),
            name: t.name.clone(),
        })
        .collect();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tools)
}

/// Remplacer la liste des outils interdits au modèle pour un connecteur.
///
/// La liste entière plutôt qu'un outil à la fois : l'écran la détient, et un
/// « tout désactiver » est un seul geste, pas quarante.
#[tauri::command]
pub async fn set_mcp_disabled_tools(
    core: State<'_, Core>,
    name: String,
    disabled: Vec<String>,
) -> Result<(), String> {
    if core.remote_client().is_some() {
        return Err(REMOTE_TOOLS_UNSUPPORTED.into());
    }
    {
        let mut cfg = core.mcp.config.lock().unwrap();
        let entry = cfg
            .mcp_servers
            .get_mut(&name)
            .ok_or_else(|| format!("« {name} » n'est pas enregistré."))?;
        let mut list = disabled;
        list.sort();
        list.dedup();
        entry.disabled_tools = list;
    }
    core.mcp.save();
    tracing::info!(server = %name, "outils interdits au modèle mis à jour");
    Ok(())
}

/// Le réglage « Alléger les outils » : désactivé par défaut, le modèle reçoit
/// tous les outils des connecteurs actifs.
#[tauri::command]
pub async fn outils_alleges() -> Result<bool, String> {
    Ok(locaryn_config::load(None)
        .map(|c| c.assistance.trim_tools)
        .unwrap_or(false))
}

#[tauri::command]
pub async fn definir_outils_alleges(actif: bool) -> Result<bool, String> {
    locaryn_config::set_global("assistance", serde_json::json!({ "trim_tools": actif }))
        .map_err(|e| e.to_string())?;
    Ok(actif)
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
            env: e.env,
            tools,
            disabled_tools: e.disabled_tools.clone(),
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
    validate_server_name(&name)?;
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
                disabled_tools: Vec::new(),
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
                disabled_tools: Vec::new(),
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
    remember_auto_start(&core.mcp, &name, true);
    tracing::info!(server = %name, tools = tools.len(), "serveur MCP démarré");
    Ok(tools)
}

/// Démarrer ou arrêter un connecteur, c'est dire s'il doit tourner : le choix
/// vaut aussi pour les lancements suivants. Sans cela, un connecteur démarré à
/// la main disparaissait au redémarrage de l'application, et le modèle
/// répondait qu'il n'avait aucun de ses outils.
fn remember_auto_start(state: &McpState, name: &str, value: bool) {
    let changed = {
        let mut cfg = state.config.lock().unwrap();
        match cfg.mcp_servers.get_mut(name) {
            Some(entry) if entry.auto_start != value => {
                entry.auto_start = value;
                true
            }
            _ => false,
        }
    };
    if changed {
        state.save();
    }
}

#[tauri::command]
pub async fn stop_mcp_server(core: State<'_, Core>, name: String) -> Result<(), String> {
    if let Some(client) = core.remote_client() {
        let _ = client.stop_mcp(&name).await;
        return Ok(());
    }
    remember_auto_start(&core.mcp, &name, false);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_bloc_mcp_servers_de_claude_code_est_lu_tel_quel() {
        let json = r#"{ "mcpServers": {
            "fs": { "command": "npx", "args": ["-y", "@scope/server", "C:/Program Files/x"],
                    "env": { "TOKEN": "abc" } },
            "web": { "url": "https://exemple.com/mcp", "headers": { "Authorization": "Bearer x" } }
        } }"#;
        let mut v = parse_mcp_json(json, "").unwrap();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(v[0].0, "fs");
        // Les arguments sont conservés un par un, espaces compris.
        assert_eq!(v[0].1.args, ["-y", "@scope/server", "C:/Program Files/x"]);
        assert_eq!(v[0].1.env["TOKEN"], "abc");
        assert!(matches!(v[0].1.transport, Transport::Stdio));
        assert_eq!(v[1].1.url.as_deref(), Some("https://exemple.com/mcp"));
        assert_eq!(v[1].1.headers["Authorization"], "Bearer x");
        // Rien ne démarre de soi-même.
        assert!(v.iter().all(|(_, e)| !e.auto_start));
    }

    #[test]
    fn une_entree_seule_prend_son_nom_du_champ_dedie() {
        let json = r#"{ "command": "uvx", "args": ["server"] }"#;
        assert!(parse_mcp_json(json, "").is_err());
        let v = parse_mcp_json(json, "mon-serveur").unwrap();
        assert_eq!(v[0].0, "mon-serveur");
    }

    #[test]
    fn une_table_sans_enveloppe_et_la_cle_servers_de_vs_code_sont_acceptees() {
        let nue = r#"{ "a": { "url": "http://localhost:3000/mcp" } }"#;
        assert_eq!(parse_mcp_json(nue, "").unwrap().len(), 1);
        let vscode = r#"{ "servers": { "b": { "command": "node", "args": ["s.js"] } } }"#;
        assert_eq!(parse_mcp_json(vscode, "").unwrap()[0].0, "b");
    }

    #[test]
    fn les_erreurs_disent_quoi_corriger() {
        assert!(parse_mcp_json("pas du json", "")
            .unwrap_err()
            .contains("JSON"));
        assert!(parse_mcp_json("{}", "").unwrap_err().contains("mcpServers"));
        let sans_cible = r#"{ "mcpServers": { "x": { "args": [] } } }"#;
        assert!(parse_mcp_json(sans_cible, "")
            .unwrap_err()
            .contains("command"));
        let mauvais_nom = r#"{ "mcpServers": { "mon serveur": { "command": "a" } } }"#;
        assert!(parse_mcp_json(mauvais_nom, "")
            .unwrap_err()
            .contains("mon serveur"));
        let ftp = r#"{ "mcpServers": { "x": { "url": "ftp://h" } } }"#;
        assert!(parse_mcp_json(ftp, "").unwrap_err().contains("http"));
        let args_faux = r#"{ "mcpServers": { "x": { "command": "a", "args": [1] } } }"#;
        assert!(parse_mcp_json(args_faux, "").is_err());
    }

    #[test]
    fn la_relecture_ne_montre_jamais_les_valeurs_secretes() {
        let json =
            r#"{ "mcpServers": { "x": { "command": "a", "env": { "API_KEY": "sk-secret" } } } }"#;
        let (n, e) = parse_mcp_json(json, "").unwrap().remove(0);
        let p = serde_json::to_string(&preview_of(&n, &e, false)).unwrap();
        assert!(p.contains("API_KEY"));
        assert!(!p.contains("sk-secret"));
    }

    #[test]
    fn une_ligne_de_commande_jointe_se_relit_a_l_identique() {
        let args = vec!["-y".to_string(), "C:/Program Files/x".to_string()];
        let line = join_command("npx", &args).unwrap();
        assert_eq!(split_command(&line), ("npx".to_string(), args));
        assert!(join_command("a", &[String::new()]).is_err());
    }
}
