//! Ce que Locaryn retient de la personne.
//!
//!   GET    /v1/memory              — toutes les fiches
//!   POST   /v1/memory               — retenir {group, title, detail}
//!   PUT    /v1/memory/{id}/summary  — corriger le résumé d'une fiche
//!   PUT    /v1/memory/{id}/title    — renommer une fiche
//!   DELETE /v1/memory/{id}          — oublier une fiche
//!   DELETE /v1/memory               — tout oublier
//!
//! La mémoire d'un compte n'est lisible que par lui. Sur une installation
//! personnelle il n'y a pas de compte : la mémoire est celle de la machine, et
//! la personne au clavier est déjà la seule concernée.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::DaemonState;

#[derive(Deserialize)]
pub struct RememberBody {
    #[serde(default = "default_group")]
    pub group: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_group() -> String {
    "sujets".to_string()
}

fn default_source() -> String {
    "utilisateur".to_string()
}

#[derive(Deserialize)]
pub struct SummaryBody {
    pub summary: String,
}

#[derive(Deserialize)]
pub struct TitleBody {
    pub title: String,
}

#[derive(Deserialize)]
pub struct DetailsBody {
    pub details: Vec<String>,
}

fn error(status: StatusCode, code: &str, message: String) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": { "code": code, "message": message } })),
    )
        .into_response()
}

/// Le compte concerné, ou `None` sur une installation personnelle.
///
/// L'identité vient de l'authentification, jamais du corps de la requête :
/// sinon n'importe qui lirait la mémoire de n'importe qui en changeant un
/// champ.
fn current_user(_s: &DaemonState) -> Option<String> {
    None
}

pub async fn list(State(s): State<Arc<DaemonState>>) -> Response {
    let user = current_user(&s);
    match s.storage.memory.list(user.as_deref()).await {
        Ok(entries) => (StatusCode::OK, Json(entries)).into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            e.to_string(),
        ),
    }
}

pub async fn remember(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<RememberBody>,
) -> Response {
    let user = current_user(&s);
    match s
        .storage
        .memory
        .remember(
            user.as_deref(),
            &body.group,
            &body.title,
            &body.detail,
            &body.source,
        )
        .await
    {
        Ok(entry) => (StatusCode::CREATED, Json(entry)).into_response(),
        Err(e) => error(StatusCode::UNPROCESSABLE_ENTITY, "bad_entry", e.to_string()),
    }
}

pub async fn set_summary(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<SummaryBody>,
) -> Response {
    match s.storage.memory.set_summary(&id, &body.summary).await {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(e) => error(StatusCode::NOT_FOUND, "not_found", e.to_string()),
    }
}

pub async fn rename(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<TitleBody>,
) -> Response {
    match s.storage.memory.rename(&id, &body.title).await {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(e) => error(StatusCode::NOT_FOUND, "not_found", e.to_string()),
    }
}

pub async fn set_details(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<DetailsBody>,
) -> Response {
    match s.storage.memory.set_details(&id, &body.details).await {
        Ok(entry) => (StatusCode::OK, Json(entry)).into_response(),
        Err(e) => error(StatusCode::NOT_FOUND, "not_found", e.to_string()),
    }
}

pub async fn forget(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    match s.storage.memory.forget(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error(StatusCode::NOT_FOUND, "not_found", e.to_string()),
    }
}

pub async fn forget_all(State(s): State<Arc<DaemonState>>) -> Response {
    let user = current_user(&s);
    match s.storage.memory.forget_all(user.as_deref()).await {
        Ok(n) => (StatusCode::OK, Json(serde_json::json!({ "oubliees": n }))).into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            e.to_string(),
        ),
    }
}
