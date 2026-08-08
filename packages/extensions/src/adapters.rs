//! Turn a foreign bundle into a Locaryn plugin.
//!
//! Adaptation happens **in place**: we read whatever layout the bundle
//! actually has, then write a Locaryn `plugin.json` beside it pointing at the
//! files that are already there. Nothing is duplicated and nothing is thrown
//! away, so `plugin.json` can be regenerated at any time by re-running this.
//!
//! Three of the four ecosystems already agree with Locaryn on the important
//! parts — markdown with YAML frontmatter for skills/agents/commands, and the
//! `mcpServers` object for MCP. What differs is *where* those files live and
//! which variables they interpolate, and that is what this module normalises.
//!
//! Where a bundle carries host-specific executable code (an OpenCode
//! TypeScript plugin, a Gemini policy engine rule), we do not pretend to
//! support it: the file is left alone and a note explains what was skipped.

use crate::manifest::{Components, PermissionRequest, PermissionValue, PluginManifest};
use locaryn_shared_types::ExtensionEcosystem;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Locaryn's own manifest name. Claude Code puts its manifest one level down in
/// `.claude-plugin/`, which is how the two are told apart.
pub const LOCARYN_MANIFEST: &str = "plugin.json";
pub const CLAUDE_MANIFEST: &str = ".claude-plugin/plugin.json";
pub const CLAUDE_MARKETPLACE: &str = ".claude-plugin/marketplace.json";
pub const GEMINI_MANIFEST: &str = "gemini-extension.json";

#[derive(Debug, thiserror::Error)]
pub enum AdaptError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error in {file}: {source}")]
    Parse {
        file: String,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "no recognisable plugin found in {0} — expected plugin.json, \
             .claude-plugin/plugin.json, gemini-extension.json, opencode.json, \
             or a commands/agents/skills directory"
    )]
    Unrecognised(String),
}

/// The outcome of adapting a directory.
#[derive(Debug, Clone)]
pub struct AdaptReport {
    pub ecosystem: ExtensionEcosystem,
    pub manifest: PluginManifest,
    /// Human-readable notes: what was converted, and what was deliberately
    /// skipped. Surfaced in the UI so "it installed" never hides "…but the
    /// half you wanted needs a Node runtime we don't have".
    pub notes: Vec<String>,
    /// True when part of the bundle could not be represented in Locaryn.
    pub partial: bool,
}

/// Identify a bundle without modifying it.
pub fn detect(dir: &Path) -> Option<ExtensionEcosystem> {
    if dir.join(LOCARYN_MANIFEST).is_file() {
        return Some(ExtensionEcosystem::Locaryn);
    }
    if dir.join(CLAUDE_MANIFEST).is_file() {
        return Some(ExtensionEcosystem::ClaudeCode);
    }
    if dir.join(GEMINI_MANIFEST).is_file() {
        return Some(ExtensionEcosystem::GeminiCli);
    }
    if dir.join("opencode.json").is_file()
        || dir.join("opencode.jsonc").is_file()
        || dir.join(".opencode").is_dir()
    {
        return Some(ExtensionEcosystem::OpenCode);
    }
    // A bare Claude-Code-shaped tree with no manifest: the manifest is
    // optional there, components are found by convention.
    if dir.join("commands").is_dir()
        || dir.join("agents").is_dir()
        || dir.join("skills").is_dir()
        || dir.join("hooks").is_dir()
    {
        return Some(ExtensionEcosystem::ClaudeCode);
    }
    if dir.join(".mcp.json").is_file() || dir.join("mcp.json").is_file() {
        return Some(ExtensionEcosystem::Mcp);
    }
    None
}

