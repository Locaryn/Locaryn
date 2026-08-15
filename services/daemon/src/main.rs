//! Locaryn local daemon — loopback HTTP/SSE API on 127.0.0.1:7474.
//!
//! Both the CLI and the desktop app (when not using the in-process core)
//! talk to this daemon. The same crate set powers the remote-server.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{delete, get, post},
    Json, Router,
};
use futures::StreamExt as _;
use locaryn_agent_runtime::{Agent, AgentInput, EventStream, OpenAiCompatAgent, StubAgent};
use locaryn_events::{sse_event_tag, StreamEvent};
use locaryn_extensions::ExtensionRegistry;
use locaryn_mcp::McpState;
use locaryn_provider_supervisor::{Supervisor, SupervisorConfig};
use locaryn_shared_types::{
    ArtifactKind, ConnectionMode, Health, MessageRole, ProviderEngine, ProviderSummary, TaskStatus,
    ToolCall,
};
use locaryn_storage::Storage;
mod routes;

use std::collections::HashMap;
mod auth;
mod media;
mod mtls;
mod port_forward;
mod tls;
mod travel;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

#[derive(Clone)]
struct DaemonState {
    pub mode: ConnectionMode,
    #[allow(dead_code)]
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub data_dir: PathBuf,
    pub storage: Storage,
    pub supervisor: Supervisor,
    /// In-memory extension registry. Wired to the daemon routes so that
    /// GET/POST /v1/extensions/* work immediately. V1.1 will persist to
    /// SQLite (the registry already has the skeleton for it).
    pub extensions: Arc<ExtensionRegistry>,
    /// MCP server registry + running clients. Arc so DaemonState stays Clone.
    pub mcp_state: Arc<McpState>,
    pub travel: Arc<travel::TravelState>,
    /// Port actually being served, so a pairing link names the right one.
    pub port: u16,
    /// Whether requests must carry a token. Travel mode refuses without it.
    pub auth_required: bool,
    /// Address to come back to when travel mode is switched off.
    pub local_url: String,
    /// Per-session cancellation flags. `send_message` inserts an `AtomicBool`
    /// when streaming begins; `POST /v1/sessions/{id}/cancel` sets it to `true`,
    /// causing the SSE stream to terminate on the next poll.
    /// The background task removes the entry when the stream ends naturally.
    cancel_map: Arc<Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
}

