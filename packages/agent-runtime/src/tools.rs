//! Tool definitions used by the agent runtime. The agent dispatches tool
//! calls to implementations registered here, gated by the project's trust
//! level and the user's approval decisions.
//!
//! Approval model (doc 11 §5 — risk-based, NOT name-based):
//!   * `requires_approval(spec, ctx)` answers "must the modal appear?".
//!   * `approval_decision(spec, ctx)` returns a rich `ApprovalDecision` that
//!     carries the diff + reason the modal will render.
//!   * Trust level is just one input; the call is also gated by:
//!       - the server's `ai_access` for remote tools (any tool flagged
//!         `is_remote` is escalated to Critical and only runs when
//!         `ai_access ∈ {Approval, Trusted}`),
//!       - the tool's declared `risk`, which is the source of truth,
//!       - the project's `Sandbox` trust (refuses every mutating tool).
//!   * The legacy name-based shortcut that previously matched
//!     `"run_command" | "write_file"` is **deleted** — any new tool takes
//!     its risk from the spec, not from a hardcoded string list.

use locaryn_shared_types::{RiskScope, TrustLevel};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// Re-export the canonical Risk alias used across crates so existing
// callers (events, daemon) keep compiling without churn.
pub use locaryn_shared_types::Risk;

/// A tool the agent can call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    /// Risk level — high-risk tools always require explicit approval.
    pub risk: Risk,
    /// Permissions required to invoke this tool.
    pub required_permissions: Vec<locaryn_shared_types::Permission>,
}

// Risk is re-exported from locaryn_events at the top of this file; see that
// definition for the canonical docstring and safety rails. The duplicate
// enum body that previously lived here was removed in favour of the
// re-export to avoid clippy `dead_code` warnings.

/// Mark-up the agent attaches to a tool call: remote call sites, target
/// server, etc. Propagated through the modal so the user can see WHERE
/// the action will execute, not just WHAT it will do.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolContext {
    pub project_id: uuid::Uuid,
    pub project_path: std::path::PathBuf,
    pub trust: TrustLevel,
    pub session_id: uuid::Uuid,
    /// When the tool touches a remote target (SSH, MCP HTTP, network
    /// extension), the runtime sets this. Drives the modal's "Remote"
    /// banner and escalates the effective risk to Critical.
    #[serde(default)]
    pub remote_target: Option<RemoteTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTarget {
    /// "ssh", "mcp-http", "mcp-stdio-remote", "web-fetch"
    pub kind: String,
    /// Server name (if SSH) or MCP server name / URL.
    pub label: String,
    /// Optional reachability probe summary (`read:yes/write:yes/sudo:no`).
    pub capabilities: Option<serde_json::Value>,
}

// LegacyToolContext alias removed: the canonical ToolContext now includes
// remote_target and the alias was dead code.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub ok: bool,
    pub output: String,
    /// Un fichier produit par l'outil.
    ///
    /// Sans cela, une image fabriquée par un outil restait un chemin dans une
    /// phrase : le fichier existait sur le serveur, et aucun client ne pouvait
    /// le montrer. C'est ce que voyait le téléphone — « voici l'image », et
    /// rien à voir.
    #[serde(default)]
    pub artifact: Option<ToolArtifact>,
}

/// Ce qu'un outil a déposé sur le disque, et de quelle nature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolArtifact {
    pub kind: locaryn_shared_types::ArtifactKind,
    pub path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("execution: {0}")]
    Exec(String),
}

/// The canonical V1 tool set. Extension tools come from MCP servers.
pub fn builtin_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "read_file".into(),
            description: "Read the complete contents of a file in the project workspace. You MUST use this tool to examine existing code before modifying it or answering questions about it.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
            risk: Risk::Low,
            required_permissions: vec![locaryn_shared_types::Permission::FilesRead],
        },
        ToolSpec {
            name: "write_file".into(),
            description: "Create or overwrite a file in the project workspace with new content. You must provide the full, complete file content.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
            risk: Risk::Medium,
            required_permissions: vec![locaryn_shared_types::Permission::FilesWrite],
        },
        ToolSpec {
            name: "search".into(),
            description: "Search the project workspace with ripgrep. Use this tool to find function definitions, variable usages, or text patterns across all files when you don't know the exact file path.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "glob": { "type": "string" }
                },
                "required": ["pattern"]
            }),
            risk: Risk::Low,
            required_permissions: vec![locaryn_shared_types::Permission::FilesRead],
        },
        ToolSpec {
            name: "run_command".into(),
            description: "Execute a shell command in the project workspace. Use this to explore directories (e.g., 'ls', 'tree'), check status, or run tests and compilers.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "command": { "type": "string" } },
                "required": ["command"]
            }),
            risk: Risk::High,
            required_permissions: vec![locaryn_shared_types::Permission::Shell],
        },
    ]
}