/// Detect, convert, and write `plugin.json` into `dir`.
///
/// `fallback_name` is used when the bundle carries no name of its own (an
/// unmanifested Claude Code tree, a bare `.mcp.json`); pass the directory or
/// repository name.
pub fn adapt(dir: &Path, fallback_name: &str) -> Result<AdaptReport, AdaptError> {
    let eco = detect(dir).ok_or_else(|| AdaptError::Unrecognised(dir.display().to_string()))?;
    match eco {
        ExtensionEcosystem::Locaryn => {
            let raw = std::fs::read_to_string(dir.join(LOCARYN_MANIFEST))?;
            let manifest: PluginManifest =
                serde_json::from_str(&raw).map_err(|e| AdaptError::Parse {
                    file: LOCARYN_MANIFEST.into(),
                    source: e,
                })?;
            Ok(AdaptReport {
                ecosystem: eco,
                manifest,
                notes: Vec::new(),
                partial: false,
            })
        }
        ExtensionEcosystem::ClaudeCode => adapt_claude_code(dir, fallback_name),
        ExtensionEcosystem::GeminiCli => adapt_gemini_cli(dir, fallback_name),
        ExtensionEcosystem::OpenCode => adapt_opencode(dir, fallback_name),
        ExtensionEcosystem::Mcp => adapt_bare_mcp(dir, fallback_name),
        // Cursor/Continue/Cline arrive through the existing import commands,
        // not through the installer.
        other => Err(AdaptError::Unrecognised(format!(
            "{} ({:?} bundles are imported, not installed)",
            dir.display(),
            other
        ))),
    }
}

// ============================================================================
// Claude Code
// ============================================================================

fn adapt_claude_code(dir: &Path, fallback_name: &str) -> Result<AdaptReport, AdaptError> {
    let mut notes = Vec::new();
    let mut m = PluginManifest {
        schema: "https://locaryn.dev/schema/plugin.json/v0.1".into(),
        api_version: "0.1".into(),
        name: sanitize_name(fallback_name),
        version: "0.0.0".into(),
        ..Default::default()
    };

    // The manifest is optional in Claude Code; components are conventional.
    let manifest_path = dir.join(CLAUDE_MANIFEST);
    let mut declared_hooks: Option<String> = None;
    let mut declared_mcp: Option<String> = None;
    let mut declared_lsp: Option<String> = None;
    if manifest_path.is_file() {
        let raw = std::fs::read_to_string(&manifest_path)?;
        let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| AdaptError::Parse {
            file: CLAUDE_MANIFEST.into(),
            source: e,
        })?;
        if let Some(n) = v.get("name").and_then(|x| x.as_str()) {
            m.name = sanitize_name(n);
        }
        if let Some(x) = v.get("displayName").and_then(|x| x.as_str()) {
            notes.push(format!("Nom affiché : {x}"));
        }
        // Claude Code treats `version` as optional and falls back to the commit
        // SHA. We need a string, so an absent version becomes 0.0.0.
        if let Some(x) = v.get("version").and_then(|x| x.as_str()) {
            m.version = x.to_string();
        }
        m.description = v
            .get("description")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        m.author = v.get("author").and_then(author_name);
        m.homepage = v
            .get("homepage")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        m.repository = v
            .get("repository")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        m.license = v
            .get("license")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        m.keywords = v
            .get("keywords")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        declared_hooks = v.get("hooks").and_then(|x| x.as_str()).map(str::to_string);
        declared_mcp = v
            .get("mcpServers")
            .and_then(|x| x.as_str())
            .map(str::to_string);
        declared_lsp = v
            .get("lspServers")
            .and_then(|x| x.as_str())
            .map(str::to_string);
    }

    let mut c = Components {
        skills: find_skills(dir),
        commands: find_markdown(&dir.join("commands")),
        agents: find_markdown(&dir.join("agents")),
        ..Default::default()
    };

    // Rules: CLAUDE.md at the plugin root is the same idea as LOCARYN.md.
    for candidate in ["CLAUDE.md", "AGENTS.md", "LOCARYN.md"] {
        if dir.join(candidate).is_file() {
            c.rules.push(candidate.to_string());
            break;
        }
    }

    // Hooks: `${CLAUDE_PLUGIN_ROOT}` means the same thing as our own variable.
    let hooks_path = declared_hooks
        .map(|p| strip_dot_slash(&p))
        .or_else(|| first_existing(dir, &["hooks/hooks.json", "hooks.json"]));
    if let Some(rel) = hooks_path {
        if dir.join(&rel).is_file() {
            rewrite_vars(
                &dir.join(&rel),
                &[("CLAUDE_PLUGIN_ROOT", "LOCARYN_PLUGIN_ROOT")],
            )?;
            c.hooks = Some(rel);
        }
    }

    // MCP: `.mcp.json` is Claude Code's conventional name.
    let mcp_path = declared_mcp
        .map(|p| strip_dot_slash(&p))
        .or_else(|| first_existing(dir, &[".mcp.json", "mcp.json", "mcp/mcp.json"]));
    if let Some(rel) = mcp_path {
        if dir.join(&rel).is_file() {
            rewrite_vars(
                &dir.join(&rel),
                &[("CLAUDE_PLUGIN_ROOT", "LOCARYN_PLUGIN_ROOT")],
            )?;
            c.mcp = Some(rel);
        }
    }

    if let Some(rel) = declared_lsp.map(|p| strip_dot_slash(&p)) {
        if dir.join(&rel).is_file() {
            c.lsp = Some(rel);
        }
    } else if dir.join(".lsp.json").is_file() {
        c.lsp = Some(".lsp.json".into());
    }

    // Components Locaryn has no equivalent for. Left on disk, reported here.
    let mut partial = false;
    for (path, what) in [
        ("output-styles", "output styles"),
        ("monitors.json", "monitors"),
        ("themes", "themes"),
    ] {
        if dir.join(path).exists() {
            notes.push(format!(
                "`{path}` ignoré ({what} n'existe pas dans Locaryn)"
            ));
            partial = true;
        }
    }

    m.permissions = infer_permissions(dir, &c);
    m.components = c;
    write_manifest(dir, &m)?;
    Ok(AdaptReport {
        ecosystem: ExtensionEcosystem::ClaudeCode,
        manifest: m,
        notes,
        partial,
    })
}