/// This machine's address on the local network, for certificate names.
fn local_ip_string() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = locaryn_config::load(None)?;
    init_tracing(&cfg);

    let bind = cfg.daemon.bind.clone();
    let port = cfg.daemon.port;
    let addr: SocketAddr = format!("{bind}:{port}").parse()?;

    // Exposure decides the security posture. On loopback the caller is the
    // person at the keyboard and the OS already vouches for them; anywhere else
    // every request must carry a token. Tying this to the address rather than a
    // setting means a server cannot end up reachable and unprotected because a
    // checkbox was missed.
    let exposed = !addr.ip().is_loopback();
    if exposed {
        tracing::info!(
            "le daemon écoute sur {} — authentification obligatoire et trafic chiffré",
            addr.ip()
        );
    }

    // Resolve the data directory and open the SQLite database.
    let data_dir = cfg
        .daemon
        .data_dir
        .clone()
        .unwrap_or_else(locaryn_config::default_data_dir);
    let data_dir_for_tls = data_dir.clone();
    // The self-hosted web client lives under {data_dir}/web; the daemon serves
    // it on the same origin as the API (no CORS, no mixed content). When the
    // folder is absent the fallback still 404s cleanly.
    let web_dir = data_dir.join("web");
    let db_path = data_dir.join("locaryn.db");
    tracing::info!(?db_path, "opening storage");
    let pool = locaryn_storage::open(&db_path).await?;
    // Kept before the pool is handed to Storage: accounts live in the same
    // database but are managed through their own repository.
    let users = locaryn_storage::users::UserRepo::new(pool.clone());
    let storage = Storage::new(pool);

    // Seed a default local Ollama provider if none exists yet so the daemon
    // is usable out of the box.
    seed_default_provider(&storage).await;

    // Create the provider supervisor and start its background healthcheck /
    // idle-shutdown loop. The supervisor auto-spawns `ollama serve` when the
    // daemon needs it and shuts it down after 30 min of inactivity.
    let supervisor = Supervisor::new(SupervisorConfig::default(), storage.clone());
    let _hc_handle = supervisor.spawn_healthcheck_loop();
    tracing::info!("provider supervisor started (healthcheck + idle-shutdown loop)");

    let extensions = ExtensionRegistry::new();
    let mcp_state = Arc::new(McpState::new());
    let travel_state = travel::TravelState::new();

    let state = Arc::new(DaemonState {
        mode: cfg.connection.mode,
        start_time: chrono::Utc::now(),
        data_dir,
        storage,
        supervisor,
        extensions,
        mcp_state,
        travel: travel_state.clone(),
        port,
        auth_required: exposed,
        local_url: format!(
            "{}://{}:{port}",
            if exposed { "https" } else { "http" },
            local_ip_string()
        ),
        cancel_map: Arc::new(Mutex::new(HashMap::new())),
    });

    if exposed && users.count().await.unwrap_or(0) == 0 {
        anyhow::bail!(
            "Le daemon est configuré pour écouter sur {} mais aucun compte n'existe.\n\
             Un serveur accessible sans compte serait ouvert à tous, donc il ne démarre pas.\n\
             Créez un administrateur avec `locaryn users add <nom> --admin`, \
             ou revenez à `bind = \"127.0.0.1\"` pour un usage local.",
            addr.ip()
        );
    }
    let auth_state = std::sync::Arc::new(auth::AuthState {
        users: users.clone(),
        required: exposed,
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/info", get(info))
        .route("/v1/projects", get(list_projects).post(create_project))
        .route(
            "/v1/projects/:pid/sessions",
            get(list_sessions).post(create_session),
        )
        .route("/v1/sessions/:id", get(get_session))
        .route(
            "/v1/sessions/:id/messages",
            get(list_messages).post(send_message),
        )
        .route("/v1/sessions/:id/cancel", post(cancel_session))
        .route(
            "/v1/sessions/:id/artifacts",
            get(list_artifacts).post(create_artifact),
        )
        .route("/v1/tasks/:id", get(get_task))
        .route("/v1/tasks/:id/cancel", post(cancel_task))
        .route("/v1/tasks/:id/approve", post(approve_task))
        .route("/v1/artifacts/:id", get(get_artifact))
        .route("/v1/artifacts/:id/raw", get(get_artifact_raw))
        // Extension routes
        .route("/v1/extensions", get(routes::extensions::list_extensions))
        .route(
            "/v1/extensions/install",
            post(routes::extensions::install_extension),
        )
        .route(
            "/v1/extensions/reload",
            post(routes::extensions::reload_extensions),
        )
        .route(
            "/v1/extensions/:name/enable",
            post(routes::extensions::enable_extension),
        )
        .route(
            "/v1/extensions/:name/disable",
            post(routes::extensions::disable_extension),
        )
        .route(
            "/v1/extensions/:name",
            delete(routes::extensions::remove_extension),
        )
        .route(
            "/v1/extensions/:name/permissions",
            get(routes::extensions::get_extension_permissions)
                .post(routes::extensions::set_extension_permission),
        )
        // MCP routes
        .route(
            "/v1/travel",
            get(routes::travel::status).post(routes::travel::set),
        )
        .route("/v1/travel/home", get(routes::travel::home))
        .route(
            "/v1/mcp/servers",
            get(routes::mcp::list_servers).post(routes::mcp::register_server),
        )
        .route(
            "/v1/mcp/servers/:name",
            delete(routes::mcp::unregister_server),
        )
        .route(
            "/v1/mcp/servers/:name/start",
            post(routes::mcp::start_server),
        )
        .route("/v1/mcp/servers/:name/stop", post(routes::mcp::stop_server))
        .route(
            "/v1/mcp/servers/:name/discover",
            get(routes::mcp::discover_server),
        )
        .route(
            "/v1/mcp/servers/:name/tools/:tool",
            post(routes::mcp::invoke_tool),
        )
        // Media generation — exposed so thin clients (the phone) can use the
        // engines that only run where the models live.
        .route("/v1/media/models", get(media::list_models))
        .route("/v1/media/image", post(media::generate_image))
        .route("/v1/media/audio", post(media::generate_audio))
        .route("/v1/providers", get(list_providers))
        .route("/v1/supervisor/status", get(supervisor_status))
        .route("/v1/supervisor/start", post(supervisor_start))
        .route("/v1/supervisor/stop", post(supervisor_stop))
        .with_state(state)
        // Auth endpoints carry their own state, so they are built separately
        // and merged in.
        .merge(
            Router::new()
                .route("/v1/auth/login", post(auth::login))
                .route("/v1/auth/me", get(auth::me))
                .with_state(auth_state.clone()),
        )
        // Applied last so it wraps every route above, including ones added later.
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth::require_token,
        ))
        // Unknown API paths keep answering JSON — never the SPA — while every
        // other path falls back to the web client (single-page application:
        // an unknown route serves index.html, and the page decides).
        .route("/v1/*rest", get(api_not_found))
        // `fallback` (not `not_found_service`, which would force a 404) serves
        // index.html with its own 200 status: that is what makes the single-
        // page app work on any route.
        .fallback_service(
            ServeDir::new(&web_dir).fallback(ServeFile::new(web_dir.join("index.html"))),
        );

    // Loopback stays plain HTTP: the traffic never leaves the machine, and a
    // certificate there would only add a warning for no gain. Exposed, the
    // bearer tokens on every request make encryption mandatory.
    if exposed {
        // rustls 0.23 refuses to pick a cipher backend for you. Selecting it
        // explicitly avoids a failure that only appears the first time someone
        // actually exposes the daemon.
        rustls::crypto::ring::default_provider()
            .install_default()
            .map_err(|_| anyhow::anyhow!("initialisation du fournisseur TLS"))?;
        let files = tls::resolve(
            &data_dir_for_tls,
            cfg.daemon.tls_cert.as_deref(),
            cfg.daemon.tls_key.as_deref(),
            &bind,
        )?;
        let config = if cfg.daemon.require_client_cert {
            // Every client must present a certificate this server issued. The
            // handshake fails without one, so an unauthorised caller never
            // reaches the application at all — which is what makes forwarding
            // a port to the internet defensible.
            let ca = mtls::authority(&data_dir_for_tls)?;
            // Serve a certificate issued by the same authority, so a client
            // that trusts the CA validates the server too — with a self-signed
            // one it would be rejected despite trusting us.
            let names = vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                local_ip_string(),
            ];
            let (srv_cert, srv_key) =
                locaryn_config::mtls::ensure_server_cert(&data_dir_for_tls, names)?;
            let cfg_rustls =
                mtls::server_config_requiring_clients(&srv_cert, &srv_key, &ca.cert_pem)?;
            tracing::info!("mTLS actif : un certificat client signé par cette machine est exigé");
            axum_server::tls_rustls::RustlsConfig::from_config(std::sync::Arc::new(cfg_rustls))
        } else {
            axum_server::tls_rustls::RustlsConfig::from_pem_file(&files.cert, &files.key)
                .await
                .map_err(|e| anyhow::anyhow!("chargement du certificat TLS : {e}"))?
        };
        tracing::info!(
            "locaryn-daemon à l écoute sur https://{addr} ({})",
            if files.self_signed {
                "certificat auto-signé"
            } else {
                "certificat fourni"
            }
        );
        // Travel mode, if it was asked for. Started before serving so the
        // code is printed with the rest of the startup rather than minutes
        // later; the relay tolerates an origin that is not answering yet.
        if let Some(name) = cfg.daemon.travel.clone() {
            match locaryn_travel::Provider::parse(&name) {
                Some(p) => {
                    match travel_state.start(p, port, &data_dir_for_tls, true).await {
                        Ok(st) => {
                            if let Some(uri) = st.link.as_deref() {
                                travel::announce(uri, p);
                            }
                        }
                        // Not fatal: the server is still useful on the local
                        // network, and saying so beats refusing to start.
                        Err(e) => tracing::warn!("mode voyage indisponible — {e}"),
                    }
                }
                None => {
                    let msg = format!(
                        "Relais inconnu : « {name} ». Valeurs possibles : cloudflare, ngrok, devtunnel."
                    );
                    tracing::warn!("{msg}");
                    travel_state.record_blocker(msg).await;
                }
            }
        }

        // Requested before serving so the address can be logged with the rest
        // of the startup, and refused loudly rather than silently skipped.
        let mut renewal = None;
        if cfg.daemon.open_router_port {
            match port_forward::open(port, cfg.daemon.require_client_cert).await {
                Ok(m) => {
                    tracing::info!(
                        "joignable depuis Internet sur https://{}:{}",
                        m.external_ip,
                        m.external_port
                    );
                    renewal = Some(port_forward::spawn_renewal(port));
                }
                // Not fatal: the server is still useful on the local network.
                Err(e) => tracing::warn!("{e}"),
            }
        }

        axum_server::bind_rustls(addr, config)
            .serve(app.into_make_service())
            .await?;

        travel_state.stop().await;
        if let Some(h) = renewal {
            h.abort();
            port_forward::close(port).await;
        }
    } else {
        let listener = TcpListener::bind(addr).await?;
        if cfg.daemon.require_client_cert {
            // Otherwise the setting reads as active while nothing enforces it:
            // on loopback there is no TLS handshake to present a certificate
            // in, so no certificate is ever asked for.
            tracing::warn!(
                "require_client_cert est activé mais sans effet sur une adresse locale : \
                 aucun échange TLS n'a lieu ici. Écoutez sur 0.0.0.0 (ou l'adresse de la \
                 machine) pour que les certificats clients soient réellement exigés."
            );
        }
        tracing::info!("locaryn-daemon à l écoute sur http://{addr} (local uniquement)");
        axum::serve(listener, app).await?;
    }
    Ok(())
}

