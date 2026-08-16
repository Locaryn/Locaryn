//! Ce que Locaryn retient de la personne, vu depuis l'application.
//!
//! Les entrées vivent dans la base du service — la même que le téléphone
//! interroge — et non dans un fichier de l'application. L'écran des réglages
//! les montre toutes, dans le texte exact qui part au modèle : c'est ce qui
//! permet de corriger une mémoire fausse plutôt que de la subir.

use crate::Core;
use locaryn_storage::memory::MemoryEntry;
use tauri::State;

#[tauri::command]
pub async fn list_memory(core: State<'_, Core>) -> Result<Vec<MemoryEntry>, String> {
    core.storage
        .memory
        .list(None)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remember(
    core: State<'_, Core>,
    category: String,
    content: String,
) -> Result<MemoryEntry, String> {
    core.storage
        .memory
        .remember(None, &category, &content, "utilisateur")
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn edit_memory(
    core: State<'_, Core>,
    id: String,
    category: String,
    content: String,
) -> Result<MemoryEntry, String> {
    core.storage
        .memory
        .update(&id, &category, &content)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn forget_memory(core: State<'_, Core>, id: String) -> Result<(), String> {
    core.storage
        .memory
        .forget(&id)
        .await
        .map_err(|e| e.to_string())
}

/// Tout oublier. Un oubli est définitif : garder une trace de ce que quelqu'un
/// a demandé d'oublier serait le contraire de ce qu'il a demandé.
#[tauri::command]
pub async fn forget_all_memory(core: State<'_, Core>) -> Result<u64, String> {
    core.storage
        .memory
        .forget_all(None)
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// Vitesses mesurées
// ============================================================================

/// Ce que chaque modèle donne sur cette machine.
///
/// Rangé ici plutôt que dans un module à part : c'est la même base, et les
/// deux répondent à la même idée — ce que le produit sait de vous et de votre
/// matériel doit être lisible.
#[tauri::command]
pub async fn list_model_metrics(
    core: State<'_, Core>,
) -> Result<Vec<locaryn_storage::metrics::ModelMetric>, String> {
    core.storage.metrics.list().await.map_err(|e| e.to_string())
}
