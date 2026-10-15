//! Réglages du NPU des PC Snapdragon (voir `locaryn_provider_supervisor::npu`).

use crate::Core;
use locaryn_provider_supervisor::{npu, runtime_install};
use locaryn_shared_types::ProviderEngine;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct EtatNpu {
    /// Un PC Snapdragon sous Windows.
    pub snapdragon: bool,
    /// Le NPU n'est pas coupé dans les réglages.
    pub voulu: bool,
    /// Le moteur NPU est téléchargé.
    pub installe: bool,
    /// La raison du dernier échec : le moteur Adreno a pris le relais.
    pub echec: Option<String>,
    /// Le dossier du moteur NPU (avec `activer-npu.ps1`).
    pub dossier: String,
}

#[tauri::command]
pub fn npu_etat() -> EtatNpu {
    EtatNpu {
        snapdragon: npu::machine_snapdragon(),
        voulu: npu::voulu(),
        installe: runtime_install::npu_llama_path().is_file(),
        echec: npu::echec(),
        dossier: runtime_install::npu_llama_dir().display().to_string(),
    }
}

/// Activer ou couper le NPU. Le moteur en marche s'arrête : le prochain
/// message le relance avec le bon choix.
#[tauri::command]
pub async fn npu_regler(core: State<'_, Core>, actif: bool) -> Result<EtatNpu, String> {
    npu::regler(actif).map_err(|e| e.to_string())?;
    npu::effacer_echec();
    let _ = core.supervisor.shutdown(&ProviderEngine::LlamaCpp).await;
    Ok(npu_etat())
}

/// Oublier l'échec (après `activer-npu.ps1` et un redémarrage, par exemple)
/// et retenter le NPU au prochain chargement.
#[tauri::command]
pub async fn npu_reessayer(core: State<'_, Core>) -> Result<EtatNpu, String> {
    npu::effacer_echec();
    let _ = core.supervisor.shutdown(&ProviderEngine::LlamaCpp).await;
    Ok(npu_etat())
}
