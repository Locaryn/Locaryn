//! Hook dispatch.
//!
//! Hooks were parsed, stored and displayed, and nothing ever ran them. This
//! module is what fires them.
//!
//! Three rules govern every dispatch:
//!
//!  1. **A hook is arbitrary shell**, so the plugin must have been granted
//!     `shell`. Without it the hook is skipped and said so in the log — a
//!     plugin does not get to run commands because the user approved something
//!     else.
//!  2. **A hook cannot stall the turn.** `run_hook` kills the child at its
//!     declared timeout; a timeout is reported but never blocks, because a
//!     hanging script should not be able to veto the user's own action.
//!  3. **Only a clean refusal blocks.** A non-zero exit on `PreToolUse` or
//!     `UserPromptSubmit` stops the action; on every other event the failure is
//!     recorded and the turn continues.

use locaryn_hook_runtime::{hook_env, run_hook, HookError, HookEvent};
use locaryn_shared_types::Permission;
use std::path::{Path, PathBuf};

use crate::Core;

/// What the hook is being fired about.
pub struct HookContext {
    pub session_id: String,
    pub project_root: PathBuf,
    /// Tool name, for `PreToolUse` / `PostToolUse` matcher comparison.
    pub tool: Option<String>,
}

impl HookContext {
    pub fn new(session_id: impl Into<String>, project_root: impl Into<PathBuf>) -> Self {
        Self {
            session_id: session_id.into(),
            project_root: project_root.into(),
            tool: None,
        }
    }

    /// La couture pour `PreToolUse` / `PostToolUse`, qui ne sont pas encore
    /// distribués : la boucle d'outils n'a pas de point d'appel. Le filtrage par
    /// matcher qui s'appuie dessus est déjà écrit et testé, donc les brancher
    /// ne demandera que l'appel côté boucle.
    #[allow(dead_code)]
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }
}

/// What came out of firing every matching hook.
#[derive(Debug, Default)]
pub struct HookOutcome {
    /// Set when a hook refused with a non-zero exit on a blocking event.
    /// Carries the plugin name and command so the user knows what stopped them.
    pub blocked: Option<String>,
    /// Stdout of every hook that ran and succeeded, in fire order.
    pub outputs: Vec<String>,
}

/// Does a hook's `matcher` apply to this tool?
///
/// Absent or `*` matches everything. Otherwise the matcher is a `|`-separated
/// list of tool names, compared case-insensitively — the shape real
/// Claude-Code-style hook files use (`"WriteFile|Edit"`).
fn matches(matcher: Option<&str>, tool: Option<&str>) -> bool {
    let Some(pattern) = matcher.map(str::trim).filter(|m| !m.is_empty()) else {
        return true;
    };
    if pattern == "*" {
        return true;
    }
    // A tool-specific matcher on an event that carries no tool cannot apply.
    let Some(tool) = tool else {
        return false;
    };
    pattern
        .split('|')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .any(|p| p.eq_ignore_ascii_case(tool))
}

/// Whether a refusal on this event stops the action.
fn blocking(event: HookEvent) -> bool {
    matches!(event, HookEvent::PreToolUse | HookEvent::UserPromptSubmit)
}

