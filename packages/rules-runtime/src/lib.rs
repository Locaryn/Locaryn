//! Locaryn workspace rules runtime. Aggregates markdown rules across scopes:
//!   - global:  `~/.locaryn/rules/*.md` (+ `~/.locaryn/LOCARYN.md`)
//!   - workspace: `<project>/.locaryn/rules/*.md` (+ `<project>/LOCARYN.md`)
//!
//! Higher-priority (workspace) rules are appended last so they take effect
//! in the system prompt. Compatible with `CLAUDE.md` / `AGENTS.md` imports.

use std::path::{Path, PathBuf};

/// A single rules file with its resolved scope and priority.
#[derive(Debug, Clone)]
pub struct RuleFile {
    pub scope: RuleScope,
    pub priority: i32,
    pub source_path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleScope {
    Global,
    Workspace,
}

impl RuleScope {
    pub fn base_priority(&self) -> i32 {
        match self {
            Self::Global => 0,
            Self::Workspace => 100,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Load all rules for a project: global + workspace, sorted by priority.
pub fn load_all(project_root: &Path) -> Result<Vec<RuleFile>, RulesError> {
    let mut files = Vec::new();

    // Global.
    let global_dir = locaryn_config::global_dir().join("rules");
    if global_dir.is_dir() {
        for e in std::fs::read_dir(&global_dir)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                let content = std::fs::read_to_string(&p)?;
                files.push(RuleFile {
                    scope: RuleScope::Global,
                    priority: RuleScope::Global.base_priority(),
                    source_path: p,
                    content,
                });
            }
        }
    }
    let global_locaryn = locaryn_config::global_dir().join("LOCARYN.md");
    if global_locaryn.is_file() {
        files.push(RuleFile {
            scope: RuleScope::Global,
            priority: RuleScope::Global.base_priority() - 1,
            source_path: global_locaryn.clone(),
            content: std::fs::read_to_string(&global_locaryn)?,
        });
    }

    // Workspace.
    let ws_rules = project_root.join(".locaryn").join("rules");
    if ws_rules.is_dir() {
        for e in std::fs::read_dir(&ws_rules)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                let content = std::fs::read_to_string(&p)?;
                // Allow frontmatter `priority:` override.
                let priority =
                    parse_priority(&content).unwrap_or(RuleScope::Workspace.base_priority());
                files.push(RuleFile {
                    scope: RuleScope::Workspace,
                    priority,
                    source_path: p,
                    content,
                });
            }
        }
    }
    let ws_locaryn = project_root.join("LOCARYN.md");
    if ws_locaryn.is_file() {
        files.push(RuleFile {
            scope: RuleScope::Workspace,
            priority: RuleScope::Workspace.base_priority() - 1,
            source_path: ws_locaryn.clone(),
            content: std::fs::read_to_string(&ws_locaryn)?,
        });
    }

    files.sort_by_key(|f| f.priority);
    Ok(files)
}

fn parse_priority(content: &str) -> Option<i32> {
    let fm = content.trim_start_matches('\u{feff}');
    let fm = fm.strip_prefix("---\n")?;
    let end = fm.find("\n---\n")?;
    for line in fm[..end].lines() {
        if let Some(v) = line.trim().strip_prefix("priority:") {
            return v.trim().parse().ok();
        }
    }
    None
}

/// Compose the system-prompt fragment from the aggregated rules.
pub fn system_prompt_fragment(files: &[RuleFile]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let mut out = String::from("# Workspace rules\n\n");
    for f in files {
        out.push_str(&format!(
            "## {} (priority {})\n\n{}\n\n",
            f.source_path.display(),
            f.priority,
            f.content.trim()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_rules_fragment() {
        let s = system_prompt_fragment(&[]);
        assert!(s.is_empty());
    }

    #[test]
    fn parse_frontmatter_priority() {
        let raw = "---\nname: security\npriority: 50\n---\n# Rules\nDo x.";
        assert_eq!(parse_priority(raw), Some(50));
    }
}
