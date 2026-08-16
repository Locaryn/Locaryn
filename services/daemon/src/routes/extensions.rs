//! Extension routes — manage plugins, MCP servers, commands, skills, hooks,
//! agents, rules, and LSP adapters.
//!
//! All routes delegate to `locaryn_extensions::ExtensionRegistry` which owns
//! the in-memory state and permission bookkeeping. Persistence to SQLite is
//! planned for V1.1 (the registry already has the skeleton).
//!
//! Routes (API contract 06-api-contract.md):
//!   GET    /v1/extensions                     — list all installed
//!   POST   /v1/extensions/install             — install {source, scope}
//!   POST   /v1/extensions/{name}/enable       — enable by name
//!   POST   /v1/extensions/{name}/disable      — disable by name
//!   DELETE /v1/extensions/{name}              — uninstall by name
//!   GET    /v1/extensions/{name}/permissions  — list requested vs granted
//!   POST   /v1/extensions/{name}/permissions  — approve/deny one permission
//!   POST   /v1/extensions/reload              — hot-reload one or all

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use locaryn_extensions::RegistryError;
use locaryn_shared_types::{ExtensionScope, Permission};
use serde::Deserialize;
use std::sync::Arc;

use crate::DaemonState;

// ============================================================================
// Request bodies
// ============================================================================

#[derive(Deserialize)]
pub struct InstallBody {
    /// Filesystem path or download URL to the plugin directory.
    pub source: String,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "user".to_string()
}

#[derive(Deserialize)]
pub struct PermissionBody {
    pub permission: String,
    pub granted: bool,
}

// ============================================================================
// Helpers
// ============================================================================

fn parse_scope(s: &str) -> Result<ExtensionScope, &'static str> {
    match s {
        "global" => Ok(ExtensionScope::Global),
        "user" => Ok(ExtensionScope::User),
        "workspace" => Ok(ExtensionScope::Workspace),
        "session" => Ok(ExtensionScope::Session),
        _ => Err("scope must be 'global', 'user', 'workspace', or 'session'"),
    }
}

fn parse_permission(s: &str) -> Result<Permission, &'static str> {
    match s {
        "shell" => Ok(Permission::Shell),
        "files.read" => Ok(Permission::FilesRead),
        "files.write" => Ok(Permission::FilesWrite),
        "network" => Ok(Permission::Network),
        "extensions" => Ok(Permission::Extensions),
        "mcp" => Ok(Permission::Mcp),
        "preview" => Ok(Permission::Preview),
        "lsp" => Ok(Permission::Lsp),
        "env" => Ok(Permission::Env),
        _ => Err("unknown permission — see API contract for valid values"),
    }
}

fn registry_error_response(e: RegistryError) -> (StatusCode, Json<serde_json::Value>) {
    let (code, status) = match &e {
        RegistryError::NotFound(_) => ("not_found", StatusCode::NOT_FOUND),
        RegistryError::AlreadyInstalled(_) => ("conflict", StatusCode::CONFLICT),
        RegistryError::PermissionDenied(_) => ("permission_denied", StatusCode::FORBIDDEN),
        RegistryError::Manifest(_) | RegistryError::Io(_) => {
            ("install_error", StatusCode::UNPROCESSABLE_ENTITY)
        }
    };
    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": e.to_string() }
        })),
    )
}

