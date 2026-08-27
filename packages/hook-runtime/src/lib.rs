//! Locaryn hook runtime. Events match the Claude Code vocabulary for
//! compatibility: `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStop`,
//! `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreCompact`,
//! `Notification`.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Hook events (Claude-Code-compatible vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Stop,
    SubagentStop,
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreCompact,
    Notification,
}

impl HookEvent {
    /// Match an event name in either spelling. Claude Code and Gemini CLI
    /// write `PreToolUse`; our own serde default writes `pre_tool_use`.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "PreToolUse" | "pre_tool_use" => Some(Self::PreToolUse),
            "PostToolUse" | "post_tool_use" => Some(Self::PostToolUse),
            "Stop" | "stop" => Some(Self::Stop),
            "SubagentStop" | "subagent_stop" => Some(Self::SubagentStop),
            "SessionStart" | "session_start" => Some(Self::SessionStart),
            "SessionEnd" | "session_end" => Some(Self::SessionEnd),
            "UserPromptSubmit" | "user_prompt_submit" => Some(Self::UserPromptSubmit),
            "PreCompact" | "pre_compact" => Some(Self::PreCompact),
            "Notification" | "notification" => Some(Self::Notification),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::Stop => "Stop",
            Self::SubagentStop => "SubagentStop",
            Self::SessionStart => "SessionStart",
            Self::SessionEnd => "SessionEnd",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PreCompact => "PreCompact",
            Self::Notification => "Notification",
        }
    }
}

/// A `hooks.json` file: event → list of matchers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksFile {
    #[serde(default)]
    pub pre_tool_use: Vec<MatcherEntry>,
    #[serde(default)]
    pub post_tool_use: Vec<MatcherEntry>,
    #[serde(default)]
    pub stop: Vec<MatcherEntry>,
    #[serde(default)]
    pub subagent_stop: Vec<MatcherEntry>,
    #[serde(default)]
    pub session_start: Vec<MatcherEntry>,
    #[serde(default)]
    pub session_end: Vec<MatcherEntry>,
    #[serde(default)]
    pub user_prompt_submit: Vec<MatcherEntry>,
    #[serde(default)]
    pub pre_compact: Vec<MatcherEntry>,
    #[serde(default)]
    pub notification: Vec<MatcherEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatcherEntry {
    #[serde(default)]
    pub matcher: Option<String>,
    pub hooks: Vec<HookAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookAction {
    #[serde(rename = "type")]
    pub kind: String, // "command"
    pub command: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    30
}

#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("timeout")]
    Timeout,
    #[error("hook exited with code {0}")]
    ExitCode(i32),
    #[error("spawn: {0}")]
    Spawn(String),
}

/// Parse a `hooks.json` file. Accepts both PascalCase keys (Claude Code
/// style) and snake_case keys (serde default).
pub fn load_hooks(path: &Path) -> Result<HooksFile, HookError> {
    let raw = std::fs::read_to_string(path)?;
    parse_hooks_str(&raw)
}

/// Parse hooks from a JSON string.
///
/// Every field of `HooksFile` is `#[serde(default)]`, so deserializing a
/// PascalCase file directly *succeeds* and yields an empty `HooksFile` — every
/// key is simply unknown. Routing the keys ourselves is therefore not a
/// fallback but the only correct path: a plain `from_str` would silently load
/// zero hooks from any Claude-Code-style file.
pub fn parse_hooks_str(raw: &str) -> Result<HooksFile, HookError> {
    let v: serde_json::Value = serde_json::from_str(raw)?;
    // Gemini CLI nests the events one level down under `hooks`.
    let root = v.get("hooks").filter(|h| h.is_object()).unwrap_or(&v);

    let mut h = HooksFile::default();
    for (key, val) in root.as_object().map(|m| m.iter()).into_iter().flatten() {
        let Some(event) = HookEvent::from_key(key) else {
            continue;
        };
        let entries = parse_matchers(val)?;
        let slot = match event {
            HookEvent::PreToolUse => &mut h.pre_tool_use,
            HookEvent::PostToolUse => &mut h.post_tool_use,
            HookEvent::Stop => &mut h.stop,
            HookEvent::SubagentStop => &mut h.subagent_stop,
            HookEvent::SessionStart => &mut h.session_start,
            HookEvent::SessionEnd => &mut h.session_end,
            HookEvent::UserPromptSubmit => &mut h.user_prompt_submit,
            HookEvent::PreCompact => &mut h.pre_compact,
            HookEvent::Notification => &mut h.notification,
        };
        slot.extend(entries);
    }
    Ok(h)
}

fn parse_matchers(v: &serde_json::Value) -> Result<Vec<MatcherEntry>, HookError> {
    let arr = v
        .as_array()
        .ok_or_else(|| HookError::Parse(serde_json::Error::custom("expected array")))?;
    let mut out = Vec::new();
    for entry in arr {
        let matcher = entry
            .get("matcher")
            .and_then(|m| m.as_str())
            .map(String::from);
        let mut hooks = Vec::new();
        if let Some(hs) = entry.get("hooks").and_then(|h| h.as_array()) {
            for hk in hs {
                hooks.push(HookAction {
                    kind: hk
                        .get("type")
                        .and_then(|x| x.as_str())
                        .unwrap_or("command")
                        .to_string(),
                    command: hk
                        .get("command")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    timeout: hk.get("timeout").and_then(|x| x.as_u64()).unwrap_or(30),
                });
            }
        }
        out.push(MatcherEntry { matcher, hooks });
    }
    Ok(out)
}