/// Les outils apportés par les extensions actives.
///
/// C'est ici que « installer l'extension de génération d'images » devient
/// visible pour le modèle : sans elle, `generate_image` n'existe pas dans la
/// liste d'outils, et le modèle ne peut pas l'appeler — il répondra qu'il ne
/// sait pas faire, au lieu d'échouer à l'exécution.
///
/// Le prompt de l'outil demande explicitement un prompt en anglais : les
/// modèles de diffusion sont entraînés dessus, et laisser passer la phrase
/// française de l'utilisateur donne des images approximatives. C'est le modèle
/// de conversation qui traduit et enrichit, avec tout le contexte qu'il a.
pub fn capability_tools(capabilities: &[String]) -> Vec<ToolSpec> {
    let mut out = Vec::new();
    if capabilities.iter().any(|c| c == "image-gen") {
        out.push(ToolSpec {
            name: "generate_image".into(),
            description:
                "Generate an image from a text prompt, on this machine. Use it whenever the user \
                 asks for a picture, an illustration, a logo, a mockup or any visual. Write the \
                 `prompt` in ENGLISH and make it descriptive (subject, setting, lighting, style), \
                 whatever language the user wrote in: the diffusion models are trained on English. \
                 Take into account what you know about the person and the conversation so far."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Detailed English description of the image."
                    },
                    "negative_prompt": {
                        "type": "string",
                        "description": "What to keep out of the image."
                    },
                    "width": { "type": "integer", "description": "Default 1024." },
                    "height": { "type": "integer", "description": "Default 1024." },
                    "model": {
                        "type": "string",
                        "description": "Model file name. Leave empty to use the fastest installed one."
                    }
                },
                "required": ["prompt"]
            }),
            risk: Risk::Low,
            // Aucune permission : l'image est écrite dans le dossier de
            // données de l'application, jamais dans le projet de quelqu'un.
            // Il n'y a donc rien à arbitrer, et la demande aboutit même dans
            // une conversation libre où personne ne peut approuver.
            required_permissions: Vec::new(),
        });
    }

    if capabilities.iter().any(|c| c == "voice-tts") {
        out.push(ToolSpec {
            name: "generate_speech".into(),
            description:
                "Read a text out loud with a synthetic voice, on this machine. Use it whenever the                  user asks to hear something, wants an audio version, or asks you to record a                  message. Keep `text` in the language the user wants to hear."
                    .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "What the voice should say."
                    },
                    "speed": {
                        "type": "number",
                        "description": "1.0 is normal, 0.8 slower, 1.2 faster."
                    },
                    "model": {
                        "type": "string",
                        "description": "Voice model. Leave empty for the first installed one."
                    }
                },
                "required": ["text"]
            }),
            risk: Risk::Low,
            // Comme l'image : le fichier va dans le dossier de données, pas
            // dans le projet de quelqu'un. Rien à arbitrer.
            required_permissions: Vec::new(),
        });
    }

    out
}

/// The decision the runtime asks the modal to render. Returned by
/// `approval_decision` and serialised for the Tauri channel.
/// `is_remote: true` ALWAYS escalates to Critical, regardless of the
/// declared risk tier — that is the rule that closes the doc-11 §5
/// "name-based Sandbox bypass" loophole.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub call_id: String,
    pub tool: String,
    /// The effective risk after the rule table is applied.
    /// Always ≥ the spec-declared risk, escalated to Critical when remote.
    pub effective_risk: Risk,
    /// Declared risk (before escalation).
    pub declared_risk: Risk,
    /// True if a remote target elevates this call above the declared risk.
    pub escalated_to_critical: bool,
    /// True if the user MUST click. False means the runtime already
    /// auto-approved (Low risk + Trusted trust + no remote target).
    pub needs_user_consent: bool,
    /// Human-readable reason (sentence) — shown in the modal's "Why" panel.
    pub reason: String,
    /// Rendered impact the modal must show before the user clicks.
    pub diff: Option<String>,
    /// Minimum scope the modal exposes. Critical hides "Once only" but
    /// still defaults to it.
    pub min_scope: RiskScope,
    /// True if the rule table refused this call outright (e.g. Sandbox +
    /// mutating). Modal still appears but only shows the Deny button.
    pub hard_blocked: bool,
    /// Free-form explanation (debug/audit). Not shown to end users.
    pub debug_trace: String,
}

