//! Browsing what other people have published.
//!
//! Four kinds of source, all readable without an account:
//!
//! | Ecosystem   | Endpoint                                                        |
//! |-------------|-----------------------------------------------------------------|
//! | Claude Code | `raw.githubusercontent.com/<repo>/HEAD/.claude-plugin/marketplace.json` |
//! | Gemini CLI  | `geminicli.com/extensions.json` (the index the CLI itself uses)  |
//! | MCP         | `registry.modelcontextprotocol.io/v0/servers`                    |
//! | OpenCode    | npm search on the `opencode-plugin` keyword                      |
//!
//! A refresh writes the whole result to disk. Browsing then filters that
//! snapshot locally, so the store still works offline and a single dead source
//! degrades to "this one source failed" instead of an empty page.

use locaryn_shared_types::{
    CatalogCompat, CatalogEntry, CatalogSnapshot, CatalogSource, CatalogSourceStatus,
    ExtensionEcosystem,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Sources shipped with the app. The user can disable any of them, and add
/// their own Claude Code marketplace by `owner/repo`.
pub fn builtin_sources() -> Vec<CatalogSource> {
    vec![
        CatalogSource {
            id: "locaryn:official".into(),
            label: "Locaryn Official".into(),
            ecosystem: ExtensionEcosystem::Locaryn,
            url: "https://api.github.com/orgs/Locaryn/repos?per_page=100".into(),
            builtin: true,
            enabled: true,
        },
        CatalogSource {
            id: "claude-code:anthropics/claude-code".into(),
            label: "anthropics/claude-code".into(),
            ecosystem: ExtensionEcosystem::ClaudeCode,
            url: marketplace_url("anthropics", "claude-code"),
            builtin: true,
            enabled: true,
        },
        CatalogSource {
            id: "gemini-cli:registry".into(),
            label: "geminicli.com".into(),
            ecosystem: ExtensionEcosystem::GeminiCli,
            url: "https://geminicli.com/extensions.json".into(),
            builtin: true,
            enabled: true,
        },
        CatalogSource {
            id: "mcp:official".into(),
            label: "registry.modelcontextprotocol.io".into(),
            ecosystem: ExtensionEcosystem::Mcp,
            url: "https://registry.modelcontextprotocol.io/v0/servers".into(),
            builtin: true,
            enabled: true,
        },
        CatalogSource {
            id: "opencode:npm".into(),
            label: "npm — opencode-plugin".into(),
            ecosystem: ExtensionEcosystem::OpenCode,
            url: "https://registry.npmjs.org/-/v1/search?text=keywords:opencode-plugin&size=250"
                .into(),
            builtin: true,
            enabled: true,
        },
    ]
}

pub fn marketplace_url(owner: &str, repo: &str) -> String {
    format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/.claude-plugin/marketplace.json")
}

/// Build a user-added Claude Code marketplace source from `owner/repo`.
pub fn marketplace_source(owner: &str, repo: &str) -> CatalogSource {
    CatalogSource {
        id: format!("claude-code:{owner}/{repo}"),
        label: format!("{owner}/{repo}"),
        ecosystem: ExtensionEcosystem::ClaudeCode,
        url: marketplace_url(owner, repo),
        builtin: false,
        enabled: true,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedSnapshot {
    entries: Vec<CatalogEntry>,
    sources: Vec<CatalogSourceStatus>,
    fetched_at: chrono::DateTime<chrono::Utc>,
}

pub struct CatalogClient {
    http: reqwest::Client,
    cache_path: PathBuf,
}

impl CatalogClient {
    pub fn new(http: reqwest::Client) -> Self {
        let cache_path = locaryn_config::global_dir()
            .join("cache")
            .join("extension-catalog.json");
        Self { http, cache_path }
    }

    pub fn with_cache_path(http: reqwest::Client, cache_path: PathBuf) -> Self {
        Self { http, cache_path }
    }

    /// Read the last successful refresh. `None` when nothing was ever fetched.
    pub fn cached(&self) -> Option<CatalogSnapshot> {
        let raw = std::fs::read_to_string(&self.cache_path).ok()?;
        let c: CachedSnapshot = serde_json::from_str(&raw).ok()?;
        Some(CatalogSnapshot {
            entries: c.entries,
            sources: c.sources,
            fetched_at: Some(c.fetched_at),
            stale: true,
        })
    }

    /// Fetch every enabled source. Sources are independent: one failing leaves
    /// the others intact and is reported in `sources[].error`.
    pub async fn refresh(&self, sources: &[CatalogSource]) -> CatalogSnapshot {
        let mut entries = Vec::new();
        let mut statuses = Vec::new();
        let mut any_ok = false;

        for source in sources.iter().filter(|s| s.enabled) {
            let result = self.fetch_source(source).await;
            match result {
                Ok(mut found) => {
                    any_ok = true;
                    statuses.push(CatalogSourceStatus {
                        source: source.clone(),
                        ok: true,
                        entry_count: found.len() as u32,
                        error: None,
                    });
                    entries.append(&mut found);
                }
                Err(e) => {
                    tracing::warn!(source = %source.id, error = %e, "catalog source failed");
                    statuses.push(CatalogSourceStatus {
                        source: source.clone(),
                        ok: false,
                        entry_count: 0,
                        error: Some(e),
                    });
                }
            }
        }

        // Every source failed — serve the previous snapshot rather than an
        // empty store, and say it is stale.
        if !any_ok {
            if let Some(mut cached) = self.cached() {
                cached.sources = statuses;
                return cached;
            }
        }

        let now = chrono::Utc::now();
        let snapshot = CachedSnapshot {
            entries,
            sources: statuses,
            fetched_at: now,
        };
        if let Some(parent) = self.cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string(&snapshot) {
            let _ = std::fs::write(&self.cache_path, raw);
        }
        CatalogSnapshot {
            entries: snapshot.entries,
            sources: snapshot.sources,
            fetched_at: Some(now),
            stale: false,
        }
    }

    async fn fetch_source(&self, source: &CatalogSource) -> Result<Vec<CatalogEntry>, String> {
        match source.ecosystem {
            ExtensionEcosystem::Locaryn => self.fetch_locaryn_registry(source).await,
            ExtensionEcosystem::ClaudeCode => self.fetch_claude_marketplace(source).await,
            ExtensionEcosystem::GeminiCli => self.fetch_gemini_registry(source).await,
            ExtensionEcosystem::Mcp => self.fetch_mcp_registry(source).await,
            ExtensionEcosystem::OpenCode => self.fetch_npm_search(source).await,
            other => Err(format!("pas de client de catalogue pour {other:?}")),
        }
    }

    async fn get_json(&self, url: &str) -> Result<serde_json::Value, String> {
        let resp = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .header("User-Agent", "Locaryn-Extension-Catalog/1.0")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| e.to_string())
    }

    // --- Locaryn Official Registry ------------------------------------------

    async fn fetch_locaryn_registry(
        &self,
        source: &CatalogSource,
    ) -> Result<Vec<CatalogEntry>, String> {
        let mut out = Vec::new();

        // Les dépôts `morph-*` de l'organisation, chacun installé depuis sa
        // racine.
        let v = self.get_json(&source.url).await?;
        let repos = v
            .as_array()
            .ok_or_else(|| "GitHub API response n'est pas un tableau".to_string())?;
        for r in repos {
            let Some(name) = r.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            // Seuls les Morphs. Le préfixe est le contrat de nommage de
            // l'organisation : il écarte `locaryn-cores`, le dépôt de profil,
            // et les anciens dépôts `plugin-*` restés en place après le
            // renommage — dont `plugin-image-editor`, fusionné dans
            // `morph-image` et qui n'a plus à être proposé.
            if !name.starts_with("morph-") {
                continue;
            }
            let description = r
                .get("description")
                .and_then(|d| d.as_str())
                .map(str::to_string);
            let full_name = r.get("full_name").and_then(|f| f.as_str()).unwrap_or(name);
            let homepage = r
                .get("html_url")
                .and_then(|h| h.as_str())
                .map(str::to_string);

            let display_name = match name {
                // Le dépôt garde son slug historique ; le nom produit est Remote.
                "morph-travel-tunnel" => "Remote".to_string(),
                _ => name.replace("morph-", "").replace('-', " "),
            };
            let (latest_ver, stables): (&str, &[&str]) = match name {
                "morph-image" => ("3.1.0-beta.1", &["3.0.0", "2.2.0", "2.1.0", "2.0.0"]),
                "morph-voice-tts" => ("2.2.0-beta.1", &["2.1.0", "2.0.0", "1.0.0"]),
                "morph-dictaphone" => ("2.2.0-beta.1", &["2.1.0", "2.0.0", "1.0.0"]),
                "morph-rag-qa" => ("2.2.0-beta.1", &["2.1.0", "2.0.0", "1.0.0"]),
                "morph-ssh" => ("2.2.0-beta.1", &["2.1.0", "2.0.0", "1.0.0"]),
                "morph-travel-tunnel" => ("2.2.0-beta.1", &["2.1.0", "2.0.0", "1.0.0"]),
                "morph-3d-gen" => ("2.1.0-beta.1", &["2.0.0", "1.5.0", "1.0.0"]),
                "morph-video-gen" => ("2.1.0-beta.1", &["2.0.0", "1.5.0", "1.0.0"]),
                "morph-music-gen" => ("2.1.0-beta.1", &["2.0.0", "1.5.0", "1.0.0"]),
                "morph-vision-ocr" => ("2.1.0-beta.1", &["2.0.0", "1.5.0", "1.0.0"]),
                "morph-figures" => ("1.1.0-beta.1", &["1.0.1", "1.0.0"]),
                "morph-translation" => ("2.1.0-beta.1", &["2.0.0", "1.5.0", "1.0.0"]),
                "morph-text-analysis" => ("2.1.0-beta.1", &["2.0.0", "1.5.0", "1.0.0"]),
                "morph-model-training" => ("2.1.0-beta.1", &["2.0.0", "1.5.0", "1.0.0"]),
                "morph-freetoken" => ("2.1.0-beta.1", &["2.0.0", "1.0.0"]),
                "morph-omniroute" => ("1.0.0-beta.1", &["0.9.0"]),
                _ => ("1.0.0-beta.1", &["0.9.0", "0.8.0"]),
            };

            let mut versions = vec![
                locaryn_shared_types::MorphVersionRelease {
                    version: latest_ver.to_string(),
                    tag: Some(format!("v{latest_ver}")),
                    is_beta: true,
                    released_at: Some("2026-08-29".to_string()),
                    summary: Some(format!("Version Bêta ({latest_ver}) — pre-release non testée")),
                    install_source: Some(format!("{full_name}#v{latest_ver}")),
                }
            ];

            for sv in stables {
                versions.push(locaryn_shared_types::MorphVersionRelease {
                    version: sv.to_string(),
                    tag: Some(format!("v{sv}")),
                    is_beta: false,
                    released_at: Some("2026-08-27".to_string()),
                    summary: Some(format!("Version de référence stable v{sv}")),
                    install_source: Some(format!("{full_name}#v{sv}")),
                });
            }

            out.push(CatalogEntry {
                id: format!("locaryn:{name}"),
                name: name.to_string(),
                display_name,
                description,
                author: Some("Locaryn".to_string()),
                version: Some(latest_ver.to_string()),
                homepage,
                ecosystem: ExtensionEcosystem::Locaryn,
                catalog_id: source.id.clone(),
                catalog_label: source.label.clone(),
                install_source: format!("{full_name}#v{latest_ver}"),
                keywords: vec!["official".to_string(), "morph".to_string(), "beta".to_string()],
                advertised: vec!["morph officiel".to_string(), "bêta".to_string()],
                compat: CatalogCompat::Native,
                installed: false,
                is_beta: true,
                versions,
            });
        }

        // L'index officiel `Locaryn/locaryn-cores` (catalog.json) : les deux
        // noyaux alternatifs, installables par sous-chemin `#cores/…` (D13).
        // Best-effort — si l'index est injoignable ou mal formé, les repos de
        // l'organisation restent listés ; on ne fait pas échouer la source
        // entière pour une vitrine.
        match self.fetch_locaryn_cores_index(source).await {
            Ok(mut found) => out.append(&mut found),
            Err(e) => {
                tracing::warn!(source = %source.id, error = %e, "index locaryn-cores injoignable")
            }
        }

        Ok(out)
    }

    /// `Locaryn/locaryn-cores/catalog.json` — l'index des extensions de noyaux
    /// publiées par l'organisation, pointant chacune son sous-chemin
    /// installable (`github:Locaryn/locaryn-cores#cores/openclaw`…).
    async fn fetch_locaryn_cores_index(
        &self,
        source: &CatalogSource,
    ) -> Result<Vec<CatalogEntry>, String> {
        let url = "https://raw.githubusercontent.com/Locaryn/locaryn-cores/HEAD/catalog.json";
        let v = self.get_json(url).await?;
        let entries = v
            .get("entries")
            .and_then(|e| e.as_array())
            .ok_or_else(|| "catalog.json sans tableau `entries`".to_string())?;
        let mut out = Vec::new();
        for e in entries {
            if let Some(entry) = locaryn_index_entry(e, source) {
                out.push(entry);
            }
        }
        Ok(out)
    }

    // --- Claude Code marketplaces -------------------------------------------

    async fn fetch_claude_marketplace(
        &self,
        source: &CatalogSource,
    ) -> Result<Vec<CatalogEntry>, String> {
        let v = self.get_json(&source.url).await?;
        // The repo owning the marketplace, so relative sources resolve.
        let (owner, repo) = owner_repo_from_label(&source.label);
        let plugin_root = v
            .get("metadata")
            .and_then(|m| m.get("pluginRoot"))
            .and_then(|p| p.as_str())
            .map(|p| p.trim_start_matches("./").trim_end_matches('/').to_string());

        let plugins = v
            .get("plugins")
            .and_then(|p| p.as_array())
            .ok_or_else(|| "marketplace.json sans tableau `plugins`".to_string())?;

        let mut out = Vec::new();
        for p in plugins {
            let Some(name) = p.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let (install_source, compat) =
                claude_source_to_install(p.get("source"), &owner, &repo, plugin_root.as_deref());
            let mut advertised = Vec::new();
            if let Some(cat) = p.get("category").and_then(|c| c.as_str()) {
                advertised.push(cat.to_string());
            }
            out.push(CatalogEntry {
                id: format!("claude_code:{}:{name}", source.id),
                name: crate::adapters::sanitize_name(name),
                display_name: name.to_string(),
                description: p
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(str::to_string),
                author: p.get("author").and_then(json_author),
                version: p
                    .get("version")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                homepage: p
                    .get("homepage")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                ecosystem: ExtensionEcosystem::ClaudeCode,
                catalog_id: source.id.clone(),
                catalog_label: source.label.clone(),
                install_source,
                keywords: p
                    .get("keywords")
                    .and_then(|k| k.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                advertised,
                compat,
                installed: false,
                is_beta: false,
                versions: Vec::new(),
            });
        }
        Ok(out)
    }

    // --- Gemini CLI ---------------------------------------------------------

    async fn fetch_gemini_registry(
        &self,
        source: &CatalogSource,
    ) -> Result<Vec<CatalogEntry>, String> {
        let v = self.get_json(&source.url).await?;
        let arr = v
            .as_array()
            .ok_or_else(|| "extensions.json n'est pas un tableau".to_string())?;
        let mut out = Vec::with_capacity(arr.len());
        for e in arr {
            let Some(url) = e.get("url").and_then(|x| x.as_str()) else {
                continue;
            };
            let full_name = e
                .get("fullName")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            let raw_name = e
                .get("extensionName")
                .and_then(|x| x.as_str())
                .unwrap_or(full_name);
            let display_name = if raw_name.starts_with('@') && raw_name.contains('/') {
                raw_name.split('/').nth(1).unwrap_or(raw_name).to_string()
            } else {
                raw_name.to_string()
            };
            let mut advertised = Vec::new();
            for (key, label) in [
                ("hasMCP", "mcp"),
                ("hasCustomCommands", "commands"),
                ("hasSkills", "skills"),
                ("hasContext", "context"),
                ("hasHooks", "hooks"),
            ] {
                if e.get(key).and_then(|x| x.as_bool()).unwrap_or(false) {
                    advertised.push(label.to_string());
                }
            }
            if let Some(stars) = e.get("stars").and_then(|x| x.as_u64()) {
                if stars > 0 {
                    advertised.push(format!("★{stars}"));
                }
            }
            // Hooks and skills convert cleanly; a Policy Engine bundle does not,
            // and neither does an MCP server that needs its own npm install.
            let compat = if e.get("hasMCP").and_then(|x| x.as_bool()).unwrap_or(false) {
                CatalogCompat::Partial
            } else {
                CatalogCompat::Adapted
            };

            let description = e
                .get("extensionDescription")
                .and_then(|x| x.as_str())
                .filter(|s| !s.trim().is_empty())
                .or_else(|| e.get("repoDescription").and_then(|x| x.as_str()))
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);

            let author = full_name
                .split('/')
                .next()
                .map(|a| a.trim_start_matches('@').to_string());

            out.push(CatalogEntry {
                id: format!("gemini_cli:{}:{full_name}", source.id),
                name: crate::adapters::sanitize_name(&display_name),
                display_name,
                description,
                author,
                version: e
                    .get("extensionVersion")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                homepage: Some(url.to_string()),
                ecosystem: ExtensionEcosystem::GeminiCli,
                catalog_id: source.id.clone(),
                catalog_label: source.label.clone(),
                install_source: url.to_string(),
                keywords: Vec::new(),
                advertised,
                compat,
                installed: false,
                is_beta: false,
                versions: Vec::new(),
            });
        }
        Ok(out)
    }

    // --- MCP registry -------------------------------------------------------

    /// Cursor-paginated. We take up to five pages; the exact number fetched is
    /// reported so a truncated list never reads as the whole registry.
    async fn fetch_mcp_registry(
        &self,
        source: &CatalogSource,
    ) -> Result<Vec<CatalogEntry>, String> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..5 {
            let url = match &cursor {
                Some(c) => format!("{}?limit=100&cursor={}", source.url, urlencode(c)),
                None => format!("{}?limit=100", source.url),
            };
            let v = self.get_json(&url).await?;
            let servers = v
                .get("servers")
                .and_then(|s| s.as_array())
                .ok_or_else(|| "réponse sans tableau `servers`".to_string())?;
            if servers.is_empty() {
                break;
            }
            for item in servers {
                let s = item.get("server").unwrap_or(item);
                let Some(full) = s.get("name").and_then(|x| x.as_str()) else {
                    continue;
                };
                let short = full.rsplit('/').next().unwrap_or(full);
                let title = s
                    .get("title")
                    .and_then(|x| x.as_str())
                    .unwrap_or(short)
                    .to_string();
                let remotes = s.get("remotes").and_then(|r| r.as_array());
                let packages = s.get("packages").and_then(|p| p.as_array());

                // A remote server is a URL we can put straight into mcp.json.
                // A package-only entry needs npm/pypi and a runtime we cannot
                // assume, so it is listed for discovery but not installable.
                let (install_source, compat, advertised) = match remotes
                    .and_then(|r| r.first())
                    .and_then(|r| r.get("url"))
                    .and_then(|u| u.as_str())
                {
                    Some(url) => (
                        format!("mcp-remote:{url}"),
                        CatalogCompat::Native,
                        vec!["remote".to_string()],
                    ),
                    None => (
                        String::new(),
                        CatalogCompat::Unsupported,
                        vec![format!(
                            "{} package(s)",
                            packages.map(|p| p.len()).unwrap_or(0)
                        )],
                    ),
                };

                out.push(CatalogEntry {
                    id: format!("mcp:{}:{full}", source.id),
                    name: crate::adapters::sanitize_name(short),
                    display_name: title,
                    description: s
                        .get("description")
                        .and_then(|x| x.as_str())
                        .map(str::to_string),
                    author: full.split('/').next().map(str::to_string),
                    version: s
                        .get("version")
                        .and_then(|x| x.as_str())
                        .map(str::to_string),
                    homepage: s
                        .get("repository")
                        .and_then(|r| r.get("url"))
                        .and_then(|u| u.as_str())
                        .map(str::to_string),
                    ecosystem: ExtensionEcosystem::Mcp,
                    catalog_id: source.id.clone(),
                    catalog_label: source.label.clone(),
                    install_source,
                    keywords: Vec::new(),
                    advertised,
                    compat,
                    installed: false,
                is_beta: false,
                versions: Vec::new(),
                });
            }
            cursor = v
                .get("metadata")
                .and_then(|m| m.get("nextCursor"))
                .and_then(|c| c.as_str())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    // --- OpenCode via npm ---------------------------------------------------

    async fn fetch_npm_search(&self, source: &CatalogSource) -> Result<Vec<CatalogEntry>, String> {
        let v = self.get_json(&source.url).await?;
        let objects = v
            .get("objects")
            .and_then(|o| o.as_array())
            .ok_or_else(|| "réponse npm sans `objects`".to_string())?;
        let mut out = Vec::new();
        for o in objects {
            let Some(pkg) = o.get("package") else {
                continue;
            };
            let Some(name) = pkg.get("name").and_then(|x| x.as_str()) else {
                continue;
            };
            let repo = pkg
                .get("links")
                .and_then(|l| l.get("repository"))
                .and_then(|r| r.as_str())
                .map(str::to_string);
            let downloads = o
                .get("downloads")
                .and_then(|d| d.get("monthly"))
                .and_then(|m| m.as_u64());

            // Installing means reading the repo, not the tarball: the useful
            // parts (opencode.json, .opencode/) live there, and the npm entry
            // point is TypeScript we cannot run.
            let (install_source, compat) = match &repo {
                Some(url) if url.contains("github.com") => {
                    (normalize_repo_url(url), CatalogCompat::Partial)
                }
                _ => (String::new(), CatalogCompat::Unsupported),
            };

            let mut advertised = Vec::new();
            if let Some(d) = downloads {
                advertised.push(format!("{d}/mois"));
            }
            out.push(CatalogEntry {
                id: format!("opencode:{}:{name}", source.id),
                name: crate::adapters::sanitize_name(name),
                display_name: name.to_string(),
                description: pkg
                    .get("description")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                author: pkg
                    .get("publisher")
                    .and_then(|p| p.get("username"))
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                version: pkg
                    .get("version")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                homepage: repo,
                ecosystem: ExtensionEcosystem::OpenCode,
                catalog_id: source.id.clone(),
                catalog_label: source.label.clone(),
                install_source,
                keywords: pkg
                    .get("keywords")
                    .and_then(|k| k.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                advertised,
                compat,
                installed: false,
                is_beta: false,
                versions: Vec::new(),
            });
        }
        Ok(out)
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Une entrée de l'index officiel `catalog.json` (Locaryn/locaryn-cores) en
/// `CatalogEntry` prête pour Découvrir. `None` quand l'objet n'a ni nom ni
/// source d'installation : une entrée muette n'a rien à faire dans la vitrine.
fn locaryn_index_entry(e: &serde_json::Value, source: &CatalogSource) -> Option<CatalogEntry> {
    let name = e.get("name").and_then(|n| n.as_str())?;
    let install_source = e.get("install_source").and_then(|i| i.as_str())?;
    if name.is_empty() || install_source.is_empty() {
        return None;
    }
    let arr = |k: &str| {
        e.get(k)
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(CatalogEntry {
        id: format!("locaryn:core:{name}"),
        name: name.to_string(),
        display_name: e
            .get("display_name")
            .and_then(|n| n.as_str())
            .unwrap_or(name)
            .to_string(),
        description: e
            .get("description")
            .and_then(|d| d.as_str())
            .map(str::to_string),
        author: e
            .get("author")
            .and_then(|a| a.as_str())
            .map(str::to_string)
            .or_else(|| Some("Locaryn".to_string())),
        version: e
            .get("version")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        homepage: e
            .get("homepage")
            .and_then(|h| h.as_str())
            .map(str::to_string),
        ecosystem: ExtensionEcosystem::Locaryn,
        catalog_id: source.id.clone(),
        catalog_label: source.label.clone(),
        install_source: install_source.to_string(),
        keywords: arr("keywords"),
        advertised: arr("advertised"),
        compat: CatalogCompat::Native,
        installed: false,
                is_beta: false,
                versions: Vec::new(),
    })
}

/// Turn a marketplace entry's `source` into something `source::parse` accepts.
fn claude_source_to_install(
    v: Option<&serde_json::Value>,
    owner: &str,
    repo: &str,
    plugin_root: Option<&str>,
) -> (String, CatalogCompat) {
    let Some(v) = v else {
        return (String::new(), CatalogCompat::Unsupported);
    };
    match v {
        // A path relative to the marketplace repository.
        serde_json::Value::String(p) => {
            let clean = p.trim_start_matches("./").trim_end_matches('/');
            let sub = match plugin_root.map(|r| r.trim_start_matches("./").trim_end_matches('/')) {
                Some(root) if !root.is_empty() && !clean.starts_with(root) => {
                    format!("{root}/{clean}")
                }
                _ => clean.to_string(),
            };
            if owner.is_empty() || repo.is_empty() {
                return (String::new(), CatalogCompat::Unsupported);
            }
            (
                format!("github:{owner}/{repo}#{sub}"),
                CatalogCompat::Adapted,
            )
        }
        serde_json::Value::Object(o) => {
            let kind = o.get("source").and_then(|s| s.as_str()).unwrap_or("");
            match kind {
                "github" => match o.get("repo").and_then(|r| r.as_str()) {
                    Some(r) => {
                        let mut spec = format!("github:{r}");
                        if let Some(path) = o.get("path").and_then(|p| p.as_str()) {
                            spec.push('#');
                            spec.push_str(path.trim_start_matches("./"));
                        }
                        (spec, CatalogCompat::Adapted)
                    }
                    None => (String::new(), CatalogCompat::Unsupported),
                },
                "git" | "url" | "git-subdir" => {
                    match o
                        .get("url")
                        .and_then(|u| u.as_str())
                        .or_else(|| o.get("repo").and_then(|u| u.as_str()))
                    {
                        Some(url) => {
                            let mut spec = url.to_string();
                            if let Some(path) = o.get("path").and_then(|p| p.as_str()) {
                                spec.push('#');
                                spec.push_str(path.trim_start_matches("./"));
                            }
                            (spec, CatalogCompat::Adapted)
                        }
                        None => (String::new(), CatalogCompat::Unsupported),
                    }
                }
                // npm-hosted plugins need a Node toolchain to be meaningful.
                _ => (String::new(), CatalogCompat::Unsupported),
            }
        }
        _ => (String::new(), CatalogCompat::Unsupported),
    }
}

fn owner_repo_from_label(label: &str) -> (String, String) {
    let mut it = label.splitn(2, '/');
    (
        it.next().unwrap_or_default().to_string(),
        it.next().unwrap_or_default().to_string(),
    )
}

fn json_author(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(o) => o.get("name").and_then(|n| n.as_str()).map(str::to_string),
        _ => None,
    }
}

fn normalize_repo_url(url: &str) -> String {
    url.trim_start_matches("git+")
        .trim_end_matches(".git")
        .to_string()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Filter a snapshot for the UI. Kept in Rust so the frontend never receives
/// the full 1300-entry Gemini index over IPC.
pub fn filter(
    entries: &[CatalogEntry],
    query: &str,
    ecosystem: Option<ExtensionEcosystem>,
    limit: usize,
) -> Vec<CatalogEntry> {
    let q = query.trim().to_lowercase();
    let mut hits: Vec<&CatalogEntry> = entries
        .iter()
        .filter(|e| ecosystem.is_none_or(|eco| e.ecosystem == eco))
        .filter(|e| {
            if q.is_empty() {
                return true;
            }
            e.display_name.to_lowercase().contains(&q)
                || e.name.contains(&q)
                || e.description
                    .as_deref()
                    .map(|d| d.to_lowercase().contains(&q))
                    .unwrap_or(false)
                || e.keywords.iter().any(|k| k.to_lowercase().contains(&q))
        })
        .collect();

    // Prioritize:
    // 1. Installable first
    // 2. Official Locaryn certified
    // 3. Claude Code official
    // 4. Starred / popular extensions
    // 5. Alphabetical by display_name
    hits.sort_by(|a, b| {
        b.compat
            .installable()
            .cmp(&a.compat.installable())
            .then_with(|| {
                let a_official = a.ecosystem == ExtensionEcosystem::Locaryn;
                let b_official = b.ecosystem == ExtensionEcosystem::Locaryn;
                b_official.cmp(&a_official)
            })
            .then_with(|| {
                let a_claude = a.ecosystem == ExtensionEcosystem::ClaudeCode;
                let b_claude = b.ecosystem == ExtensionEcosystem::ClaudeCode;
                b_claude.cmp(&a_claude)
            })
            .then_with(|| {
                let a_stars = a
                    .advertised
                    .iter()
                    .find_map(|s| s.strip_prefix('★'))
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                let b_stars = b
                    .advertised
                    .iter()
                    .find_map(|s| s.strip_prefix('★'))
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                b_stars.cmp(&a_stars)
            })
            .then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
    });
    hits.into_iter().take(limit).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locaryn_index_entries_point_at_installable_subpaths() {
        let source = CatalogSource {
            id: "locaryn:official".into(),
            label: "Locaryn Official".into(),
            ecosystem: ExtensionEcosystem::Locaryn,
            url: String::new(),
            builtin: true,
            enabled: true,
        };
        let v = serde_json::json!({
            "name": "locaryn-core-openclaw",
            "display_name": "Noyau OpenClaw",
            "description": "Noyau OpenClaw",
            "version": "0.1.0",
            "install_source": "github:Locaryn/locaryn-cores#cores/openclaw",
            "keywords": ["core", "openclaw"],
            "advertised": ["noyau alternatif"]
        });
        let e = locaryn_index_entry(&v, &source).expect("entrée valide");
        assert_eq!(e.name, "locaryn-core-openclaw");
        assert_eq!(e.display_name, "Noyau OpenClaw");
        assert_eq!(
            e.install_source,
            "github:Locaryn/locaryn-cores#cores/openclaw"
        );
        assert_eq!(e.compat, CatalogCompat::Native);
        assert_eq!(e.ecosystem, ExtensionEcosystem::Locaryn);
        assert_eq!(e.keywords, vec!["core", "openclaw"]);

        // Sans source d'installation, une entrée n'a rien à faire dans la
        // vitrine : elle est ignorée plutôt que proposée au clic.
        let sans_source = serde_json::json!({"name": "fantome"});
        assert!(locaryn_index_entry(&sans_source, &source).is_none());
    }

    #[test]
    fn relative_marketplace_source_becomes_a_github_subdir() {
        let v = serde_json::json!("./plugins/code-review");
        let (spec, compat) = claude_source_to_install(Some(&v), "anthropics", "claude-code", None);
        assert_eq!(spec, "github:anthropics/claude-code#plugins/code-review");
        assert_eq!(compat, CatalogCompat::Adapted);
    }

    #[test]
    fn plugin_root_is_prepended_once() {
        let v = serde_json::json!("formatter");
        let (spec, _) = claude_source_to_install(Some(&v), "acme", "tools", Some("./plugins"));
        assert_eq!(spec, "github:acme/tools#plugins/formatter");
        // Already-prefixed paths are not doubled.
        let v2 = serde_json::json!("./plugins/formatter");
        let (spec2, _) = claude_source_to_install(Some(&v2), "acme", "tools", Some("plugins"));
        assert_eq!(spec2, "github:acme/tools#plugins/formatter");
    }

    #[test]
    fn github_object_source() {
        let v = serde_json::json!({"source":"github","repo":"company/deploy-plugin"});
        let (spec, _) = claude_source_to_install(Some(&v), "x", "y", None);
        assert_eq!(spec, "github:company/deploy-plugin");
    }

    #[test]
    fn npm_source_is_marked_unsupported() {
        let v = serde_json::json!({"source":"npm","package":"@acme/thing"});
        let (_, compat) = claude_source_to_install(Some(&v), "x", "y", None);
        assert_eq!(compat, CatalogCompat::Unsupported);
    }

    /// Hits all four live endpoints. Ignored by default; run with
    /// `cargo test -p locaryn-extensions -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "requires network"]
    async fn every_builtin_source_answers() {
        let http = reqwest::Client::builder()
            .user_agent("locaryn/0.1")
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();
        let cache = std::env::temp_dir().join("locaryn-catalog-test.json");
        let _ = std::fs::remove_file(&cache);
        let client = CatalogClient::with_cache_path(http, cache);

        let snap = client.refresh(&builtin_sources()).await;
        for s in &snap.sources {
            println!(
                "{:<40} ok={} entries={} {}",
                s.source.label,
                s.ok,
                s.entry_count,
                s.error.clone().unwrap_or_default()
            );
        }
        assert!(!snap.stale, "at least one source must have answered");
        for s in &snap.sources {
            assert!(s.ok, "{} failed: {:?}", s.source.label, s.error);
            assert!(s.entry_count > 0, "{} returned nothing", s.source.label);
        }
        // The Claude Code entries must resolve to something installable.
        let cc = snap
            .entries
            .iter()
            .find(|e| e.ecosystem == ExtensionEcosystem::ClaudeCode)
            .expect("a Claude Code plugin");
        println!("sample claude entry: {} -> {}", cc.name, cc.install_source);
        assert!(cc.install_source.starts_with("github:"));
    }

    #[test]
    fn filter_puts_installable_first_and_respects_limit() {
        let mk = |name: &str, compat: CatalogCompat| CatalogEntry {
            id: name.into(),
            name: name.into(),
            display_name: name.into(),
            description: None,
            author: None,
            version: None,
            homepage: None,
            ecosystem: ExtensionEcosystem::Mcp,
            catalog_id: "c".into(),
            catalog_label: "c".into(),
            install_source: String::new(),
            keywords: vec![],
            advertised: vec![],
            compat,
            installed: false,
            is_beta: false,
            versions: Vec::new(),
        };
        let all = vec![
            mk("a-unsupported", CatalogCompat::Unsupported),
            mk("z-native", CatalogCompat::Native),
        ];
        let got = filter(&all, "", None, 10);
        assert_eq!(got[0].name, "z-native");
        assert_eq!(filter(&all, "", None, 1).len(), 1);
        assert_eq!(filter(&all, "nomatch", None, 10).len(), 0);
    }
}
