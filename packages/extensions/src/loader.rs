//! Read a Locaryn plugin directory into live components.
//!
//! `enable()` used to flip a boolean. This is the part that was missing: the
//! manifest's `components` paths (or, when the manifest declares none, the
//! conventional directories) are parsed into the same types the runtimes
//! already use — `SkillDef`, `CommandDef`, `McpServerEntry`, `HooksFile`,
//! `LspAdapterEntry` — so a plugin genuinely contributes something.
//!
//! Loading is deliberately tolerant. One malformed skill must not stop the
//! other six components from working, so every failure is collected in
//! `errors` and surfaced in the UI rather than aborting the load.

use crate::manifest::PluginManifest;
use locaryn_command_runtime::CommandDef;
use locaryn_lsp_adapters::LspAdapterEntry;
use locaryn_mcp::{McpConfig, McpServerEntry};
use locaryn_shared_types::ExtensionComponents;
use locaryn_skill_runtime::SkillDef;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest error: {0}")]
    Manifest(#[from] crate::manifest::ManifestError),
}

/// An agent profile shipped by a plugin.
#[derive(Debug, Clone)]
pub struct AgentDef {
    pub name: String,
    pub description: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub output_style: Option<String>,
    pub system_prompt: String,
    pub source_path: PathBuf,
}

/// A workspace rules document shipped by a plugin.
#[derive(Debug, Clone)]
pub struct RuleDoc {
    pub name: String,
    pub priority: i32,
    pub content: String,
    pub source_path: PathBuf,
}

/// One hook action, flattened out of `hooks.json` with its event and matcher.
#[derive(Debug, Clone)]
pub struct LoadedHook {
    pub event: locaryn_hook_runtime::HookEvent,
    pub matcher: Option<String>,
    pub action: locaryn_hook_runtime::HookAction,
}

/// Everything a plugin contributes, ready to register.
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub root: PathBuf,
    pub manifest: PluginManifest,
    pub skills: Vec<SkillDef>,
    pub commands: Vec<CommandDef>,
    pub agents: Vec<AgentDef>,
    pub rules: Vec<RuleDoc>,
    pub hooks: Vec<LoadedHook>,
    pub mcp: Vec<(String, McpServerEntry)>,
    pub lsp: Vec<LspAdapterEntry>,
    /// Non-fatal problems. A plugin with errors still loads whatever parsed.
    pub errors: Vec<String>,
}

impl LoadedPlugin {
    pub fn counts(&self) -> ExtensionComponents {
        ExtensionComponents {
            skills: self.skills.len() as u32,
            commands: self.commands.len() as u32,
            agents: self.agents.len() as u32,
            rules: self.rules.len() as u32,
            hooks: self.hooks.len() as u32,
            mcp_servers: self.mcp.len() as u32,
            lsp_adapters: self.lsp.len() as u32,
        }
    }

    /// The text this plugin adds to the system prompt: its rules verbatim, and
    /// a one-line index of its skills so the model knows they exist and can ask
    /// for one by name.
    pub fn system_prompt_fragment(&self) -> String {
        let mut out = String::new();
        for r in &self.rules {
            if !r.content.trim().is_empty() {
                out.push_str(r.content.trim());
                out.push_str("\n\n");
            }
        }
        if !self.skills.is_empty() {
            out.push_str(&format!(
                "## Skills disponibles ({})\n\n",
                self.manifest.name
            ));
            for s in &self.skills {
                match &s.description {
                    Some(d) => out.push_str(&format!("- `{}` — {}\n", s.name, d.trim())),
                    None => out.push_str(&format!("- `{}`\n", s.name)),
                }
            }
            out.push('\n');
        }
        out
    }
}

/// Load a plugin from `root`, which must contain a Locaryn `plugin.json`.
pub fn load(root: &Path) -> Result<LoadedPlugin, LoadError> {
    let manifest = crate::manifest::load(root)?;
    Ok(load_with_manifest(root, manifest))
}

