//! Locaryn skill runtime. A skill is a `SKILL.md` file with YAML frontmatter
//! (compatible with Claude Code) plus a markdown body that is injected into
//! the system prompt when triggered.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub auto_trigger: bool,
    pub allowed_tools: Vec<String>,
    pub body: String,
    pub source_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
    #[error("not found: {0}")]
    NotFound(String),
}

/// Parse a `SKILL.md` file.
pub fn parse_file(path: &Path) -> Result<SkillDef, SkillError> {
    let raw = std::fs::read_to_string(path)?;
    parse_str(&raw, path.to_path_buf())
}

pub fn parse_str(raw: &str, source_path: PathBuf) -> Result<SkillDef, SkillError> {
    let (fm, body) = split_frontmatter(raw);
    let mut name = source_path
        .parent()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("skill")
        .to_string();
    let mut description: Option<String> = None;
    let mut version: Option<String> = None;
    let mut auto_trigger = false;
    let mut allowed_tools: Vec<String> = Vec::new();
    for line in fm.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = line.strip_prefix("version:") {
            version = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = line.strip_prefix("auto_trigger:") {
            auto_trigger = v.trim() == "true";
        } else if let Some(v) = line.strip_prefix("allowed_tools:") {
            allowed_tools = parse_list(v.trim());
        }
    }
    Ok(SkillDef {
        name,
        description,
        version,
        auto_trigger,
        allowed_tools,
        body: body.trim().to_string(),
        source_path,
    })
}

fn split_frontmatter(raw: &str) -> (String, String) {
    let raw_clean = raw.trim_start_matches('\u{feff}');
    let raw_stripped = raw_clean
        .strip_prefix("---\n")
        .or_else(|| raw_clean.strip_prefix("---\r\n"));
    let Some(rest) = raw_stripped else {
        return (String::new(), raw.to_string());
    };
    if let Some(end) = rest.find("\n---\n").or_else(|| rest.find("\r\n---\r\n")) {
        let (fm, body) = rest.split_at(end);
        let body = body.trim_start_matches(['\n', '\r', '-', '-']);
        (fm.to_string(), body.trim_start().to_string())
    } else {
        (String::new(), raw.to_string())
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

/// The skill registry.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    map: HashMap<String, SkillDef>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, def: SkillDef) {
        self.map.insert(def.name.clone(), def);
    }

    pub fn list(&self) -> Vec<&SkillDef> {
        self.map.values().collect()
    }

    pub fn get(&self, name: &str) -> Option<&SkillDef> {
        self.map.get(name)
    }

    /// Skills whose `auto_trigger` is true and whose description matches the
    /// user's prompt heuristically.
    pub fn auto_trigger_candidates(&self, user_prompt: &str) -> Vec<&SkillDef> {
        let p = user_prompt.to_lowercase();
        self.map
            .values()
            .filter(|s| {
                s.auto_trigger
                    && s.description
                        .as_ref()
                        .map(|d| {
                            d.split_whitespace()
                                .any(|kw| p.contains(&kw.to_lowercase()))
                        })
                        .unwrap_or(false)
            })
            .collect()
    }

    /// Build the system-prompt fragment to inject for an active skill.
    pub fn inject(&self, name: &str) -> Result<String, SkillError> {
        let s = self
            .get(name)
            .ok_or_else(|| SkillError::NotFound(name.into()))?;
        Ok(format!("# Skill: {name}\n\n{}", s.body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_md() {
        let raw = "---\nname: db-mig\ndescription: When the user wants to run or create SQL migrations.\nversion: 1.0.0\nauto_trigger: true\nallowed_tools: [read_file, write_file, run_command]\n---\n# Instructions\nAnalyze schema.";
        let def = parse_str(raw, PathBuf::from("skills/db-mig/SKILL.md")).unwrap();
        assert_eq!(def.name, "db-mig");
        assert!(def.auto_trigger);
        assert!(def.body.contains("Analyze schema."));
    }

    #[test]
    fn auto_trigger_matches() {
        let mut reg = SkillRegistry::new();
        reg.register(SkillDef {
            name: "db-mig".into(),
            description: Some("When the user wants to run or create SQL migrations.".into()),
            version: None,
            auto_trigger: true,
            allowed_tools: vec![],
            body: "x".into(),
            source_path: PathBuf::from("x"),
        });
        let c = reg.auto_trigger_candidates("please run SQL migrations");
        assert_eq!(c.len(), 1);
    }
}
