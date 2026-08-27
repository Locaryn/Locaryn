//! Fetch → adapt → load, in one call.
//!
//! This is the whole install pipeline as a pure function: it puts files on
//! disk and tells the caller what it found. Persistence (the extension row,
//! the permission decisions) belongs to whoever owns the database, so nothing
//! here touches storage.

use crate::adapters::{self, AdaptError};
use crate::loader::{self, LoadedPlugin};
use crate::manifest::PluginManifest;
use crate::source::{self, SourceError};
use locaryn_shared_types::{ExtensionEcosystem, ExtensionScope};
use std::path::{Path, PathBuf};

/// A catalog entry from the MCP registry installs as a one-server plugin
/// rather than a repository checkout.
pub const MCP_REMOTE_PREFIX: &str = "mcp-remote:";

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Adapt(#[from] AdaptError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid manifest: {0}")]
    Manifest(#[from] crate::manifest::ManifestError),
    #[error(transparent)]
    Load(#[from] crate::loader::LoadError),
    #[error("nothing to install: the source contains no components Locaryn can load")]
    Empty,
}

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub root: PathBuf,
    pub manifest: PluginManifest,
    pub ecosystem: ExtensionEcosystem,
    pub loaded: LoadedPlugin,
    /// What the adapter converted or skipped.
    pub notes: Vec<String>,
    /// Part of the bundle could not be represented in Locaryn.
    pub partial: bool,
    /// Canonical source string, stored so an update can re-fetch.
    pub source: String,
}

/// Install `spec` into `scope`, replacing any previous copy of the same plugin.
pub async fn install(
    http: &reqwest::Client,
    spec: &str,
    scope: ExtensionScope,
    workspace_root: Option<&Path>,
) -> Result<InstallOutcome, InstallError> {
    let scope_root = crate::scope_dir(scope, workspace_root);
    std::fs::create_dir_all(&scope_root)?;

    // Staging directory: a failed install must never leave a half-written
    // plugin where the loader would find it.
    let staging = scope_root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let result = install_into(http, spec, &staging, &scope_root).await;
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

async fn install_into(
    http: &reqwest::Client,
    spec: &str,
    staging: &Path,
    scope_root: &Path,
) -> Result<InstallOutcome, InstallError> {
    let (fetched, canonical, fallback_name) =
        if let Some(url) = spec.strip_prefix(MCP_REMOTE_PREFIX) {
            let name = remote_server_name(url);
            write_remote_mcp_plugin(staging, &name, url)?;
            (staging.to_path_buf(), spec.to_string(), name)
        } else {
            let parsed = source::parse(spec)?;
            let dir = source::fetch(http, &parsed, staging).await?;
            let fallback = parsed.label();
            let fallback = fallback
                .rsplit('/')
                .next()
                .unwrap_or(&fallback)
                .trim_end_matches(".git")
                .trim_end_matches(".zip")
                .to_string();
            (dir, parsed.canonical(), fallback)
        };

    let report = adapters::adapt(&fetched, &fallback_name)?;
    crate::manifest::validate(&report.manifest)?;

    let loaded = loader::load_with_manifest(&fetched, report.manifest.clone());
    if loaded.counts().is_empty() {
        return Err(InstallError::Empty);
    }

    // Move the staged copy to its final home, replacing a previous version.
    let final_root = scope_root.join(&report.manifest.name);
    if final_root.exists() {
        std::fs::remove_dir_all(&final_root)?;
    }
    move_dir(&fetched, &final_root)?;
    if staging.exists() {
        let _ = std::fs::remove_dir_all(staging);
    }

    // Re-load from the final location so every recorded path is the real one.
    let loaded = loader::load(&final_root)?;

    Ok(InstallOutcome {
        root: final_root,
        manifest: report.manifest,
        ecosystem: report.ecosystem,
        loaded,
        notes: report.notes,
        partial: report.partial,
        source: canonical,
    })
}

/// `rename` first — same volume, instant. Falls back to copy+delete, which is
/// what happens when the staging dir and the scope dir differ (a plugin
/// installed from a local path on another drive).
fn move_dir(from: &Path, to: &Path) -> Result<(), InstallError> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            source::copy_dir(from, to)?;
            let _ = std::fs::remove_dir_all(from);
            Ok(())
        }
    }
}