/// Environment injected into every hook command.
pub fn hook_env(
    plugin_root: &Path,
    project_root: &Path,
    session_id: &str,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert(
        "LOCARYN_MORPH_ROOT".into(),
        plugin_root.display().to_string(),
    );
    env.insert(
        "LOCARYN_PROJECT_ROOT".into(),
        project_root.display().to_string(),
    );
    env.insert("LOCARYN_SESSION_ID".into(), session_id.to_string());
    env
}

/// Run a hook action, enforcing its declared timeout. Returns stdout.
///
/// The timeout is the point of the field: a hook is arbitrary user shell, and
/// one that waits on input or hangs on a network call would otherwise stall the
/// turn forever. On expiry the child is killed and `HookError::Timeout` is
/// returned, so the caller can decide whether that blocks the action or not.
pub fn run_hook(action: &HookAction, env: &HashMap<String, String>) -> Result<String, HookError> {
    use std::process::Command;
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(&action.command);
        c
    } else {
        let mut c = Command::new("bash");
        c.arg("-c").arg(&action.command);
        c
    };
    cmd.envs(env);
    // Même raison que pour un serveur MCP : sans ce drapeau, chaque hook fait
    // clignoter une fenêtre de console au-dessus de l'application.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| HookError::Spawn(e.to_string()))?;

    let deadline = std::time::Instant::now() + hook_timeout(action);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = child
                    .wait_with_output()
                    .map_err(|e| HookError::Spawn(e.to_string()))?;
                if !status.success() {
                    return Err(HookError::ExitCode(status.code().unwrap_or(-1)));
                }
                return Ok(String::from_utf8_lossy(&out.stdout).to_string());
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    // Best-effort kill: the child may have exited between the
                    // check and here, in which case killing is a harmless no-op.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(HookError::Timeout);
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(HookError::Spawn(e.to_string())),
        }
    }
}

/// Helper to compute a Duration from a hook's timeout seconds.
pub fn hook_timeout(action: &HookAction) -> Duration {
    Duration::from_secs(action.timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_pascal_case_hooks() {
        let raw = r#"{
            "PreToolUse": [
                { "matcher": "WriteFile", "hooks": [
                    { "type": "command", "command": "echo hi", "timeout": 5 }
                ]}
            ]
        }"#;
        let h = load_hooks_from_str(raw);
        assert_eq!(h.pre_tool_use.len(), 1);
        assert_eq!(h.pre_tool_use[0].matcher.as_deref(), Some("WriteFile"));
        assert_eq!(h.pre_tool_use[0].hooks[0].timeout, 5);
    }

    fn load_hooks_from_str(raw: &str) -> HooksFile {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let mut h = HooksFile::default();
        for (key, val) in v.as_object().unwrap() {
            if key.as_str() == "PreToolUse" {
                h.pre_tool_use = parse_matchers(val).unwrap();
            }
        }
        h
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;

    fn action(command: &str, timeout: u64) -> HookAction {
        HookAction {
            kind: "command".into(),
            command: command.into(),
            timeout,
        }
    }

    #[test]
    fn a_successful_hook_returns_its_stdout() {
        let out = run_hook(&action("echo bonjour", 10), &HashMap::new()).unwrap();
        assert!(out.contains("bonjour"), "stdout attendu, obtenu {out:?}");
    }

    #[test]
    fn a_failing_hook_reports_its_exit_code() {
        let err = run_hook(&action("exit 3", 10), &HashMap::new()).unwrap_err();
        assert!(matches!(err, HookError::ExitCode(3)), "obtenu {err:?}");
    }

    #[test]
    fn the_environment_reaches_the_hook() {
        let mut env = HashMap::new();
        env.insert("LOCARYN_SESSION_ID".to_string(), "abc123".to_string());
        let cmd = if cfg!(target_os = "windows") {
            "echo %LOCARYN_SESSION_ID%"
        } else {
            "echo $LOCARYN_SESSION_ID"
        };
        let out = run_hook(&action(cmd, 10), &env).unwrap();
        assert!(out.contains("abc123"), "obtenu {out:?}");
    }

    #[test]
    fn a_hanging_hook_is_killed_at_its_timeout() {
        // Le point de la correction : sans application du délai, ce hook
        // bloquerait le tour indéfiniment.
        let cmd = if cfg!(target_os = "windows") {
            "ping -n 30 127.0.0.1 >nul"
        } else {
            "sleep 30"
        };
        let started = std::time::Instant::now();
        let err = run_hook(&action(cmd, 1), &HashMap::new()).unwrap_err();
        let elapsed = started.elapsed();

        assert!(matches!(err, HookError::Timeout), "obtenu {err:?}");
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "le hook aurait dû être tué vers 1 s, il a duré {elapsed:?}"
        );
    }
}