fn init_tracing(cfg: &locaryn_config::Config) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.logging.level));
    let sub = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false);
    if cfg.logging.json {
        sub.json().init();
    } else {
        sub.init();
    }
}

/// Ensure a default local llama-server provider exists (idempotent).
async fn seed_default_provider(storage: &Storage) {
    let existing = storage.providers.list().await;
    if existing.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
        tracing::info!("seeding default local llama-server provider");
        if let Err(e) = storage
            .providers
            .upsert_local(ProviderEngine::LlamaCpp, "http://127.0.0.1:8080", None)
            .await
        {
            tracing::warn!(error = %e, "failed to seed default provider");
        }
    }
}

/// JSON 404 for unknown `/v1/*` paths, so an API consumer never receives the
/// web client's `index.html` as a reply to a typo.
async fn api_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "route inconnue" })),
    )
        .into_response()
}

async fn health(State(s): State<Arc<DaemonState>>) -> Json<Health> {
    let active = s.storage.providers.active().await.ok().flatten();
    let provider_summary = active.as_ref().map(|p| ProviderSummary {
        kind: p.kind,
        engine: p.engine,
        endpoint: p.endpoint.clone(),
        model: p.model.clone(),
    });
    Json(Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        mode: s.mode,
        active_provider: provider_summary,
    })
}

