//! Aperçu d'une source d'installation, sans installer le paquet.
//!
//! Ce que le manifeste déclare (nom, version, écosystème, permissions
//! demandées) pour alimenter la carte de confirmation de la fenêtre d'ajout.
//! Pour GitHub, un seul petit fichier est récupéré (`raw.githubusercontent`)
//! — jamais l'archive complète. Un dossier local est lu tel quel ; un zip est
//! lu en mémoire, sans extraction.

use std::io::Read;
use std::path::Path;

use locaryn_shared_types::{ExtensionEcosystem, Permission};
use serde::Serialize;

use crate::adapters;
use crate::manifest;
use crate::source::{parse, InstallSource, SourceError};

/// Fichiers manifestes reconnus, dans l'ordre de détection (`detect`).
const MANIFEST_CANDIDATES: &[&str] = &[
    "plugin.json",
    ".claude-plugin/plugin.json",
    "gemini-extension.json",
    "opencode.json",
    ".mcp.json",
];

/// Aperçu d'une source, renvoyé tel quel au front.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SourcePreview {
    /// Fichier manifeste trouvé (plugin.json, .claude-plugin/plugin.json, …).
    pub manifest_file: String,
    /// Écosystème détecté (locaryn, claude_code, gemini_cli, opencode, mcp).
    pub ecosystem: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    /// Permissions demandées, telles que déclarées par le manifeste.
    pub requested_permissions: Vec<String>,
    /// Serveurs MCP déclarés par la source : lus depuis le manifeste lui-même
    /// (`.mcp.json`) ou depuis le fichier mcp qu'il référence (`components.mcp`
    /// pour Locaryn, `mcpServers` pour Claude Code) — sans jamais télécharger
    /// le paquet complet.
    pub mcp_servers: Vec<McpServerPreview>,
}

/// Un serveur MCP déclaré par la source, pour la carte de confirmation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct McpServerPreview {
    pub name: String,
    /// Commande stdio (ex. `npx -y server-filesystem`), quand le transport est stdio.
    pub command: Option<String>,
    /// URL du serveur (streamable HTTP), quand le transport est distant.
    pub url: Option<String>,
}