// ============================================================================
// Gemini CLI
// ============================================================================

fn adapt_gemini_cli(dir: &Path, fallback_name: &str) -> Result<AdaptReport, AdaptError> {
    let mut notes = Vec::new();
    let mut partial = false;
    let raw = std::fs::read_to_string(dir.join(GEMINI_MANIFEST))?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| AdaptError::Parse {
        file: GEMINI_MANIFEST.into(),
        source: e,
    })?;

    let mut m = PluginManifest {
        schema: "https://locaryn.dev/schema/plugin.json/v0.1".into(),
        api_version: "0.1".into(),
        name: sanitize_name(
            v.get("name")
                .and_then(|x| x.as_str())
                .unwrap_or(fallback_name),
        ),
        version: v
            .get("version")
            .and_then(|x| x.as_str())
            .unwrap_or("0.0.0")
            .to_string(),
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        ..Default::default()
    };

    let mut c = Components::default();

    // `mcpServers` is inline in the manifest here; Locaryn keeps it in its own
    // file, so extract it. `${extensionPath}` and `${/}` are Gemini's variables.
    if let Some(servers) = v.get("mcpServers").filter(|s| s.is_object()) {
        let json = serde_json::to_string_pretty(&serde_json::json!({ "mcpServers": servers }))
            .unwrap_or_default();
        let json = json
            .replace("${extensionPath}", "${LOCARYN_PLUGIN_ROOT}")
            .replace("${/}", "/");
        std::fs::create_dir_all(dir.join("mcp"))?;
        std::fs::write(dir.join("mcp/mcp.json"), json)?;
        c.mcp = Some("mcp/mcp.json".into());
    }

    // Context files become workspace rules.
    match v.get("contextFileName") {
        Some(serde_json::Value::String(s)) => c.rules.push(s.clone()),
        Some(serde_json::Value::Array(a)) => {
            for x in a {
                if let Some(s) = x.as_str() {
                    c.rules.push(s.to_string());
                }
            }
        }
        _ => {
            if dir.join("GEMINI.md").is_file() {
                c.rules.push("GEMINI.md".into());
            }
        }
    }
    c.rules.retain(|r| dir.join(r).is_file());

    c.skills = find_skills(dir);
    c.agents = find_markdown(&dir.join("agents"));

    // Slash commands are TOML here, markdown in Locaryn. Convert them into a
    // sibling directory so the originals stay readable next to them.
    let converted = convert_gemini_commands(dir, &mut notes)?;
    c.commands = converted;

    // Hooks: same event vocabulary, but nested under a "hooks" key.
    if dir.join("hooks/hooks.json").is_file() {
        let path = dir.join("hooks/hooks.json");
        let raw = std::fs::read_to_string(&path)?;
        let hv: serde_json::Value = serde_json::from_str(&raw).map_err(|e| AdaptError::Parse {
            file: "hooks/hooks.json".into(),
            source: e,
        })?;
        let inner = hv.get("hooks").cloned().unwrap_or(hv);
        let text = serde_json::to_string_pretty(&inner)
            .unwrap_or_default()
            .replace("${extensionPath}", "${LOCARYN_PLUGIN_ROOT}")
            .replace("${workspacePath}", "${LOCARYN_PROJECT_ROOT}")
            .replace("${/}", "/");
        std::fs::write(&path, text)?;
        c.hooks = Some("hooks/hooks.json".into());
    }

    if dir.join("policies").is_dir() {
        notes.push(
            "`policies/` ignoré : les règles du Policy Engine de Gemini n'ont pas \
             d'équivalent Locaryn."
                .into(),
        );
        partial = true;
    }
    if v.get("themes").is_some() {
        notes.push("`themes` ignoré : thèmes spécifiques au TUI Gemini.".into());
        partial = true;
    }
    if let Some(excluded) = v.get("excludeTools").and_then(|x| x.as_array()) {
        if !excluded.is_empty() {
            notes.push(format!(
                "`excludeTools` ({} entrées) non appliqué : Locaryn filtre les outils \
                 par permission, pas par nom.",
                excluded.len()
            ));
            partial = true;
        }
    }

    m.permissions = infer_permissions(dir, &c);
    m.components = c;
    write_manifest(dir, &m)?;
    Ok(AdaptReport {
        ecosystem: ExtensionEcosystem::GeminiCli,
        manifest: m,
        notes,
        partial,
    })
}

