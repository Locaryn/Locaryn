//! Locaryn LSP adapters. Register language servers per project and expose
//! their capabilities (symbols, definitions, diagnostics) as agent tools.
//!
//! V1 skeleton: V1.1 wires `tower-lsp` or a manual LSP client over stdio.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// `lsp/lsp.json` descriptor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LspConfig {
    #[serde(default)]
    pub adapters: Vec<LspAdapterEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspAdapterEntry {
    pub language: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// A registered LSP server instance for a project.
#[derive(Debug, Clone)]
pub struct LspHandle {
    pub language: String,
    pub command: String,
    pub args: Vec<String>,
    pub project_root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("not registered: {0}")]
    NotRegistered(String),
    #[error("lsp failure: {0}")]
    Failure(String),
}

/// The per-project LSP registry.
#[derive(Debug, Default)]
pub struct LspRegistry {
    map: HashMap<String, LspHandle>,
}

impl LspRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, handle: LspHandle) {
        self.map.insert(handle.language.clone(), handle);
    }

    pub fn list(&self) -> Vec<&LspHandle> {
        self.map.values().collect()
    }

    pub fn get(&self, language: &str) -> Option<&LspHandle> {
        self.map.get(language)
    }

    /// Query symbols in a file (V1.1 wires real LSP `textDocument/symbol`).
    pub async fn symbols(
        &self,
        _language: &str,
        _path: &std::path::Path,
    ) -> Result<Vec<LspSymbol>, LspError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSymbol {
    pub name: String,
    pub kind: String,
    pub range: String,
}

/// Load an `lsp/lsp.json` descriptor.
pub fn load_config(path: &std::path::Path) -> Result<LspConfig, LspError> {
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

/// Build the LSP tool spec exposed to the agent.
pub fn lsp_tool_spec() -> serde_json::Value {
    serde_json::json!({
        "name": "lsp_symbols",
        "description": "Query LSP symbols for a file in the project.",
        "input_schema": {
            "type": "object",
            "properties": {
                "language": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["language", "path"]
        }
    })
}
