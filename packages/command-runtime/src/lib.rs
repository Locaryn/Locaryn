//! Lochor command runtime — slash commands and plain commands.
//!
//! Commands are markdown files with YAML frontmatter. Example:
//!
//! ```md
//! ---
//! name: refactor
//! description: Refactor code extract module
//! allowed_tools: [read_file, write_file, search]
//! arguments: ["operation", "target"]
//! ---
//! Extract `$1` from `$2` into a dedicated module...
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A parsed command definition (frontmatter + body).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDef {
    pub name: String,
    pub description: Option<String>,
    pub allowed_tools: Vec<String>,
    pub arguments: Vec<String>,
    pub body: String,
    pub source_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("tool not allowed: {0}")]
    ToolNotAllowed(String),
}

/// Parse a `.md` command file into a `CommandDef`. Frontmatter is delimited
/// by `---` lines; the rest is the prompt body.
pub fn parse_file(path: &Path) -> Result<CommandDef, CommandError> {
    let raw = std::fs::read_to_string(path)?;
    parse_str(&raw, path.to_path_buf())
}

pub fn parse_str(raw: &str, source_path: PathBuf) -> Result<CommandDef, CommandError> {
    let (fm, body) = split_frontmatter(raw);
    let mut name = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("command")
        .to_string();
    let mut description: Option<String> = None;
    let mut allowed_tools: Vec<String> = Vec::new();
    let mut arguments: Vec<String> = Vec::new();
    for line in fm.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("name:") {
            name = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().trim_matches('"').to_string());
        } else if let Some(v) = line.strip_prefix("allowed_tools:") {
            allowed_tools = parse_list(v.trim());
        } else if let Some(v) = line.strip_prefix("arguments:") {
            arguments = parse_list(v.trim());
        }
    }
    Ok(CommandDef {
        name,
        description,
        allowed_tools,
        arguments,
        body: body.trim().to_string(),
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

/// Resolve `$1`, `$2`, ... variables in a command body using the user's args.
/// `$0` is the whole args string; `$1`..`$N` are positional.
pub fn resolve(body: &str, args: &[String]) -> String {
    let joined = args.join(" ");
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let num: usize = body[i + 1..j].parse().unwrap_or(0);
            if num == 0 {
                out.push_str(&joined);
            } else if num <= args.len() {
                out.push_str(&args[num - 1]);
            }
            i = j;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// The in-memory command registry.
#[derive(Debug, Default)]
pub struct CommandRegistry {
    map: HashMap<String, CommandDef>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, def: CommandDef) {
        self.map.insert(def.name.clone(), def);
    }

    pub fn list(&self) -> Vec<&CommandDef> {
        self.map.values().collect()
    }

    pub fn get(&self, name: &str) -> Option<&CommandDef> {
        self.map.get(name)
    }

    /// Resolve a `/name arg1 arg2` invocation into a ready-to-send prompt.
    pub fn invoke(&self, name: &str, raw_args: &str) -> Result<String, CommandError> {
        let def = self
            .get(name)
            .ok_or_else(|| CommandError::NotFound(name.into()))?;
        let args: Vec<String> = raw_args.split_whitespace().map(str::to_string).collect();
        Ok(resolve(&def.body, &args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_positional() {
        let body = "Extract `$1` from `$2` now.";
        let out = resolve(body, &["extract-module".into(), "src/auth.ts".into()]);
        assert_eq!(out, "Extract `extract-module` from `src/auth.ts` now.");
    }

    #[test]
    fn resolve_dollar_zero() {
        let body = "Args: $0";
        let out = resolve(body, &["a".into(), "b".into()]);
        assert_eq!(out, "Args: a b");
    }

    #[test]
    fn parse_command_md() {
        let raw = "---\nname: refactor\ndescription: x\nallowed_tools: [read_file, write_file]\narguments: [op, target]\n---\nDo `$1` on `$2`.";
        let def = parse_str(raw, PathBuf::from("commands/refactor.md")).unwrap();
        assert_eq!(def.name, "refactor");
        assert_eq!(def.allowed_tools, vec!["read_file", "write_file"]);
        assert!(def.body.contains("Do `$1` on `$2`."));
    }
}
