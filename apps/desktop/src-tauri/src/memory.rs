//! Ce que Locaryn retient de la personne, vu depuis l'application.
//!
//! Les entrées vivent dans la base du service — la même que le téléphone
//! interroge — et non dans un fichier de l'application. L'écran des réglages
//! les montre toutes, dans le texte exact qui part au modèle : c'est ce qui
//! permet de corriger une mémoire fausse plutôt que de la subir.
//!
//! `run_memory_command` est la boîte de commande en langage naturel : au lieu
//! de supprimer ou corriger une fiche à la main, on décrit ce qu'on veut et le
//! modèle actif traduit ça en actions précises (`titling::ask_memory_command`),
//! appliquées seulement sur des fiches qui existent réellement.

use crate::Core;
use locaryn_agent_runtime::titling::{self, FicheResumee, MemoryAction};
use locaryn_storage::memory::MemoryEntry;
use serde::Serialize;
use tauri::State;

#[tauri::command]
pub async fn list_memory(core: State<'_, Core>) -> Result<Vec<MemoryEntry>, String> {
    core.storage
        .memory
        .list(None)
        .await
        .map_err(|e| e.to_string())
}

/// Retenir un détail à la main — depuis un champ de saisie plutôt qu'une
/// conversation. `group` retombe sur `sujets` s'il n'est pas l'un des quatre
/// groupes connus.
#[tauri::command]
pub async fn remember(
    core: State<'_, Core>,
    group: String,
    title: String,
    detail: String,
) -> Result<MemoryEntry, String> {
    core.storage
        .memory
        .remember(None, &group, &title, &detail, "utilisateur")
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_memory_summary(
    core: State<'_, Core>,
    id: String,
    summary: String,
) -> Result<MemoryEntry, String> {
    core.storage
        .memory
        .set_summary(&id, &summary)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_memory_entry(
    core: State<'_, Core>,
    id: String,
    title: String,
) -> Result<MemoryEntry, String> {
    core.storage
        .memory
        .rename(&id, &title)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_memory_group(
    core: State<'_, Core>,
    id: String,
    group: String,
) -> Result<MemoryEntry, String> {
    core.storage
        .memory
        .set_group(&id, &group)
        .await
        .map_err(|e| e.to_string())
}

/// Retirer un détail précis d'une fiche, en gardant les autres.
#[tauri::command]
pub async fn remove_memory_detail(
    core: State<'_, Core>,
    id: String,
    detail: String,
) -> Result<MemoryEntry, String> {
    let entry = core
        .storage
        .memory
        .find(&id)
        .await
        .map_err(|e| e.to_string())?;
    let restants: Vec<String> = entry.details.into_iter().filter(|d| d != &detail).collect();
    core.storage
        .memory
        .set_details(&id, &restants)
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
// La boîte de commande : décrire ce qu'il faut changer, plutôt que le faire
// à la main.
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCommandResult {
    /// Ce qui a été fait, en une phrase — calculé ici à partir des actions
    /// réellement appliquées, jamais recopié depuis ce que le modèle a dit :
    /// c'est le seul moyen de garantir que la confirmation correspond à la
    /// réalité.
    pub summary: String,
    pub applied: u32,
    pub entries: Vec<MemoryEntry>,
}

/// Traduit une instruction en actions sur la mémoire existante, les
/// applique, et renvoie la liste à jour.
///
/// N'agit que sur des fiches qui existent déjà : `ask_memory_command` filtre
/// tout identifiant que le modèle aurait inventé, et chaque action est
/// réappliquée ici une par une, pour qu'une action mal formée n'empêche pas
/// les autres.
#[tauri::command]
pub async fn run_memory_command(
    core: State<'_, Core>,
    instruction: String,
) -> Result<MemoryCommandResult, String> {
    let instruction = instruction.trim();
    if instruction.is_empty() {
        return Err("l'instruction est vide".into());
    }

    let entries = core
        .storage
        .memory
        .list(None)
        .await
        .map_err(|e| e.to_string())?;
    if entries.is_empty() {
        return Ok(MemoryCommandResult {
            summary: "La mémoire est vide : il n'y a rien sur quoi agir.".into(),
            applied: 0,
            entries,
        });
    }

    let (endpoint, modele) = endpoint_et_micro_modele(&core).await?;
    let fiches: Vec<FicheResumee> = entries
        .iter()
        .map(|e| FicheResumee {
            id: e.id.clone(),
            group: e.group.clone(),
            title: e.title.clone(),
            summary: e.summary.clone(),
        })
        .collect();

    let client = reqwest::Client::new();
    let actions =
        titling::ask_memory_command(&endpoint, &client, &modele, instruction, &fiches).await;

    if actions.is_empty() {
        return Ok(MemoryCommandResult {
            summary:
                "Aucune fiche ne correspond à cette instruction. Reformulez, ou ouvrez la fiche \
                 visée pour la corriger directement."
                    .into(),
            applied: 0,
            entries,
        });
    }

    let mut oubliees = 0u32;
    let mut resumees = 0u32;
    let mut renommees = 0u32;
    for action in &actions {
        let resultat = match action {
            MemoryAction::Forget { id } => core.storage.memory.forget(id).await.map(|_| ()),
            MemoryAction::SetSummary { id, summary } => core
                .storage
                .memory
                .set_summary(id, summary)
                .await
                .map(|_| ()),
            MemoryAction::Rename { id, title } => {
                core.storage.memory.rename(id, title).await.map(|_| ())
            }
        };
        match (resultat, action) {
            (Ok(()), MemoryAction::Forget { .. }) => oubliees += 1,
            (Ok(()), MemoryAction::SetSummary { .. }) => resumees += 1,
            (Ok(()), MemoryAction::Rename { .. }) => renommees += 1,
            (Err(e), _) => tracing::debug!(error = %e, "action de mémoire non appliquée"),
        }
    }

    let mut parts = Vec::new();
    if oubliees > 0 {
        parts.push(format!(
            "{oubliees} fiche{} oubliée{}",
            if oubliees == 1 { "" } else { "s" },
            if oubliees == 1 { "" } else { "s" }
        ));
    }
    if resumees > 0 {
        parts.push(format!(
            "{resumees} résumé{} corrigé{}",
            if resumees == 1 { "" } else { "s" },
            if resumees == 1 { "" } else { "s" }
        ));
    }
    if renommees > 0 {
        parts.push(format!(
            "{renommees} fiche{} renommée{}",
            if renommees == 1 { "" } else { "s" },
            if renommees == 1 { "" } else { "s" }
        ));
    }
    let applied = oubliees + resumees + renommees;
    let summary = if applied == 0 {
        "Aucune des actions proposées n'a pu être appliquée.".to_string()
    } else {
        parts.join(", ") + "."
    };

    let entries = core
        .storage
        .memory
        .list(None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(MemoryCommandResult {
        summary,
        applied,
        entries,
    })
}

/// Le point d'entrée compatible OpenAI actif, et le modèle à charger pour une
/// micro-tâche — le même calcul que pour les autres tâches secondaires
/// (titrage, plongements) : le modèle dédié s'il est configuré, sinon celui
/// déjà chargé pour ne pas échanger de modèle en VRAM pour une tâche courte.
async fn endpoint_et_micro_modele(core: &Core) -> Result<(String, String), String> {
    let providers = core
        .storage
        .providers
        .list()
        .await
        .map_err(|e| e.to_string())?;
    let actif = providers
        .into_iter()
        .find(|p| {
            p.is_active
                && matches!(
                    p.engine,
                    locaryn_shared_types::ProviderEngine::LlamaCpp
                        | locaryn_shared_types::ProviderEngine::OpenAiCompat
                        | locaryn_shared_types::ProviderEngine::Ollama
                        | locaryn_shared_types::ProviderEngine::Extension(_)
                )
        })
        .ok_or_else(|| {
            "Aucun moteur d'inférence actif. Démarrez-en un pour pouvoir gérer la mémoire par \
             instruction."
                .to_string()
        })?;

    let micro = locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model)
        .filter(|m| !m.trim().is_empty());
    let modele = match micro {
        Some(choisi) => locaryn_config::micro_effectif(&choisi, actif.model.as_deref()),
        None => actif
            .model
            .ok_or_else(|| "Le moteur actif n'annonce aucun modèle.".to_string())?,
    };
    Ok((actif.endpoint, modele))
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