/// Inputs to the approval decision. Captures everything that influences
/// whether a click is required, so the rule table stays declarative.
#[derive(Debug, Clone)]
pub struct ApprovalInput<'a> {
    pub spec: &'a ToolSpec,
    pub args: &'a serde_json::Value,
    pub ctx: &'a ToolContext,
    /// Optional caller-supplied reason (from hook or agent commentary).
    pub agent_reason: Option<&'a str>,
}

/// Decide whether a tool call needs the modal. Convenience wrapper that
/// preserves the old boolean signature used by the daemon & CLI; new
/// callers should prefer `approval_decision` for the rich payload.
pub fn requires_approval(spec: &ToolSpec, trust: TrustLevel) -> bool {
    let ctx = ToolContext {
        // These fields are unused by the boolean helper; only `trust`
        // matters for the legacy behaviour. Real callers use
        // `approval_decision` and supply a full context.
        project_id: uuid::Uuid::nil(),
        project_path: std::path::PathBuf::new(),
        trust,
        session_id: uuid::Uuid::nil(),
        remote_target: None,
    };
    approval_decision(&ApprovalInput {
        spec,
        args: &serde_json::Value::Null,
        ctx: &ctx,
        agent_reason: None,
    })
    .needs_user_consent
}

/// The rule table. Pure function — same inputs → same output, no I/O.
/// Mirrors the contract documented in `docs/architecture/11-ssh-connector-plan.md`
/// §5 (Sandbox arm). The matrix is intentionally small; everything else
/// stays on the default path.
pub fn approval_decision(input: &ApprovalInput<'_>) -> ApprovalDecision {
    let spec = input.spec;
    let declared = spec.risk;

    // ── Hard-block layer ────────────────────────────────────────────────
    // Sandbox is non-negotiable: any mutating tool in a Sandbox project
    // is refused outright (the modal still appears so the user understands
    // why nothing happened, but only the "OK / understand" button is
    // enabled — no Allow).
    if input.ctx.trust == TrustLevel::Sandbox && declared.tier() >= 1 {
        return ApprovalDecision {
            call_id: String::new(),
            tool: spec.name.clone(),
            effective_risk: declared,
            declared_risk: declared,
            escalated_to_critical: false,
            needs_user_consent: true,
            reason: "Sandbox project: file writes and shell execution are disabled.".to_string(),
            diff: render_diff(spec, input.args, input.ctx),
            min_scope: RiskScope::Once,
            hard_blocked: true,
            debug_trace: "trust=Sandbox && risk>=Medium → hard_blocked".into(),
        };
    }

    // Remote-targeted tools are ALWAYS escalated to Critical regardless
    // of the declared tier. This is the close-out for the doc-11 §5
    // sandbox arm: we never trust a remote tool's declared risk alone.
    let is_remote = input.ctx.remote_target.is_some();
    let effective = if is_remote { Risk::Critical } else { declared };
    let escalated = is_remote && declared.tier() < Risk::Critical.tier();

    // ── Couche d'auto-approbation (exécution silencieuse) ──────────────
    // Un outil sans aucune permission ne touche ni aux fichiers du projet, ni
    // au shell, ni au réseau : il n'y a rien à arbitrer. C'est le cas de la
    // génération d'images, qui n'écrit que dans le dossier de données de
    // l'application. Sans cette règle, demander une image dans une
    // conversation libre se soldait par un refus — personne n'étant là pour
    // approuver côté service — et le modèle répondait qu'il ne savait pas
    // faire, alors qu'il venait d'appeler l'outil.
    if declared == Risk::Low && !is_remote && spec.required_permissions.is_empty() {
        return ApprovalDecision {
            call_id: String::new(),
            tool: spec.name.clone(),
            effective_risk: declared,
            declared_risk: declared,
            escalated_to_critical: false,
            needs_user_consent: false,
            reason: "Outil sans permission : rien à arbitrer.".into(),
            diff: None,
            min_scope: RiskScope::Once,
            hard_blocked: false,
            debug_trace: "Low + aucune permission + local → silencieux".into(),
        };
    }

    // The ONLY other auto-approved case is: Low risk + no remote + Trusted.
    // Everything else requires the modal. This is intentionally strict;
    // better to ask one extra time than ship a silent execution.
    if declared == Risk::Low && !is_remote && input.ctx.trust == TrustLevel::Trusted {
        return ApprovalDecision {
            call_id: String::new(),
            tool: spec.name.clone(),
            effective_risk: declared,
            declared_risk: declared,
            escalated_to_critical: false,
            needs_user_consent: false,
            reason: "Low-risk, trusted project: auto-approved.".into(),
            diff: None,
            min_scope: RiskScope::Once,
            hard_blocked: false,
            debug_trace: "Low + Trusted + local → silent".into(),
        };
    }

    // ── Modal layer (interactive consent) ──────────────────────────────
    ApprovalDecision {
        call_id: String::new(), // populated by the agent when streaming
        tool: spec.name.clone(),
        effective_risk: effective,
        declared_risk: declared,
        escalated_to_critical: escalated,
        needs_user_consent: true,
        reason: input
            .agent_reason
            .map(|r| r.to_string())
            .unwrap_or_else(|| default_reason(spec, input.ctx)),
        diff: render_diff(spec, input.args, input.ctx),
        // Critical tools still let the user pick the scope; the UI shows
        // chips for Once/Session/Project/Always and Critical defaults to
        // Once (the safer choice) on first render.
        min_scope: RiskScope::Once,
        hard_blocked: false,
        debug_trace: format!(
            "declared={:?} remote={} trust={:?} effective={:?} escalated={}",
            declared, is_remote, input.ctx.trust, effective, escalated
        ),
    }
}

