//! Specialized agent profiles (subagents). A profile is a markdown file with
//! YAML frontmatter compatible with Claude Code's `.claude/agents/*.md`:
//!
//! ```md
//! ---
//! name: code-reviewer
//! description: Expert in security and performance code reviews.
//! model: qwen2.5-coder:32b
//! tools: [read_file, search]
//! output_style: concise
//! ---
//! You are a senior staff engineer...
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub name: String,
    pub description: Option<String>,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub output_style: Option<String>,
    pub system_prompt: String,
    pub source_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub fn parse_file(path: &Path) -> Result<AgentProfile, ProfileError> {
    let raw = std::fs::read_to_string(path)?;
    parse_str(&raw, path.to_path_buf())
}

pub fn parse_str(raw: &str, source_path: PathBuf) -> Result<AgentProfile, ProfileError> {
    let (fm, body) = split_frontmatter(raw);
    let mut name = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("agent")
        .to_string();
    let mut description: Option<String> = None;
    let mut model: Option<String> = None;
    let mut tools: Vec<String> = Vec::new();
    let mut output_style: Option<String> = None;
    for line in fm.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = line.strip_prefix("model:") {
            model = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = line.strip_prefix("tools:") {
            tools = parse_list(v.trim());
        } else if let Some(v) = line.strip_prefix("output_style:") {
            output_style = Some(v.trim().trim_matches('"').to_string());
        }
    }
    Ok(AgentProfile {
        name,
        description,
        model,
        tools,
        output_style,
        system_prompt: body.trim().to_string(),
        source_path,
    })
}

fn split_frontmatter(raw: &str) -> (String, String) {
    let raw = raw.trim_start_matches('\u{feff}');
    let raw = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"));
    let Some(rest) = raw else {
        return (String::new(), String::new());
    };
    if let Some(end) = rest.find("\n---\n").or_else(|| rest.find("\r\n---\r\n")) {
        let (fm, body) = rest.split_at(end);
        let body = body.trim_start_matches(['\n', '\r', '-', '-']);
        (fm.to_string(), body.trim_start().to_string())
    } else {
        (rest.to_string(), String::new())
    }
}

fn parse_list(s: &str) -> Vec<String> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        inner
            .split(',')
            .map(|x| x.trim().trim_matches('"').to_string())
            .filter(|x| !x.is_empty())
            .collect()
    } else {
        s.split_whitespace().map(str::to_string).collect()
    }
}

#[derive(Debug, Default)]
pub struct AgentRegistry {
    map: HashMap<String, AgentProfile>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, p: AgentProfile) {
        self.map.insert(p.name.clone(), p);
    }

    pub fn list(&self) -> Vec<&AgentProfile> {
        self.map.values().collect()
    }

    pub fn get(&self, name: &str) -> Option<&AgentProfile> {
        self.map.get(name)
    }

    /// Compose the effective system prompt for a profile (system_prompt body
    /// + optional output_style note).
    pub fn system_prompt(&self, name: &str) -> Result<String, ProfileError> {
        let p = self
            .get(name)
            .ok_or_else(|| ProfileError::NotFound(name.into()))?;
        let mut s = p.system_prompt.clone();
        if let Some(style) = &p.output_style {
            s.push_str(&format!("\n\n## Output style\n{style}"));
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_md() {
        let raw = "---\nname: code-reviewer\ndescription: Expert in reviews.\nmodel: qwen2.5-coder:32b\ntools: [read_file, search]\noutput_style: concise\n---\nYou are a senior staff engineer.";
        let p = parse_str(raw, PathBuf::from("agents/code-reviewer.md")).unwrap();
        assert_eq!(p.name, "code-reviewer");
        assert_eq!(p.model.as_deref(), Some("qwen2.5-coder:32b"));
        assert_eq!(p.tools, vec!["read_file", "search"]);
        assert!(p.system_prompt.contains("senior staff engineer"));
    }
}