/// Une extension telle que tous les clients la lisent.
///
/// Le service renvoyait auparavant une forme à lui, où `permissions` était un
/// objet `{requested, granted}` — la CLI et le téléphone, qui attendent le type
/// partagé, échouaient au décodage. Un seul contrat, celui du type partagé.
fn entry_to_installed(
    e: &locaryn_extensions::ExtensionEntry,
) -> locaryn_shared_types::InstalledExtension {
    let dir = e.manifest_path.parent().map(|p| p.to_path_buf());
    let manifest = dir
        .as_deref()
        .and_then(|d| locaryn_extensions::manifest::load(d).ok());
    let components = match (&dir, &manifest) {
        (Some(d), Some(m)) => locaryn_extensions::loader::load_with_manifest(d, m.clone()).counts(),
        _ => locaryn_shared_types::ExtensionComponents::default(),
    };
    let now = chrono::Utc::now();
    locaryn_shared_types::InstalledExtension {
        id: e.id,
        name: e.name.clone(),
        display_name: e.name.clone(),
        version: e.version.clone(),
        api_version: e.api_version.clone(),
        description: manifest.as_ref().and_then(|m| m.description.clone()),
        author: manifest.as_ref().and_then(|m| m.author.clone()),
        homepage: manifest.as_ref().and_then(|m| m.homepage.clone()),
        kind: e.kind.clone(),
        scope: e.scope,
        ecosystem: locaryn_shared_types::ExtensionEcosystem::Locaryn,
        source: dir.as_ref().map(|d| d.display().to_string()),
        install_dir: dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        enabled: e.enabled,
        components,
        // Une extension désactivée n'apporte rien : sinon un écran survivrait
        // à l'extinction de ce qui le portait.
        capabilities: if e.enabled {
            e.capabilities.clone()
        } else {
            Vec::new()
        },
        ui: locaryn_shared_types::ExtensionUi {
            nav_items: e.ui.nav_items.iter().map(to_ui_entry).collect(),
            studio_tabs: e.ui.studio_tabs.iter().map(to_ui_entry).collect(),
        },
        permissions: e
            .permissions
            .requested
            .iter()
            .map(|(p, r)| locaryn_shared_types::ExtensionPermissionState {
                permission: p.clone(),
                reason: r.reason.clone(),
                granted: e.permissions.granted.contains(p),
            })
            .collect(),
        load_errors: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}

fn to_ui_entry(
    e: &locaryn_extensions::manifest::UiEntry,
) -> locaryn_shared_types::ExtensionUiEntry {
    locaryn_shared_types::ExtensionUiEntry {
        id: e.id.clone(),
        label: e.label.clone(),
        icon: e.icon.clone(),
    }
}

/// Recharge le registre depuis la base au démarrage du service.
///
/// Le registre vivait en mémoire seule : installer une extension puis
/// redémarrer le service la faisait disparaître, avec l'écran qu'elle
/// apportait. Une installation doit survivre à un redémarrage — c'est le
/// minimum qu'on attend d'une installation.
pub async fn restore_from_storage(state: &DaemonState) {
    let records = match state.storage.extensions.list().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "extensions : lecture de la base impossible");
            return;
        }
    };
    let mut restored = 0usize;
    for rec in records {
        let dir = std::path::Path::new(&rec.manifest_path)
            .parent()
            .map(|p| p.to_path_buf());
        let Some(dir) = dir else { continue };
        match state.extensions.install_from_dir(&dir, rec.scope) {
            Ok(entry) => {
                if rec.enabled {
                    let _ = state.extensions.enable(&entry.name);
                }
                restored += 1;
            }
            Err(e) => tracing::warn!(
                name = %rec.name,
                error = %e,
                "extension enregistrée mais introuvable sur le disque"
            ),
        }
    }
    if restored > 0 {
        tracing::info!(restored, "extensions rechargées depuis la base");
    }
}

/// Écrit dans la base ce que le registre vient de faire.
async fn persist(state: &DaemonState, entry: &locaryn_extensions::ExtensionEntry) {
    let new = locaryn_storage::repos::NewExtension {
        name: entry.name.clone(),
        version: entry.version.clone(),
        api_version: entry.api_version.clone(),
        kind: entry.kind.clone(),
        scope: entry.scope,
        ecosystem: locaryn_shared_types::ExtensionEcosystem::Locaryn,
        source: entry
            .manifest_path
            .parent()
            .map(|p| p.display().to_string()),
        manifest_path: entry.manifest_path.display().to_string(),
        requested: entry
            .permissions
            .requested
            .iter()
            .map(|(p, _)| p.clone())
            .collect(),
    };
    if let Err(e) = state.storage.extensions.upsert(new).await {
        tracing::warn!(name = %entry.name, error = %e, "extension non enregistrée en base");
    }
}

/// Met à jour l'état actif en base, par nom.
async fn persist_enabled(state: &DaemonState, name: &str, enabled: bool) {
    let Some(rec) = find_record(state, name).await else {
        tracing::warn!(name, "extension absente de la base");
        return;
    };
    if let Err(e) = state.storage.extensions.set_enabled(rec.id, enabled).await {
        tracing::warn!(name, error = %e, "état de l'extension non enregistré");
    }
}

/// Retrouve l'enregistrement quelle que soit sa portée : le nom est unique
/// dans le registre du service, la portée n'est qu'un détail de rangement.
async fn find_record(
    state: &DaemonState,
    name: &str,
) -> Option<locaryn_storage::repos::ExtensionRecord> {
    for scope in [
        ExtensionScope::User,
        ExtensionScope::Global,
        ExtensionScope::Workspace,
        ExtensionScope::Session,
    ] {
        if let Ok(Some(rec)) = state.storage.extensions.get_by_name(name, scope).await {
            return Some(rec);
        }
    }
    None
}