/// Lève l'aperçu d'une source sans l'installer.
///
/// - GitHub : récupère le manifeste brut à la ref donnée (branche par défaut
///   quand aucune ref n'est épinglée) — pas l'archive.
/// - Dossier local : `detect` + `adapt` (lecture seule, même résultat que
///   l'installation).
/// - Archive `.zip` locale : le manifeste est lu en mémoire.
/// - Dépôt git distant (non GitHub) : pas d'aperçu — l'installation reste
///   possible, le message le dit clairement.
pub async fn preview_source(
    http: &reqwest::Client,
    spec: &str,
) -> Result<SourcePreview, SourceError> {
    let src = parse(spec)?;
    match src {
        InstallSource::GitHub {
            owner,
            repo,
            git_ref,
            subdir,
        } => {
            let prefix = subdir
                .map(|d| format!("{}/", d.replace('\\', "/")))
                .unwrap_or_default();
            let base = git_ref.unwrap_or_else(|| "HEAD".to_string());
            for name in MANIFEST_CANDIDATES {
                let url =
                    format!("https://raw.githubusercontent.com/{owner}/{repo}/{base}/{prefix}{name}");
                let resp = http
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| SourceError::Http(e.to_string()))?;
                if !resp.status().is_success() {
                    continue;
                }
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| SourceError::Http(e.to_string()))?;
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    let mut preview = preview_from_json(name, &v, &format!("{owner}/{repo}"));
                    // Le manifeste peut référencer un fichier mcp séparé
                    // (`components.mcp` / `mcpServers`) : on le lit aussi en
                    // raw — jamais l'archive complète.
                    if preview.mcp_servers.is_empty() {
                        for rel in declared_mcp_candidates(name, &v) {
                            let murl = format!(
                                "https://raw.githubusercontent.com/{owner}/{repo}/{base}/{prefix}{rel}"
                            );
                            if let Ok(mresp) = http.get(&murl).send().await {
                                if mresp.status().is_success() {
                                    if let Ok(mbytes) = mresp.bytes().await {
                                        if let Ok(mv) = serde_json::from_slice::<serde_json::Value>(
                                            &mbytes,
                                        ) {
                                            preview.mcp_servers = extract_mcp_servers(&mv);
                                            if !preview.mcp_servers.is_empty() {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return Ok(preview);
                }
            }
            Err(SourceError::NotFound(format!(
                "aucun manifeste reconnu dans github.com/{owner}/{repo}"
            )))
        }
        InstallSource::Local { path } => {
            let path = crate::source::expand_home(&path);
            if path.is_dir() {
                preview_dir(&path)
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                preview_zip(&path)
            } else {
                Err(SourceError::Unsupported(format!(
                    "`{}` n'est ni un dossier ni une archive .zip",
                    path.display()
                )))
            }
        }
        InstallSource::Git { url, .. } => Err(SourceError::Unsupported(format!(
            "aperçu indisponible pour un dépôt git distant (`{url}`) — l'installation reste possible."
        ))),
    }
}

/// Dossier local : détection + adaptation en mémoire, exactement ce que
/// l'installation fera.
fn preview_dir(dir: &Path) -> Result<SourcePreview, SourceError> {
    let fallback = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "plugin".to_string());
    let report =
        adapters::adapt(dir, &fallback).map_err(|e| SourceError::Unsupported(e.to_string()))?;
    let mut preview = from_manifest(
        &report.ecosystem,
        &report.manifest,
        detected_manifest_file(dir),
    );
    // Le manifeste adapté sait où vivent les serveurs MCP (`components.mcp`).
    // On lit ce fichier pour les lister — lecture seule, rien n'est installé.
    if preview.mcp_servers.is_empty() {
        for rel in dir_mcp_candidates(&report.manifest) {
            let path = dir.join(&rel);
            if path.is_file() {
                if let Ok(raw) = std::fs::read_to_string(&path) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                        preview.mcp_servers = extract_mcp_servers(&v);
                        if !preview.mcp_servers.is_empty() {
                            break;
                        }
                    }
                }
            }
        }
    }
    Ok(preview)
}

/// Archive zip locale : lit l'entrée du manifeste en mémoire, sans extraire.
fn preview_zip(path: &Path) -> Result<SourcePreview, SourceError> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| SourceError::Archive(e.to_string()))?;
    let fallback = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "plugin".to_string());
    let mut manifest: Option<(String, serde_json::Value)> = None;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| SourceError::Archive(e.to_string()))?;
        let name = entry.name().replace('\\', "/");
        // Le manifeste peut être à la racine ou sous le dossier racine unique
        // généré par GitHub (`repo-main/plugin.json`).
        let Some(candidate) = MANIFEST_CANDIDATES
            .iter()
            .find(|c| name == **c || name.ends_with(&format!("/{c}")))
        else {
            continue;
        };
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf) {
            manifest = Some(((*candidate).to_string(), v));
            break;
        }
    }
    let Some((manifest_file, v)) = manifest else {
        return Err(SourceError::NotFound(format!(
            "aucun manifeste reconnu dans l'archive `{}`",
            path.display()
        )));
    };
    let mut preview = preview_from_json(&manifest_file, &v, &fallback);
    // Le manifeste peut référencer un fichier mcp séparé : on cherche son
    // entrée dans l'archive (en mémoire, sans extraire).
    if preview.mcp_servers.is_empty() {
        for rel in declared_mcp_candidates(&manifest_file, &v) {
            for j in 0..zip.len() {
                let mut mentry = zip
                    .by_index(j)
                    .map_err(|e| SourceError::Archive(e.to_string()))?;
                let mname = mentry.name().replace('\\', "/");
                if mname != rel && !mname.ends_with(&format!("/{rel}")) {
                    continue;
                }
                let mut mbuf = Vec::new();
                mentry.read_to_end(&mut mbuf)?;
                if let Ok(mv) = serde_json::from_slice::<serde_json::Value>(&mbuf) {
                    preview.mcp_servers = extract_mcp_servers(&mv);
                    if !preview.mcp_servers.is_empty() {
                        break;
                    }
                }
            }
            if !preview.mcp_servers.is_empty() {
                break;
            }
        }
    }
    Ok(preview)
}

/// Aperçu depuis un manifeste Locaryn adapté (résultat de `adapt`).
fn from_manifest(
    eco: &ExtensionEcosystem,
    m: &manifest::PluginManifest,
    file: &str,
) -> SourcePreview {
    let requested = manifest::requested_permissions(m)
        .into_iter()
        .map(|(p, _)| permission_str(&p).to_string())
        .collect();
    SourcePreview {
        manifest_file: file.to_string(),
        ecosystem: eco.as_str().to_string(),
        name: m.name.clone(),
        version: Some(m.version.clone()),
        description: m.description.clone(),
        author: m.author.clone(),
        requested_permissions: requested,
        mcp_servers: Vec::new(),
    }
}