/// `commands/**/*.toml` → `commands-locaryn/<name>.md`.
///
/// Gemini names a command after its path relative to `commands/`, with `/`
/// becoming `:`. Its `{{args}}` placeholder is our `$0`. `!{cmd}` (shell
/// substitution) and `@{path}` (file inclusion) are host behaviour we do not
/// reproduce, so a command using them is converted but flagged.
fn convert_gemini_commands(dir: &Path, notes: &mut Vec<String>) -> Result<Vec<String>, AdaptError> {
    let src = dir.join("commands");
    if !src.is_dir() {
        return Ok(Vec::new());
    }
    let out_dir = dir.join("commands-locaryn");
    let mut out = Vec::new();
    let mut needs_shell = 0usize;
    let mut files = Vec::new();
    collect_files(&src, "toml", &mut files);

    for path in files {
        let rel = path.strip_prefix(&src).unwrap_or(&path);
        let name = rel
            .with_extension("")
            .to_string_lossy()
            .replace(['\\', '/'], ":");
        let raw = std::fs::read_to_string(&path)?;
        let parsed: toml::Value = match raw.parse() {
            Ok(v) => v,
            Err(e) => {
                notes.push(format!("commande `{name}` ignorée (TOML invalide : {e})"));
                continue;
            }
        };
        let Some(prompt) = parsed.get("prompt").and_then(|p| p.as_str()) else {
            notes.push(format!("commande `{name}` ignorée (pas de champ `prompt`)"));
            continue;
        };
        let description = parsed
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        if prompt.contains("!{") || prompt.contains("@{") {
            needs_shell += 1;
        }
        let body = prompt.replace("{{args}}", "$0");
        let md = format!(
            "---\nname: {name}\ndescription: {}\n---\n\n{body}\n",
            description.replace('\n', " ")
        );
        std::fs::create_dir_all(&out_dir)?;
        let file_name = format!("{}.md", name.replace(':', "__"));
        std::fs::write(out_dir.join(&file_name), md)?;
        out.push(format!("commands-locaryn/{file_name}"));
    }

    if needs_shell > 0 {
        notes.push(format!(
            "{needs_shell} commande(s) utilisent `!{{shell}}` ou `@{{fichier}}` : le \
             texte est conservé mais ces substitutions ne sont pas exécutées."
        ));
    }
    if !out.is_empty() {
        notes.push(format!(
            "{} commande(s) TOML converties en markdown.",
            out.len()
        ));
    }
    Ok(out)
}