async fn persist_removed(state: &DaemonState, name: &str) {
    if let Some(rec) = find_record(state, name).await {
        if let Err(e) = state.storage.extensions.delete(rec.id).await {
            tracing::warn!(name, error = %e, "extension non retirée de la base");
        }
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /v1/extensions — list all installed extensions.
pub async fn list_extensions(State(s): State<Arc<DaemonState>>) -> Response {
    let entries = s.extensions.list();
    let json: Vec<_> = entries.iter().map(entry_to_installed).collect();
    (StatusCode::OK, Json(json)).into_response()
}

/// POST /v1/extensions/install — install an extension from a local path.
///
/// Body: `{ source: "/path/to/plugin/dir", scope: "user" | "workspace" | "global" }`
/// URL-based install is not yet implemented (V1.1).
pub async fn install_extension(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<InstallBody>,
) -> Response {
    let scope = match parse_scope(&body.scope) {
        Ok(s) => s,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };

    // Un dossier du serveur, ou un dépôt du catalogue. Le second cas est le
    // seul possible depuis un téléphone, qui ne connaît aucun chemin de la
    // machine d'en face — et c'est aussi ce que la CLI annonçait déjà sans que
    // le service sache le faire.
    let local;
    let dir = if std::path::Path::new(&body.source).is_dir() {
        std::path::PathBuf::from(&body.source)
    } else {
        match fetch_from_catalogue(&body.source).await {
            Ok(p) => {
                local = p;
                local.clone()
            }
            Err(msg) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": { "code": "bad_request", "message": msg }
                    })),
                )
                    .into_response();
            }
        }
    };
    let dir = dir.as_path();

    match s.extensions.install_from_dir(dir, scope) {
        Ok(entry) => {
            persist(&s, &entry).await;
            (StatusCode::CREATED, Json(entry_to_installed(&entry))).into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// Ramener `owner/repo` à ce qu'il désigne vraiment.
///
/// On accepte ce qu'une personne écrit : `Locaryn/locaryn-image-gen`,
/// l'adresse complète du dépôt, ou la forme `github:owner/repo`. Tout le
/// reste est refusé — une source arbitraire téléchargée et exécutée par le
/// service serait une porte d'entrée, pas une commodité.
fn parse_repo(source: &str) -> Result<(String, String), String> {
    let s = source
        .trim()
        .trim_end_matches('/')
        .trim_start_matches("github:")
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_end_matches(".git");
    let mut parts = s.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!(
            "« {source} » n'est ni un dossier du serveur, ni un dépôt « propriétaire/nom »."
        ));
    };
    let valide = |x: &str| {
        !x.is_empty()
            && x.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    };
    if !valide(owner) || !valide(repo) {
        return Err(format!("Nom de dépôt inattendu : « {source} »."));
    }
    Ok((owner.to_string(), repo.to_string()))
}

/// Trouver, dans une archive dépliée, le dossier qui porte le manifeste.
///
/// GitHub emballe tout dans un dossier `repo-main/`, et une extension peut y
/// être rangée dans un sous-dossier. On cherche donc `plugin.json` sur deux
/// niveaux plutôt que de supposer une disposition.
fn find_manifest_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    if root.join("plugin.json").is_file() {
        return Some(root.to_path_buf());
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.join("plugin.json").is_file() {
                return Some(p);
            }
            if let Ok(sous) = std::fs::read_dir(&p) {
                for s in sous.flatten() {
                    let sp = s.path();
                    if sp.is_dir() && sp.join("plugin.json").is_file() {
                        return Some(sp);
                    }
                }
            }
        }
    }
    None
}

/// Télécharger un dépôt du catalogue et rendre le dossier à installer.
async fn fetch_from_catalogue(source: &str) -> Result<std::path::PathBuf, String> {
    let (owner, repo) = parse_repo(source)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("locaryn-daemon")
        .build()
        .map_err(|e| e.to_string())?;

    // `main` d'abord, `master` ensuite : les deux existent dans la nature.
    let mut derniere = String::new();
    let mut octets = None;
    for branche in ["main", "master"] {
        let url = format!("https://github.com/{owner}/{repo}/archive/refs/heads/{branche}.zip");
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                octets = Some(r.bytes().await.map_err(|e| e.to_string())?);
                break;
            }
            Ok(r) => derniere = format!("{} a répondu {}", url, r.status()),
            Err(e) => derniere = format!("{url} : {e}"),
        }
    }
    let Some(octets) = octets else {
        return Err(format!(
            "Impossible de récupérer {owner}/{repo} ({derniere})."
        ));
    };

    let cible = locaryn_config::default_data_dir()
        .join("extensions-telechargees")
        .join(format!("{owner}-{repo}"));
    let _ = std::fs::remove_dir_all(&cible);
    std::fs::create_dir_all(&cible).map_err(|e| format!("dossier : {e}"))?;

    let curseur = std::io::Cursor::new(octets);
    let mut archive =
        zip::ZipArchive::new(curseur).map_err(|e| format!("archive illisible : {e}"))?;
    for i in 0..archive.len() {
        let mut entree = archive
            .by_index(i)
            .map_err(|e| format!("entrée illisible : {e}"))?;
        // `enclosed_name` refuse les chemins qui sortent du dossier cible.
        let Some(rel) = entree.enclosed_name() else {
            continue;
        };
        let out = cible.join(rel);
        if entree.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut fichier = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entree, &mut fichier).map_err(|e| e.to_string())?;
    }

    find_manifest_dir(&cible).ok_or_else(|| {
        format!("{owner}/{repo} ne contient pas de plugin.json : ce n'est pas une extension.")
    })
}