/// Human reason per spec, used when the agent didn't supply its own.
fn default_reason(spec: &ToolSpec, ctx: &ToolContext) -> String {
    if let Some(target) = &ctx.remote_target {
        return format!(
            "Executes on {} ({}) — cannot be undone locally.",
            target.label, target.kind
        );
    }
    match spec.name.as_str() {
        "read_file" => "Reads the contents of a file in the workspace.".into(),
        "write_file" => "Replaces the contents of a file. Revertible via git.".into(),
        "search" => "Searches the workspace for text patterns (no mutation).".into(),
        "run_command" => "Runs a shell command in the project workspace.".into(),
        "update_server_description" => "Edits a stored SSH server description.".into(),
        "ssh_run_command" => "Runs a shell command on a remote SSH server.".into(),
        _ => spec.description.clone(),
    }
}

/// Renders the preview/impact shown in the modal. Pure function — takes
/// the spec + args + context, returns the string the modal displays.
/// The runtime never executes anything here; the diff is purely textual.
fn render_diff(spec: &ToolSpec, args: &serde_json::Value, ctx: &ToolContext) -> Option<String> {
    match spec.name.as_str() {
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let new = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            // Diff segment: show absolute path, byte size, head + tail of
            // the new content. Full unified diff against the current file
            // is computed by the agent runtime (V1.1) — for now we expose
            // enough that the user knows what is being overwritten.
            let head: String = new.chars().take(400).collect();
            let tail: String = new
                .chars()
                .rev()
                .take(200)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            Some(format!(
                "WRITE {}\n  size: {} bytes\n  --- head ---\n{}\n  --- tail ---\n{}\n",
                path,
                new.len(),
                head,
                tail,
            ))
        }
        "run_command" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            Some(format!("EXEC in {}\n$ {}", ctx.project_path.display(), cmd))
        }
        "ssh_run_command" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let target = ctx
                .remote_target
                .as_ref()
                .map(|t| t.label.as_str())
                .unwrap_or("?");
            Some(format!("EXEC on ssh://{target}\n$ {cmd}"))
        }
        "update_server_description" => {
            let id = args
                .get("server_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let desc = args
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(format!("UPDATE server {id}\n  → {desc}"))
        }
        _ => None, // read-only tools show no diff
    }
}

// ── Tests for the rule table ────────────────────────────────────────────
#[cfg(test)]
mod approval_tests {
    use super::*;
    use locaryn_shared_types::Permission;