// ============================================================================
// OpenCode
// ============================================================================

fn adapt_opencode(dir: &Path, fallback_name: &str) -> Result<AdaptReport, AdaptError> {
    let mut notes = Vec::new();
    let mut partial = false;
    let mut m = PluginManifest {
        schema: "https://locaryn.dev/schema/plugin.json/v0.1".into(),
        api_version: "0.1".into(),
        name: sanitize_name(fallback_name),
        version: "0.0.0".into(),
        ..Default::default()
    };

    // opencode.json / .jsonc — the `mcp` block is the portable part.
    let cfg_path = first_existing(dir, &["opencode.json", "opencode.jsonc"]);
    let mut c = Components::default();
    if let Some(rel) = &cfg_path {
        let raw = std::fs::read_to_string(dir.join(rel))?;
        // `.jsonc` allows comments; strip them before parsing.
        let cleaned = strip_json_comments(&raw);
        let v: serde_json::Value =
            serde_json::from_str(&cleaned).map_err(|e| AdaptError::Parse {
                file: rel.clone(),
                source: e,
            })?;
        if let Some(mcp) = v.get("mcp").and_then(|x| x.as_object()) {
            let mut servers = serde_json::Map::new();
            for (name, entry) in mcp {
                if let Some(converted) = opencode_mcp_entry(entry) {
                    servers.insert(name.clone(), converted);
                } else {
                    notes.push(format!("serveur MCP `{name}` ignoré (forme inconnue)"));
                    partial = true;
                }
            }
            if !servers.is_empty() {
                std::fs::create_dir_all(dir.join("mcp"))?;
                std::fs::write(
                    dir.join("mcp/mcp.json"),
                    serde_json::to_string_pretty(&serde_json::json!({ "mcpServers": servers }))
                        .unwrap_or_default(),
                )?;
                c.mcp = Some("mcp/mcp.json".into());
            }
        }
        if let Some(d) = v.get("description").and_then(|x| x.as_str()) {
            m.description = Some(d.to_string());
        }
    }

    // Markdown components live under `.opencode/`.
    let oc = dir.join(".opencode");
    c.commands = find_markdown(&oc.join("command"));
    c.agents = find_markdown(&oc.join("agent"));
    c.skills = {
        let mut s = find_skills(dir);
        s.extend(find_skills(&oc));
        s
    };
    for candidate in ["AGENTS.md", "CLAUDE.md"] {
        if dir.join(candidate).is_file() {
            c.rules.push(candidate.to_string());
            break;
        }
    }

    // TypeScript plugins run against OpenCode's own API. There is no way to
    // execute them here, and pretending otherwise would be the bug.
    let plugin_dir = oc.join("plugin");
    let ts_count = if plugin_dir.is_dir() {
        let mut files = Vec::new();
        collect_files(&plugin_dir, "ts", &mut files);
        collect_files(&plugin_dir, "js", &mut files);
        files.len()
    } else {
        0
    };
    if ts_count > 0 {
        notes.push(format!(
            "{ts_count} plugin(s) TypeScript non exécutés : ils ciblent l'API runtime \
             d'OpenCode. Les serveurs MCP, commandes, agents et skills du paquet \
             fonctionnent normalement."
        ));
        partial = true;
    }

    if c.is_empty() && ts_count == 0 {
        notes.push("Aucun composant portable trouvé dans ce paquet OpenCode.".into());
    }

    m.permissions = infer_permissions(dir, &c);
    m.components = c;
    write_manifest(dir, &m)?;
    Ok(AdaptReport {
        ecosystem: ExtensionEcosystem::OpenCode,
        manifest: m,
        notes,
        partial,
    })
}

