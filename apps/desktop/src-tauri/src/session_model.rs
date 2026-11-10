//! Le modèle de chaque conversation.
//!
//! Le fournisseur actif est un réglage global : changer de conversation ne
//! changeait donc pas de modèle, et deux conversations ne pouvaient pas parler
//! à deux modèles différents (Claude ici, OmniRoute là, un modèle local
//! ailleurs). Chaque conversation retient maintenant le fournisseur et le modèle
//! avec lesquels on lui a parlé ; à l'envoi, c'est le sien qui est activé.

use crate::Core;
use locaryn_shared_types::{ProviderKind, Session};
use tauri::State;
use uuid::Uuid;

/// Une autre conversation est-elle en train de répondre ?
///
/// Changer de modèle local arrête le moteur : le faire pendant qu'une autre
/// réponse se génère la couperait en plein milieu.
async fn autre_reponse_en_cours(core: &Core, session_id: Uuid) -> bool {
    core.chat_cancels
        .lock()
        .await
        .iter()
        .any(|(id, jetons)| *id != session_id && !jetons.is_empty())
}

/// Retenir, pour cette conversation, le modèle actif en ce moment.
pub(crate) async fn retenir(core: &Core, session_id: Uuid) -> Result<(), String> {
    let Some(actif) = core
        .storage
        .providers
        .active()
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    let Some(modele) = actif.model.clone().filter(|m| !m.trim().is_empty()) else {
        return Ok(());
    };
    core.storage
        .sessions
        .set_model(session_id, Some(actif.id), Some(&modele))
        .await
        .map_err(|e| e.to_string())
}

/// Activer le modèle que cette conversation a retenu.
///
/// `Ok(false)` : elle n'en a pas retenu (ou son fournisseur a disparu) ; rien
/// ne change. `Err` : il y a un modèle à activer et cela ne se peut pas
/// maintenant, avec la raison.
///
/// Rien n'est arrêté ni chargé ici : parcourir ses conversations ne doit pas
/// éjecter le modèle en mémoire. C'est à l'envoi que le superviseur compare
/// le modèle réellement servi à celui attendu, et recharge s'il le faut.
/// `a_l_envoi` : refuser si ce rechargement couperait la réponse en cours
/// d'une autre conversation.
pub(crate) async fn appliquer(
    core: &Core,
    session: &Session,
    a_l_envoi: bool,
) -> Result<bool, String> {
    let (Some(fournisseur_id), Some(modele)) = (session.provider_id, session.model.clone()) else {
        return Ok(false);
    };
    let Some(voulu) = core
        .storage
        .providers
        .get(fournisseur_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(false);
    };
    let actif = core
        .storage
        .providers
        .active()
        .await
        .map_err(|e| e.to_string())?;
    // Un message va partir vers un modèle local qui n'est pas celui en
    // mémoire : le moteur va recharger, ce qui couperait une réponse en cours
    // ailleurs.
    if a_l_envoi
        && voulu.kind == ProviderKind::Local
        && matches!(voulu.engine, locaryn_shared_types::ProviderEngine::LlamaCpp)
        && autre_reponse_en_cours(core, session.id).await
        && !locaryn_provider_supervisor::is_llama_server_model_match(
            &core.http,
            &voulu.endpoint,
            Some(modele.as_str()),
        )
        .await
    {
        return Err(
            "Une autre conversation est en train de répondre avec un autre modèle local :              attendez qu'elle ait fini avant d'envoyer ici."
                .into(),
        );
    }
    if actif
        .as_ref()
        .is_some_and(|a| a.id == voulu.id && a.model.as_deref() == Some(modele.as_str()))
    {
        return Ok(true);
    }

    match voulu.kind {
        ProviderKind::Remote => {
            let Some(identifiant) = voulu
                .config
                .as_ref()
                .and_then(|c| c.get("cloud_provider"))
                .and_then(|c| c.as_str())
            else {
                return Ok(false);
            };
            core.storage
                .providers
                .upsert_cloud(identifiant, &voulu.endpoint, &modele)
                .await
                .map_err(|e| e.to_string())?;
            // Le moteur local n'a plus rien à servir : le garder résident
            // occuperait de la mémoire pour un modèle que cette conversation
            // n'appelle pas.
            core.supervisor
                .set_pinned(&locaryn_shared_types::ProviderEngine::LlamaCpp, false)
                .await;
        }
        ProviderKind::Local => {
            core.storage
                .providers
                .upsert_local(&voulu.engine, &voulu.endpoint, Some(modele))
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    crate::refresh_mcp_runtime_env(core).await;
    Ok(true)
}

/// La conversation ouverte retient le modèle qui vient d'être choisi.
#[tauri::command]
pub async fn remember_session_model(core: State<'_, Core>, session_id: Uuid) -> Result<(), String> {
    retenir(&core, session_id).await
}

/// Passer à la conversation : son modèle redevient le modèle actif.
#[tauri::command]
pub async fn apply_session_model(core: State<'_, Core>, session_id: Uuid) -> Result<bool, String> {
    let session = core
        .storage
        .sessions
        .get(session_id)
        .await
        .map_err(|e| e.to_string())?;
    appliquer(&core, &session, false).await
}
