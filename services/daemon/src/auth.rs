//! Who is allowed to talk to this daemon.
//!
//! Authentication is tied to the listening address rather than to a setting.
//! On loopback the only caller is the person at the keyboard, and demanding a
//! token there would be friction with no benefit. The moment the daemon is
//! reachable from the network, every request must carry one — a server exposed
//! without protection should not be reachable through a forgotten checkbox.

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use locaryn_storage::users::{Role, User, UserRepo};
use serde::Deserialize;
use std::sync::Arc;

/// Endpoints reachable without a token.
///
/// Health is needed for discovery and reveals nothing; login is how a caller
/// obtains a token in the first place.
/// Paths a browser may reach without a token. The web client itself (served
/// by the daemon) holds no secret: it is static assets plus a login form, so
/// it must stay reachable before authentication on an exposed server.
fn is_public(path: &str) -> bool {
    path == "/health"
        || path == "/v1/auth/login"
        || path == "/"
        || path == "/index.html"
        || path == "/manifest.webmanifest"
        || path == "/sw.js"
        || path.starts_with("/assets/")
        || path.starts_with("/icons/")
        || path.starts_with("/fonts/")
}

#[derive(Clone)]
pub struct AuthState {
    pub users: UserRepo,
    /// False on loopback: the local user is already trusted by the OS.
    pub required: bool,
}

/// Extract a bearer token from the `Authorization` header.
fn bearer(req: &Request<Body>) -> Option<String> {
    let raw = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let rest = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?;
    let t = rest.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn unauthorised(detail: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "unauthorized", "detail": detail })),
    )
        .into_response()
}

/// Reject anything without a valid token once authentication is required.
pub async fn require_token(
    State(state): State<Arc<AuthState>>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    if !state.required || is_public(req.uri().path()) {
        return next.run(req).await;
    }

    let Some(token) = bearer(&req) else {
        return unauthorised(
            "Jeton manquant. Authentifiez-vous sur /v1/auth/login, puis envoyez \
             l'en-tête « Authorization: Bearer <jeton> ».",
        );
    };

    match state.users.user_for_token(&token).await {
        Ok(Some(user)) => {
            // Downstream handlers can scope work to the caller — which is what
            // will keep one user's distributed tasks off another user's machines.
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        Ok(None) => unauthorised("Jeton invalide, expiré ou révoqué."),
        Err(e) => {
            // A storage failure must not open the door.
            tracing::error!(error = %e, "vérification du jeton impossible");
            unauthorised("Vérification impossible.")
        }
    }
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
    /// Optional label to recognise this session later ("portable", "téléphone").
    #[serde(default)]
    pub label: Option<String>,
}

/// Exchange a username and password for a token.
///
/// The reply is identical for a wrong password and an unknown account: telling
/// them apart would let someone enumerate who has an account here.
pub async fn login(State(state): State<Arc<AuthState>>, Json(body): Json<LoginBody>) -> Response {
    match state
        .users
        .authenticate(&body.username, &body.password)
        .await
    {
        Ok(Some(user)) => match state
            .users
            .issue_token(user.id, body.label.as_deref(), 30)
            .await
        {
            Ok(tok) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "token": tok.plaintext,
                    "expires_at": tok.expires_at,
                    "user": {
                        "id": user.id,
                        "username": user.username,
                        "role": if user.role == Role::Admin { "admin" } else { "member" },
                    }
                })),
            )
                .into_response(),
            Err(e) => {
                tracing::error!(error = %e, "émission du jeton impossible");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "token_issue_failed" })),
                )
                    .into_response()
            }
        },
        Ok(None) => {
            tracing::info!(user = %body.username, "échec d'authentification");
            unauthorised("Identifiants incorrects.")
        }
        Err(e) => {
            tracing::error!(error = %e, "authentification impossible");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "auth_unavailable" })),
            )
                .into_response()
        }
    }
}