/// OpenCode describes a server as `{type:"local", command:[bin, ...args]}` or
/// `{type:"remote", url}`. Locaryn uses the `mcpServers` shape shared by Claude
/// Code and Cursor.
fn opencode_mcp_entry(entry: &serde_json::Value) -> Option<serde_json::Value> {
    let obj = entry.as_object()?;
    let enabled = obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true);
    let kind = obj.get("type").and_then(|t| t.as_str()).unwrap_or("local");
    match kind {
        "remote" => {
            let url = obj.get("url")?.as_str()?;
            Some(serde_json::json!({
                "url": url,
                "headers": obj.get("headers").cloned().unwrap_or(serde_json::json!({})),
                "transport": "http",
                "auto_start": enabled,
            }))
        }
        _ => {
            let cmd = obj.get("command")?.as_array()?;
            let mut it = cmd.iter().filter_map(|x| x.as_str());
            let bin = it.next()?;
            let args: Vec<&str> = it.collect();
            Some(serde_json::json!({
                "command": bin,
                "args": args,
                "env": obj.get("environment").cloned().unwrap_or(serde_json::json!({})),
                "transport": "stdio",
                "auto_start": enabled,
            }))
        }
    }
}

// ============================================================================
// A bare MCP config
// ============================================================================

fn adapt_bare_mcp(dir: &Path, fallback_name: &str) -> Result<AdaptReport, AdaptError> {
    let rel = first_existing(dir, &[".mcp.json", "mcp.json"])
        .ok_or_else(|| AdaptError::Unrecognised(dir.display().to_string()))?;
    rewrite_vars(
        &dir.join(&rel),
        &[("CLAUDE_PLUGIN_ROOT", "LOCARYN_PLUGIN_ROOT")],
    )?;
    let c = Components {
        mcp: Some(rel),
        ..Default::default()
    };
    let mut m = PluginManifest {
        schema: "https://locaryn.dev/schema/plugin.json/v0.1".into(),
        api_version: "0.1".into(),
        name: sanitize_name(fallback_name),
        version: "0.0.0".into(),
        description: Some("Serveurs MCP".into()),
        ..Default::default()
    };
    m.permissions = infer_permissions(dir, &c);
    m.components = c;
    write_manifest(dir, &m)?;
    Ok(AdaptReport {
        ecosystem: ExtensionEcosystem::Mcp,
        manifest: m,
        notes: Vec::new(),
        partial: false,
    })
}

// ============================================================================
// Shared helpers
// ============================================================================

/// Derive the permission requests from what the bundle actually ships. Foreign
/// manifests carry no permission model, so asking the user to approve "shell"
/// for a plugin with no hooks would train them to click through prompts.
fn infer_permissions(dir: &Path, c: &Components) -> crate::manifest::PermissionsMap {
    let mut map = crate::manifest::PermissionsMap::new();
    let obj = |reason: &str| {
        PermissionValue::Object(PermissionRequest {
            reason: Some(reason.to_string()),
            scope: Some("project".into()),
            require_approval: None,
        })
    };

    if c.hooks.is_some() {
        map.insert("shell".into(), obj("Exécuter les hooks du plugin"));
    }
    if let Some(mcp_rel) = &c.mcp {
        map.insert(
            "mcp".into(),
            obj("Enregistrer et démarrer ses serveurs MCP"),
        );
        // An HTTP server means outbound network; a stdio one does not.
        if let Ok(raw) = std::fs::read_to_string(dir.join(mcp_rel)) {
            if raw.contains("\"url\"") {
                map.insert("network".into(), obj("Contacter un serveur MCP distant"));
            }
            if raw.contains("\"command\"") {
                map.entry("shell".into())
                    .or_insert_with(|| obj("Lancer un serveur MCP local"));
            }
        }
    }
    if c.lsp.is_some() {
        map.insert("lsp".into(), obj("Enregistrer des serveurs LSP"));
    }
    if !c.skills.is_empty() || !c.commands.is_empty() || !c.agents.is_empty() {
        map.insert(
            "files.read".into(),
            obj("Lire les fichiers du projet pour ses commandes et skills"),
        );
    }
    map
}

