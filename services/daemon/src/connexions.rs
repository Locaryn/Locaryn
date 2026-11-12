//! Le journal des connexions des comptes, côté service.
//!
//! Chaque session ouverte y laisse une ligne (heure, adresse, client), puis
//! chaque retour d'un appareil depuis une autre adresse. La route
//! `GET /v1/auth/devices` rend au compte appelant ses appareils et leur
//! historique — jamais ceux d'un autre.

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use locaryn_storage::connexions::{ConnectionRepo, Method, NewConnection};
use locaryn_storage::users::User;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

/// Le jeton qui porte la requête en cours, posé par l'authentification : la
/// liste des appareils marque celui d'où l'on regarde.
#[derive(Clone, Copy, Debug)]
pub struct JetonCourant(pub Uuid);

/// L'en-tête `User-Agent`, s'il est lisible.
pub fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// L'adresse du pair, sans le préfixe IPv4-dans-IPv6.
pub fn adresse(pair: Option<SocketAddr>) -> Option<String> {
    pair.map(|p| p.ip().to_canonical().to_string())
}

/// Inscrire une connexion ; un échec se journalise sans refuser la session.
pub async fn noter(
    journal: &ConnectionRepo,
    user_id: Uuid,
    token_id: Uuid,
    ip: Option<&str>,
    user_agent: Option<&str>,
    method: Method,
) {
    if let Some(ip) = ip {
        derniere_adresse()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(token_id, ip.to_string());
    }
    if let Err(e) = journal
        .record(NewConnection {
            user_id,
            token_id: Some(token_id),
            ip,
            user_agent,
            method,
        })
        .await
    {
        tracing::warn!(error = %e, "connexion non inscrite au journal");
    }
}

/// La dernière adresse vue par appareil, en mémoire : sans elle, chaque
/// requête relirait la base pour savoir si l'adresse a changé.
fn derniere_adresse() -> &'static Mutex<HashMap<Uuid, String>> {
    static VUES: OnceLock<Mutex<HashMap<Uuid, String>>> = OnceLock::new();
    VUES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Un appareil déjà connecté fait une requête : s'il vient d'une adresse
/// différente de la dernière connue, une ligne « nouvelle adresse ».
pub async fn noter_activite(
    journal: &ConnectionRepo,
    user_id: Uuid,
    token_id: Uuid,
    ip: Option<&str>,
    user_agent: Option<&str>,
) {
    let Some(ip) = ip else { return };
    let connue = derniere_adresse()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&token_id)
        .cloned();
    if connue.as_deref() == Some(ip) {
        return;
    }
    let en_base = match connue {
        Some(c) => Some(c),
        None => match journal.last_ip(token_id).await {
            Ok(ip) => ip,
            Err(e) => {
                tracing::warn!(error = %e, "journal des connexions illisible");
                return;
            }
        },
    };
    if en_base.as_deref() == Some(ip) {
        derniere_adresse()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(token_id, ip.to_string());
        return;
    }
    noter(
        journal,
        user_id,
        token_id,
        Some(ip),
        user_agent,
        Method::NewAddress,
    )
    .await;
}

/// GET /v1/auth/devices — les appareils du compte appelant et leur historique.
pub async fn appareils(
    axum::extract::State(state): axum::extract::State<Arc<crate::auth::AuthState>>,
    user: Option<axum::extract::Extension<User>>,
    courant: Option<axum::extract::Extension<JetonCourant>>,
) -> Response {
    let Some(axum::extract::Extension(u)) = user else {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "no_account" })),
        )
            .into_response();
    };
    let base = state.geo_base.as_deref().filter(|p| p.is_file());
    match state
        .users
        .connections()
        .devices(u.id, courant.map(|c| c.0 .0), base)
        .await
    {
        Ok(appareils) => Json(serde_json::json!({
            "devices": appareils,
            "geolocation": base.is_some(),
            "attribution": base.map(|_| locaryn_storage::geoip::ATTRIBUTION),
        }))
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "liste des appareils impossible");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "devices_unavailable" })),
            )
                .into_response()
        }
    }
}
