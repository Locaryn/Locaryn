//! `/v1/travel` — turning travel mode on, and getting the code to scan.

use crate::travel::TravelStatus;
use crate::DaemonState;
use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::net::SocketAddr;
use std::sync::Arc;

/// Ces routes sont publiques (sans jeton) pour que l'hôte les consulte avant
/// d'avoir un compte — mais seulement depuis cette machine. Sans cette garde,
/// n'importe quel appareil du réseau ouvrait un tunnel vers l'adresse de son
/// choix, le coupait, ou recevait un lien d'appairage tout neuf.
fn hors_de_cette_machine(pair: SocketAddr) -> Option<Response> {
    (!pair.ip().to_canonical().is_loopback()).then(|| {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": { "code": "local_only", "message": "Le mode voyage se règle depuis la machine qui le sert." } })),
        )
            .into_response()
    })
}

fn data_dir(s: &DaemonState) -> std::path::PathBuf {
    s.data_dir.clone()
}

/// GET /v1/travel — is it on, and what should be shown.
///
/// Calling this mints a fresh link when the tunnel is up, so a screen left
/// open does not end up displaying an expired code.
pub async fn status(
    State(s): State<Arc<DaemonState>>,
    ConnectInfo(pair): ConnectInfo<SocketAddr>,
) -> Response {
    if let Some(refus) = hors_de_cette_machine(pair) {
        return refus;
    }
    Json(s.travel.status(&data_dir(&s)).await).into_response()
}

#[derive(serde::Deserialize)]
pub struct SetBody {
    /// `"cloudflare"`, `"ngrok"`, `"devtunnel"` — or absent to switch off.
    #[serde(default)]
    pub provider: Option<String>,
}

/// POST /v1/travel — switch it on or off.
pub async fn set(
    State(s): State<Arc<DaemonState>>,
    ConnectInfo(pair): ConnectInfo<SocketAddr>,
    Json(body): Json<SetBody>,
) -> Response {
    if let Some(refus) = hors_de_cette_machine(pair) {
        return refus;
    }
    let Some(name) = body.provider.filter(|p| !p.trim().is_empty()) else {
        s.travel.stop().await;
        return Json(TravelStatus::default()).into_response();
    };

    // « ssh:moi@serveur.fr:8443 » : le relais, puis ce qu'il lui faut. Un seul
    // champ plutot que deux, parce que le seul relais qui reclame une cible est
    // aussi le seul dont l'adresse ne s'annonce pas toute seule.
    let (nom_relais, cible_brute) = match name.split_once(':') {
        Some((tete, reste)) => (tete.to_string(), Some(reste.to_string())),
        None => (name.clone(), None),
    };

    let Some(provider) = locaryn_travel::Provider::parse(&nom_relais) else {
        return bad_request(format!(
            "Relais inconnu : « {nom_relais} ». Valeurs possibles : cloudflare, ngrok,              devtunnel, ssh."
        ));
    };

    let cible = if provider.needs_target() {
        match locaryn_travel::SshTarget::parse(cible_brute.as_deref().unwrap_or("")) {
            Ok(t) => Some(t),
            Err(e) => return bad_request(e),
        }
    } else {
        None
    };

    // Authentication is what makes exposing this defensible, and on any
    // address but loopback the daemon already requires it.
    match s
        .travel
        .start(provider, cible.as_ref(), s.port, &data_dir(&s), s.auth_required)
        .await
    {
        Ok(st) => Json(st).into_response(),
        Err(message) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": { "code": "travel_unavailable", "message": message } })),
        )
            .into_response(),
    }
}

/// GET /v1/travel/home — the code that puts a phone back on the local address.
///
/// Available whether or not the tunnel is running: coming home is exactly what
/// someone does after the tunnel has already been switched off.
pub async fn home(
    State(s): State<Arc<DaemonState>>,
    ConnectInfo(pair): ConnectInfo<SocketAddr>,
) -> Response {
    if let Some(refus) = hors_de_cette_machine(pair) {
        return refus;
    }
    match crate::travel::TravelState::home_link(&data_dir(&s), &s.local_url) {
        Ok((link, qr_svg)) => {
            Json(serde_json::json!({ "link": link, "qr_svg": qr_svg })).into_response()
        }
        Err(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": { "code": "travel_home", "message": message } })),
        )
            .into_response(),
    }
}

fn bad_request(message: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": { "code": "bad_request", "message": message } })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::hors_de_cette_machine;

    /// La boucle locale passe, en IPv4 comme en IPv6 ; une adresse du réseau
    /// est refusée.
    #[test]
    fn le_mode_voyage_ne_se_regle_que_depuis_cette_machine() {
        assert!(hors_de_cette_machine("127.0.0.1:5000".parse().unwrap()).is_none());
        assert!(hors_de_cette_machine("[::1]:5000".parse().unwrap()).is_none());
        assert!(hors_de_cette_machine("[::ffff:127.0.0.1]:5000".parse().unwrap()).is_none());
        assert!(hors_de_cette_machine("192.168.1.40:5000".parse().unwrap()).is_some());
    }
}
