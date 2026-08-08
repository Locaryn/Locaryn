//! Lochor extension system — first-class plugins, MCP servers, commands,
//! slash commands, hooks, skills, agents, workspace rules, and LSP adapters.
//!
//! This crate owns:
//! - the `plugin.json` manifest schema (validation),
//! - the registry (install / enable / disable / remove / hot-reload),
//! - permission bookkeeping,
//! - the ecosystem import layer (Claude Code, Cursor, Continue, Cline).

pub mod adapters;
pub mod catalog;
pub mod install;
pub mod loader;
pub mod manifest;
pub mod preview;
pub mod registry;
pub mod source;

pub use adapters::{adapt, detect, AdaptError, AdaptReport};
pub use catalog::{builtin_sources, CatalogClient};
pub use install::{install, remove_files, InstallError, InstallOutcome};
pub use loader::{load, AgentDef, LoadError, LoadedHook, LoadedPlugin, RuleDoc};
pub use manifest::{PermissionRequest, PluginManifest};
pub use registry::{ExtensionRegistry, RegistryError};
pub use preview::{preview_source, SourcePreview};
pub use source::{latest_github_version, version_gt, InstallSource, SourceError};

use lochor_shared_types::{ExtensionKind, ExtensionScope, Permission};

/// Where a plugin physically lives for a given scope.
pub fn scope_dir(
    scope: ExtensionScope,
    workspace_root: Option<&std::path::Path>,
) -> std::path::PathBuf {
    match scope {
        ExtensionScope::Global | ExtensionScope::User => {
            lochor_config::global_dir().join("plugins")
        }
        ExtensionScope::Workspace => workspace_root
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".lochor")
            .join("plugins"),
        // Session-scoped plugins live in a transient temp dir owned by the daemon.
        ExtensionScope::Session => std::env::temp_dir().join("lochor-session-plugins"),
    }
}

/// A loaded, in-memory extension entry.
#[derive(Debug, Clone)]
pub struct ExtensionEntry {
    pub id: uuid::Uuid,
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub kind: ExtensionKind,
    pub scope: ExtensionScope,
    pub manifest_path: std::path::PathBuf,
    pub enabled: bool,
    pub permissions: PermissionSet,
}

/// Resolved permission set: requested (paired with the `Permission`) vs granted.
#[derive(Debug, Clone, Default)]
pub struct PermissionSet {
    pub requested: Vec<(Permission, PermissionRequest)>,
    pub granted: Vec<Permission>,
}

impl PermissionSet {
    pub fn is_granted(&self, p: &Permission) -> bool {
        self.granted.iter().any(|g| g == p)
    }
}