/// POST /v1/extensions/{name}/enable — enable an extension by name.
pub async fn enable_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.enable(&name) {
        Ok(()) => {
            persist_enabled(&s, &name, true).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "enabled", "name": name })),
            )
                .into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// POST /v1/extensions/{name}/disable — disable an extension by name.
pub async fn disable_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.disable(&name) {
        Ok(()) => {
            persist_enabled(&s, &name, false).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "disabled", "name": name })),
            )
                .into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// DELETE /v1/extensions/{name} — uninstall (remove) an extension by name.
pub async fn remove_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.remove(&name) {
        Ok(()) => {
            persist_removed(&s, &name).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "removed", "name": name })),
            )
                .into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// GET /v1/extensions/{name}/permissions — list requested vs granted permissions.
pub async fn get_extension_permissions(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.get(&name) {
        Some(entry) => {
            let json = serde_json::json!({
                "name": entry.name,
                "permissions": {
                    "requested": entry.permissions.requested.iter().map(|(p, r)| {
                        serde_json::json!({
                            "permission": format!("{p:?}"),
                            "reason": r.reason,
                            "scope": r.scope,
                            "require_approval": r.require_approval,
                        })
                    }).collect::<Vec<_>>(),
                    "granted": entry.permissions.granted.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
                }
            });
            (StatusCode::OK, Json(json)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("extension not found: {name}") }
            })),
        )
            .into_response(),
    }
}

/// POST /v1/extensions/{name}/permissions — approve or deny a specific permission.
///
/// Body: `{ permission: "files.write", granted: true }`
pub async fn set_extension_permission(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
    Json(body): Json<PermissionBody>,
) -> Response {
    let perm = match parse_permission(&body.permission) {
        Ok(p) => p,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };

    if body.granted {
        match s.extensions.grant_permission(&name, perm) {
            Ok(()) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "granted",
                    "name": name,
                    "permission": body.permission,
                })),
            )
                .into_response(),
            Err(e) => registry_error_response(e).into_response(),
        }
    } else {
        // Denying a permission is a no-op in the current registry (permissions
        // are addition-only). We still return OK for API consistency.
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "denied",
                "name": name,
                "permission": body.permission,
            })),
        )
            .into_response()
    }
}

/// POST /v1/extensions/reload — hot-reload one extension by name in the body,
/// or all extensions if no name is provided.
///
/// Body (optional): `{ name: "my-plugin" }`
pub async fn reload_extensions(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        match s.extensions.reload(name) {
            Ok(entry) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "reloaded",
                    "name": name,
                    "entry": entry_to_installed(&entry),
                })),
            )
                .into_response(),
            Err(e) => registry_error_response(e).into_response(),
        }
    } else {
        // Reload all: iterate through entries, re-read each manifest.
        let names: Vec<String> = s.extensions.list().into_iter().map(|e| e.name).collect();
        let mut results: Vec<serde_json::Value> = Vec::new();
        for name in &names {
            match s.extensions.reload(name) {
                Ok(_entry) => results.push(serde_json::json!({
                    "name": name, "status": "reloaded"
                })),
                Err(e) => results.push(serde_json::json!({
                    "name": name, "status": "error", "error": e.to_string()
                })),
            }
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "completed", "results": results })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod catalogue_tests {
    use super::parse_repo;

    #[test]
    fn les_formes_courantes_donnent_le_meme_depot() {
        for source in [
            "Locaryn/locaryn-image-gen",
            "github:Locaryn/locaryn-image-gen",
            "https://github.com/Locaryn/locaryn-image-gen",
            "https://github.com/Locaryn/locaryn-image-gen.git",
            "https://github.com/Locaryn/locaryn-image-gen/",
        ] {
            assert_eq!(
                parse_repo(source).unwrap(),
                ("Locaryn".to_string(), "locaryn-image-gen".to_string()),
                "source : {source}"
            );
        }
    }

    #[test]
    fn tout_le_reste_est_refuse() {
        // Une source arbitraire téléchargée par le service serait une porte
        // d'entrée : rien qui ne ressemble pas à un dépôt ne passe.
        for source in [
            "",
            "juste-un-mot",
            "trop/de/segments",
            "https://exemple.invalide/paquet.zip",
            "../../etc",
            "Locaryn/nom avec espace",
        ] {
            assert!(parse_repo(source).is_err(), "aurait dû refuser : {source}");
        }
    }
}