    fn spec(name: &str, risk: Risk) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            description: "test".into(),
            input_schema: serde_json::json!({}),
            risk,
            required_permissions: vec![Permission::FilesRead],
        }
    }

    fn ctx_with(trust: TrustLevel, remote: Option<RemoteTarget>) -> ToolContext {
        ToolContext {
            project_id: uuid::Uuid::nil(),
            project_path: std::path::PathBuf::from("/tmp/proj"),
            trust,
            session_id: uuid::Uuid::nil(),
            remote_target: remote,
        }
    }

    #[test]
    fn low_trusted_local_auto_runs() {
        let s = spec("read_file", Risk::Low);
        let ctx = ctx_with(TrustLevel::Trusted, None);
        let d = approval_decision(&ApprovalInput {
            spec: &s,
            args: &serde_json::Value::Null,
            ctx: &ctx,
            agent_reason: None,
        });
        assert!(!d.needs_user_consent);
        assert!(!d.hard_blocked);
        assert_eq!(d.effective_risk, Risk::Low);
    }

    #[test]
    fn medium_trusted_local_still_needs_consent() {
        let s = spec("write_file", Risk::Medium);
        let ctx = ctx_with(TrustLevel::Trusted, None);
        let d = approval_decision(&ApprovalInput {
            spec: &s,
            args: &serde_json::json!({"path":"a.rs","content":"hi"}),
            ctx: &ctx,
            agent_reason: Some("refactor"),
        });
        assert!(d.needs_user_consent);
        assert_eq!(d.reason, "refactor");
        assert!(d.diff.unwrap().contains("WRITE a.rs"));
    }

    #[test]
    fn sandbox_blocks_all_mutation() {
        let s = spec("write_file", Risk::Medium);
        let ctx = ctx_with(TrustLevel::Sandbox, None);
        let d = approval_decision(&ApprovalInput {
            spec: &s,
            args: &serde_json::Value::Null,
            ctx: &ctx,
            agent_reason: None,
        });
        assert!(d.hard_blocked);
    }

    #[test]
    fn remote_any_tier_escalates_to_critical() {
        // A Medium SSH tool gets escalated even if the spec said Medium:
        let s = spec("ssh_run_command", Risk::High);
        let ctx = ctx_with(
            TrustLevel::Trusted,
            Some(RemoteTarget {
                kind: "ssh".into(),
                label: "web-prod".into(),
                capabilities: None,
            }),
        );
        let d = approval_decision(&ApprovalInput {
            spec: &s,
            args: &serde_json::json!({"command":"ls"}),
            ctx: &ctx,
            agent_reason: None,
        });
        assert_eq!(d.effective_risk, Risk::Critical);
        assert!(d.escalated_to_critical || d.declared_risk == Risk::Critical);
        assert!(d.needs_user_consent);
        assert!(d.diff.unwrap().contains("web-prod"));
    }

    #[test]
    fn legacy_boolean_matches_rule_table() {
        // Faithful reproduction of the previous API for daemon/CLI callers.
        assert!(!requires_approval(
            &spec("read_file", Risk::Low),
            TrustLevel::Trusted
        ));
        assert!(requires_approval(
            &spec("write_file", Risk::Medium),
            TrustLevel::Trusted
        ));
        // Sandbox + read_file (Low) requires consent (modal shows "why"):
        assert!(requires_approval(
            &spec("read_file", Risk::Low),
            TrustLevel::Sandbox
        ));
    }
}

// ============================================================================
// S4: Tool implementations + dispatch
// ============================================================================

/// Resolve a user-provided path relative to the project root, rejecting
/// escapes (../). Returns an absolute, normalized path within the project.
pub fn resolve_path(project_root: &Path, requested: &str) -> Result<PathBuf, ToolError> {
    let requested_path = Path::new(requested);
    let candidate = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        project_root.join(requested_path)
    };
    let normalized = normalize_path(&candidate);
    let root_normalized = normalize_path(project_root);
    if !normalized.starts_with(&root_normalized) {
        return Err(ToolError::PermissionDenied(format!(
            "path escapes project root: {requested}"
        )));
    }
    Ok(normalized)
}