async fn info(State(_s): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "locaryn-daemon",
        "version": env!("CARGO_PKG_VERSION"),
        "capabilities": ["sessions", "projects", "tasks", "artifacts", "extensions", "mcp", "media"],
    }))
}

async fn list_projects(State(s): State<Arc<DaemonState>>) -> Response {
    match s.storage.projects.list().await {
        Ok(projects) => (StatusCode::OK, Json(projects)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

async fn create_project(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<CreateProjectBody>,
) -> Response {
    match s
        .storage
        .projects
        .create(&body.path, &body.name, body.trust_level)
        .await
    {
        Ok(p) => (StatusCode::CREATED, Json(p)).into_response(),
        Err(e) => {
            let (code, status) = match &e {
                locaryn_storage::StorageError::Conflict(_) => ("conflict", StatusCode::CONFLICT),
                _ => ("storage_error", StatusCode::INTERNAL_SERVER_ERROR),
            };
            (
                status,
                Json(serde_json::json!({
                    "error": { "code": code, "message": e.to_string() }
                })),
            )
                .into_response()
        }
    }
}

#[derive(serde::Deserialize)]
struct CreateProjectBody {
    path: String,
    name: String,
    #[serde(default)]
    trust_level: locaryn_shared_types::TrustLevel,
}

async fn list_sessions(State(s): State<Arc<DaemonState>>, Path(pid): Path<String>) -> Response {
    let project_id = match Uuid::parse_str(&pid) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid project id" }
                })),
            )
                .into_response();
        }
    };
    match s.storage.sessions.list_for_project(project_id).await {
        Ok(sessions) => (StatusCode::OK, Json(sessions)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

async fn create_session(State(s): State<Arc<DaemonState>>, Path(pid): Path<String>) -> Response {
    let project_id = match Uuid::parse_str(&pid) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid project id" }
                })),
            )
                .into_response();
        }
    };
    // Verify the project exists.
    if let Err(e) = s.storage.projects.get(project_id).await {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": e.to_string() }
            })),
        )
            .into_response();
    }
    let title = None;
    match s.storage.sessions.create(project_id, title).await {
        Ok(session) => (StatusCode::CREATED, Json(session)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

async fn get_session(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let session_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid session id" }
                })),
            )
                .into_response();
        }
    };
    match s.storage.sessions.get(session_id).await {
        Ok(session) => (StatusCode::OK, Json(session)).into_response(),
        Err(e) => {
            let status = match &e {
                locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(serde_json::json!({
                    "error": { "code": "not_found", "message": e.to_string() }
                })),
            )
                .into_response()
        }
    }
}

/// POST /v1/sessions/{id}/cancel — cancel a running message stream.
///
/// Sets the session's cancellation flag (`AtomicBool`), which the SSE
/// stream in `send_message` checks on each poll via `take_while`. The
/// stream terminates, the background task persists what it collected,
/// and the HTTP response to the client ends.
/// Also finds any pending/running tasks for this session and cancels
/// them in storage so the status is consistent.
async fn cancel_session(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let session_uuid = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid session id" }
                })),
            )
                .into_response();
        }
    };

    // Fire the cancellation flag (if a stream is active).
    let found = s.cancel_map.lock().unwrap().get(&session_uuid).cloned();
    match found {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            tracing::info!(session_id = %session_uuid, "cancellation requested");
        }
        None => {
            tracing::info!(session_id = %session_uuid, "cancel called but no active stream");
        }
    }

    // Cancel any pending/running tasks for this session in storage.
    if let Ok(tasks) = s.storage.tasks.list_for_session(session_uuid).await {
        for task in tasks {
            if matches!(
                task.status,
                TaskStatus::Pending | TaskStatus::Running | TaskStatus::AwaitingApproval
            ) {
                if let Err(e) = s
                    .storage
                    .tasks
                    .update_status(task.id, TaskStatus::Cancelled)
                    .await
                {
                    tracing::warn!(task_id = %task.id, error = %e, "failed to cancel task");
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "cancelled" })),
    )
        .into_response()
}