fn write_manifest(dir: &Path, m: &PluginManifest) -> Result<(), AdaptError> {
    let json = serde_json::to_string_pretty(m).unwrap_or_default();
    std::fs::write(dir.join(LOCARYN_MANIFEST), json)?;
    Ok(())
}

/// `skills/<name>/SKILL.md`, plus a lone `SKILL.md` at the root.
fn find_skills(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let skills = dir.join("skills");
    if skills.is_dir() {
        if let Ok(rd) = std::fs::read_dir(&skills) {
            let mut names: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
            names.sort();
            for p in names {
                if p.join("SKILL.md").is_file() {
                    if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                        out.push(format!("skills/{n}/SKILL.md"));
                    }
                }
            }
        }
    }
    if out.is_empty() && dir.join("SKILL.md").is_file() {
        out.push("SKILL.md".into());
    }
    out
}

/// Every `.md` under `dir`, relative to the plugin root, sorted for stability.
fn find_markdown(dir: &Path) -> Vec<String> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let root = match dir.parent() {
        Some(p) if dir.file_name().is_some() => p,
        _ => return Vec::new(),
    };
    let mut files = Vec::new();
    collect_files(dir, "md", &mut files);
    let mut out: Vec<String> = files
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n != "SKILL.md")
                .unwrap_or(true)
        })
        .filter_map(|p| p.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    out.sort();
    out
}

fn collect_files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect_files(&p, ext, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(p);
        }
    }
}

fn first_existing(dir: &Path, candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|c| dir.join(c).exists())
        .map(|c| c.to_string())
}

fn strip_dot_slash(p: &str) -> String {
    p.trim_start_matches("./").replace('\\', "/")
}

/// Rewrite `${FROM}` to `${TO}` inside a JSON file, in place.
fn rewrite_vars(path: &Path, pairs: &[(&str, &str)]) -> Result<(), AdaptError> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    let mut out = raw.clone();
    for (from, to) in pairs {
        out = out.replace(&format!("${{{from}}}"), &format!("${{{to}}}"));
    }
    if out != raw {
        std::fs::write(path, out)?;
    }
    Ok(())
}

/// Author may be a string or `{ "name": ... }`.
fn author_name(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(o) => o.get("name").and_then(|n| n.as_str()).map(str::to_string),
        _ => None,
    }
}

/// Locaryn requires lowercase ascii names; foreign ecosystems are laxer.
pub fn sanitize_name(raw: &str) -> String {
    let mut out: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "plugin".into()
    } else {
        out
    }
}

