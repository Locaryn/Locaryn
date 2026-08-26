//! Moteurs d'inférence — commandes Tauri de l'écran Réglages → Moteur.
//!
//! Un moteur d'inférence calcule les jetons. Ce n'est pas un noyau : le noyau
//! change l'agent (sa boucle, sa mémoire) et vit dans [`crate::core_engines`] ;
//! un moteur change seulement qui fait tourner les poids, et la boucle
//! d'outils, l'approbation et le streaming de Locaryn restent les siens.
//!
//! Deux familles cohabitent dans la même liste :
//!
//! - les **runtimes intégrés** — llama.cpp et AirLLM, livrés avec
//!   l'application ;
//! - les **moteurs apportés par une extension** — décrits par la section
//!   `engine` de leur manifeste, supervisés par le même code.
//!
//! L'application ne nomme aucune extension : elle lit ce que le manifeste
//! déclare. Le nom affiché, les formats servis, la commande de lancement et la
//! phrase d'exigence matérielle viennent tous de l'auteur du moteur.

use locaryn_shared_types::ProviderEngine;
use serde::Serialize;
use tauri::State;

use crate::Core;

/// Un moteur, tel que l'écran des réglages doit le montrer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    /// Jeton du moteur (`llama_cpp`, `ext:mon-moteur`) — l'identifiant que
    /// les autres commandes attendent.
    pub engine: String,
    /// Nom affiché.
    pub label: String,
    /// Extension qui l'apporte, `None` pour un runtime intégré.
    pub extension: Option<String>,
    pub extension_version: Option<String>,
    pub endpoint: String,
    /// Le moteur répond.
    pub healthy: bool,
    /// Le processus est celui que l'application a lancé (donc qu'elle peut
    /// arrêter).
    pub owned: bool,
    /// Ce moteur est celui du fournisseur actif.
    pub active: bool,
    /// Modèle enregistré pour le fournisseur actif, quand c'est ce moteur.
    pub model: Option<String>,
    /// Formats de poids servis, en mots lisibles (`gguf`, `safetensors`,
    /// `répertoire de checkpoint`, `dépôt Hugging Face`).
    pub formats: Vec<String>,
    /// Ce qui manque sur cette machine pour que ce moteur tourne — la phrase
    /// écrite par l'auteur du moteur, affichée telle quelle. `None` quand la
    /// machine convient.
    pub unmet_requirement: Option<String>,
    /// Chemin du journal du moteur, quand il en a un. C'est là que se lit la
    /// raison d'un démarrage manqué.
    pub log_path: Option<String>,
}

fn nom_integre(engine: &ProviderEngine) -> &'static str {
    match engine {
        ProviderEngine::Ollama => "Ollama",
        ProviderEngine::LlamaCpp => "llama.cpp (intégré)",
        ProviderEngine::Lmstudio => "LM Studio",
        ProviderEngine::Vllm => "vLLM",
        ProviderEngine::OpenAiCompat => "Serveur compatible OpenAI",
        ProviderEngine::AirLlm => "AirLLM",
        ProviderEngine::Extension(_) => "Moteur d'extension",
    }
}

/// Les formats d'un runtime intégré. Écrits ici parce que ces runtimes sont
/// livrés avec l'application ; ceux des extensions viennent de leur manifeste.
fn formats_integres(engine: &ProviderEngine) -> Vec<String> {
    match engine {
        ProviderEngine::LlamaCpp => vec!["gguf".to_string()],
        ProviderEngine::AirLlm => vec!["dépôt Hugging Face".to_string()],
        _ => Vec::new(),
    }
}

/// Traduit les formats déclarés par un moteur d'extension en mots que l'écran
/// peut afficher.
fn formats_declares(m: &locaryn_extensions::manifest::EngineManifest) -> Vec<String> {
    let mut out: Vec<String> = m.model_formats.files.clone();
    if m.model_formats.directories {
        out.push("répertoire de checkpoint".to_string());
    }
    if m.model_formats.hf_repo_ids {
        out.push("dépôt Hugging Face".to_string());
    }
    out
}

/// Tous les moteurs connus, intégrés et apportés, avec leur état.
#[tauri::command]
pub async fn list_inference_engines(core: State<'_, Core>) -> Result<Vec<EngineInfo>, String> {
    let actif = core.storage.providers.active().await.ok().flatten();
    let snapshot = core.supervisor.status_snapshot().await;
    let specs = core.supervisor.extension_engines().await;

    let mut out = Vec::with_capacity(snapshot.len());
    for entry in snapshot {
        let spec = specs.iter().find(|s| s.engine() == entry.engine);
        let est_actif = actif.as_ref().is_some_and(|p| p.engine == entry.engine);
        out.push(EngineInfo {
            engine: entry.engine.as_token(),
            label: entry
                .label
                .clone()
                .unwrap_or_else(|| nom_integre(&entry.engine).to_string()),
            extension: spec.map(|s| s.extension_name.clone()),
            extension_version: spec.map(|s| s.extension_version.clone()),
            endpoint: entry.endpoint.clone(),
            healthy: entry.healthy,
            owned: entry.owned,
            active: est_actif,
            model: if est_actif {
                actif.as_ref().and_then(|p| p.model.clone())
            } else {
                None
            },
            formats: match spec {
                Some(s) => formats_declares(&s.manifest),
                None => formats_integres(&entry.engine),
            },
            unmet_requirement: spec.and_then(|s| s.unmet_requirement()),
            log_path: spec.map(|s| {
                locaryn_provider_supervisor::extension_engine::log_file_path(&s.id)
                    .display()
                    .to_string()
            }),
        });
    }
    Ok(out)
}