/// Same as [`load`], for callers that already parsed the manifest.
pub fn load_with_manifest(root: &Path, manifest: PluginManifest) -> LoadedPlugin {
    let mut p = LoadedPlugin {
        root: root.to_path_buf(),
        manifest,
        skills: Vec::new(),
        commands: Vec::new(),
        agents: Vec::new(),
        rules: Vec::new(),
        hooks: Vec::new(),
        mcp: Vec::new(),
        lsp: Vec::new(),
        errors: Vec::new(),
    };

    let c = p.manifest.components.clone();
    let declared_nothing = c.is_empty();

    // --- Skills -------------------------------------------------------------
    let skill_paths: Vec<PathBuf> = if declared_nothing {
        discover_skills(root)
    } else {
        c.skills.iter().map(|s| root.join(s)).collect()
    };
    for path in skill_paths {
        match locaryn_skill_runtime::parse_file(&path) {
            Ok(def) => p.skills.push(def),
            Err(e) => p.errors.push(format!("skill {}: {e}", rel(root, &path))),
        }
    }

    // --- Commands -----------------------------------------------------------
    let command_paths: Vec<PathBuf> = if declared_nothing {
        discover_markdown(&root.join("commands"))
    } else {
        c.commands.iter().map(|s| root.join(s)).collect()
    };
    for path in command_paths {
        match locaryn_command_runtime::parse_file(&path) {
            Ok(def) => p.commands.push(def),
            Err(e) => p.errors.push(format!("command {}: {e}", rel(root, &path))),
        }
    }

    // --- Agents -------------------------------------------------------------
    let agent_paths: Vec<PathBuf> = if declared_nothing {
        discover_markdown(&root.join("agents"))
    } else {
        c.agents.iter().map(|s| root.join(s)).collect()
    };
    for path in agent_paths {
        match parse_agent(&path) {
            Ok(def) => p.agents.push(def),
            Err(e) => p.errors.push(format!("agent {}: {e}", rel(root, &path))),
        }
    }

    // --- Rules --------------------------------------------------------------
    let rule_paths: Vec<PathBuf> = if declared_nothing {
        let mut v = discover_markdown(&root.join("rules"));
        for candidate in ["LOCARYN.md", "CLAUDE.md", "AGENTS.md", "GEMINI.md"] {
            if root.join(candidate).is_file() {
                v.push(root.join(candidate));
                break;
            }
        }
        v
    } else {
        c.rules.iter().map(|s| root.join(s)).collect()
    };
    for path in rule_paths {
        match parse_rule(&path) {
            Ok(def) => p.rules.push(def),
            Err(e) => p.errors.push(format!("rule {}: {e}", rel(root, &path))),
        }
    }

    // --- Hooks --------------------------------------------------------------
    let hooks_path = c
        .hooks
        .as_ref()
        .map(|h| root.join(h))
        .or_else(|| {
            let d = root.join("hooks/hooks.json");
            d.is_file().then_some(d)
        })
        .filter(|p| p.is_file());
    if let Some(path) = hooks_path {
        match locaryn_hook_runtime::load_hooks(&path) {
            Ok(file) => p.hooks = flatten_hooks(&file),
            Err(e) => p.errors.push(format!("hooks {}: {e}", rel(root, &path))),
        }
    }

    // --- MCP ----------------------------------------------------------------
    let mcp_path = c
        .mcp
        .as_ref()
        .map(|m| root.join(m))
        .or_else(|| {
            ["mcp/mcp.json", ".mcp.json", "mcp.json"]
                .iter()
                .map(|n| root.join(n))
                .find(|p| p.is_file())
        })
        .filter(|p| p.is_file());
    if let Some(path) = mcp_path {
        match McpConfig::load(&path) {
            Ok(cfg) => {
                for (name, mut entry) in cfg.mcp_servers {
                    expand_entry(&mut entry, root);
                    p.mcp.push((name, entry));
                }
                p.mcp.sort_by(|a, b| a.0.cmp(&b.0));
            }
            Err(e) => p.errors.push(format!("mcp {}: {e}", rel(root, &path))),
        }
    }

    // --- LSP ----------------------------------------------------------------
    let lsp_path = c
        .lsp
        .as_ref()
        .map(|l| root.join(l))
        .or_else(|| {
            ["lsp/lsp.json", ".lsp.json"]
                .iter()
                .map(|n| root.join(n))
                .find(|p| p.is_file())
        })
        .filter(|p| p.is_file());
    if let Some(path) = lsp_path {
        match locaryn_lsp_adapters::load_config(&path) {
            Ok(cfg) => {
                p.lsp = cfg
                    .adapters
                    .into_iter()
                    .map(|mut a| {
                        a.command = expand_str(&a.command, root);
                        a.args = a.args.iter().map(|x| expand_str(x, root)).collect();
                        a
                    })
                    .collect()
            }
            Err(e) => p.errors.push(format!("lsp {}: {e}", rel(root, &path))),
        }
    }

    p
}

