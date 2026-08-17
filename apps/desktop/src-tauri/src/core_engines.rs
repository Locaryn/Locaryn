//! Noyaux alternatifs (OpenClaw, Hermes Agent…) — commandes Tauri de
//! l'application de bureau.
//!
//! La logique (processus, jetons, statut, skills, agent) vit dans
//! [`locaryn_core_bridge::manager`], partagée avec le daemon : une seule
//! implémentation du superviseur, deux hôtes (décision D4 du document 14).
//! Ici ne restent que l'adaptation à l'hôte `Core` et les commandes Tauri.

use locaryn_core_bridge::manager::{CoreSkillEntry, CoreStatus};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

use crate::{extensions, Core};

/// L'hôte de bureau : dossier de données, client HTTP, manifestes lus depuis
/// la base, permission shell du registre.
#[async_trait::async_trait]
impl locaryn_core_bridge::manager::CoreHost for Core {
    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn http(&self) -> &reqwest::Client {
        &self.http
    }

    async fn core_manifest(
        &self,
        id: Uuid,
    ) -> Result<(locaryn_extensions::manifest::CoreManifest, PathBuf), String> {
        let row = self
            .storage
            .extensions
            .get(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "extension introuvable".to_string())?;
        let root = extensions::plugin_root(&row.manifest_path)
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

pub use locaryn_core_bridge::manager::CoreManager;

/// Agent prêt à parler au noyau pour une session (délègue au superviseur
/// partagé). `Err` = le noyau n'est pas joignable — l'appelant affiche le
/// message sans fallback silencieux vers le noyau natif (D2).
pub async fn agent_for_core(
    core: &Core,
    id: &str,
) -> Result<(locaryn_core_bridge::CoreAgent, Option<String>), String> {
    locaryn_core_bridge::manager::agent_for_core(&core.cores, core, id).await
}

// ============================================================================
// Commandes Tauri
// ============================================================================

#[tauri::command]
pub async fn core_status(core: State<'_, Core>, id: String) -> Result<CoreStatus, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    locaryn_core_bridge::manager::status(&core.cores, &*core, uid).await
}

#[tauri::command]
pub async fn core_start(core: State<'_, Core>, id: String) -> Result<CoreStatus, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    locaryn_core_bridge::manager::start(&core.cores, &*core, uid).await
}

#[tauri::command]
pub async fn core_stop(core: State<'_, Core>, id: String) -> Result<CoreStatus, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    locaryn_core_bridge::manager::stop(&core.cores, &*core, uid).await
}

#[tauri::command]
pub async fn core_skills(core: State<'_, Core>, id: String) -> Result<Vec<CoreSkillEntry>, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    locaryn_core_bridge::manager::skills(&core.cores, &*core, uid).await
}

#[tauri::command]
pub async fn core_install_skill(
    core: State<'_, Core>,
    id: String,
    slug: String,
) -> Result<String, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    locaryn_core_bridge::manager::install_skill(&core.cores, &*core, uid, &slug).await
}
