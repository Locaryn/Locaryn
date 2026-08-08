//! `/v1/travel` — turning travel mode on, and getting the code to scan.

use crate::travel::TravelStatus;
use crate::DaemonState;
use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use std::sync::Arc;

fn data_dir(s: &DaemonState) -> std::path::PathBuf {
    s.data_dir.clone()
}

/// GET /v1/travel — is it on, and what should be shown.
///
/// Calling this mints a fresh link when the tunnel is up, so a screen left
/// open does not end up displaying an expired code.
pub async fn status(State(s): State<Arc<DaemonState>>) -> Response {
    Json(s.travel.status(&data_dir(&s)).await).into_response()
}

#[derive(serde::Deserialize)]
pub struct SetBody {
    /// `"cloudflare"`, `"ngrok"`, `"devtunnel"` — or absent to switch off.
    #[serde(default)]
    pub provider: Option<String>,
}

/// POST /v1/travel — switch it on or off.
pub async fn set(State(s): State<Arc<DaemonState>>, Json(body): Json<SetBody>) -> Response {
    let Some(name) = body.provider.filter(|p| !p.trim().is_empty()) else {
        s.travel.stop().await;
        return Json(TravelStatus::default()).into_response();
    };

    let Some(provider) = lochor_travel::Provider::parse(&name) else {
        return bad_request(format!(
            "Relais inconnu : « {name} ». Valeurs possibles : cloudflare, ngrok, devtunnel."
        ));
    };

    // Authentication is what makes exposing this defensible, and on any
    // address but loopback the daemon already requires it.
    match s
        .travel
        .start(provider, s.port, &data_dir(&s), s.auth_required)
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
pub async fn home(State(s): State<Arc<DaemonState>>) -> Response {
    match crate::travel::TravelState::home_link(&data_dir(&s), &s.local_url) {
        Ok((link, qr_svg)) => Json(serde_json::json!({ "link": link, "qr_svg": qr_svg })).into_response(),
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
