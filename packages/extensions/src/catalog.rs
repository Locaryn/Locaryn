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

use futures::StreamExt as _;
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

        // Les sources sont indépendantes : la plus lente fixe la durée, pas
        // leur somme.
        let actives: Vec<&CatalogSource> = sources.iter().filter(|s| s.enabled).collect();
        let taches: Vec<_> = actives.iter().map(|s| self.fetch_source(s)).collect();
        let resultats = futures::future::join_all(taches).await;
        for (source, result) in actives.into_iter().zip(resultats) {
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
            // Une source qui ne répond pas ne doit pas tenir tout le
            // catalogue en otage.
            .timeout(std::time::Duration::from_secs(REQUETE_DELAI_SECS))
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
        // Les dépôts `morph-*` de l'organisation, chacun installé depuis sa
        // racine.
        let repos = self.depots_de_l_organisation(source).await?;
        // Deux requêtes par dépôt (manifeste, releases) : faites l'une après
        // l'autre pour une vingtaine de dépôts, « Actualiser » prenait 15 à
        // 30 s (Locaryn/Locaryn#82). En parallèle, bornées, ordre conservé.
        // Les futurs sont rassemblés avant le flux : une fermeture qui les
        // fabrique à la volée rend le tout non `Send`, et les commandes de
        // l'application, qui l'exigent, ne compilent plus.
        let taches: Vec<_> = repos.iter().map(|r| self.entree_morph(source, r)).collect();
        let entrees: Vec<Option<CatalogEntry>> = futures::stream::iter(taches)
            .buffered(DEPOTS_EN_PARALLELE)
            .collect()
            .await;
        let mut out: Vec<CatalogEntry> = entrees.into_iter().flatten().collect();

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

    /// Les dépôts de l'organisation, par l'API GitHub. Quota épuisé (403) :
    /// ceux de la dernière actualisation réussie — leurs manifestes et leurs
    /// releases se relisent hors quota, seule l'apparition d'un nouveau dépôt
    /// attend l'heure suivante.
    async fn depots_de_l_organisation(
        &self,
        source: &CatalogSource,
    ) -> Result<Vec<serde_json::Value>, String> {
        let erreur = match self.get_json(&source.url).await {
            Ok(v) => {
                return v
                    .as_array()
                    .cloned()
                    .ok_or_else(|| "GitHub API response n'est pas un tableau".to_string())
            }
            Err(e) => e,
        };
        let connus = depots_connus(self.cached().as_ref(), &source.id);
        if connus.is_empty() {
            return Err(erreur);
        }
        tracing::warn!(source = %source.id, erreur = %erreur, depots = connus.len(), "liste des dépôts refusée, dépôts connus relus");
        Ok(connus)
    }

    /// L'entrée d'un dépôt de l'organisation, ou rien si ce n'est pas un morph.
    async fn entree_morph(
        &self,
        source: &CatalogSource,
        r: &serde_json::Value,
    ) -> Option<CatalogEntry> {
        let name = r.get("name").and_then(|n| n.as_str())?;
        // Seuls les Morphs. Le préfixe est le contrat de nommage de
        // l'organisation : il écarte `locaryn-cores`, le dépôt de profil,
        // et les anciens dépôts `plugin-*` restés en place après le
        // renommage, qui n'ont plus à être proposés.
        if !name.starts_with("morph-") {
            return None;
        }
        {
            let full_name = r.get("full_name").and_then(|f| f.as_str()).unwrap_or(name);
            let homepage = r
                .get("html_url")
                .and_then(|h| h.as_str())
                .map(str::to_string);

            // Le tagline GitHub du dépôt reste positif quoi qu'il arrive
            // ("Official Locaryn extension: …") — ce que morph.json dit de
            // lui-même, lui, porte le vrai statut : plusieurs morphs
            // commencent leur description par « NON FONCTIONNEL (squelette
            // d'API) ». Avant ce correctif, cette mention n'apparaissait
            // qu'après installation (Locaryn/Locaryn#6) : la carte Découvrir
            // montrait le tagline, jamais le manifeste. Best-effort — un
            // manifeste injoignable retombe sur le tagline plutôt que de
            // faire échouer tout le registre pour un seul dépôt.
            let manifeste = self
                .get_json(&format!(
                    "https://raw.githubusercontent.com/Locaryn/{name}/HEAD/morph.json"
                ))
                .await
                .ok();
            let manifest_own_description = manifeste.as_ref().and_then(|m| {
                m.get("description")
                    .and_then(|d| d.as_str())
                    .map(str::to_string)
            });
            let description = manifest_own_description.or_else(|| {
                r.get("description")
                    .and_then(|d| d.as_str())
                    .map(str::to_string)
            });

            // Le nom produit vient du manifeste (`displayName`) : aucune table
            // de noms tenue ici. À défaut, le slug sans son préfixe.
            let display_name = manifeste
                .as_ref()
                .and_then(|m| m.get("displayName"))
                .and_then(|d| d.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| name.trim_start_matches("morph-").replace('-', " "));
            // Versions réelles d'abord : les releases GitHub du dépôt sont la
            // source de vérité. À défaut, la version que le morph déclare
            // lui-même dans son manifeste — jamais une table tenue dans
            // l'application, qui nommait chaque morph officiel et retardait
            // sur leurs publications. Sans l'une ni l'autre, aucune version
            // inventée : un tag fantôme faisait échouer l'installation.
            let versions: Vec<locaryn_shared_types::MorphVersionRelease> =
                match self.fetch_morph_releases(full_name).await {
                    Some(v) if !v.is_empty() => v,
                    _ => version_declaree(manifeste.as_ref(), full_name),
                };
            // La dernière version décide de la maturité affichée : une
            // pre-release rend l'entrée bêta, une version stable la laisse
            // verte. L'écosystème ne suffit pas — marquer bêta tous les morphs
            // officiels rendait le filtre bêta muet et le point orange vide de
            // sens.
            let derniere_est_beta = versions.first().is_some_and(|v| v.is_beta);
            let latest_ver = versions
                .first()
                .map(|v| v.version.clone())
                .unwrap_or_default();
            let mut keywords = vec!["official".to_string(), "morph".to_string()];
            let mut advertised = vec!["morph officiel".to_string()];
            if derniere_est_beta {
                keywords.push("beta".to_string());
                advertised.push("bêta".to_string());
            }

            Some(CatalogEntry {
                id: format!("locaryn:{name}"),
                name: name.to_string(),
                display_name,
                description,
                author: Some("Locaryn".to_string()),
                version: if latest_ver.is_empty() {
                    None
                } else {
                    Some(latest_ver.clone())
                },
                homepage,
                ecosystem: ExtensionEcosystem::Locaryn,
                catalog_id: source.id.clone(),
                catalog_label: source.label.clone(),
                // Sans version connue : la racine du dépôt — release
                // « latest » puis archive du code — plutôt qu'un tag fantôme
                // qui fait échouer l'installation.
                install_source: if latest_ver.is_empty() {
                    full_name.to_string()
                } else {
                    format!("{full_name}@v{latest_ver}")
                },
                keywords,
                advertised,
                compat: CatalogCompat::Native,
                installed: false,
                is_beta: derniere_est_beta,
                versions,
            })
        }
    }

    /// Releases GitHub d'un dépôt morph, plus récentes d'abord, plafonnées à
    /// [`MORPH_RELEASES_PER_REPO`]. `None` = injoignable ou réponse illisible :
    /// l'appelant retombe sur la table connue plutôt que de faire échouer la
    /// source entière.
    async fn fetch_morph_releases(
        &self,
        full_name: &str,
    ) -> Option<Vec<locaryn_shared_types::MorphVersionRelease>> {
        // Le flux Atom d'abord : public et hors quota. L'API sans jeton
        // plafonne à 60 requêtes par heure et par adresse, et ce catalogue en
        // demandait une par morph — deux actualisations l'épuisaient, et la
        // liste des dépôts de l'organisation, qui n'a pas d'autre source,
        // tombait avec elle.
        if let Some(found) = self.releases_atom(full_name).await {
            return Some(found);
        }
        let url = format!(
            "https://api.github.com/repos/{full_name}/releases?per_page={MORPH_RELEASES_PER_REPO}"
        );
        match self.get_json(&url).await {
            Ok(v) => Some(
                v.as_array()?
                    .iter()
                    .filter_map(|r| release_from_json(r, full_name))
                    .collect(),
            ),
            Err(e) => {
                tracing::warn!(depot = %full_name, erreur = %e, "releases illisibles (flux et API)");
                None
            }
        }
    }

    /// Les releases du flux Atom ; `None` s'il est injoignable ou vide.
    async fn releases_atom(
        &self,
        full_name: &str,
    ) -> Option<Vec<locaryn_shared_types::MorphVersionRelease>> {
        let url = format!("https://github.com/{full_name}/releases.atom");
        let resp = self
            .http
            .get(&url)
            .header("User-Agent", "Locaryn-Extension-Catalog/1.0")
            .timeout(std::time::Duration::from_secs(REQUETE_DELAI_SECS))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let xml = resp.text().await.ok()?;
        let found = releases_from_atom(&xml, full_name);
        (!found.is_empty()).then_some(found)
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

/// Nombre maximal de releases lues par dépôt morph. La liste du catalogue
/// n'a pas besoin de l'historique complet : la plus récente d'abord suffit,
/// et plafonner protège le quota de l'API GitHub (60 req/h sans jeton).
const MORPH_RELEASES_PER_REPO: usize = 10;

/// Dépôts lus en même temps. Assez pour que la liste se remplisse en une ou
/// deux secondes, pas au point d'inonder GitHub.
const DEPOTS_EN_PARALLELE: usize = 8;

/// Délai d'une requête de catalogue.
const REQUETE_DELAI_SECS: u64 = 15;

/// Une release GitHub → une entrée de versions du catalogue.
///
/// `draft` est écartée (invisible côté GitHub jusqu'à publication) ; une
/// release marquée `prerelease` — ou un numéro portant un suffixe (`-beta.1`,
/// `-rc.1`), même si GitHub ne l'a pas cochée — devient « bêta » ; une release
/// stable reste verte. Le tag `v0.2.0` devient la version `0.2.0` et la
/// source d'installation `owner/repo@v0.2.0` (grammaire des sources : `@`
/// épingle un tag, `#` désigne un sous-dossier).
/// Les releases d'un flux Atom GitHub (`…/releases.atom`), plus récentes
/// d'abord. Le flux ne dit pas « pre-release » : une version à suffixe
/// (`-beta.1`) en est une, comme partout ailleurs dans ce catalogue.
pub(crate) fn releases_from_atom(
    xml: &str,
    full_name: &str,
) -> Vec<locaryn_shared_types::MorphVersionRelease> {
    let marque = format!("https://github.com/{full_name}/releases/tag/");
    xml.split("<entry>")
        .skip(1)
        .filter_map(|entree| {
            let debut = entree.find(&marque)? + marque.len();
            let tag = &entree[debut..];
            let tag = &tag[..tag.find('"')?];
            let publie = entree
                .split_once("<updated>")
                .and_then(|(_, r)| r.split_once("</updated>"))
                .map(|(d, _)| d.trim().to_string());
            release_from_json(
                &serde_json::json!({
                    "tag_name": tag,
                    "prerelease": tag.contains('-'),
                    "published_at": publie,
                }),
                full_name,
            )
        })
        .take(MORPH_RELEASES_PER_REPO)
        .collect()
}

/// Les dépôts morphs d'une source tels que la dernière actualisation les a
/// vus, sous la forme que rend l'API GitHub (`name`, `full_name`…).
fn depots_connus(snapshot: Option<&CatalogSnapshot>, source_id: &str) -> Vec<serde_json::Value> {
    let Some(snapshot) = snapshot else {
        return Vec::new();
    };
    snapshot
        .entries
        .iter()
        .filter(|e| e.catalog_id == source_id && e.name.starts_with("morph-"))
        .filter_map(|e| {
            let full_name = e.install_source.split('@').next()?.trim();
            full_name.contains('/').then(|| {
                serde_json::json!({
                    "name": e.name,
                    "full_name": full_name,
                    "html_url": e.homepage,
                    "description": e.description,
                })
            })
        })
        .collect()
}

fn release_from_json(
    r: &serde_json::Value,
    full_name: &str,
) -> Option<locaryn_shared_types::MorphVersionRelease> {
    let tag = r.get("tag_name")?.as_str()?.trim().to_string();
    if tag.is_empty() {
        return None;
    }
    let draft = r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false);
    if draft {
        return None;
    }
    // `v` initial conventionnel ; les tags qui l'omettent restent valides.
    let version = tag.strip_prefix('v').unwrap_or(&tag).to_string();
    let is_beta = r
        .get("prerelease")
        .and_then(|p| p.as_bool())
        .unwrap_or(false)
        || version.contains('-');
    let released_at = r
        .get("published_at")
        .and_then(|p| p.as_str())
        .map(|s| s.get(..10).unwrap_or(s).to_string());
    let summary = if is_beta {
        format!("Version Bêta ({version}) — pre-release non testée")
    } else {
        format!("Version stable v{version}")
    };
    Some(locaryn_shared_types::MorphVersionRelease {
        version,
        tag: Some(tag.clone()),
        is_beta,
        released_at,
        summary: Some(summary),
        install_source: Some(format!("{full_name}@{tag}")),
    })
}

/// Filet de sécurité hors-ligne : la version que le morph déclare dans son
/// `morph.json`. Sans manifeste lisible ni version déclarée, rien — mieux vaut
/// une entrée sans version, installable depuis la racine du dépôt, qu'un tag
/// fantôme qui fait échouer l'installation.
fn version_declaree(
    manifeste: Option<&serde_json::Value>,
    full_name: &str,
) -> Vec<locaryn_shared_types::MorphVersionRelease> {
    let Some(version) = manifeste
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_str())
        .map(|v| v.trim().trim_start_matches('v').to_string())
        .filter(|v| !v.is_empty())
    else {
        return Vec::new();
    };
    // Une version n'est une pre-release que si son numéro le dit.
    let beta = version.contains('-');
    vec![locaryn_shared_types::MorphVersionRelease {
        tag: Some(format!("v{version}")),
        is_beta: beta,
        released_at: None,
        summary: Some(if beta {
            format!("Version Bêta ({version}) — pre-release non testée")
        } else {
            format!("Version stable v{version}")
        }),
        // `@` et non `#` : dans la grammaire des sources, `#` désigne un
        // sous-dossier.
        install_source: Some(format!("{full_name}@v{version}")),
        version,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_json_becomes_a_version_with_a_real_tag_source() {
        let r = serde_json::json!({
            "tag_name": "v0.2.0",
            "prerelease": false,
            "draft": false,
            "published_at": "2026-09-10T12:00:00Z"
        });
        let v = release_from_json(&r, "Locaryn/morph-grappe").expect("version");
        assert_eq!(v.version, "0.2.0");
        assert_eq!(v.tag.as_deref(), Some("v0.2.0"));
        assert!(!v.is_beta);
        assert_eq!(
            v.install_source.as_deref(),
            Some("Locaryn/morph-grappe@v0.2.0")
        );
        assert_eq!(v.released_at.as_deref(), Some("2026-09-10"));
    }

    #[test]
    fn prerelease_or_suffixed_tag_is_beta() {
        let r =
            serde_json::json!({"tag_name": "v1.0.0-beta.2", "prerelease": true, "draft": false});
        let v = release_from_json(&r, "Locaryn/morph-exemple").expect("version");
        assert!(v.is_beta);
        // Un suffixe suffit, même si GitHub ne marque pas la release
        // pre-release (l'oubli humain est le cas commun).
        let r2 =
            serde_json::json!({"tag_name": "v2.2.0-rc.1", "prerelease": false, "draft": false});
        assert!(release_from_json(&r2, "Locaryn/morph-x").unwrap().is_beta);
    }

    #[test]
    fn drafts_and_unnamed_tags_are_not_proposed_for_install() {
        let draft = serde_json::json!({"tag_name": "v9.9.9", "draft": true, "prerelease": false});
        assert!(release_from_json(&draft, "Locaryn/x").is_none());
        let sans_tag = serde_json::json!({"draft": false, "prerelease": false});
        assert!(release_from_json(&sans_tag, "Locaryn/x").is_none());
    }

    #[test]
    fn unknown_morph_gets_no_invented_version() {
        // morph-grappe est désormais dans la table, avec ses VRAIES versions.
        let atom = r#"<feed><entry><updated>2026-09-21T15:38:20Z</updated>
            <link rel="alternate" type="text/html" href="https://github.com/Locaryn/morph-distant/releases/tag/v2.3.0"/></entry>
            <entry><link rel="alternate" href="https://github.com/Locaryn/morph-distant/releases/tag/v2.2.0-beta.1"/></entry></feed>"#;
        let depuis_flux = releases_from_atom(atom, "Locaryn/morph-distant");
        assert_eq!(depuis_flux.len(), 2);
        assert_eq!(depuis_flux[0].version, "2.3.0");
        assert!(!depuis_flux[0].is_beta && depuis_flux[1].is_beta);
        assert_eq!(
            depuis_flux[0].install_source.as_deref(),
            Some("Locaryn/morph-distant@v2.3.0")
        );
        // Hors ligne, la version vient du manifeste du morph lui-même.
        let manifeste = serde_json::json!({ "name": "morph-exemple", "version": "0.2.0" });
        let v = version_declaree(Some(&manifeste), "Locaryn/morph-exemple");
        assert_eq!(v[0].version, "0.2.0");
        assert_eq!(v[0].tag.as_deref(), Some("v0.2.0"));
        assert_eq!(
            v[0].install_source.as_deref(),
            Some("Locaryn/morph-exemple@v0.2.0")
        );
        let beta = serde_json::json!({ "version": "1.0.0-beta.2" });
        assert!(version_declaree(Some(&beta), "Locaryn/morph-exemple")[0].is_beta);
        // Le bug d'origine : le fallback inventait « 1.0.0-beta.1 » pour tout
        // dépôt inconnu. Sans manifeste ni version déclarée : rien.
        assert!(version_declaree(None, "Locaryn/morph-nouveau").is_empty());
        assert!(version_declaree(Some(&serde_json::json!({})), "Locaryn/morph-nouveau").is_empty());
    }

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

    /// Locaryn/Locaryn#6 : la carte Découvrir doit porter le statut réel du
    /// morph (« NON FONCTIONNEL (squelette d'API) »), pas seulement le
    /// tagline GitHub qui, lui, ne le dit jamais.
    #[tokio::test]
    #[ignore = "requires network"]
    async fn the_locaryn_registry_carries_the_manifests_own_description() {
        let http = reqwest::Client::builder()
            .user_agent("locaryn/0.1")
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();
        let cache = std::env::temp_dir().join("locaryn-catalog-manifest-desc-test.json");
        let _ = std::fs::remove_file(&cache);
        let client = CatalogClient::with_cache_path(http, cache);

        let source = CatalogSource {
            id: "locaryn:official".into(),
            label: "Locaryn Official".into(),
            ecosystem: ExtensionEcosystem::Locaryn,
            url: "https://api.github.com/orgs/Locaryn/repos?per_page=100".into(),
            builtin: true,
            enabled: true,
        };
        let entries = client
            .fetch_locaryn_registry(&source)
            .await
            .expect("le registre officiel doit répondre");

        let gen_3d = entries
            .iter()
            .find(|e| e.name == "morph-3d-gen")
            .expect("morph-3d-gen doit être listé");
        let description = gen_3d.description.as_deref().unwrap_or_default();
        assert!(
            description.starts_with("NON FONCTIONNEL"),
            "la description devrait porter le statut du manifeste, reçu : {description:?}"
        );
        assert_ne!(
            description, "Official Locaryn extension: 3D Asset Generation",
            "ne doit plus être le tagline GitHub générique qui ne dit jamais le statut réel"
        );
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

    /// Quota GitHub épuisé : les dépôts de la dernière actualisation
    /// reviennent sous la forme de l'API, et seulement ceux de cette source.
    #[test]
    fn les_depots_connus_remplacent_la_liste_refusee() {
        let mk = |name: &str, catalog: &str, source: &str| CatalogEntry {
            id: name.into(),
            name: name.into(),
            display_name: name.into(),
            description: Some("d".into()),
            author: None,
            version: None,
            homepage: None,
            ecosystem: ExtensionEcosystem::Locaryn,
            catalog_id: catalog.into(),
            catalog_label: catalog.into(),
            install_source: source.into(),
            keywords: vec![],
            advertised: vec![],
            compat: CatalogCompat::Native,
            installed: false,
            is_beta: false,
            versions: Vec::new(),
        };
        let snap = CatalogSnapshot {
            entries: vec![
                mk("morph-a", "locaryn:official", "Locaryn/morph-a@v1.0.0"),
                mk("morph-b", "locaryn:official", "Locaryn/morph-b"),
                mk("morph-c", "ailleurs", "Autre/morph-c"),
                mk(
                    "openclaw",
                    "locaryn:official",
                    "github:Locaryn/locaryn-cores#cores/openclaw",
                ),
            ],
            sources: vec![],
            fetched_at: None,
            stale: true,
        };
        let depots = depots_connus(Some(&snap), "locaryn:official");
        let noms: Vec<&str> = depots
            .iter()
            .filter_map(|d| d["full_name"].as_str())
            .collect();
        assert_eq!(noms, ["Locaryn/morph-a", "Locaryn/morph-b"]);
        assert!(depots_connus(None, "locaryn:official").is_empty());
    }
}