/// Fire every hook registered for `event` by every enabled plugin.
///
/// Never returns an error: a broken hook is a plugin problem, not a reason to
/// fail the user's turn. Refusals surface through `HookOutcome::blocked`.
pub async fn fire(core: &Core, event: HookEvent, ctx: HookContext) -> HookOutcome {
    // Collect first, then run: the runtime lock must not be held across the
    // blocking shell calls, or a slow hook would freeze every other command.
    struct Pending {
        plugin: String,
        root: PathBuf,
        action: locaryn_hook_runtime::HookAction,
    }

    let pending: Vec<Pending> = {
        let rt = core.extensions.read().await;
        let granted = shell_granted(core).await;
        rt.loaded
            .values()
            .filter(|p| granted.iter().any(|n| n == &p.manifest.name))
            .flat_map(|p| {
                p.hooks
                    .iter()
                    .filter(|h| h.event == event)
                    .filter(|h| matches(h.matcher.as_deref(), ctx.tool.as_deref()))
                    .map(|h| Pending {
                        plugin: p.manifest.name.clone(),
                        root: p.root.clone(),
                        action: h.action.clone(),
                    })
            })
            .collect()
    };

    let mut outcome = HookOutcome::default();
    if pending.is_empty() {
        return outcome;
    }

    for p in pending {
        let env = hook_env(&p.root, &ctx.project_root, &ctx.session_id);
        let action = p.action.clone();
        // Shell is blocking; keep it off the async runtime's worker threads.
        let result = tokio::task::spawn_blocking(move || run_hook(&action, &env))
            .await
            .unwrap_or_else(|e| Err(HookError::Spawn(e.to_string())));

        match result {
            Ok(out) => {
                tracing::debug!(plugin = %p.plugin, event = event.as_str(), "hook exécuté");
                if !out.trim().is_empty() {
                    outcome.outputs.push(out.trim().to_string());
                }
            }
            Err(HookError::Timeout) => {
                // Deliberately not blocking: a script that hangs must not be
                // able to veto what the user asked for.
                tracing::warn!(
                    plugin = %p.plugin,
                    event = event.as_str(),
                    command = %p.action.command,
                    "hook interrompu au bout de son délai"
                );
            }
            Err(e) => {
                tracing::warn!(
                    plugin = %p.plugin,
                    event = event.as_str(),
                    error = %e,
                    "hook en échec"
                );
                if blocking(event) && outcome.blocked.is_none() {
                    outcome.blocked = Some(format!(
                        "Bloqué par le hook {} du plugin « {} » : {}",
                        p.action.command, p.plugin, e
                    ));
                }
            }
        }
    }

    outcome
}

/// Names of the enabled plugins the user granted `shell` to.
async fn shell_granted(core: &Core) -> Vec<String> {
    match core.storage.extensions.list().await {
        Ok(rows) => rows
            .into_iter()
            .filter(|r| r.enabled && r.granted.contains(&Permission::Shell))
            .map(|r| r.name)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "lecture des permissions impossible, aucun hook exécuté");
            Vec::new()
        }
    }
}

/// Project root to hand a hook, falling back to the current directory when the
/// session is not attached to a project (free chat).
pub fn project_root_or_cwd(path: Option<&str>) -> PathBuf {
    path.map(Path::new)
        .filter(|p| p.is_dir())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_or_star_matcher_applies_to_everything() {
        assert!(matches(None, Some("WriteFile")));
        assert!(matches(Some("*"), Some("WriteFile")));
        assert!(matches(Some("  "), None));
        assert!(matches(Some("*"), None));
    }

    #[test]
    fn named_matcher_compares_case_insensitively() {
        assert!(matches(Some("WriteFile"), Some("writefile")));
        assert!(!matches(Some("WriteFile"), Some("ReadFile")));
    }

    #[test]
    fn alternation_matches_any_branch() {
        assert!(matches(Some("WriteFile|Edit"), Some("Edit")));
        assert!(matches(Some("WriteFile | Edit"), Some("WriteFile")));
        assert!(!matches(Some("WriteFile|Edit"), Some("Bash")));
    }

    #[test]
    fn a_tool_matcher_cannot_apply_to_an_event_without_a_tool() {
        // Otherwise a `PreToolUse`-shaped matcher would fire on SessionStart.
        assert!(!matches(Some("WriteFile"), None));
    }

    #[test]
    fn only_pre_tool_use_and_prompt_submit_can_block() {
        assert!(blocking(HookEvent::PreToolUse));
        assert!(blocking(HookEvent::UserPromptSubmit));
        assert!(!blocking(HookEvent::PostToolUse));
        assert!(!blocking(HookEvent::Stop));
        assert!(!blocking(HookEvent::SessionStart));
        assert!(!blocking(HookEvent::SessionEnd));
    }
}
