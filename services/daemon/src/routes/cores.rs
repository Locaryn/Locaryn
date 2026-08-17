//! Routes des noyaux alternatifs — supervision des processus par le daemon.
//!
//! Décision D4 (document 14) : le daemon supervise les processus de noyaux,
//! comme le desktop. La logique est partagée (`locaryn-core-bridge::manager`) ;
//! ici ne restent que l'hôte `DaemonState` et les routes :
//!
//!   GET  /v1/cores                  — les noyaux installés et leur état
//!   GET  /v1/cores/:id              — état d'un noyau
//!   POST /v1/cores/:id/start        — lancer le processus, attendre la sonde
//!   POST /v1/cores/:id/stop         — arrêter le processus
//!   GET  /v1/cores/:id/skills       — l'index de skills du noyau
//!   POST /v1/cores/:id/skills/install — installer un skill (permission shell)
//!
//! Une session CLI confiée à un noyau passe par ces routes pour le démarrer,
//! puis `send_message` construit l'agent via le même superviseur.

use axum::{
    extract::{Path as AxPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use locaryn_core_bridge::manager::{self, CoreHost};
use locaryn_extensions::manifest::CoreManifest;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use crate::DaemonState;

/// Dossier racine d'une extension, à partir du chemin du manifeste.
fn plugin_root(manifest_path: &Path) -> Option<PathBuf> {
    if manifest_path.is_dir() {
        return Some(manifest_path.to_path_buf());
    }
    manifest_path.parent().map(|x| x.to_path_buf())
}

#[async_trait::async_trait]
impl CoreHost for DaemonState {
    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn http(&self) -> &reqwest::Client {
        &self.http
    }

    async fn core_manifest(&self, id: Uuid) -> Result<(CoreManifest, PathBuf), String> {
        let row = self
            .storage
            .extensions
            .get(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "extension introuvable".to_string())?;
        let root = plugin_root(Path::new(&row.manifest_path))
            .ok_or_else(|| "dossier de l'extension introuvable".to_string())?;
        let manifest = locaryn_extensions::manifest::load(&root).map_err(|e| e.to_string())?;
        let core_m = manifest
            .core
            .clone()
            .ok_or_else(|| "cette extension n'est pas un noyau".to_string())?;
        Ok((core_m, root))
    }

    async fn shell_granted(&self, id: Uuid) -> Result<bool, String> {
        let row = self
            .storage
            .extensions
            .get(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "extension introuvable".to_string())?;
        Ok(row
            .granted
            .contains(&locaryn_shared_types::Permission::Shell))
    }
}

/// Les noyaux installés (extensions avec section `core`), dans l'ordre du
/// registre, chacun avec son état. Le nom accompagne le statut : la CLI
/// liste des noms, pas des identifiants à deviner.
pub async fn list_cores(State(s): State<Arc<DaemonState>>) -> Response {
    let mut out = Vec::new();
    for e in s.extensions.list() {
        let Some(root) = plugin_root(&e.manifest_path) else {
            continue;
        };
        let Ok(manifest) = locaryn_extensions::manifest::load(&root) else {
            continue;
        };
        if manifest.core.is_none() {
            continue;
        }
        match manager::status(&s.cores, &*s, e.id).await {
            Ok(st) => {
                let mut v = serde_json::to_value(&st).unwrap_or(serde_json::json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("name".into(), serde_json::json!(e.name));
                }
                out.push(v);
            }
            Err(err) => {
                out.push(serde_json::json!({
                    "id": e.id.to_string(),
                    "name": e.name,
                    "state": "error",
                    "driver": manifest.core.as_ref().map(|c| c.driver.clone()).unwrap_or_default(),
                    "api_url": manifest.core.as_ref().map(|c| c.api_url.clone()).unwrap_or_default(),
                    "error": err,
                }));
            }
        }
    }
    (StatusCode::OK, Json(out)).into_response()
}

pub async fn status(State(s): State<Arc<DaemonState>>, AxPath(id): AxPath<String>) -> Response {
    let uid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return mauvaise_requete("id de noyau invalide"),
    };
    match manager::status(&s.cores, &*s, uid).await {
        Ok(st) => (StatusCode::OK, Json(st)).into_response(),
        Err(e) => erreur(&e),
    }
}

pub async fn start(State(s): State<Arc<DaemonState>>, AxPath(id): AxPath<String>) -> Response {
    let uid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return mauvaise_requete("id de noyau invalide"),
    };
    match manager::start(&s.cores, &*s, uid).await {
        Ok(st) => (StatusCode::OK, Json(st)).into_response(),
        Err(e) => erreur(&e),
    }
}

pub async fn stop(State(s): State<Arc<DaemonState>>, AxPath(id): AxPath<String>) -> Response {
    let uid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return mauvaise_requete("id de noyau invalide"),
    };
    match manager::stop(&s.cores, &*s, uid).await {
        Ok(st) => (StatusCode::OK, Json(st)).into_response(),
        Err(e) => erreur(&e),
    }
}

pub async fn skills(State(s): State<Arc<DaemonState>>, AxPath(id): AxPath<String>) -> Response {
    let uid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return mauvaise_requete("id de noyau invalide"),
    };
    match manager::skills(&s.cores, &*s, uid).await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => erreur(&e),
    }
}

#[derive(Deserialize)]
pub struct InstallSkillBody {
    pub slug: String,
}

pub async fn install_skill(
    State(s): State<Arc<DaemonState>>,
    AxPath(id): AxPath<String>,
    Json(body): Json<InstallSkillBody>,
) -> Response {
    let uid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return mauvaise_requete("id de noyau invalide"),
    };
    match manager::install_skill(&s.cores, &*s, uid, &body.slug).await {
        Ok(msg) => (StatusCode::OK, Json(serde_json::json!({ "message": msg }))).into_response(),
        Err(e) => erreur(&e),
    }
}

/// Agent prêt à parler au noyau pour une session (le superviseur partagé
/// construit le pont selon le driver du manifeste). `Err` = le noyau n'est
/// pas démarré — `send_message` affiche le message sans fallback silencieux
/// vers le noyau natif (D2).
pub async fn agent_for_core(
    s: &DaemonState,
    id: &str,
) -> Result<(locaryn_core_bridge::CoreAgent, Option<String>), String> {
    manager::agent_for_core(&s.cores, s, id).await
}

/// L'extension désignée est-elle un noyau installé ? `Err` = base illisible.
pub async fn verifier_noyau(s: &DaemonState, id: Uuid) -> Result<bool, String> {
    match s.storage.extensions.get(id).await {
        Ok(Some(row)) => {
            let Some(root) = plugin_root(Path::new(&row.manifest_path)) else {
                return Ok(false);
            };
            match locaryn_extensions::manifest::load(&root) {
                Ok(m) => Ok(m.core.is_some()),
                Err(_) => Ok(false),
            }
        }
        Ok(None) => Ok(false),
        Err(e) => Err(format!("lecture de la base impossible : {e}")),
    }
}

fn mauvaise_requete(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": { "code": "bad_request", "message": message }
        })),
    )
        .into_response()
}

fn erreur(message: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({
            "error": { "code": "core_error", "message": message }
        })),
    )
        .into_response()
}