/// Synthesise a minimal plugin wrapping one remote MCP server.
fn write_remote_mcp_plugin(dir: &Path, name: &str, url: &str) -> Result<(), InstallError> {
    std::fs::create_dir_all(dir.join("mcp"))?;
    let mcp = serde_json::json!({
        "mcpServers": {
            name: { "url": url, "transport": "http", "auto_start": true }
        }
    });
    std::fs::write(
        dir.join("mcp/mcp.json"),
        serde_json::to_string_pretty(&mcp).unwrap_or_default(),
    )?;
    Ok(())
}

fn remote_server_name(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("mcp");
    adapters::sanitize_name(host)
}

/// Delete an installed plugin's files. Refuses to touch anything outside the
/// scope directory, so a corrupted row cannot turn removal into `rm -rf`.
pub fn remove_files(root: &Path, scope: ExtensionScope, workspace_root: Option<&Path>) -> bool {
    let scope_root = crate::scope_dir(scope, workspace_root);
    let (Ok(root_abs), Ok(scope_abs)) = (root.canonicalize(), scope_root.canonicalize()) else {
        return false;
    };
    if !root_abs.starts_with(&scope_abs) || root_abs == scope_abs {
        tracing::warn!(
            path = %root.display(),
            "refusing to delete an extension outside its scope directory"
        );
        return false;
    }
    std::fs::remove_dir_all(&root_abs).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_name_is_derived_from_the_host() {
        assert_eq!(
            remote_server_name("https://mcp.example.com/x"),
            "mcp-example-com"
        );
    }

    #[tokio::test]
    async fn installs_a_local_claude_code_plugin_end_to_end() {
        let base = std::env::temp_dir().join("locaryn-install-local");
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        std::fs::create_dir_all(src.join(".claude-plugin")).unwrap();
        std::fs::write(
            src.join(".claude-plugin/plugin.json"),
            r#"{"name":"Local Tool","version":"3.0.0"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(src.join("commands")).unwrap();
        std::fs::write(src.join("commands/go.md"), "---\nname: go\n---\nDo it").unwrap();

        let workspace = base.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let http = reqwest::Client::new();
        let out = install(
            &http,
            &src.display().to_string(),
            ExtensionScope::Workspace,
            Some(&workspace),
        )
        .await
        .expect("install succeeds");

        assert_eq!(out.manifest.name, "local-tool");
        assert_eq!(out.ecosystem, ExtensionEcosystem::ClaudeCode);
        assert_eq!(out.loaded.commands.len(), 1);
        assert!(out.root.ends_with("local-tool"));
        assert!(out.root.join("morph.json").is_file());
        // Staging directories are cleaned up.
        let leftovers: Vec<_> = std::fs::read_dir(workspace.join(".locaryn/plugins"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(".staging-"))
            .collect();
        assert!(leftovers.is_empty(), "staging dir left behind");
    }

    #[tokio::test]
    async fn installs_a_local_zip_end_to_end() {
        use std::io::Write;

        let base = std::env::temp_dir().join("locaryn-install-zip");
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        std::fs::create_dir_all(src.join(".claude-plugin")).unwrap();
        std::fs::write(
            src.join(".claude-plugin/plugin.json"),
            r#"{"name":"Zipped Tool","version":"2.0.0"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(src.join("commands")).unwrap();
        std::fs::write(src.join("commands/run.md"), "---\nname: run\n---\nRun it").unwrap();

        // Zip the plugin under one generated root, as GitHub codeload does.
        let zip_path = base.join("zipped-tool.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for entry in walk(&src) {
            let rel = entry.strip_prefix(&src).unwrap();
            let name = format!(
                "zipped-tool-2.0.0/{}",
                rel.to_string_lossy().replace('\\', "/")
            );
            if entry.is_dir() {
                writer.add_directory(name, options).expect("add_directory");
            } else {
                writer.start_file(name, options).expect("start_file");
                writer
                    .write_all(&std::fs::read(entry).unwrap())
                    .expect("write");
            }
        }
        writer.finish().expect("finish");

        let workspace = base.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();
        let http = reqwest::Client::new();
        let out = install(
            &http,
            &zip_path.display().to_string(),
            ExtensionScope::Workspace,
            Some(&workspace),
        )
        .await
        .expect("zip install succeeds");

        assert_eq!(out.manifest.name, "zipped-tool");
        assert_eq!(out.ecosystem, ExtensionEcosystem::ClaudeCode);
        assert_eq!(out.loaded.commands.len(), 1);
        assert!(out.root.join("morph.json").is_file());
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(dir) else {
            return out;
        };
        for e in rd.flatten() {
            if e.path().is_dir() {
                out.push(e.path());
                out.extend(walk(&e.path()));
            } else {
                out.push(e.path());
            }
        }
        out
    }

    #[tokio::test]
    async fn an_empty_bundle_is_refused() {
        let base = std::env::temp_dir().join("locaryn-install-empty");
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        std::fs::create_dir_all(src.join(".claude-plugin")).unwrap();
        std::fs::write(
            src.join(".claude-plugin/plugin.json"),
            r#"{"name":"hollow","version":"1.0.0"}"#,
        )
        .unwrap();
        let workspace = base.join("ws");
        std::fs::create_dir_all(&workspace).unwrap();

        let http = reqwest::Client::new();
        let err = install(
            &http,
            &src.display().to_string(),
            ExtensionScope::Workspace,
            Some(&workspace),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, InstallError::Empty));
    }

    /// Hits github.com. Ignored by default so CI stays offline-safe; run with
    /// `cargo test -p locaryn-extensions -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "requires network"]
    async fn installs_a_real_claude_code_plugin_from_github() {
        let base = std::env::temp_dir().join("locaryn-install-net");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let http = reqwest::Client::builder()
            .user_agent("locaryn/0.1")
            .build()
            .unwrap();

        let out = install(
            &http,
            "github:anthropics/claude-code#plugins/code-review",
            ExtensionScope::Workspace,
            Some(&base),
        )
        .await
        .expect("install from github");

        println!(
            "installed {} v{} ({:?}) — {:?}, notes: {:?}",
            out.manifest.name,
            out.manifest.version,
            out.ecosystem,
            out.loaded.counts(),
            out.notes
        );
        assert_eq!(out.ecosystem, ExtensionEcosystem::ClaudeCode);
        assert!(
            out.loaded.counts().total() > 0,
            "a real plugin must contribute something"
        );
        assert!(out.loaded.errors.is_empty(), "{:?}", out.loaded.errors);
        assert!(out.root.join("morph.json").is_file());
    }

    /// The Gemini path exercises the parts that are not a straight copy: TOML
    /// commands become markdown, and `mcpServers` is lifted out of the
    /// manifest into its own file.
    #[tokio::test]
    #[ignore = "requires network"]
    async fn installs_a_real_gemini_extension_from_github() {
        let base = std::env::temp_dir().join("locaryn-install-gemini");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let http = reqwest::Client::builder()
            .user_agent("locaryn/0.1")
            .build()
            .unwrap();

        let out = install(
            &http,
            "https://github.com/gemini-cli-extensions/security",
            ExtensionScope::Workspace,
            Some(&base),
        )
        .await
        .expect("install from github");

        println!(
            "installed {} v{} ({:?}) — {:?}\nnotes: {:#?}\nerrors: {:?}",
            out.manifest.name,
            out.manifest.version,
            out.ecosystem,
            out.loaded.counts(),
            out.notes,
            out.loaded.errors,
        );
        assert_eq!(out.ecosystem, ExtensionEcosystem::GeminiCli);
        assert!(out.loaded.counts().total() > 0);
        assert!(out.loaded.errors.is_empty(), "{:?}", out.loaded.errors);
    }

    #[test]
    fn remove_refuses_paths_outside_the_scope_dir() {
        let outside = std::env::temp_dir().join("locaryn-not-a-plugin");
        std::fs::create_dir_all(&outside).unwrap();
        let ws = std::env::temp_dir().join("locaryn-rm-ws");
        std::fs::create_dir_all(ws.join(".locaryn/plugins")).unwrap();
        assert!(!remove_files(
            &outside,
            ExtensionScope::Workspace,
            Some(&ws)
        ));
        assert!(outside.is_dir(), "the directory must survive");
    }
}