// ============================================================================
// Variable expansion
// ============================================================================

/// Substitute the variables a plugin may use in commands, args and env:
/// `${LOCARYN_PLUGIN_ROOT}` (this plugin's directory) and `${env:NAME}`.
///
/// An `${env:NAME}` that is not set expands to the empty string, matching what
/// every other client does — a missing token yields an auth failure from the
/// server, which is a clearer error than a literal `${env:TOKEN}` being sent.
pub fn expand_str(s: &str, root: &Path) -> String {
    let mut out = s.replace(
        "${LOCARYN_PLUGIN_ROOT}",
        &root.to_string_lossy().replace('\\', "/"),
    );
    while let Some(start) = out.find("${env:") {
        let Some(end) = out[start..].find('}').map(|i| start + i) else {
            break;
        };
        let name = &out[start + 6..end];
        let value = std::env::var(name).unwrap_or_default();
        out.replace_range(start..=end, &value);
    }
    out
}

fn expand_entry(entry: &mut McpServerEntry, root: &Path) {
    if let Some(cmd) = &entry.command {
        entry.command = Some(expand_str(cmd, root));
    }
    entry.args = entry.args.iter().map(|a| expand_str(a, root)).collect();
    entry.env = entry
        .env
        .iter()
        .map(|(k, v)| (k.clone(), expand_str(v, root)))
        .collect();
    if let Some(url) = &entry.url {
        entry.url = Some(expand_str(url, root));
    }
    entry.headers = entry
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), expand_str(v, root)))
        .collect();
}

// ============================================================================
// Parsers for the two component kinds without a dedicated runtime parser
// ============================================================================

fn parse_agent(path: &Path) -> Result<AgentDef, std::io::Error> {
    let raw = std::fs::read_to_string(path)?;
    let (fm, body) = split_frontmatter(&raw);
    let mut def = AgentDef {
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("agent")
            .to_string(),
        description: None,
        model: None,
        tools: Vec::new(),
        output_style: None,
        system_prompt: body.trim().to_string(),
        source_path: path.to_path_buf(),
    };
    for line in fm.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            def.name = unquote(v);
        } else if let Some(v) = line.strip_prefix("description:") {
            def.description = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("model:") {
            def.model = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("output_style:") {
            def.output_style = Some(unquote(v));
        } else if let Some(v) = line.strip_prefix("tools:") {
            def.tools = parse_list(v);
        }
    }
    Ok(def)
}

fn parse_rule(path: &Path) -> Result<RuleDoc, std::io::Error> {
    let raw = std::fs::read_to_string(path)?;
    let (fm, body) = split_frontmatter(&raw);
    let mut name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("rules")
        .to_string();
    let mut priority = 0;
    for line in fm.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            name = unquote(v);
        } else if let Some(v) = line.strip_prefix("priority:") {
            priority = unquote(v).parse().unwrap_or(0);
        }
    }
    // A rules file with no frontmatter is all body.
    let content = if fm.is_empty() { raw } else { body.to_string() };
    Ok(RuleDoc {
        name,
        priority,
        content,
        source_path: path.to_path_buf(),
    })
}

fn split_frontmatter(raw: &str) -> (String, &str) {
    let trimmed = raw.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (String::new(), trimmed);
    };
    let rest = rest.trim_start_matches(['\r', '\n']);
    match rest.find("\n---") {
        Some(end) => {
            let fm = &rest[..end];
            let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
            (fm.to_string(), body)
        }
        None => (String::new(), trimmed),
    }
}