/// Rend ce moteur actif et démarre son processus.
///
/// Un modèle peut être imposé ; sinon celui déjà enregistré pour ce moteur est
/// repris, et à défaut le moteur démarre sans modèle — ce qu'il accepte ou non,
/// selon ce que son manifeste déclare.
#[tauri::command]
pub async fn start_inference_engine(
    core: State<'_, Core>,
    engine: String,
    model: Option<String>,
) -> Result<EngineInfo, String> {
    let moteur = ProviderEngine::from_token(&engine)
        .ok_or_else(|| format!("moteur inconnu : « {engine} »"))?;
    let endpoint = core
        .supervisor
        .endpoint_for(&moteur)
        .await
        .ok_or_else(|| {
            format!(
                "moteur {} inconnu — l'extension qui l'apportait n'est plus installée ou active",
                moteur.as_token()
            )
        })?;

    // Un moteur qui ne peut pas tourner ici le dit avant, avec la phrase de
    // son auteur. Le démarrer pour échouer trente secondes plus tard n'apprend
    // rien à personne.
    if let Some(spec) = core.supervisor.extension_engine_spec(&moteur).await {
        if let Some(raison) = spec.unmet_requirement() {
            return Err(raison);
        }
        if let Some(m) = model.as_deref().filter(|m| !m.trim().is_empty()) {
            if !spec.serves_model(m) {
                return Err(format!(
                    "« {} » ne sert pas « {m} » : il charge {}.",
                    spec.label,
                    formats_declares(&spec.manifest).join(", ")
                ));
            }
        }
    }

    let modele = match model.filter(|m| !m.trim().is_empty()) {
        Some(m) => Some(m),
        None => core
            .storage
            .providers
            .list()
            .await
            .ok()
            .and_then(|ps| ps.into_iter().find(|p| p.engine == moteur))
            .and_then(|p| p.model),
    };

    core.storage
        .providers
        .upsert_local(&moteur, &endpoint, modele)
        .await
        .map_err(|e| e.to_string())?;

    core.supervisor
        .ensure_running(&moteur)
        .await
        .map_err(|e| complete_avec_le_journal(&core, &moteur, &e.to_string()))?;
    crate::refresh_mcp_runtime_env(&core).await;

    let liste = list_inference_engines(core).await?;
    liste
        .into_iter()
        .find(|e| e.engine == moteur.as_token())
        .ok_or_else(|| "moteur démarré mais absent de la liste".to_string())
}

/// Arrête le processus d'un moteur que l'application a lancé.
#[tauri::command]
pub async fn stop_inference_engine(core: State<'_, Core>, engine: String) -> Result<(), String> {
    let moteur = ProviderEngine::from_token(&engine)
        .ok_or_else(|| format!("moteur inconnu : « {engine} »"))?;
    core.supervisor.set_pinned(&moteur, false).await;
    core.supervisor
        .shutdown(&moteur)
        .await
        .map_err(|e| e.to_string())
}

/// La fin du journal d'un moteur d'extension.
///
/// Un moteur qui refuse de démarrer écrit pourquoi dans son journal ; sans
/// cette commande, l'utilisateur voit « démarrage impossible » et doit aller
/// chercher un fichier dont il ignore l'emplacement.
#[tauri::command]
pub async fn inference_engine_log(
    core: State<'_, Core>,
    engine: String,
    lines: Option<usize>,
) -> Result<String, String> {
    let moteur = ProviderEngine::from_token(&engine)
        .ok_or_else(|| format!("moteur inconnu : « {engine} »"))?;
    let id = moteur
        .extension_id()
        .ok_or_else(|| "seuls les moteurs d'extension ont leur propre journal".to_string())?;
    // Le moteur doit être connu : sans ce contrôle, un jeton fabriqué ferait
    // lire un fichier nommé par l'appelant.
    if core
        .supervisor
        .extension_engine_spec(&moteur)
        .await
        .is_none()
    {
        return Err(format!("moteur {} inconnu", moteur.as_token()));
    }
    let chemin = locaryn_provider_supervisor::extension_engine::log_file_path(id);
    let contenu = std::fs::read_to_string(&chemin)
        .map_err(|e| format!("journal illisible ({}) : {e}", chemin.display()))?;
    let garder = lines.unwrap_or(200).clamp(1, 5_000);
    let toutes: Vec<&str> = contenu.lines().collect();
    let debut = toutes.len().saturating_sub(garder);
    Ok(toutes[debut..].join("\n"))
}

/// Ajoute la fin du journal à un message d'échec, quand il y en a une.
///
/// « Le moteur n'a pas démarré en 300s » n'apprend rien ; la dernière ligne du
/// journal dit s'il manque un pilote, un poids ou une bibliothèque.
fn complete_avec_le_journal(core: &Core, moteur: &ProviderEngine, message: &str) -> String {
    let _ = core;
    let Some(id) = moteur.extension_id() else {
        return message.to_string();
    };
    let chemin = locaryn_provider_supervisor::extension_engine::log_file_path(id);
    let Ok(contenu) = std::fs::read_to_string(&chemin) else {
        return message.to_string();
    };
    let queue: Vec<&str> = contenu
        .lines()
        .filter(|l| !l.trim().is_empty())
        .rev()
        .take(6)
        .collect();
    if queue.is_empty() {
        return message.to_string();
    }
    let mut lignes: Vec<&str> = queue;
    lignes.reverse();
    format!("{message}\n\nFin du journal ({}) :\n{}", chemin.display(), lignes.join("\n"))
}
