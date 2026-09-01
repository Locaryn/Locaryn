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
/// Health is needed for discovery; login issues the first token; pairing is a public onboarding payload without credentials.
///
/// Paths a browser may reach without a token. The web client itself (served
/// by the daemon) holds no secret: it is static assets plus a login form, so
/// it must stay reachable before authentication on an exposed server.
fn is_public(path: &str) -> bool {
    path == "/health"
        || path == "/v1/info"
        || path == "/v1/auth/login"
        // Le QR ne contient aucun jeton : il porte seulement l'adresse et
        // l'autorité publique du déploiement. Il doit donc être demandable par
        // le desktop lui-même avant qu'il ait pu obtenir un jeton utilisateur.
        || path == "/v1/pairing"
        // Le confirm d'appairage EST le second facteur : il porte un code
        // affiché à l'écran de l'hôte, pas un secret stocké. Il doit donc
        // être joignable par un appareil qui n'a pas encore de token.
        || path == "/v1/auth/pair/confirm"
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
pub struct CreateApiTokenBody {
    /// Le nom qui permettra de reconnaître cette clé dans la liste.
    #[serde(default)]
    pub label: Option<String>,
    /// 7, 30 ou 90 jours. Absent ou 0 : la clé n'expire jamais.
    #[serde(default)]
    pub expires_in_days: Option<i64>,
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
    match state
        .users
        .change_password(user_id, &body.current, &body.nouveau)
        .await
    {
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

/// ---- Circuit A (clés API) et Circuit B (sessions appareils) ---------------
///
/// Deux listes distinctes pour deux circuits distincts : l'écran « Clés API »
/// ne montre que ce que l'utilisateur a frappé à la main, l'écran « Appareils
/// connectés » que ce qui s'est connecté tout seul. Le plaintext d'une clé
/// n'existe qu'au moment de sa création ; après, il n'y a que le hint.
fn token_info_json(t: &locaryn_storage::users::TokenInfo) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "kind": t.kind.as_str(),
        "label": t.label,
        "hint": t.hint,
        "created_at": t.created_at,
        "last_used_at": t.last_used_at,
        "expires_at": t.expires_at,
        "revoked_at": t.revoked_at,
    })
}

/// GET /v1/auth/tokens — les deux circuits de l'appelant, séparés par kind.
pub async fn list_tokens(
    State(state): State<Arc<AuthState>>,
    user: Option<axum::extract::Extension<User>>,
) -> Response {
    let Some(axum::extract::Extension(u)) = user else {
        // Loopback sans compte : rien à lister, et le dire proprement.
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "no_account" })),
        )
            .into_response();
    };
    match state.users.list_tokens(u.id).await {
        Ok(tokens) => {
            let out: Vec<serde_json::Value> = tokens.iter().map(token_info_json).collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "liste des jetons impossible");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "tokens_unavailable" })),
            )
                .into_response()
        }
    }
}

/// POST /v1/auth/tokens — crée une clé développeur. Le plaintext part une
/// seule fois, dans cette réponse ; le serveur n'en garde que le hash.
pub async fn create_api_token(
    State(state): State<Arc<AuthState>>,
    user: Option<axum::extract::Extension<User>>,
    Json(body): Json<CreateApiTokenBody>,
) -> Response {
    let Some(axum::extract::Extension(u)) = user else {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "no_account" })),
        )
            .into_response();
    };
    let label = body.label.as_deref().filter(|l| !l.trim().is_empty());
    let expires_days = body.expires_in_days.filter(|d| *d > 0);
    match state.users.issue_api_token(u.id, label, expires_days).await {
        Ok(tok) => {
            tracing::info!(user = %u.username, "clé API créée");
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "id": tok.id,
                    "token": tok.plaintext,
                    "expires_at": tok.expires_at,
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "création de la clé impossible");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "token_issue_failed" })),
            )
                .into_response()
        }
    }
}

/// DELETE /v1/auth/tokens/:id — révoque une clé ou déconnecte un appareil.
pub async fn revoke_token(
    State(state): State<Arc<AuthState>>,
    user: Option<axum::extract::Extension<User>>,
    axum::extract::Path(token_id): axum::extract::Path<String>,
) -> Response {
    let Some(axum::extract::Extension(u)) = user else {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "no_account" })),
        )
            .into_response();
    };
    let Ok(token_id) = uuid::Uuid::parse_str(&token_id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_token_id" })),
        )
            .into_response();
    };
    match state.users.revoke_token(token_id).await {
        Ok(()) => {
            tracing::info!(user = %u.username, "jeton révoqué");
            (StatusCode::OK, Json(serde_json::json!({ "revoked": true }))).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "révocation impossible");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "revoke_failed" })),
            )
                .into_response()
        }
    }
}

/// Liste les comptes utilisateurs existants (administrateur requis si auth active).
pub async fn list_users(
    State(state): State<Arc<AuthState>>,
    user: Option<axum::extract::Extension<User>>,
) -> Response {
    if state.required && !user.as_ref().is_some_and(|u| u.0.role == Role::Admin) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "admin_required" })),
        )
            .into_response();
    }
    match state.users.list().await {
        Ok(users) => {
            let out: Vec<serde_json::Value> = users
                .into_iter()
                .map(|u| {
                    serde_json::json!({
                        "id": u.id,
                        "username": u.username,
                        "role": if u.role == Role::Admin { "admin" } else { "member" },
                        "disabled": u.disabled,
                    })
                })
                .collect();
            (StatusCode::OK, Json(out)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateUserBody {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub is_admin: bool,
}

/// Créer un nouvel identifiant / compte utilisateur.
pub async fn create_user(
    State(state): State<Arc<AuthState>>,
    user: Option<axum::extract::Extension<User>>,
    Json(body): Json<CreateUserBody>,
) -> Response {
    let count = state.users.count().await.unwrap_or(0);
    // Si aucun compte n'existe, on autorise le premier compte admin initial (bootstrap)
    if state.required && count > 0 && !user.as_ref().is_some_and(|u| u.0.role == Role::Admin) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "admin_required" })),
        )
            .into_response();
    }
    let role = if body.is_admin || count == 0 {
        Role::Admin
    } else {
        Role::Member
    };
    match state
        .users
        .create(&body.username, &body.password, role)
        .await
    {
        Ok(u) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": u.id,
                "username": u.username,
                "role": if u.role == Role::Admin { "admin" } else { "member" },
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Supprimer un compte utilisateur.
pub async fn delete_user(
    State(state): State<Arc<AuthState>>,
    user: Option<axum::extract::Extension<User>>,
    axum::extract::Path(user_id_str): axum::extract::Path<String>,
) -> Response {
    if state.required && !user.as_ref().is_some_and(|u| u.0.role == Role::Admin) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "admin_required" })),
        )
            .into_response();
    }
    let Ok(user_id) = uuid::Uuid::parse_str(&user_id_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_user_id" })),
        )
            .into_response();
    };
    match state.users.delete(user_id).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "deleted": true }))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "user_not_found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
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
    fn only_discovery_login_and_pairing_are_public() {
        assert!(is_public("/health"));
        assert!(is_public("/v1/info"));
        assert!(is_public("/v1/auth/login"));
        assert!(is_public("/v1/pairing"));
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