fn unquote(v: &str) -> String {
    v.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn parse_list(v: &str) -> Vec<String> {
    let v = v.trim();
    let inner = v.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(unquote)
        .filter(|s| !s.is_empty())
        .collect()
}

fn flatten_hooks(file: &locaryn_hook_runtime::HooksFile) -> Vec<LoadedHook> {
    use locaryn_hook_runtime::HookEvent as E;
    let groups: [(E, &Vec<locaryn_hook_runtime::MatcherEntry>); 9] = [
        (E::PreToolUse, &file.pre_tool_use),
        (E::PostToolUse, &file.post_tool_use),
        (E::Stop, &file.stop),
        (E::SubagentStop, &file.subagent_stop),
        (E::SessionStart, &file.session_start),
        (E::SessionEnd, &file.session_end),
        (E::UserPromptSubmit, &file.user_prompt_submit),
        (E::PreCompact, &file.pre_compact),
        (E::Notification, &file.notification),
    ];
    let mut out = Vec::new();
    for (event, entries) in groups {
        for entry in entries {
            for action in &entry.hooks {
                out.push(LoadedHook {
                    event,
                    matcher: entry.matcher.clone(),
                    action: action.clone(),
                });
            }
        }
    }
    out
}

// ============================================================================
// Convention-based discovery (used when the manifest declares no components)
// ============================================================================

fn discover_skills(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let skills = root.join("skills");
    if let Ok(rd) = std::fs::read_dir(&skills) {
        let mut dirs: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            let f = d.join("SKILL.md");
            if f.is_file() {
                out.push(f);
            }
        }
    }
    if out.is_empty() && root.join("SKILL.md").is_file() {
        out.push(root.join("SKILL.md"));
    }
    out
}

fn discover_markdown(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_md(dir, &mut out);
    out.sort();
    out
}