/// Who the current token belongs to.
pub async fn me(user: Option<axum::extract::Extension<User>>) -> Response {
    match user {
        Some(axum::extract::Extension(u)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": u.id,
                "username": u.username,
                "role": if u.role == Role::Admin { "admin" } else { "member" },
            })),
        )
            .into_response(),
        // Only reachable on loopback, where no token is demanded.
        None => (
            StatusCode::OK,
            Json(serde_json::json!({ "username": "local", "role": "admin", "local": true })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct ChangePasswordBody {
    pub current: String,
    pub nouveau: String,
}

/// Change the caller's own password.
///
/// The caller must know the current password: on a loopback-only daemon there
/// is no token, and the request arrives from the web client itself — the
/// person at the keyboard. A wrong current password answers 403, indistinct
/// from a rejected token, so an attacker cannot tell the two apart.
pub async fn change_password(
    State(state): State<Arc<AuthState>>,
    user: Option<axum::extract::Extension<User>>,
    Json(body): Json<ChangePasswordBody>,
) -> Response {
    let user_id = match user {
        Some(axum::extract::Extension(u)) => u.id,
        None => {
            // Loopback without auth: there is no account to change.
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "no_account" })),
            )
                .into_response();
        }
    };
    match state.users.change_password(user_id, &body.current, &body.nouveau).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "changed": true }))).into_response(),
        Ok(false) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "current_password_wrong" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "password_refused", "detail": e.to_string() })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use locaryn_storage::users::Role;

    #[tokio::test]
    async fn password_changes_only_for_the_caller_with_the_right_current_password() {
        let pool = locaryn_storage::open_in_memory().await.unwrap();
        let repo = UserRepo::new(pool);
        let user = repo
            .create("Marie", "un-mot-de-passe-solide", Role::Admin)
            .await
            .unwrap();
        let state = Arc::new(AuthState {
            users: repo.clone(),
            required: true,
        });

        // Mauvais mot de passe actuel : 403, et le compte n'a pas bougé.
        let resp = change_password(
            State(state.clone()),
            Some(axum::extract::Extension(user.clone())),
            Json(ChangePasswordBody {
                current: "mauvais".into(),
                nouveau: "tout-neuf-et-solide".into(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(repo
            .authenticate("Marie", "un-mot-de-passe-solide")
            .await
            .unwrap()
            .is_some());

        // Bon mot de passe : 200, et le nouveau prend effet.
        let resp = change_password(
            State(state),
            Some(axum::extract::Extension(user.clone())),
            Json(ChangePasswordBody {
                current: "un-mot-de-passe-solide".into(),
                nouveau: "tout-neuf-et-solide".into(),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(repo
            .authenticate("Marie", "tout-neuf-et-solide")
            .await
            .unwrap()
            .is_some());
    }

    #[test]
    fn only_health_and_login_are_public() {
        assert!(is_public("/health"));
        assert!(is_public("/v1/auth/login"));
        for guarded in [
            "/v1/projects",
            "/v1/sessions",
            "/v1/auth/me",
            "/v1/providers",
            "/v1/extensions",
            // A near-miss must not slip through a prefix check.
            "/v1/auth/login/../projects",
            "/health/../v1/projects",
        ] {
            assert!(!is_public(guarded), "{guarded} ne doit pas être public");
        }
    }

    #[test]
    fn bearer_is_read_only_from_a_well_formed_header() {
        let with = |v: &str| {
            let mut r = Request::new(Body::empty());
            r.headers_mut()
                .insert(axum::http::header::AUTHORIZATION, v.parse().unwrap());
            r
        };
        assert_eq!(bearer(&with("Bearer abc123")).as_deref(), Some("abc123"));
        // Some clients lowercase the scheme.
        assert_eq!(bearer(&with("bearer abc123")).as_deref(), Some("abc123"));
        assert_eq!(
            bearer(&with("Bearer   spaced  ")).as_deref(),
            Some("spaced")
        );

        assert!(
            bearer(&with("Bearer ")).is_none(),
            "un jeton vide n'est pas un jeton"
        );
        assert!(bearer(&with("Basic abc123")).is_none(), "mauvais schéma");
        assert!(bearer(&with("abc123")).is_none(), "sans schéma");
        assert!(bearer(&Request::new(Body::empty())).is_none());
    }
}
