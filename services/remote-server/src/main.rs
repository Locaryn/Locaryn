//! Locaryn remote server — a secured TLS gateway that exposes the same API
//! as the local daemon, plus authentication, audit, and (optionally) the
//! enterprise collaboration module.
//!
//! Licensed Apache-2.0 (core). The enterprise module is BSL 1.1 and is
//! compiled in only when the `enterprise` feature is enabled (default).

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "locaryn-remote-server",
    version,
    about = "Locaryn secured remote server"
)]
struct Cli {
    /// Bind address. Default 0.0.0.0:7473 (TLS required).
    #[arg(long, default_value = "0.0.0.0:7473")]
    bind: String,
    /// Path to TLS cert PEM.
    #[arg(long, env = "LOCARYN_TLS_CERT")]
    tls_cert: Option<String>,
    /// Path to TLS key PEM.
    #[arg(long, env = "LOCARYN_TLS_KEY")]
    tls_key: Option<String>,
    /// Disable the enterprise module even if compiled in.
    #[arg(long)]
    no_enterprise: bool,
}

#[derive(Clone)]
struct ServerState {
    enterprise_enabled: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let addr: SocketAddr = cli.bind.parse()?;

    let enterprise_enabled = cfg!(feature = "enterprise") && !cli.no_enterprise;
    if enterprise_enabled {
        tracing::info!("enterprise module enabled (BSL 1.1)");
        #[cfg(feature = "enterprise")]
        {
            let _ = locaryn_enterprise::version_string();
        }
    }

    let state = Arc::new(ServerState { enterprise_enabled });

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/me", get(me))
        .with_state(state);

    if cli.tls_cert.is_none() || cli.tls_key.is_none() {
        tracing::warn!(
            "no TLS cert/key provided — running PLAIN HTTP on {addr}. \
             This is unsafe for anything other than local testing. \
             Use --tls-cert/--tls-key or a reverse proxy (Caddy/Traefik) for production."
        );
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("locaryn-remote-server (plain) listening on http://{addr}");
        axum::serve(listener, app).await?;
    } else {
        // V1.1 wires rustls + axum-server for TLS. Skeleton warns.
        tracing::warn!("TLS wiring (rustls) lands in V1.1; falling back to plain HTTP for now");
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("locaryn-remote-server (tls-pending) listening on http://{addr}");
        axum::serve(listener, app).await?;
    }

    Ok(())
}

async fn health(State(s): State<Arc<ServerState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "enterprise": s.enterprise_enabled,
        "tls": "pending-v1.1",
    }))
}

async fn login(State(_s): State<Arc<ServerState>>, Json(body): Json<LoginBody>) -> Response {
    // V1 wires real auth (Argon2id + token issuance). Skeleton accepts anything.
    let token = locaryn_auth::generate_token();
    let expires = locaryn_auth::default_expiry();
    tracing::info!(user = %body.user, "login issued (skeleton)");
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "token": token,
            "expires_at": expires,
            "user": body.user,
        })),
    )
        .into_response()
}

async fn me(State(_s): State<Arc<ServerState>>) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error": { "code": "unauthorized", "message": "no token (V1 wires bearer auth)" }
        })),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct LoginBody {
    user: String,
    #[serde(default, rename = "password")]
    _password: Option<String>,
}

type Response = axum::response::Response;
