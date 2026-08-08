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
            reason: "Sandbox project: file writes and shell execution are disabled."
                .to_string(),
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

    // ── Auto-approval layer (silent run, no modal) ─────────────────────
    // The ONLY auto-approved case is: Low risk + no remote + Trusted.
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
fn render_diff(
    spec: &ToolSpec,
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Option<String> {
    match spec.name.as_str() {
        "write_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            let new = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            // Diff segment: show absolute path, byte size, head + tail of
            // the new content. Full unified diff against the current file
            // is computed by the agent runtime (V1.1) — for now we expose
            // enough that the user knows what is being overwritten.
            let head: String = new.chars().take(400).collect();
            let tail: String = new.chars().rev().take(200).collect::<String>().chars().rev().collect();
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
            Some(format!(
                "EXEC in {}\n$ {}",
                ctx.project_path.display(),
                cmd
            ))
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
            let id = args.get("server_id").and_then(|v| v.as_str()).unwrap_or("?");
            let desc = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
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
            spec: &s, args: &serde_json::Value::Null, ctx: &ctx, agent_reason: None,
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
            spec: &s, args: &serde_json::json!({"command":"ls"}), ctx: &ctx, agent_reason: None,
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
        _ => ToolResult {
            ok: false,
            output: format!("unknown tool: {tool_name}"),
        },
    }
}

async fn exec_read_file(args: &serde_json::Value, project_root: &Path) -> ToolResult {
    let path = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return err("missing required argument: path"),
    };
    match resolve_path(project_root, path) {
        Ok(full) => match tokio::fs::read_to_string(&full).await {
            Ok(content) => ToolResult { ok: true, output: content },
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
                ToolResult { ok: false, output: "no matches found".into() }
            } else {
                ToolResult { ok: true, output: stdout }
            }
        }
        Err(_) => {
            // Fallback: grep -rn
            let mut gcmd = tokio::process::Command::new("grep");
            gcmd.arg("-rn").arg("--max-count=50").arg(pattern).arg(".").current_dir(project_root);
            match gcmd.output().await {
                Ok(go) => ToolResult { ok: true, output: String::from_utf8_lossy(&go.stdout).to_string() },
                Err(e) => err(&format!("search error: ripgrep not found and grep failed: {e}")),
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
            }
        }
        Ok(Err(e)) => err(&format!("run_command error: {e}")),
        Err(_) => err("run_command timed out (60s)"),
    }
}

fn err(msg: &str) -> ToolResult {
    ToolResult { ok: false, output: msg.to_string() }
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