/// Normalize `.` and `..` components without touching the filesystem.
fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.as_os_str().is_empty() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Dispatch a tool call to its implementation. Returns the tool's output.
pub async fn dispatch_tool(
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> ToolResult {
    let project_root = ctx.project_path.as_path();
    match tool_name {
        "read_file" => exec_read_file(args, project_root).await,
        "write_file" => exec_write_file(args, project_root).await,
        "search" => exec_search(args, project_root).await,
        "run_command" => exec_run_command(args, project_root).await,
        "generate_image" => exec_generate_image(args).await,
        "generate_speech" => exec_generate_speech(args).await,
        _ => ToolResult {
            ok: false,
            output: format!("unknown tool: {tool_name}"),
            artifact: None,
        },
    }
}

/// Génère une image et rend son chemin.
///
/// Le fichier va dans le dossier de données, pas dans le projet : une image
/// demandée en conversation n'a pas à atterrir dans le dépôt de quelqu'un.
async fn exec_generate_image(args: &serde_json::Value) -> ToolResult {
    let Some(prompt) = args.get("prompt").and_then(|v| v.as_str()) else {
        return err("il manque le prompt");
    };
    let candidates = match args.get("model").and_then(|v| v.as_str()) {
        Some(m) if !m.is_empty() => vec![m.to_string()],
        // Aucun modèle demandé : on prend le plus rapide *utilisable*. Un
        // modèle « turbo » rend en quelques secondes là où un modèle complet
        // prend des minutes — dans une conversation, l'attente compte. Mais
        // rapide ne suffit pas : certains modèles de diffusion ne sont qu'une
        // moitié d'installation et réclament un VAE et des encodeurs de texte.
        // En choisir un sans ses compagnons, c'est promettre une image puis
        // afficher une erreur de fichier manquant.
        _ => {
            let models_dir = locaryn_config::models_dir();
            let usable: Vec<String> = locaryn_media::image::list_image_models()
                .into_iter()
                .filter(|m| {
                    let family = locaryn_media::image::classify(m);
                    let companions = locaryn_media::image::discover_companions(&models_dir, family);
                    locaryn_media::image::missing_companions(family, &companions).is_empty()
                })
                .collect();
            if usable.is_empty() {
                return err(
                    "aucun modèle d'image complet n'est installé sur cette machine — \
                     ouvrez le catalogue de modèles pour en ajouter un, ou complétez \
                     celui déjà présent avec son VAE et ses encodeurs",
                );
            }
            // Les modèles rapides d'abord : dans une conversation, l'attente
            // compte. On en garde deux : un modèle peut être présent et
            // complet, et refuser malgré tout de se charger sur cette machine.
            // Réessayer une fois vaut mieux que rendre la main sur un échec.
            let mut ordered = usable.clone();
            ordered.sort_by_key(|m| !(m.contains("turbo") || m.contains("schnell")));
            ordered.truncate(2);
            ordered
        }
    };

    // 768 par défaut, pas 1024 : dans une conversation, une image qui arrive
    // en une minute vaut mieux qu'une image plus fine qui en prend trois.
    let width = args.get("width").and_then(|v| v.as_u64()).unwrap_or(768) as u32;
    let height = args.get("height").and_then(|v| v.as_u64()).unwrap_or(768) as u32;
    let negative = args
        .get("negative_prompt")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut last_error = String::new();
    for model in &candidates {
        let req = locaryn_media::image::ImageRequest {
            model: model.clone(),
            prompt: prompt.to_string(),
            negative_prompt: negative.clone(),
            width,
            height,
            steps: None,
            cfg_scale: None,
            variants: 1,
            output_dir: locaryn_config::generated_images_dir(),
        };
        match locaryn_media::image::generate_image(req, &|_, _| {}).await {
            Ok(file) => {
                return ToolResult {
                    ok: true,
                    output: format!(
                        "Image générée avec {model} : {}\n\
                         Décris-la en une phrase — la personne la voit déjà.",
                        file.path.display()
                    ),
                    // Le fichier est déclaré, pas seulement mentionné dans une
                    // phrase : c'est ce qui permet à un client de l'afficher.
                    artifact: Some(ToolArtifact {
                        kind: locaryn_shared_types::ArtifactKind::ImagePng,
                        path: file.path.display().to_string(),
                    }),
                };
            }
            Err(e) => {
                tracing::warn!(model, error = %e, "génération d'image échouée");
                last_error = format!("{model} : {e}");
            }
        }
    }
    err(&format!("génération impossible — {last_error}"))
}

/// Faire lire un texte à voix haute, et rendre le fichier produit.
async fn exec_generate_speech(args: &serde_json::Value) -> ToolResult {
    let Some(text) = args.get("text").and_then(|v| v.as_str()) else {
        return err("il manque le texte à dire");
    };
    if text.trim().is_empty() {
        return err("il n'y a rien à lire");
    }

    // Aucun modèle demandé : le premier installé. Sans modèle du tout, le dire
    // plutôt que d'échouer sur un nom vide.
    let model = match args.get("model").and_then(|v| v.as_str()) {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => match locaryn_media::audio::list_tts_models().into_iter().next() {
            Some(m) => m,
            None => {
                return err(
                    "aucune voix n'est installée sur cette machine —                      ajoutez un modèle de synthèse vocale",
                )
            }
        },
    };

    let req = locaryn_media::audio::TtsRequest {
        model: model.clone(),
        text: text.to_string(),
        speed: args
            .get("speed")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.5, 2.0) as f32,
        language: args
            .get("language")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        output_dir: locaryn_config::generated_audio_dir(),
    };
    match locaryn_media::audio::generate_tts(req, &|_, _| {}).await {
        Ok(file) => ToolResult {
            ok: true,
            output: format!(
                "Voix générée avec {model} : {}
                 Dis en une phrase ce qui a été enregistré.",
                file.path.display()
            ),
            artifact: Some(ToolArtifact {
                kind: locaryn_shared_types::ArtifactKind::AudioWav,
                path: file.path.display().to_string(),
            }),
        },
        Err(e) => err(&format!("synthèse impossible — {e}")),
    }
}

