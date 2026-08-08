//! Locaryn plugin SDK — the API extension authors code against.
//!
//! In V1, plugins are mostly markdown + MCP server declarations (no native
//! code). This SDK therefore exposes:
//! - helper builders for composing a `plugin.json`-equivalent in code,
//! - trait interfaces a native (V1.1 WASM) plugin would implement,
//! - re-exports of the manifest types authors need.
//!
//! V1.1 wires `wasmtime` and proc macros so a Rust plugin can compile to
//! WASM and register tools/hooks/skills directly.

pub use locaryn_extensions::manifest::{
    Components, PermissionRequest, PermissionValue, PermissionsMap, PluginManifest,
};
pub use locaryn_shared_types::Permission;

use serde::{Deserialize, Serialize};

/// A tool the plugin exposes to the agent. (Native tools land in V1.1; V1
/// tools come from MCP servers declared in `mcp/mcp.json`.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDecl {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A builder for constructing a `PluginManifest` in code.
#[derive(Debug, Default)]
pub struct PluginBuilder {
    manifest: PluginManifest,
}

impl PluginBuilder {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        let m = PluginManifest {
            schema: "https://locaryn.dev/schema/plugin.json/v0.1".into(),
            api_version: "0.1".into(),
            name: name.into(),
            version: version.into(),
            ..Default::default()
        };
        Self { manifest: m }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.manifest.description = Some(d.into());
        self
    }

    pub fn author(mut self, a: impl Into<String>) -> Self {
        self.manifest.author = Some(a.into());
        self
    }

    pub fn license(mut self, l: impl Into<String>) -> Self {
        self.manifest.license = Some(l.into());
        self
    }

    pub fn permission(mut self, name: impl Into<String>, value: PermissionValue) -> Self {
        self.manifest.permissions.insert(name.into(), value);
        self
    }

    pub fn skill(mut self, path: impl Into<String>) -> Self {
        self.manifest.components.skills.push(path.into());
        self
    }

    pub fn command(mut self, path: impl Into<String>) -> Self {
        self.manifest.components.commands.push(path.into());
        self
    }

    pub fn agent(mut self, path: impl Into<String>) -> Self {
        self.manifest.components.agents.push(path.into());
        self
    }

    pub fn hooks(mut self, path: impl Into<String>) -> Self {
        self.manifest.components.hooks = Some(path.into());
        self
    }

    pub fn mcp(mut self, path: impl Into<String>) -> Self {
        self.manifest.components.mcp = Some(path.into());
        self
    }

    pub fn rule(mut self, path: impl Into<String>) -> Self {
        self.manifest.components.rules.push(path.into());
        self
    }

    pub fn lsp(mut self, path: impl Into<String>) -> Self {
        self.manifest.components.lsp = Some(path.into());
        self
    }

    pub fn build(self) -> PluginManifest {
        self.manifest
    }
}

/// Validate a manifest (delegates to `locaryn-extensions`).
pub fn validate(m: &PluginManifest) -> Result<(), locaryn_extensions::manifest::ManifestError> {
    locaryn_extensions::manifest::validate(m)
}

/// Trait a native plugin (V1.1) would implement. Skeleton now.
#[async_trait::async_trait]
pub trait LocarynPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn tools(&self) -> Vec<ToolDecl> {
        Vec::new()
    }
    async fn invoke_tool(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, PluginError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("no such tool: {0}")]
    NoSuchTool(String),
    #[error("tool failed: {0}")]
    Failed(String),
}