/// List all messages for a session, including assistant messages with their
/// tool_calls and any tool result messages.
async fn list_messages(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let session_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid session id" }
                })),
            )
                .into_response();
        }
    };
    match s.storage.messages.list_for_session(session_id).await {
        Ok(messages) => (StatusCode::OK, Json(messages)).into_response(),
        Err(e) => {
            let status = match &e {
                locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(serde_json::json!({
                    "error": { "code": "storage_error", "message": e.to_string() }
                })),
            )
                .into_response()
        }
    }
}

/// Send a message and stream the agent's reply as SSE.
async fn send_message(
    State(s): State<Arc<DaemonState>>,
    Path(session_id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>> + Send> {
    let session_uuid = Uuid::parse_str(&session_id).unwrap_or(Uuid::nil());

    // 1. Persist the user's message to storage (best-effort).
    if let Err(e) = s
        .storage
        .messages
        .append(session_uuid, MessageRole::User, &body.content)
        .await
    {
        tracing::warn!(error = %e, "failed to persist user message");
    }

    // 2. Look up the session → project to get trust level and project path.
    let (project_id, project_path, trust) = match s.storage.sessions.get(session_uuid).await {
        Ok(session) => {
            match s.storage.projects.get(session.project_id).await {
                // The tool loop only runs when all three are present, so a
                // project whose path is not a real directory — the internal
                // free-chat container, or a folder since deleted — must not
                // supply one. Otherwise the agent is handed a workspace it
                // cannot read and every file tool fails.
                Ok(project) if std::path::Path::new(&project.path).is_dir() => (
                    Some(session.project_id),
                    Some(std::path::PathBuf::from(&project.path)),
                    Some(project.trust_level),
                ),
                Ok(project) => {
                    tracing::debug!(
                        path = %project.path,
                        "projet sans dossier réel — conversation simple, sans outils"
                    );
                    (None, None, None)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "project not found for session");
                    (None, None, None)
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "session not found, running without project context");
            (None, None, None)
        }
    };

    // 3. Determine which agent to use based on the active provider.
    let active_provider = s.storage.providers.active().await.ok().flatten();

    let mut supervisor_ok = true;
    if let Some(ref p) = active_provider {
        if p.engine == ProviderEngine::LlamaCpp {
            tracing::debug!(endpoint = %p.endpoint, "ensuring llama-server is running");
            if let Err(e) = s.supervisor.ensure_running(ProviderEngine::LlamaCpp).await {
                tracing::warn!(error = %e, "supervisor could not ensure llama-server running — falling back to StubAgent");
                supervisor_ok = false;
            } else {
                s.supervisor.note_activity(ProviderEngine::LlamaCpp).await;
            }
        }
    }

    let model = active_provider.as_ref().and_then(|p| p.model.clone());

    // Conversation memory: replay prior turns.
    let history = {
        let prior = s
            .storage
            .messages
            .list_for_session(session_uuid)
            .await
            .unwrap_or_default();
        let mut turns: Vec<locaryn_agent_runtime::ChatTurn> = prior
            .into_iter()
            .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
            .map(|m| locaryn_agent_runtime::ChatTurn {
                role: match m.role {
                    MessageRole::Assistant => "assistant".to_string(),
                    _ => "user".to_string(),
                },
                content: m.content,
            })
            .collect();
        turns.pop(); // the message just persisted is sent separately
        turns
    };

    let mcp_state = Some(s.mcp_state.clone());
    let input = AgentInput {
        session_id: session_uuid,
        message: body.content,
        mode: s.mode,
        model: model.clone(),
        agent: None,
        project_id,
        project_path,
        trust,
        images: body.images.unwrap_or_default(),
        params: None,
        history,
        mcp_state,
        // The daemon has no extension runtime yet; the desktop shell owns it.
        extra_system: None,
        // Le démon tourne sans interface : personne ne peut arbitrer, donc
        // tout appel exigeant un accord est refusé. C'est le comportement
        // voulu pour un service, pas un oubli.
        approval: None,
    };

    // 4. Run the agent: OpenAiCompatAgent (llama-server) if possible, otherwise StubAgent.
    let event_stream: EventStream = {
        match &active_provider {
            Some(p)
                if (p.engine == ProviderEngine::LlamaCpp
                    || p.engine == ProviderEngine::OpenAiCompat)
                    && supervisor_ok =>
            {
                tracing::info!(endpoint = %p.endpoint, model = ?model, "using OpenAiCompatAgent");
                let agent = OpenAiCompatAgent::with_defaults(Some(&p.endpoint), model.as_deref());
                match agent.run(input.clone()).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::warn!(error = %e, "OpenAiCompatAgent run failed, falling back to StubAgent");
                        run_stub_agent(input).await
                    }
                }
            }
            _ => {
                tracing::warn!(
                    "falling back to StubAgent (no local provider or supervisor failed)"
                );
                run_stub_agent(input).await
            }
        }
    };

    // 5. Register a cancellation flag so POST /v1/sessions/{id}/cancel can
    //    terminate this stream. Cleaned up when the background task exits.
    let cancelled = Arc::new(AtomicBool::new(false));
    s.cancel_map
        .lock()
        .unwrap()
        .insert(session_uuid, cancelled.clone());

    // 6. Background task: collect tokens, persist on stream end, and clean
    //    up the cancellation entry.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(128);
    let storage_bg = s.storage.clone();
    let cancel_map_bg = s.cancel_map.clone();
    tokio::spawn(async move {
        let mut full_text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut tool_results: Vec<(String, String)> = Vec::new();
        let mut tokens_in = 0u64;
        let mut tokens_out = 0u64;
        while let Some(ev) = rx.recv().await {
            match ev {
                StreamEvent::Token { text } => full_text.push_str(&text),
                StreamEvent::ToolCall {
                    call_id,
                    tool,
                    args,
                } => {
                    tool_calls.push(ToolCall {
                        call_id,
                        tool,
                        args,
                    });
                }
                StreamEvent::ToolResult {
                    call_id, output, ..
                } => {
                    tool_results.push((call_id, output));
                }
                StreamEvent::MessageEnd {
                    tokens_in: ti,
                    tokens_out: to,
                    ..
                } => {
                    tokens_in = ti;
                    tokens_out = to;
                }
                StreamEvent::Artifact {
                    artifact_id,
                    kind,
                    path,
                } => {
                    // Persist artifact metadata to SQLite so GET /v1/artifacts/{id}
                    // can find it. The agent already wrote file content to disk
                    // at data_dir/path before emitting the event.
                    if let Ok(aid) = Uuid::parse_str(&artifact_id) {
                        if let Err(e) = storage_bg
                            .artifacts
                            .create_with_id(aid, session_uuid, kind, &path, None)
                            .await
                        {
                            tracing::warn!(
                                artifact_id = %artifact_id,
                                error = %e,
                                "failed to persist artifact metadata"
                            );
                        }
                    } else {
                        tracing::warn!(
                            artifact_id = %artifact_id,
                            "invalid artifact id in StreamEvent::Artifact"
                        );
                    }
                }
                _ => {}
            }
        }

        let assistant_persisted = if !full_text.is_empty() || !tool_calls.is_empty() {
            match storage_bg
                .messages
                .append_full(
                    session_uuid,
                    MessageRole::Assistant,
                    &full_text,
                    Some(&tool_calls),
                    None,
                    tokens_in,
                    tokens_out,
                    None,
                )
                .await
            {
                Ok(_) => true,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to persist assistant message");
                    false
                }
            }
        } else {
            false
        };

        if assistant_persisted {
            for (call_id, output) in tool_results {
                if let Err(e) = storage_bg
                    .messages
                    .append_full(
                        session_uuid,
                        MessageRole::Tool,
                        &output,
                        None,
                        Some(&call_id),
                        0,
                        0,
                        None,
                    )
                    .await
                {
                    tracing::warn!(call_id = %call_id, error = %e, "failed to persist tool result message");
                }
            }
        } else if !tool_results.is_empty() {
            tracing::warn!("tool results received but assistant message was not persisted");
        }

        // Clean up cancellation entry regardless of how the stream ended.
        cancel_map_bg.lock().unwrap().remove(&session_uuid);
    });

    // 7. Convert to SSE, teeing events to the background collector. The
    //    `take_while` gates the stream on the cancel flag — when cancelled
    //    the stream yields `None` and axum closes the SSE connection.
    let cancelled_clone = cancelled.clone();
    let sse_stream = event_stream
        .take_while(move |_| {
            let keep = !cancelled_clone.load(Ordering::Relaxed);
            futures::future::ready(keep)
        })
        .map(move |ev: StreamEvent| {
            let _ = tx.try_send(ev.clone());
            let json = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
            Ok::<Event, std::convert::Infallible>(
                Event::default().event(sse_event_tag(&ev)).data(json),
            )
        });

    Sse::new(sse_stream).keep_alive(KeepAlive::default())
}