async fn exec_read_file(args: &serde_json::Value, project_root: &Path) -> ToolResult {
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return err("missing required argument: path"),
    };
    match resolve_path(project_root, path) {
        Ok(full) => match tokio::fs::read_to_string(&full).await {
            Ok(content) => ToolResult {
                ok: true,
                output: content,
                artifact: None,
            },
            Err(e) => err(&format!("read_file error: {e}")),
        },
        Err(e) => err(&format!("read_file error: {e}")),
    }
}

async fn exec_write_file(args: &serde_json::Value, project_root: &Path) -> ToolResult {
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return err("missing required argument: path"),
    };
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return err("missing required argument: content"),
    };
    match resolve_path(project_root, path) {
        Ok(full) => {
            if let Some(parent) = full.parent() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return err(&format!("write_file error (create_dir): {e}"));
                }
            }
            match tokio::fs::write(&full, content).await {
                Ok(()) => ToolResult {
                    ok: true,
                    output: format!("wrote {} bytes to {}", content.len(), path),
                    artifact: None,
                },
                Err(e) => err(&format!("write_file error: {e}")),
            }
        }
        Err(e) => err(&format!("write_file error: {e}")),
    }
}

async fn exec_search(args: &serde_json::Value, project_root: &Path) -> ToolResult {
    let pattern = match args.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return err("missing required argument: pattern"),
    };
    let glob = args.get("glob").and_then(|v| v.as_str());

    // Try ripgrep first.
    let mut cmd = tokio::process::Command::new("rg");
    cmd.arg("--line-number").arg("--max-count").arg("50");
    if let Some(g) = glob {
        cmd.arg("-g").arg(g);
    }
    cmd.arg("--").arg(pattern).current_dir(project_root);
    match cmd.output().await {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            if stdout.is_empty() && !out.status.success() {
                ToolResult {
                    ok: false,
                    output: "no matches found".into(),
                    artifact: None,
                }
            } else {
                ToolResult {
                    ok: true,
                    output: stdout,
                    artifact: None,
                }
            }
        }
        Err(_) => {
            // Fallback: grep -rn
            let mut gcmd = tokio::process::Command::new("grep");
            gcmd.arg("-rn")
                .arg("--max-count=50")
                .arg(pattern)
                .arg(".")
                .current_dir(project_root);
            match gcmd.output().await {
                Ok(go) => ToolResult {
                    ok: true,
                    output: String::from_utf8_lossy(&go.stdout).to_string(),
                    artifact: None,
                },
                Err(e) => err(&format!(
                    "search error: ripgrep not found and grep failed: {e}"
                )),
            }
        }
    }
}

