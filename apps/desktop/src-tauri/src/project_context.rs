//! Le contexte d'un projet, et ce que la portée exige.
//!
//! Le stockage accepte les trois portées sans discuter — c'est le serveur
//! lui-même qui écrit les fiches partagées, il ne peut pas s'interdire de le
//! faire. La règle vit donc ici, à l'endroit qui sait si un serveur répond.
//!
//! **Deux portées sur trois n'ont de sens qu'avec un serveur.** « Suivre la
//! personne entre ses appareils » suppose un point commun entre ces appareils ;
//! « visible de tous les collaborateurs » suppose des collaborateurs. Sans
//! serveur, écrire une fiche partagée produirait une fiche que personne ne
//! verra jamais, rangée dans une base locale — c'est-à-dire exactement une
//! fiche `machine`, mais avec une étiquette qui mentirait.
//!
//! On refuse donc, en disant quoi faire. C'est plus utile qu'un enregistrement
//! silencieux dont on découvrirait l'inutilité en changeant d'ordinateur.

use locaryn_storage::project_context::{ContextEntry, ContextScope};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::Core;

/// Ce que cette installation peut faire du contexte, et pourquoi.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ContextAvailability {
    /// Les portées utilisables ici, `machine` toujours comprise.
    pub scopes: Vec<String>,
    /// Un serveur répond — le sien, ou celui auquel on est connecté.
    pub server: bool,
    /// Ce qui manque pour les deux autres portées. `None` quand rien ne manque.
    pub blocker: Option<String>,
}

/// Un serveur répond-il, d'une façon ou d'une autre ?
///
/// Deux situations valent : cette machine sert (le mode serveur est allumé), ou
/// elle est cliente d'un serveur distant. Dans les deux cas il existe un
/// endroit commun où une fiche partagée a un sens. Les distinguer n'apporterait
/// rien à l'appelant.
async fn serveur_present(core: &Core) -> bool {
    if core.remote_client().is_some() {
        return true;
    }
    crate::server_mode::server_status()
        .await
        .map(|s| s.running)
        .unwrap_or(false)
}

#[tauri::command]
pub async fn context_availability(core: State<'_, Core>) -> Result<ContextAvailability, String> {
    let server = serveur_present(&core).await;
    Ok(ContextAvailability {
        scopes: if server {
            vec!["machine".into(), "compte".into(), "partage".into()]
        } else {
            vec!["machine".into()]
        },
        server,
        blocker: if server {
            None
        } else {
            Some(
                "Le contexte du compte et le contexte partagé passent par un serveur : \
                 activez le mode serveur, ou connectez-vous à un serveur existant. \
                 Sans lui, une fiche « partagée » resterait sur cet ordinateur."
                    .into(),
            )
        },
    })
}

/// Refuse une portée que cette installation ne peut pas honorer.
async fn verifier_portee(core: &Core, scope: ContextScope) -> Result<(), String> {
    if !scope.exige_serveur() {
        return Ok(());
    }
    if serveur_present(core).await {
        return Ok(());
    }
    Err(format!(
        "La portée « {} » exige un serveur : activez le mode serveur, ou connectez-vous à un \
         serveur existant. En attendant, rangez cette fiche en « machine » — elle ne quittera \
         pas cet ordinateur, mais au moins son étiquette dira la vérité.",
        scope.as_str()
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberContextArgs {
    pub project_id: String,
    /// `machine`, `compte` ou `partage`. Un mot inconnu retombe sur `machine`.
    pub scope: String,
    pub title: String,
    pub detail: String,
    /// `utilisateur` ou `assistant`.
    #[serde(default)]
    pub source: Option<String>,
}

#[tauri::command]
pub async fn remember_context(
    core: State<'_, Core>,
    args: RememberContextArgs,
) -> Result<ContextEntry, String> {
    let scope = ContextScope::depuis(&args.scope);
    verifier_portee(&core, scope).await?;
    // L'auteur n'est renseigné qu'en mode serveur, où plusieurs personnes
    // écrivent : sur un poste seul, il n'apprendrait rien à personne.
    let author = crate::client_cert::current_session()
        .ok()
        .flatten()
        .map(|s| s.username);
    core.storage
        .project_context
        .remember(
            &args.project_id,
            scope,
            author.as_deref(),
            &args.title,
            &args.detail,
            args.source.as_deref().unwrap_or("utilisateur"),
        )
        .await
        .map_err(|e| e.to_string())
}

/// Les fiches d'un projet.
///
/// Sans serveur, seules les fiches `machine` sont rendues : annoncer un
/// contexte partagé qu'on ne peut pas relire ferait croire à une synchronisation
/// qui n'a pas lieu.
#[tauri::command]
pub async fn list_context(
    core: State<'_, Core>,
    project_id: String,
) -> Result<Vec<ContextEntry>, String> {
    let scopes: Vec<ContextScope> = if serveur_present(&core).await {
        vec![
            ContextScope::Machine,
            ContextScope::Compte,
            ContextScope::Partage,
        ]
    } else {
        vec![ContextScope::Machine]
    };
    core.storage
        .project_context
        .list(&project_id, &scopes)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetContextScopeArgs {
    pub id: String,
    pub scope: String,
}

/// Change la portée d'une fiche.
///
/// C'est ce que la question posée à l'écran déclenche : une fiche écrite en
/// personnel se révèle concerner tout le projet, ou l'inverse.
#[tauri::command]
pub async fn set_context_scope(
    core: State<'_, Core>,
    args: SetContextScopeArgs,
) -> Result<ContextEntry, String> {
    let scope = ContextScope::depuis(&args.scope);
    verifier_portee(&core, scope).await?;
    core.storage
        .project_context
        .set_scope(&args.id, scope)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_context_summary(
    core: State<'_, Core>,
    id: String,
    summary: String,
) -> Result<(), String> {
    core.storage
        .project_context
        .set_summary(&id, &summary)
        .await
        .map_err(|e| e.to_string())
}

/// Retire un détail d'une fiche, en gardant les autres.
#[tauri::command]
pub async fn remove_context_detail(
    core: State<'_, Core>,
    id: String,
    detail: String,
) -> Result<ContextEntry, String> {
    let fiche = core
        .storage
        .project_context
        .find(&id)
        .await
        .map_err(|e| e.to_string())?;
    let restants: Vec<String> = fiche.details.into_iter().filter(|d| d != &detail).collect();
    core.storage
        .project_context
        .set_details(&id, &restants)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn forget_context(core: State<'_, Core>, id: String) -> Result<(), String> {
    core.storage
        .project_context
        .forget(&id)
        .await
        .map_err(|e| e.to_string())
}