#[derive(serde::Deserialize)]
struct SendMessageBody {
    content: String,
    #[serde(default)]
    images: Option<Vec<String>>,
}

async fn run_stub_agent(input: AgentInput) -> EventStream {
    match StubAgent.run(input).await {
        Ok(stream) => stream,
        Err(e) => {
            tracing::warn!(error = %e, "StubAgent run failed, returning empty stream");
            Box::pin(futures::stream::empty())
        }
    }
}

async fn list_providers(State(s): State<Arc<DaemonState>>) -> Response {
    match s.storage.providers.list().await {
        Ok(providers) => (StatusCode::OK, Json(providers)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

// ============================================================================
// Tasks endpoints
// ============================================================================

/// GET /v1/tasks/{id} — get task status
async fn get_task(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let task_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid task id" }
                })),
            )
                .into_response();
        }
    };
    match s.storage.tasks.get(task_id).await {
        Ok(task) => (StatusCode::OK, Json(task)).into_response(),
        Err(e) => {
            let status = match &e {
                locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(serde_json::json!({
                    "error": { "code": "not_found", "message": e.to_string() }
                })),
            )
                .into_response()
        }
    }
}

/// POST /v1/tasks/{id}/cancel — cancel a task (sets status to `cancelled`)
async fn cancel_task(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let task_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid task id" }
                })),
            )
                .into_response();
        }
    };
    match s
        .storage
        .tasks
        .update_status(task_id, TaskStatus::Cancelled)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "cancelled" })),
        )
            .into_response(),
        Err(e) => {
            let status = match &e {
                locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(serde_json::json!({
                    "error": { "code": "storage_error", "message": e.to_string() }
                })),
            )
                .into_response()
        }
    }
}