/// Minimal `//` and `/* */` stripper for `.jsonc`. String-aware so a URL
/// containing `//` survives.
fn strip_json_comments(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let b = raw.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    let mut escaped = false;
    while i < b.len() {
        let c = b[i] as char;
        if in_str {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
        } else if c == '/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == '/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Advertised component counts for a catalog entry, keyed by label.
pub fn summarize(c: &Components) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    if !c.skills.is_empty() {
        m.insert("skills", c.skills.len());
    }
    if !c.commands.is_empty() {
        m.insert("commands", c.commands.len());
    }
    if !c.agents.is_empty() {
        m.insert("agents", c.agents.len());
    }
    if !c.rules.is_empty() {
        m.insert("rules", c.rules.len());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("locaryn-adapt-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("My Cool Plugin!"), "my-cool-plugin");
        assert_eq!(sanitize_name("---"), "plugin");
        assert_eq!(sanitize_name("ok_name-2"), "ok_name-2");
    }

    #[test]
    fn detects_and_adapts_a_claude_code_plugin() {
        let d = tmp("cc");
        std::fs::create_dir_all(d.join(".claude-plugin")).unwrap();
        std::fs::write(
            d.join(".claude-plugin/plugin.json"),
            r#"{"name":"Code Review","version":"1.2.0","description":"reviews"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(d.join("commands")).unwrap();
        std::fs::write(d.join("commands/review.md"), "---\nname: review\n---\nGo").unwrap();
        std::fs::create_dir_all(d.join("skills/audit")).unwrap();
        std::fs::write(d.join("skills/audit/SKILL.md"), "---\nname: audit\n---\nX").unwrap();
        std::fs::write(
            d.join(".mcp.json"),
            r#"{"mcpServers":{"x":{"command":"${CLAUDE_PLUGIN_ROOT}/s.js"}}}"#,
        )
        .unwrap();

        assert_eq!(detect(&d), Some(ExtensionEcosystem::ClaudeCode));
        let r = adapt(&d, "fallback").unwrap();
        assert_eq!(r.manifest.name, "code-review");
        assert_eq!(r.manifest.version, "1.2.0");
        assert_eq!(r.manifest.components.commands, vec!["commands/review.md"]);
        assert_eq!(r.manifest.components.skills, vec!["skills/audit/SKILL.md"]);
        assert_eq!(r.manifest.components.mcp.as_deref(), Some(".mcp.json"));
        // The plugin-root variable was rewritten to Locaryn's spelling.
        let mcp = std::fs::read_to_string(d.join(".mcp.json")).unwrap();
        assert!(mcp.contains("${LOCARYN_PLUGIN_ROOT}"));
        // A Locaryn manifest now exists, so a second detect() sees it as native.
        assert_eq!(detect(&d), Some(ExtensionEcosystem::Locaryn));
    }

    #[test]
    fn converts_gemini_toml_commands_and_inline_mcp() {
        let d = tmp("gemini");
        std::fs::write(
            d.join("gemini-extension.json"),
            r#"{"name":"sec","version":"0.5.0","mcpServers":{"osv":{"command":"node","args":["${extensionPath}${/}s.js"]}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(d.join("commands/scan")).unwrap();
        std::fs::write(
            d.join("commands/scan/deep.toml"),
            "description = \"Deep scan\"\nprompt = \"Scan {{args}} now\"\n",
        )
        .unwrap();

        let r = adapt(&d, "fallback").unwrap();
        assert_eq!(r.ecosystem, ExtensionEcosystem::GeminiCli);
        assert_eq!(r.manifest.components.commands.len(), 1);
        let md = std::fs::read_to_string(d.join("commands-locaryn/scan__deep.md")).unwrap();
        assert!(md.contains("name: scan:deep"));
        assert!(md.contains("Scan $0 now"));
        let mcp = std::fs::read_to_string(d.join("mcp/mcp.json")).unwrap();
        assert!(mcp.contains("${LOCARYN_PLUGIN_ROOT}/s.js"));
    }

    #[test]
    fn opencode_ts_plugins_are_reported_not_silently_dropped() {
        let d = tmp("oc");
        std::fs::write(
            d.join("opencode.json"),
            r#"{"mcp":{"db":{"type":"local","command":["bun","x","srv"],"enabled":true}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(d.join(".opencode/plugin")).unwrap();
        std::fs::write(d.join(".opencode/plugin/hi.ts"), "export const x = 1").unwrap();

        let r = adapt(&d, "my-oc").unwrap();
        assert_eq!(r.ecosystem, ExtensionEcosystem::OpenCode);
        assert!(r.partial);
        assert!(r.notes.iter().any(|n| n.contains("TypeScript")));
        let mcp = std::fs::read_to_string(d.join("mcp/mcp.json")).unwrap();
        assert!(mcp.contains("\"command\": \"bun\""));
        assert!(mcp.contains("\"stdio\""));
    }

    #[test]
    fn strips_jsonc_comments_but_not_urls() {
        let s = strip_json_comments("{\n // hi\n \"u\": \"https://a/b\" /* x */ }");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["u"], "https://a/b");
    }
}