async fn exec_run_command(args: &serde_json::Value, project_root: &Path) -> ToolResult {
    let command = match args.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return err("missing required argument: command"),
    };
    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::process::Command::new(shell)
            .arg(flag)
            .arg(command)
            .current_dir(project_root)
            .output(),
    )
    .await;
    match result {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit_code = out.status.code().unwrap_or(-1);
            ToolResult {
                ok: out.status.success(),
                output: if stderr.is_empty() {
                    format!("[exit {exit_code}]\n{stdout}")
                } else {
                    format!("[exit {exit_code}]\n{stdout}\n--- stderr ---\n{stderr}")
                },
                artifact: None,
            }
        }
        Ok(Err(e)) => err(&format!("run_command error: {e}")),
        Err(_) => err("run_command timed out (60s)"),
    }
}

fn err(msg: &str) -> ToolResult {
    ToolResult {
        ok: false,
        output: msg.to_string(),
        artifact: None,
    }
}

/// Convert Locaryn's ToolSpec list to the Ollama tools JSON format.
pub fn ollama_tools_json(specs: &[ToolSpec]) -> serde_json::Value {
    serde_json::Value::Array(
        specs
            .iter()
            .map(|s| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": s.name,
                        "description": s.description,
                        "parameters": s.input_schema,
                    }
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_escape() {
        let root = Path::new("/home/user/project");
        assert!(resolve_path(root, "src/main.rs").is_ok());
        assert!(resolve_path(root, "../etc/passwd").is_err());
        assert!(resolve_path(root, "../../etc/passwd").is_err());
    }

    #[test]
    fn resolve_accepts_nested() {
        let root = Path::new("/home/user/project");
        assert!(resolve_path(root, "src/../src/main.rs").is_ok());
    }
}

#[cfg(test)]
mod artefact_tests {
    use super::*;

    /// Un outil qui fabrique un fichier doit le déclarer, pas seulement en
    /// parler. Tant que `generate_image` se contentait de mettre le chemin dans
    /// sa phrase, le fichier existait sur le serveur et aucun client ne pouvait
    /// l'afficher : le téléphone recevait « voici l'image » et montrait du
    /// texte.
    #[test]
    fn un_resultat_sans_fichier_ne_declare_rien() {
        let r = err("quelque chose a échoué");
        assert!(!r.ok);
        assert!(r.artifact.is_none());
    }

    #[test]
    fn un_artefact_declare_porte_son_type_et_son_chemin() {
        let r = ToolResult {
            ok: true,
            output: "Image générée".into(),
            artifact: Some(ToolArtifact {
                kind: locaryn_shared_types::ArtifactKind::ImagePng,
                path: "/donnees/generated_images/img_1.png".into(),
            }),
        };
        let a = r.artifact.expect("l'artefact doit être présent");
        assert_eq!(a.kind, locaryn_shared_types::ArtifactKind::ImagePng);
        assert!(a.path.ends_with(".png"));
        // Le nom sérialisé est celui que les clients filtrent.
        assert_eq!(
            serde_json::to_value(a.kind).unwrap(),
            serde_json::json!("image_png")
        );
    }
}

#[cfg(test)]
mod capacites_tests {
    use super::capability_tools;

    fn noms(caps: &[&str]) -> Vec<String> {
        capability_tools(&caps.iter().map(|c| c.to_string()).collect::<Vec<_>>())
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    /// Une capacité déclarée par une extension doit se traduire par un outil
    /// que le modèle peut appeler. Sans cela, installer l'extension change une
    /// ligne dans une liste et rien d'autre : le modèle continue de répondre
    /// qu'il ne sait pas faire.
    #[test]
    fn chaque_capacite_branchee_donne_un_outil() {
        assert_eq!(noms(&["image-gen"]), vec!["generate_image"]);
        assert_eq!(noms(&["voice-tts"]), vec!["generate_speech"]);
        assert_eq!(
            noms(&["image-gen", "voice-tts"]),
            vec!["generate_image", "generate_speech"]
        );
    }

    #[test]
    fn une_capacite_sans_moteur_ne_promet_rien() {
        // Mieux vaut aucun outil qu'un outil qui échouera à l'exécution :
        // le modèle dira honnêtement qu'il ne sait pas plutôt que d'essayer.
        assert!(noms(&["video-gen", "3d-gen", "model-training"]).is_empty());
        assert!(noms(&[]).is_empty());
    }
}