/// POST /v1/tasks/{id}/approve — approve or deny a tool call
///
/// Body: `{ call_id: string, decision: "allow"|"deny", scope: "once"|"session"|"project" }`
/// On deny, the task transitions to `cancelled`. On allow, the task transitions
/// back to `running` so the agent loop can proceed.
async fn approve_task(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<ApproveBody>,
) -> Response {
    let task_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid task id" }
                })),
            )
                .into_response();
        }
    };

    // Verify the task exists.
    if let Err(e) = s.storage.tasks.get(task_id).await {
        let status = match &e {
            locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        return (
            status,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": e.to_string() }
            })),
        )
            .into_response();
    }

    let new_status = match body.decision.as_str() {
        "allow" => TaskStatus::Running,
        "deny" => TaskStatus::Cancelled,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "decision must be 'allow' or 'deny'" }
                })),
            )
                .into_response();
        }
    };

    match s.storage.tasks.update_status(task_id, new_status).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": match new_status { TaskStatus::Running => "approved", _ => "denied" }
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct ApproveBody {
    call_id: String,
    decision: String,
    #[serde(default = "default_scope")]
    scope: String,
}

fn default_scope() -> String {
    "once".to_string()
}

// ============================================================================
// Artifacts endpoints
// ============================================================================

/// GET /v1/sessions/{id}/artifacts — list artifacts for a session
async fn list_artifacts(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let session_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid session id" }
                })),
            )
                .into_response();
        }
    };
    match s.storage.artifacts.list_for_session(session_id).await {
        Ok(artifacts) => (StatusCode::OK, Json(artifacts)).into_response(),
        Err(e) => {
            let status = match &e {
                locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                Json(serde_json::json!({
                    "error": { "code": "storage_error", "message": e.to_string() }
                })),
            )
                .into_response()
        }
    }
}

