//! `POST /v1/providers/active` — choisir le fournisseur actif par mode.
//!
//! C'est ce qu'appelle `locaryn provider use auto|local|remote` : sans cette
//! route, la commande finissait sur un 405.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use locaryn_shared_types::{ConnectionMode, Provider, ProviderKind, ProviderStatus};

use crate::DaemonState;

#[derive(serde::Deserialize)]
pub(crate) struct CorpsMode {
    mode: ConnectionMode,
}

/// Le fournisseur que le mode désigne parmi ceux enregistrés.
///
/// `local` et `remote` prennent le premier du genre demandé (la liste arrive
/// actif d'abord, puis par ancienneté). `auto` préfère un local en bonne
/// santé, puis un distant, puis n'importe quel local.
pub(crate) fn choisir(fournisseurs: &[Provider], mode: ConnectionMode) -> Option<&Provider> {
    let du_genre = |genre: ProviderKind| fournisseurs.iter().find(|p| p.kind == genre);
    match mode {
        ConnectionMode::Local => du_genre(ProviderKind::Local),
        ConnectionMode::Remote => du_genre(ProviderKind::Remote),
        ConnectionMode::Auto => fournisseurs
            .iter()
            .find(|p| p.kind == ProviderKind::Local && p.status == ProviderStatus::Healthy)
            .or_else(|| du_genre(ProviderKind::Remote))
            .or_else(|| du_genre(ProviderKind::Local)),
    }
}

fn erreur(statut: StatusCode, code: &str, message: String) -> Response {
    (
        statut,
        Json(serde_json::json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

pub(crate) async fn activer(
    State(s): State<Arc<DaemonState>>,
    Json(corps): Json<CorpsMode>,
) -> Response {
    let fournisseurs = match s.storage.providers.list().await {
        Ok(f) => f,
        Err(e) => return erreur(StatusCode::INTERNAL_SERVER_ERROR, "storage_error", e.to_string()),
    };
    let Some(cible) = choisir(&fournisseurs, corps.mode) else {
        return erreur(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("aucun fournisseur ne correspond au mode {:?}", corps.mode),
        );
    };
    match s.storage.providers.set_active(cible.id).await {
        Ok(actif) => (StatusCode::OK, Json(actif)).into_response(),
        Err(e) => erreur(StatusCode::INTERNAL_SERVER_ERROR, "storage_error", e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use locaryn_shared_types::ProviderEngine;

    fn fournisseur(kind: ProviderKind, status: ProviderStatus) -> Provider {
        let maintenant = chrono::Utc::now();
        Provider {
            id: uuid::Uuid::new_v4(),
            kind,
            engine: ProviderEngine::LlamaCpp,
            endpoint: "http://127.0.0.1:8080".into(),
            model: None,
            is_active: false,
            status,
            config: None,
            created_at: maintenant,
            updated_at: maintenant,
        }
    }

    #[test]
    fn chaque_mode_designe_son_genre() {
        let liste = [
            fournisseur(ProviderKind::Remote, ProviderStatus::Healthy),
            fournisseur(ProviderKind::Local, ProviderStatus::Unknown),
        ];
        assert_eq!(choisir(&liste, ConnectionMode::Local).unwrap().id, liste[1].id);
        assert_eq!(choisir(&liste, ConnectionMode::Remote).unwrap().id, liste[0].id);
    }

    #[test]
    fn auto_prefere_un_local_en_bonne_sante() {
        let liste = [
            fournisseur(ProviderKind::Remote, ProviderStatus::Healthy),
            fournisseur(ProviderKind::Local, ProviderStatus::Healthy),
        ];
        assert_eq!(choisir(&liste, ConnectionMode::Auto).unwrap().id, liste[1].id);
    }

    #[test]
    fn auto_retombe_sur_le_distant_puis_le_local() {
        let local_eteint = fournisseur(ProviderKind::Local, ProviderStatus::Unhealthy);
        let distant = fournisseur(ProviderKind::Remote, ProviderStatus::Unknown);
        let deux = [local_eteint.clone(), distant.clone()];
        assert_eq!(choisir(&deux, ConnectionMode::Auto).unwrap().id, distant.id);
        let seul = [local_eteint.clone()];
        assert_eq!(choisir(&seul, ConnectionMode::Auto).unwrap().id, local_eteint.id);
        assert!(choisir(&[], ConnectionMode::Remote).is_none());
    }
}