/// Aperçu depuis un manifeste brut (GitHub ou zip) : champs communs, tolérants
/// aux formes des différents écosystèmes.
fn preview_from_json(
    manifest_file: &str,
    v: &serde_json::Value,
    fallback_name: &str,
) -> SourcePreview {
    let ecosystem = match manifest_file {
        ".claude-plugin/plugin.json" => "claude_code",
        "gemini-extension.json" => "gemini_cli",
        "opencode.json" => "opencode",
        ".mcp.json" => "mcp",
        _ => "locaryn",
    };
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("displayName").and_then(|x| x.as_str()))
        .unwrap_or(fallback_name)
        .to_string();
    let mcp_servers = if manifest_file == ".mcp.json" {
        extract_mcp_servers(v)
    } else {
        Vec::new()
    };
    SourcePreview {
        manifest_file: manifest_file.to_string(),
        ecosystem: ecosystem.to_string(),
        name,
        version: v
            .get("version")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        author: v
            .get("author")
            .and_then(|x| x.as_str())
            .or_else(|| v.get("authorName").and_then(|x| x.as_str()))
            .map(str::to_string),
        requested_permissions: extract_permissions(v),
        mcp_servers,
    }
}

/// Permissions demandées, telles que déclarées par le manifeste brut. Accepte
/// les trois formes rencontrées : map Locaryn (`"shell": true`), objet avec
/// listes (Claude Code : `{ permissions: [...], org: [...] }`), ou simple
/// liste de chaînes (Gemini CLI, OpenCode).
fn extract_permissions(v: &serde_json::Value) -> Vec<String> {
    let Some(p) = v.get("permissions") else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    if let Some(obj) = p.as_object() {
        for (key, val) in obj {
            match val {
                serde_json::Value::Bool(b) if *b => out.push(key.clone()),
                serde_json::Value::Object(_) => out.push(key.clone()),
                serde_json::Value::Array(arr) => {
                    for it in arr {
                        if let Some(s) = it.as_str() {
                            out.push(s.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        if out.is_empty() {
            for list_key in ["permissions", "org"] {
                if let Some(arr) = obj.get(list_key).and_then(|a| a.as_array()) {
                    for it in arr {
                        if let Some(s) = it.as_str() {
                            out.push(s.to_string());
                        }
                    }
                }
            }
        }
    } else if let Some(arr) = p.as_array() {
        for it in arr {
            if let Some(s) = it.as_str() {
                out.push(s.to_string());
            }
        }
    }
    // L'ordre des clés d'une map JSON n'est pas garanti : trier pour que
    // l'aperçu soit stable (et sans doublons).
    out.sort();
    out.dedup();
    out
}

/// Chemins de fichiers mcp déclarés par un manifeste brut, dans l'ordre :
/// chemin explicite (Locaryn `components.mcp`, Claude Code `mcpServers`), puis
/// les conventions usuelles du dossier.
fn declared_mcp_candidates(manifest_file: &str, v: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let explicit = match manifest_file {
        "plugin.json" => v
            .get("components")
            .and_then(|c| c.get("mcp"))
            .and_then(|m| m.as_str()),
        ".claude-plugin/plugin.json" => v.get("mcpServers").and_then(|m| m.as_str()),
        _ => None,
    };
    if let Some(p) = explicit {
        let p = p.trim_start_matches("./").replace('\\', "/");
        if !p.is_empty() {
            out.push(p);
        }
    }
    for c in [".mcp.json", "mcp.json", "mcp/mcp.json"] {
        if !out.iter().any(|x| x == c) {
            out.push((*c).to_string());
        }
    }
    out
}

/// Idem pour un manifeste déjà adapté (le chemin est dans `components.mcp`).
fn dir_mcp_candidates(m: &manifest::PluginManifest) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(p) = m.components.mcp.as_deref() {
        let p = p.trim_start_matches("./").replace('\\', "/");
        if !p.is_empty() {
            out.push(p);
        }
    }
    for c in [".mcp.json", "mcp.json", "mcp/mcp.json"] {
        if !out.iter().any(|x| x == c) {
            out.push((*c).to_string());
        }
    }
    out
}

/// Serveurs MCP lus depuis une valeur `{ "mcpServers": { name: entry } }`.
/// Triés par nom : l'ordre des clés d'une map JSON n'est pas garanti.
fn extract_mcp_servers(v: &serde_json::Value) -> Vec<McpServerPreview> {
    let Some(obj) = v.get("mcpServers").and_then(|x| x.as_object()) else {
        return Vec::new();
    };
    let mut out: Vec<McpServerPreview> = obj
        .iter()
        .map(|(name, entry)| McpServerPreview {
            name: name.clone(),
            command: entry
                .get("command")
                .and_then(|c| c.as_str())
                .map(str::to_string),
            url: entry
                .get("url")
                .and_then(|c| c.as_str())
                .map(str::to_string),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Nom de la permission tel que le front le connaît (snake_case).
fn permission_str(p: &Permission) -> &'static str {
    match p {
        Permission::Shell => "shell",
        Permission::FilesRead => "files_read",
        Permission::FilesWrite => "files_write",
        Permission::Network => "network",
        Permission::Extensions => "extensions",
        Permission::Mcp => "mcp",
        Permission::Preview => "preview",
        Permission::Lsp => "lsp",
        Permission::Env => "env",
    }
}

/// Le fichier manifeste présent dans `dir`, pour l'afficher.
fn detected_manifest_file(dir: &Path) -> &'static str {
    for name in MANIFEST_CANDIDATES {
        if dir.join(name).is_file() {
            return name;
        }
    }
    "manifeste"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_names_match_the_frontend_vocabulary() {
        assert_eq!(permission_str(&Permission::Shell), "shell");
        assert_eq!(permission_str(&Permission::FilesRead), "files_read");
        assert_eq!(permission_str(&Permission::Mcp), "mcp");
        assert_eq!(permission_str(&Permission::FilesWrite), "files_write");
    }

    #[test]
    fn raw_manifest_extracts_common_fields_and_permissions() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
                "name": "snap-mcp",
                "version": "0.3.0",
                "description": "Capture MCP",
                "author": "tealo",
                "permissions": {
                    "shell": true,
                    "files.read": { "reason": "pour lire" },
                    "files.write": false
                }
            }"#,
        )
        .unwrap();
        let p = preview_from_json("plugin.json", &v, "fallback");
        assert_eq!(p.name, "snap-mcp");
        assert_eq!(p.version.as_deref(), Some("0.3.0"));
        assert_eq!(p.ecosystem, "locaryn");
        // Trié : l'ordre des clés d'une map JSON n'est pas garanti.
        assert_eq!(p.requested_permissions, vec!["files.read", "shell"]);
    }

    #[test]
    fn claude_code_permissions_lists_are_extracted() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{
                "name": "cc-plugin",
                "version": "1.0.0",
                "permissions": { "permissions": ["Read", "Edit"], "org": ["Bash(npx foo)"] }
            }"#,
        )
        .unwrap();
        let p = preview_from_json(".claude-plugin/plugin.json", &v, "fallback");
        assert_eq!(p.ecosystem, "claude_code");
        assert_eq!(
            p.requested_permissions,
            vec!["Bash(npx foo)", "Edit", "Read"]
        );
    }

    #[test]
    fn bare_mcp_json_lists_servers() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"mcpServers": {"graphify": {"command": "npx graphify"}, "notes": {"url": "https://x"}}}"#,
        )
        .unwrap();
        let p = preview_from_json(".mcp.json", &v, "my-repo");
        assert_eq!(p.ecosystem, "mcp");
        assert_eq!(p.name, "my-repo");
        // Trié par nom ; commande stdio ou URL selon le transport.
        assert_eq!(p.mcp_servers.len(), 2);
        assert_eq!(p.mcp_servers[0].name, "graphify");
        assert_eq!(p.mcp_servers[0].command.as_deref(), Some("npx graphify"));
        assert_eq!(p.mcp_servers[0].url, None);
        assert_eq!(p.mcp_servers[1].name, "notes");
        assert_eq!(p.mcp_servers[1].command, None);
        assert_eq!(p.mcp_servers[1].url.as_deref(), Some("https://x"));
    }

    #[test]
    fn locaryn_manifest_declared_mcp_is_picked_up() {
        // plugin.json qui référence un fichier mcp séparé (components.mcp).
        let v: serde_json::Value = serde_json::from_str(
            r#"{"name":"snap-mcp","version":"0.3.0","components":{"mcp":"mcp/mcp.json"}}"#,
        )
        .unwrap();
        let candidates = declared_mcp_candidates("plugin.json", &v);
        assert_eq!(candidates[0], "mcp/mcp.json");
        // Les conventions usuelles suivent, sans doublon.
        assert!(candidates[1..].contains(&".mcp.json".to_string()));

        // Claude Code : `mcpServers` est un chemin, pas une liste.
        let cc: serde_json::Value =
            serde_json::from_str(r#"{"name":"cc","mcpServers":"servers/mcp.json"}"#).unwrap();
        assert_eq!(
            declared_mcp_candidates(".claude-plugin/plugin.json", &cc)[0],
            "servers/mcp.json"
        );

        // Extrait les serveurs depuis le fichier mcp référencé.
        let mcp: serde_json::Value = serde_json::from_str(
            r#"{"mcpServers":{"fs":{"command":"npx fs","args":["-y"]},"web":{"url":"https://m"}}}"#,
        )
        .unwrap();
        let servers = extract_mcp_servers(&mcp);
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "fs");
        assert_eq!(servers[0].command.as_deref(), Some("npx fs"));
        assert_eq!(servers[1].name, "web");
        assert_eq!(servers[1].url.as_deref(), Some("https://m"));
    }

    #[tokio::test]
    async fn local_dir_previews_through_adapt() {
        let base = std::env::temp_dir().join("locaryn-preview-dir");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(
            base.join("plugin.json"),
            r#"{"schema":"https://locaryn.dev/schema/plugin.json/v0.1","apiVersion":"0.1","name":"preview-me","version":"1.2.3","permissions":{"shell":true}}"#,
        )
        .unwrap();
        std::fs::write(base.join("README.md"), "readme").unwrap();

        let http = reqwest::Client::new();
        let p = preview_source(&http, base.to_str().unwrap()).await.unwrap();
        assert_eq!(p.name, "preview-me");
        assert_eq!(p.version.as_deref(), Some("1.2.3"));
        assert_eq!(p.ecosystem, "locaryn");
        assert_eq!(p.requested_permissions, vec!["shell"]);
        assert_eq!(p.manifest_file, "plugin.json");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn local_zip_previews_without_extracting() {
        use std::io::Write;

        let base = std::env::temp_dir().join("locaryn-preview-zip");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let zip_path = base.join("plugin.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("repo-main/plugin.json", options).unwrap();
        writer
            .write_all(
                r#"{"name":"zipped","version":"0.9.0","permissions":{"network":true}}"#.as_bytes(),
            )
            .unwrap();
        writer.finish().unwrap();

        let http = reqwest::Client::new();
        let p = preview_source(&http, zip_path.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(p.name, "zipped");
        assert_eq!(p.version.as_deref(), Some("0.9.0"));
        assert_eq!(p.requested_permissions, vec!["network"]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn zip_previews_declared_mcp_servers() {
        use std::io::Write;

        let base = std::env::temp_dir().join("locaryn-preview-zip-mcp");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let zip_path = base.join("plugin.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        writer.start_file("repo-main/plugin.json", options).unwrap();
        writer
            .write_all(
                r#"{"name":"snap-mcp","version":"0.3.0","components":{"mcp":"mcp/mcp.json"}}"#
                    .as_bytes(),
            )
            .unwrap();
        writer
            .start_file("repo-main/mcp/mcp.json", options)
            .unwrap();
        writer
            .write_all(r#"{"mcpServers":{"graphify":{"command":"npx graphify"}}}"#.as_bytes())
            .unwrap();
        writer.finish().unwrap();

        let http = reqwest::Client::new();
        let p = preview_source(&http, zip_path.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(p.name, "snap-mcp");
        assert_eq!(p.mcp_servers.len(), 1);
        assert_eq!(p.mcp_servers[0].name, "graphify");
        assert_eq!(p.mcp_servers[0].command.as_deref(), Some("npx graphify"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn dir_previews_declared_mcp_servers() {
        let base = std::env::temp_dir().join("locaryn-preview-dir-mcp");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("mcp")).unwrap();
        std::fs::write(
            base.join("plugin.json"),
            r#"{"schema":"https://locaryn.dev/schema/plugin.json/v0.1","apiVersion":"0.1","name":"snap-mcp","version":"0.3.0","components":{"mcp":"mcp/mcp.json"}}"#,
        )
        .unwrap();
        std::fs::write(
            base.join("mcp/mcp.json"),
            r#"{"mcpServers":{"notes":{"url":"https://notes"}}}"#,
        )
        .unwrap();

        let http = reqwest::Client::new();
        let p = preview_source(&http, base.to_str().unwrap()).await.unwrap();
        assert_eq!(p.name, "snap-mcp");
        assert_eq!(p.mcp_servers.len(), 1);
        assert_eq!(p.mcp_servers[0].name, "notes");
        assert_eq!(p.mcp_servers[0].url.as_deref(), Some("https://notes"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn git_remote_preview_is_refused_with_a_reason() {
        // Un dépôt git (non GitHub) n'a pas d'aperçu — pas de réseau nécessaire.
        let http = reqwest::Client::new();
        let err = preview_source(&http, "https://gitlab.com/acme/tools.git")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("dépôt git distant"));
    }
}