/// POST /v1/sessions/{id}/artifacts — create a new artifact
async fn create_artifact(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<CreateArtifactBody>,
) -> Response {
    let session_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid session id" }
                })),
            )
                .into_response();
        }
    };
    let kind = parse_artifact_kind(&body.kind);
    match s
        .storage
        .artifacts
        .create(session_id, kind, &body.path, body.title)
        .await
    {
        Ok(artifact) => (StatusCode::CREATED, Json(artifact)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

#[derive(serde::Deserialize)]
struct CreateArtifactBody {
    kind: String,
    path: String,
    #[serde(default)]
    title: Option<String>,
}

fn parse_artifact_kind(s: &str) -> ArtifactKind {
    match s {
        "html" => ArtifactKind::Html,
        "markdown" => ArtifactKind::Markdown,
        "python_text" => ArtifactKind::PythonText,
        "image_png" => ArtifactKind::ImagePng,
        "plotly_html" => ArtifactKind::PlotlyHtml,
        _ => ArtifactKind::Html,
    }
}

/// GET /v1/artifacts/{id} — artifact metadata + base64 content (if file exists)
async fn get_artifact(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let artifact_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid artifact id" }
                })),
            )
                .into_response();
        }
    };

    let artifact = match s.storage.artifacts.get(artifact_id).await {
        Ok(a) => a,
        Err(e) => {
            let status = match &e {
                locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            return (
                status,
                Json(serde_json::json!({
                    "error": { "code": "not_found", "message": e.to_string() }
                })),
            )
                .into_response();
        }
    };

    // Attempt to read the artifact file from disk. The path is relative to the
    // data directory. If the file doesn't exist, return metadata without content.
    let file_path = s.data_dir.join(&artifact.path);
    let content = match tokio::fs::read(&file_path).await {
        Ok(bytes) => Some(base64_encode(&bytes)),
        Err(e) => {
            tracing::warn!(path = %file_path.display(), error = %e, "artifact file not found");
            None
        }
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "id": artifact.id,
            "session_id": artifact.session_id,
            "kind": artifact.kind,
            "path": artifact.path,
            "title": artifact.title,
            "created_at": artifact.created_at,
            "content": content,   // base64-encoded, null if file missing
        })),
    )
        .into_response()
}

/// GET /v1/artifacts/{id}/raw — serve the raw artifact file with Content-Type
async fn get_artifact_raw(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let artifact_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "invalid artifact id" }
                })),
            )
                .into_response();
        }
    };

    let artifact = match s.storage.artifacts.get(artifact_id).await {
        Ok(a) => a,
        Err(e) => {
            let status = match &e {
                locaryn_storage::StorageError::NotFound(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            return (
                status,
                Json(serde_json::json!({
                    "error": { "code": "not_found", "message": e.to_string() }
                })),
            )
                .into_response();
        }
    };

    let file_path = s.data_dir.join(&artifact.path);
    match tokio::fs::read(&file_path).await {
        Ok(bytes) => {
            let mime = match artifact.kind {
                ArtifactKind::Html => "text/html; charset=utf-8",
                ArtifactKind::Markdown => "text/markdown; charset=utf-8",
                ArtifactKind::PythonText => "text/plain; charset=utf-8",
                ArtifactKind::ImagePng => "image/png",
                ArtifactKind::PlotlyHtml => "text/html; charset=utf-8",
            };
            Response::builder()
                .header("Content-Type", mime)
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            tracing::warn!(path = %file_path.display(), error = %e, "artifact file not found");
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": { "code": "not_found", "message": format!("artifact file not found: {e}")
                }})),
            )
                .into_response()
        }
    }
}

/// Simple base64 encoding helper. Uses the `base64` crate if available,
/// otherwise falls back to a minimal implementation.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// ============================================================================
// Supervisor endpoints
// ============================================================================

/// GET /v1/supervisor/status — snapshot of all local runtimes.
async fn supervisor_status(
    State(s): State<Arc<DaemonState>>,
) -> Json<Vec<locaryn_provider_supervisor::EngineSnapshot>> {
    let snapshot = s.supervisor.status_snapshot().await;
    Json(snapshot)
}

#[derive(serde::Deserialize)]
struct SupervisorActionBody {
    engine: String,
}

/// POST /v1/supervisor/start — manually start an engine.
async fn supervisor_start(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<SupervisorActionBody>,
) -> Response {
    let engine = match parse_engine_str(&body.engine) {
        Some(e) => e,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "unknown engine" }
                })),
            )
                .into_response();
        }
    };
    match s.supervisor.ensure_running(engine).await {
        Ok(endpoint) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "engine": format!("{engine:?}").to_lowercase(),
                "endpoint": endpoint,
                "status": "healthy"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": { "code": "supervisor_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

/// POST /v1/supervisor/stop — manually stop an engine we own.
async fn supervisor_stop(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<SupervisorActionBody>,
) -> Response {
    let engine = match parse_engine_str(&body.engine) {
        Some(e) => e,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": "unknown engine" }
                })),
            )
                .into_response();
        }
    };
    match s.supervisor.shutdown(engine).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "engine": format!("{engine:?}").to_lowercase(),
                "status": "stopped"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "supervisor_error", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

fn parse_engine_str(s: &str) -> Option<ProviderEngine> {
    match s.to_lowercase().as_str() {
        "ollama" => Some(ProviderEngine::Ollama),
        "llama_cpp" | "llama-cpp" | "llamacpp" => Some(ProviderEngine::LlamaCpp),
        "lmstudio" | "lm_studio" => Some(ProviderEngine::Lmstudio),
        "vllm" => Some(ProviderEngine::Vllm),
        _ => None,
    }
}
