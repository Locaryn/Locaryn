//! Extension registry — install / enable / disable / remove / hot-reload.

use crate::manifest;
use crate::{ExtensionEntry, PermissionSet};
use locaryn_shared_types::{ExtensionKind, ExtensionScope, Permission};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("manifest error: {0}")]
    Manifest(#[from] manifest::ManifestError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not installed: {0}")]
    NotFound(String),
    #[error("already installed: {0}")]
    AlreadyInstalled(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

/// The in-memory extension registry. Backed by the DB in V1; the skeleton
/// keeps an in-process map for compile + early tests.
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    by_name: RwLock<HashMap<String, ExtensionEntry>>,
}

impl ExtensionRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Install a plugin from a local directory containing `plugin.json`.
    pub fn install_from_dir(
        &self,
        dir: &Path,
        scope: ExtensionScope,
    ) -> Result<ExtensionEntry, RegistryError> {
        let m = manifest::load(dir)?;
        let mut by_name = self.by_name.write();
        if by_name.contains_key(&m.name) {
            return Err(RegistryError::AlreadyInstalled(m.name.clone()));
        }
        let entry = ExtensionEntry {
            id: uuid::Uuid::new_v4(),
            name: m.name.clone(),
            version: m.version.clone(),
            api_version: m.api_version.clone(),
            kind: ExtensionKind::Plugin,
            scope,
            manifest_path: dir.join("plugin.json"),
            enabled: false, // disabled until permissions are approved
            permissions: PermissionSet {
                requested: manifest::requested_permissions(&m),
                granted: Vec::new(),
            },
        };
        by_name.insert(m.name.clone(), entry.clone());
        Ok(entry)
    }

    pub fn get(&self, name: &str) -> Option<ExtensionEntry> {
        self.by_name.read().get(name).cloned()
    }

    pub fn list(&self) -> Vec<ExtensionEntry> {
        self.by_name.read().values().cloned().collect()
    }

    pub fn enable(&self, name: &str) -> Result<(), RegistryError> {
        let mut by_name = self.by_name.write();
        let e = by_name
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.into()))?;
        e.enabled = true;
        Ok(())
    }

    pub fn disable(&self, name: &str) -> Result<(), RegistryError> {
        let mut by_name = self.by_name.write();
        let e = by_name
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.into()))?;
        e.enabled = false;
        Ok(())
    }

    pub fn remove(&self, name: &str) -> Result<(), RegistryError> {
        let mut by_name = self.by_name.write();
        by_name
            .remove(name)
            .ok_or_else(|| RegistryError::NotFound(name.into()))?;
        Ok(())
    }

    /// Grant a permission to an extension (after user approval).
    pub fn grant_permission(&self, name: &str, perm: Permission) -> Result<(), RegistryError> {
        let mut by_name = self.by_name.write();
        let e = by_name
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.into()))?;
        if !e.permissions.granted.contains(&perm) {
            e.permissions.granted.push(perm);
        }
        Ok(())
    }

    /// Hot-reload: re-read the manifest from disk and swap the entry.
    pub fn reload(&self, name: &str) -> Result<ExtensionEntry, RegistryError> {
        let path: PathBuf = {
            let by_name = self.by_name.read();
            by_name
                .get(name)
                .ok_or_else(|| RegistryError::NotFound(name.into()))?
                .manifest_path
                .clone()
        };
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let m = manifest::load(dir)?;
        let mut by_name = self.by_name.write();
        let e = by_name
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotFound(name.into()))?;
        e.version = m.version.clone();
        e.permissions.requested = manifest::requested_permissions(&m);
        Ok(e.clone())
    }
}

// ============================================================================
// Ecosystem import layer
// ============================================================================

/// Result of importing a foreign bundle into Locaryn's format.
#[derive(Debug, Clone)]
pub struct ImportSummary {
    pub agents: usize,
    pub commands: usize,
    pub skills: usize,
    pub hooks_files: usize,
    pub rules_files: usize,
    pub mcp_servers: usize,
    pub output: PathBuf,
}

/// Import a Claude Code-style `.claude/` bundle.
/// Scans `agents/*.md`, `commands/*.md`, `skills/*/SKILL.md`, `hooks.json`,
/// `output-styles/*.md`, `CLAUDE.md`, `rules/*.md`, and `plugin.json` if
/// present. Writes the converted structure under `out`.
pub fn import_claude_code(src: &Path, out: &Path) -> Result<ImportSummary, RegistryError> {
    let mut s = ImportSummary {
        agents: 0,
        commands: 0,
        skills: 0,
        hooks_files: 0,
        rules_files: 0,
        mcp_servers: 0,
        output: out.to_path_buf(),
    };
    std::fs::create_dir_all(out.join("agents"))?;
    std::fs::create_dir_all(out.join("commands"))?;
    std::fs::create_dir_all(out.join("skills"))?;
    std::fs::create_dir_all(out.join("rules"))?;

    let agents = src.join("agents");
    if agents.is_dir() {
        for e in std::fs::read_dir(&agents)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                let dest = out.join("agents").join(e.file_name());
                std::fs::copy(&p, &dest)?;
                s.agents += 1;
            }
        }
    }
    let commands = src.join("commands");
    if commands.is_dir() {
        for e in std::fs::read_dir(&commands)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                std::fs::copy(&p, out.join("commands").join(e.file_name()))?;
                s.commands += 1;
            }
        }
    }
    let skills = src.join("skills");
    if skills.is_dir() {
        for entry in std::fs::read_dir(&skills)? {
            let entry = entry?;
            let skill_md = entry.path().join("SKILL.md");
            if skill_md.exists() {
                let dest_dir = out.join("skills").join(entry.file_name());
                std::fs::create_dir_all(&dest_dir)?;
                std::fs::copy(&skill_md, dest_dir.join("SKILL.md"))?;
                s.skills += 1;
            }
        }
    }
    if src.join("hooks.json").exists() {
        std::fs::copy(src.join("hooks.json"), out.join("hooks.json"))?;
        s.hooks_files += 1;
    }
    if src.join("CLAUDE.md").exists() {
        std::fs::copy(src.join("CLAUDE.md"), out.join("LOCARYN.md"))?;
        s.rules_files += 1;
    }
    let rules = src.join("rules");
    if rules.is_dir() {
        for e in std::fs::read_dir(&rules)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                std::fs::copy(&p, out.join("rules").join(e.file_name()))?;
                s.rules_files += 1;
            }
        }
    }
    Ok(s)
}

/// Import a Cursor `.cursor/` bundle (`mcp.json` + `rules/*.md`).
pub fn import_cursor(src: &Path, out: &Path) -> Result<ImportSummary, RegistryError> {
    let mut s = ImportSummary {
        agents: 0,
        commands: 0,
        skills: 0,
        hooks_files: 0,
        rules_files: 0,
        mcp_servers: 0,
        output: out.to_path_buf(),
    };
    std::fs::create_dir_all(out.join("rules"))?;
    let mcp = src.join("mcp.json");
    if mcp.exists() {
        std::fs::copy(&mcp, out.join("mcp.json"))?;
        // count servers
        if let Ok(raw) = std::fs::read_to_string(&mcp) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(servers) = v.get("mcpServers").and_then(|x| x.as_object()) {
                    s.mcp_servers = servers.len();
                }
            }
        }
    }
    let rules = src.join("rules");
    if rules.is_dir() {
        for e in std::fs::read_dir(&rules)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("md") {
                std::fs::copy(&p, out.join("rules").join(e.file_name()))?;
                s.rules_files += 1;
            }
        }
    }
    Ok(s)
}