fn walk_md(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            walk_md(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("md")
            && p.file_name().and_then(|n| n.to_str()) != Some("SKILL.md")
        {
            out.push(p);
        }
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("locaryn-loader-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Une extension apporte ses modèles au catalogue par un slot de données.
    /// Le chargeur doit le conserver tel quel : sans lui, la liste arrive vide
    /// et l'écran des modèles ne montre rien, sans erreur.
    #[test]
    fn a_data_slot_survives_loading() {
        let root = tmp("data-slot");
        std::fs::write(
            root.join("plugin.json"),
            r#"{
              "apiVersion": "0.1",
              "name": "avec-catalogue",
              "version": "1.0.0",
              "ui_contributions": {
                "slots": [
                  {
                    "id": "catalogue",
                    "slot": "marketplace.catalogs",
                    "type": "data",
                    "entry": "dist/marketplace.json"
                  }
                ]
              }
            }"#,
        )
        .unwrap();

        let plugin = load(&root).expect("manifeste lisible");
        let slots = &plugin.manifest.ui.slots;
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].slot, "marketplace.catalogs");
        assert_eq!(slots[0].kind, "data");
        assert_eq!(slots[0].entry.as_deref(), Some("dist/marketplace.json"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Deux formes du même écran, une par surface : c'est ce qui permet à une
    /// extension de poser un grand panneau sur l'ordinateur et autre chose sur
    /// le téléphone, sans que l'hôte ait à connaître l'extension.
    #[test]
    fn a_slot_can_target_one_surface_at_a_time() {
        let root = tmp("slot-platforms");
        std::fs::write(
            root.join("plugin.json"),
            r#"{
              "apiVersion": "0.1",
              "name": "deux-formes",
              "version": "1.0.0",
              "ui_contributions": {
                "slots": [
                  {
                    "id": "grand-panneau",
                    "slot": "studio.tabs",
                    "type": "custom-element",
                    "entry": "dist/desktop.js",
                    "tag": "x-grand",
                    "platforms": ["desktop"]
                  },
                  {
                    "id": "feuille",
                    "slot": "studio.tabs",
                    "type": "custom-element",
                    "entry": "dist/mobile.js",
                    "tag": "x-feuille",
                    "platforms": ["mobile", "web"]
                  },
                  { "id": "partout", "slot": "studio.tabs", "type": "action" }
                ]
              }
            }"#,
        )
        .unwrap();

        let plugin = load(&root).expect("manifeste lisible");
        let slots = &plugin.manifest.ui.slots;
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].platforms, vec!["desktop".to_string()]);
        assert_eq!(
            slots[1].platforms,
            vec!["mobile".to_string(), "web".to_string()]
        );
        // Sans `platforms`, la contribution vise toutes les surfaces : c'est le
        // cas courant, et il ne doit rien coûter à écrire.
        assert!(slots[2].platforms.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loads_the_bundled_example_plugin() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/plugins/my-plugin")
            .canonicalize()
            .expect("example plugin exists");
        let p = load(&root).expect("example plugin loads");
        assert_eq!(p.manifest.name, "my-plugin");
        assert_eq!(p.skills.len(), 1, "skills: {:?}", p.errors);
        assert_eq!(p.commands.len(), 1, "commands: {:?}", p.errors);
        assert_eq!(p.agents.len(), 1, "agents: {:?}", p.errors);
        assert_eq!(p.rules.len(), 1, "rules: {:?}", p.errors);
        assert_eq!(p.mcp.len(), 1, "mcp: {:?}", p.errors);
        assert_eq!(p.mcp[0].0, "schema-introspect");
        assert_eq!(p.lsp.len(), 2, "lsp: {:?}", p.errors);
        assert!(!p.hooks.is_empty(), "hooks: {:?}", p.errors);
        assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
        let counts = p.counts();
        assert!(counts.total() >= 7);
        // `${LOCARYN_PLUGIN_ROOT}` in the MCP args resolved to a real path.
        assert!(
            p.mcp[0].1.args.iter().all(|a| !a.contains("${")),
            "unexpanded variable: {:?}",
            p.mcp[0].1.args
        );
    }

    #[test]
    fn a_broken_component_does_not_sink_the_others() {
        let d = tmp("partial");
        std::fs::write(
            d.join("plugin.json"),
            r#"{"schema":"x","apiVersion":"0.1","name":"p","version":"1.0.0",
                "components":{"skills":["skills/gone/SKILL.md"],"commands":["commands/ok.md"]}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(d.join("commands")).unwrap();
        std::fs::write(d.join("commands/ok.md"), "---\nname: ok\n---\nBody").unwrap();

        let p = load(&d).unwrap();
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.skills.len(), 0);
        assert_eq!(p.errors.len(), 1);
        assert!(p.errors[0].starts_with("skill skills/gone/SKILL.md"));
    }

    #[test]
    fn discovers_components_when_the_manifest_declares_none() {
        let d = tmp("convention");
        std::fs::write(
            d.join("plugin.json"),
            r#"{"schema":"x","apiVersion":"0.1","name":"p","version":"1.0.0"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(d.join("commands")).unwrap();
        std::fs::write(d.join("commands/a.md"), "---\nname: a\n---\nX").unwrap();
        std::fs::create_dir_all(d.join("skills/s")).unwrap();
        std::fs::write(d.join("skills/s/SKILL.md"), "---\nname: s\n---\nY").unwrap();

        let p = load(&d).unwrap();
        assert_eq!(p.commands.len(), 1);
        assert_eq!(p.skills.len(), 1);
    }

    #[test]
    fn expands_plugin_root_and_env() {
        let d = tmp("expand");
        std::env::set_var("LOCARYN_TEST_TOKEN", "s3cret");
        let s = expand_str("${LOCARYN_PLUGIN_ROOT}/x --t ${env:LOCARYN_TEST_TOKEN}", &d);
        assert!(s.ends_with("/x --t s3cret"), "{s}");
        assert!(!s.contains("${"));
    }

    #[test]
    fn unset_env_expands_to_empty_not_literal() {
        let d = tmp("expand2");
        std::env::remove_var("LOCARYN_DEFINITELY_UNSET");
        let s = expand_str("a${env:LOCARYN_DEFINITELY_UNSET}b", &d);
        assert_eq!(s, "ab");
    }
}
