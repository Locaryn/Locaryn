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
use locaryn_agent_runtime::{Agent, AgentInput, EventStream, OpenAiCompatAgent};
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
    /// Client HTTP partagé (sondes de santé des noyaux, pont).
    pub http: reqwest::Client,
    /// Noyaux alternatifs (OpenClaw, Hermes…) : superviseur de processus
    /// partagé avec le desktop (D4).
    pub cores: Arc<locaryn_core_bridge::manager::CoreManager>,
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

/// Réponse aux deux drapeaux qu'on attend d'un binaire de service avant de
/// l'installer : dire sa version et dire à quoi il sert.
///
/// Sans cela, `locaryn-daemon --version` démarrait le serveur : la base
/// s'ouvrait, le port se réservait, et la commande ne rendait jamais la main.
/// C'est le premier geste d'un paquet, d'un script de service ou d'un humain
/// qui vérifie une installation — il ne doit rien démarrer.
///
/// Renvoie `true` quand la demande a été traitée et qu'il n'y a rien à lancer.
fn handled_informational_flag() -> bool {
    let arg = match std::env::args().nth(1) {
        Some(a) => a,
        None => return false,
    };
    match arg.as_str() {
        "-V" | "--version" => {
            println!("locaryn-daemon {}", env!("CARGO_PKG_VERSION"));
            true
        }
        "-h" | "--help" => {
            println!(
                "locaryn-daemon {} — {}\n\n\
                 Usage: locaryn-daemon [-h | -V]\n\n\
                 Le démon n'a pas d'options de ligne de commande : il lit sa\n\
                 configuration (adresse d'écoute, port, dossier de données) dans\n\
                 le fichier de configuration Locaryn. Utilisez la CLI `locaryn`\n\
                 pour le piloter (`locaryn daemon --help`).\n\n\
                 Options:\n  \
                 -h, --help     Affiche cette aide\n  \
                 -V, --version  Affiche la version",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_DESCRIPTION"),
            );
            true
        }
        _ => false,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if handled_informational_flag() {
        return Ok(());
    }

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
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new()),
        cores: locaryn_core_bridge::manager::CoreManager::new(),
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

    // Les extensions installées reviennent de la base : sans cela, un
    // redémarrage du service les faisait disparaître, avec les écrans et les
    // outils qu'elles apportaient.
    routes::extensions::restore_from_storage(&state).await;

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
        // Renommer à la main : le titre devient définitif, aucun modèle n'y
        // touche plus.
        .route("/v1/sessions/:id/title", post(rename_session))
        // Archiver, sortir des archives, ranger dans un projet.
        .route("/v1/sessions/:id/archive", post(archive_session))
        .route("/v1/sessions/:id/project", post(move_session))
        // Ce que le petit modèle propose : un rangement, une réunion.
        .route("/v1/sessions/:id/suggest-project", get(suggest_project))
        .route("/v1/sessions/:id/merge", post(merge_sessions))
        .route("/v1/projects/:pid/archived", get(list_archived_sessions))
        // Les figures : un rôle, ses consignes, ses conversations.
        .route("/v1/figures", get(list_figures).post(save_figure))
        .route("/v1/figures/:id", delete(remove_figure))
        .route("/v1/figures/:id/sessions", get(figure_sessions))
        .route("/v1/sessions/:id/figure", post(attach_figure))
        // Le modèle des micro-tâches : lequel, et lesquels sont disponibles.
        .route(
            "/v1/assistance/micro-model",
            get(get_micro_model).post(set_micro_model),
        )
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
        // Les réglages que les extensions déclarent dans leur manifeste.
        .route(
            "/v1/extensions/config",
            get(routes::extensions::get_extension_config),
        )
        .route(
            "/v1/extensions/:name/config",
            post(routes::extensions::set_extension_config),
        )
        .route(
            "/v1/extensions/:name/permissions",
            get(routes::extensions::get_extension_permissions)
                .post(routes::extensions::set_extension_permission),
        )
        // Noyaux alternatifs : processus supervisés par le daemon (D4).
        .route("/v1/cores", get(routes::cores::list_cores))
        .route("/v1/cores/:id", get(routes::cores::status))
        .route("/v1/cores/:id/start", post(routes::cores::start))
        .route("/v1/cores/:id/stop", post(routes::cores::stop))
        .route("/v1/cores/:id/skills", get(routes::cores::skills))
        .route(
            "/v1/cores/:id/skills/install",
            post(routes::cores::install_skill),
        )
        // MCP routes
        .route(
            "/v1/travel",
            get(routes::travel::status).post(routes::travel::set),
        )
        .route("/v1/travel/home", get(routes::travel::home))
        // Le code d'appairage : local, port ouvert, ou tunnel.
        .route("/v1/pairing", get(routes::pairing::qr))
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
        // Un bouton d'extension nomme un outil, pas un serveur : on le cherche.
        .route("/v1/tools/:tool", post(routes::mcp::invoke_tool_par_nom))
        // Media generation — exposed so thin clients (the phone) can use the
        // engines that only run where the models live.
        // Vitesses mesurées : ce que chaque modèle donne sur cette machine.
        .route("/v1/metrics/models", get(list_model_metrics))
        .route("/v1/media/models", get(media::list_models))
        .route("/v1/media/image", post(media::generate_image))
        .route("/v1/media/audio", post(media::generate_audio))
        // Mémoire de l'utilisateur : ce que le service retient d'une
        // conversation à l'autre, lisible et corrigible depuis n'importe quel
        // client puisqu'elle vit ici et non dans un fichier d'application.
        .route(
            "/v1/memory",
            get(routes::memory::list)
                .post(routes::memory::remember)
                .delete(routes::memory::forget_all),
        )
        .route(
            "/v1/memory/:id",
            axum::routing::put(routes::memory::edit).delete(routes::memory::forget),
        )
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
                .route("/v1/auth/password", post(auth::change_password))
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

async fn info(State(s): State<Arc<DaemonState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "locaryn-daemon",
        "version": env!("CARGO_PKG_VERSION"),
        "capabilities": ["sessions", "projects", "tasks", "artifacts", "extensions", "mcp", "media"],
        // Sur la boucle locale, le service sert l'API sans jeton : la personne
        // au clavier est déjà celle que le système a authentifiée. Le client web
        // ne pouvait pas le savoir et présentait un écran de connexion que rien
        // n'exigeait — infranchissable pour qui n'a jamais créé de compte.
        "auth_required": s.auth_required,
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

#[derive(serde::Deserialize, Default)]
struct CreateSessionBody {
    /// Une conversation éphémère ne laisse rien : ni titre, ni trace dans les
    /// listes. Le corps est optionnel — sans lui, la conversation est normale.
    #[serde(default)]
    ephemeral: bool,
    /// Le noyau auquel confier la conversation (id d'extension de noyau).
    /// Absent : le noyau Locaryn natif, comportement historique.
    #[serde(default)]
    core_id: Option<String>,
}

async fn create_session(
    State(s): State<Arc<DaemonState>>,
    Path(pid): Path<String>,
    body: Option<Json<CreateSessionBody>>,
) -> Response {
    let b = body.map(|Json(b)| b).unwrap_or_default();
    let ephemeral = b.ephemeral;
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

    // Un noyau choisi doit exister et être un vrai noyau : une conversation
    // confiée à une extension quelconque s'ouvrirait puis échouerait à la
    // première phrase. On refuse au lieu de laisser faire (D2, pas de
    // fallback silencieux).
    if let Some(core_id) = &b.core_id {
        let uid = match Uuid::parse_str(core_id) {
            Ok(u) => u,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": { "code": "bad_request", "message": "core_id invalide" }
                    })),
                )
                    .into_response();
            }
        };
        match routes::cores::verifier_noyau(&s, uid).await {
            Ok(true) => {}
            Ok(false) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": { "code": "bad_request", "message": "cette extension n'est pas un noyau installé" }
                    })),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": { "code": "bad_request", "message": e }
                    })),
                )
                    .into_response();
            }
        }
    }

    let title = None;
    match s
        .storage
        .sessions
        .create_with_core(project_id, title, ephemeral, b.core_id.as_deref())
        .await
    {
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

    // Le titre de la conversation.
    //
    // Deux temps, parce que les deux comptent : la première phrase donne tout
    // de suite un titre lisible — la liste ne doit pas afficher « Conversation »
    // même une seconde — puis le modèle en écrit un vrai, à partir du sujet et
    // du projet ouvert, et le remplace. La demande au modèle part en tâche de
    // fond : personne n'attend son titre pour lire sa réponse.
    if let Err(e) = s
        .storage
        .sessions
        .title_if_unset(session_uuid, &body.content)
        .await
    {
        tracing::warn!(error = %e, "titre de conversation non posé");
    }
    spawn_titre_du_modele(s.clone(), session_uuid, body.content.clone());

    // 2. Look up the session → project to get trust level and project path.
    //    La session peut être confiée à un noyau alternatif : on le retient
    //    avant de construire l'agent, c'est lui qui décidera du routage.
    let session_core_id = s
        .storage
        .sessions
        .get(session_uuid)
        .await
        .ok()
        .and_then(|sess| sess.core_id);
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
                tracing::warn!(error = %e, "supervisor could not ensure llama-server running");
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
    let mut input = AgentInput {
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
        // Ce que le service retient de la personne, versé au prompt système.
        // C'est le même texte que montre l'écran de réglages : personne ne
        // doit avoir à deviner ce que le modèle sait de lui.
        extra_system: bloc_systeme(&s, session_uuid).await,
        // Ce que les extensions actives apportent : c'est ce qui décide des
        // outils offerts au modèle. Sans l'extension d'images, il n'a aucun
        // moyen d'en générer une, et le dit.
        capabilities: s
            .extensions
            .list()
            .into_iter()
            .filter(|e| e.enabled)
            .flat_map(|e| e.capabilities)
            .collect(),
        // Les outils que la figure de cette conversation autorise.
        // Absents : tout ce que l'application propose.
        tools: s
            .storage
            .figures
            .for_session(session_uuid)
            .await
            .ok()
            .flatten()
            .and_then(|f| f.tools)
            .filter(|t| !t.is_empty()),
        // Le démon tourne sans interface : personne ne peut arbitrer, donc
        // tout appel exigeant un accord est refusé. C'est le comportement
        // voulu pour un service, pas un oubli.
        approval: None,
        // Renseigné plus bas si la session est confiée à un noyau alternatif.
        bearer_token: None,
    };

    // 4. Run the agent: OpenAiCompatAgent (llama-server) when one answers,
    //    otherwise a response that names what is missing. Une session confiée
    //    à un noyau alternatif passe par le pont — sans fallback silencieux
    //    vers le noyau Locaryn (D2) : noyau choisi mais injoignable = message
    //    clair et action de réparation.
    let event_stream: EventStream = if let Some(core_id) = &session_core_id {
        match routes::cores::agent_for_core(&s, core_id).await {
            Ok((agent, token)) => {
                input.bearer_token = token;
                match agent.run(input.clone()).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::warn!(core = %core_id, error = %e, "noyau alternatif injoignable");
                        no_model_stream(&format!(
                            "Le noyau de cette conversation ne répond pas ({e}). \
                             Ouvrez Réglages → Extensions et démarrez-le."
                        ))
                    }
                }
            }
            Err(e) => no_model_stream(&e),
        }
    } else {
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
                        tracing::warn!(error = %e, "OpenAiCompatAgent run failed");
                        no_model_stream(&format!("le moteur a refusé la requête ({e})"))
                    }
                }
            }
            Some(p) if !supervisor_ok => {
                tracing::warn!(endpoint = %p.endpoint, "provider unreachable");
                no_model_stream(&format!(
                    "le moteur configuré ({}) ne répond pas",
                    p.endpoint
                ))
            }
            _ => {
                tracing::warn!("no usable provider configured");
                no_model_stream("aucun fournisseur de modèle n'est configuré")
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
    // Le profil se déduit après coup, donc la tâche de fond a besoin de tout
    // l'état, pas seulement du dépôt.
    let state_bg = s.clone();
    let cancel_map_bg = s.cancel_map.clone();
    // Le nom du modèle et l'instant de départ, pour mesurer la vitesse au bout.
    // Un chemin complet est illisible dans une liste : on garde le nom de
    // fichier, celui qui s'affiche partout ailleurs.
    let model_for_metrics = model
        .as_deref()
        .map(|m| m.rsplit(['/', '\\']).next().unwrap_or(m).to_string());
    let started_at = std::time::Instant::now();
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

        // Vitesse réellement obtenue sur cette machine, pour ce modèle. Les
        // chiffres d'un catalogue valent pour le matériel de celui qui les a
        // publiés ; ceux-ci valent pour celui qui va s'en servir.
        if let Some(model_name) = model_for_metrics.as_deref() {
            let elapsed = started_at.elapsed().as_millis() as u64;
            if let Err(e) = storage_bg
                .metrics
                .record_chat(model_name, tokens_out, elapsed)
                .await
            {
                tracing::warn!(error = %e, "vitesse non enregistrée");
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
                Ok(_) => {
                    // L'échange est écrit : on peut en tirer ce qu'il apprend
                    // de la personne, sans retarder personne.
                    spawn_profil_de_l_utilisateur(state_bg.clone(), session_uuid);
                    true
                }
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

/// Réponse rendue quand aucun modèle de conversation n'est joignable.
///
/// Le repli était l'agent factice : il renvoyait « (stub agent) echo: … », que
/// l'interface affichait comme une réponse de l'assistant. On voyait donc un
/// modèle qui répète la question au lieu d'un service qui dit ce qui lui
/// manque — et le journal, seul à porter la raison, n'est lu par personne.
fn no_model_stream(reason: &str) -> EventStream {
    let message = format!(
        "Aucun modèle de conversation n'est disponible : {reason}.\n\n\
         Vérifiez dans Réglages → Moteur que le moteur local est installé et \
         qu'un modèle de discussion est sélectionné (les modèles d'image ou de \
         voix ne savent pas répondre à une conversation).",
    );
    let message_id = Uuid::new_v4().to_string();
    Box::pin(futures::stream::iter(vec![
        StreamEvent::MessageStart {
            message_id: message_id.clone(),
            task_id: Uuid::new_v4().to_string(),
        },
        StreamEvent::Token { text: message },
        StreamEvent::MessageEnd {
            message_id,
            tokens_in: 0,
            tokens_out: 0,
            duration_ms: 0,
        },
    ]))
}

/// GET /v1/metrics/models — vitesses mesurées, par modèle.
async fn list_model_metrics(State(s): State<Arc<DaemonState>>) -> Response {
    match s.storage.metrics.list().await {
        Ok(m) => (StatusCode::OK, Json(m)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "storage_error", "message": e.to_string() }
            })),
        )
            .into_response(),
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
                ArtifactKind::AudioWav => "audio/wav",
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

/// GET /v1/assistance/micro-model — le modèle des micro-tâches, et le choix.
async fn get_micro_model(State(s): State<Arc<DaemonState>>) -> Response {
    let choisi = locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model);
    // Les modèles de conversation installés : ce sont eux qu'on peut désigner.
    let disponibles: Vec<String> = match s.storage.providers.list().await {
        Ok(ps) => ps.into_iter().filter_map(|p| p.model).collect(),
        Err(_) => Vec::new(),
    };
    let mut disponibles: Vec<String> = disponibles
        .into_iter()
        .chain(modeles_de_conversation())
        .collect();
    disponibles.sort();
    disponibles.dedup();
    Json(serde_json::json!({ "model": choisi, "available": disponibles })).into_response()
}

/// Les poids présents qui peuvent tenir une conversation.
///
/// Le dossier des modèles mélange les moteurs : ceux de diffusion produisent
/// des images et ne répondraient pas à une question. On ne propose donc que ce
/// qui reste.
fn modeles_de_conversation() -> Vec<String> {
    let dir = locaryn_config::models_dir();
    let mut out = Vec::new();
    let Ok(entrees) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entrees.flatten() {
        let chemin = e.path();
        if !chemin.is_file() {
            continue;
        }
        let Some(nom) = chemin.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let minuscule = nom.to_ascii_lowercase();
        if !minuscule.ends_with(".gguf") {
            continue;
        }
        if locaryn_media::image::is_diffusion_checkpoint(nom) {
            continue;
        }
        out.push(nom.to_string());
    }
    out
}

#[derive(serde::Deserialize)]
struct MicroModelBody {
    /// `null` ou vide : plus de micro-tâches du tout.
    #[serde(default)]
    model: Option<String>,
}

/// POST /v1/assistance/micro-model — choisir, ou n'en choisir aucun.
async fn set_micro_model(Json(body): Json<MicroModelBody>) -> Response {
    let choix = body.model.filter(|m| !m.trim().is_empty());
    if let Err(e) =
        locaryn_config::set_global("assistance", serde_json::json!({ "micro_model": choix }))
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "config", "message": e.to_string() }
            })),
        )
            .into_response();
    }
    Json(serde_json::json!({ "model": choix })).into_response()
}

#[derive(serde::Deserialize)]
struct ArchiveBody {
    /// Faux pour ressortir une conversation des archives.
    #[serde(default = "vrai")]
    archived: bool,
}

fn vrai() -> bool {
    true
}

/// POST /v1/sessions/{id}/archive — ranger, ou sortir des archives.
async fn archive_session(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<ArchiveBody>,
) -> Response {
    let Ok(session_id) = Uuid::parse_str(&id) else {
        return mauvaise_requete("identifiant de session invalide");
    };
    match s
        .storage
        .sessions
        .set_archived(session_id, body.archived)
        .await
    {
        Ok(()) => Json(serde_json::json!({ "id": id, "archived": body.archived })).into_response(),
        Err(e) => introuvable(&e.to_string()),
    }
}

#[derive(serde::Deserialize)]
struct MoveBody {
    project_id: String,
}

/// POST /v1/sessions/{id}/project — ranger une conversation dans un projet.
async fn move_session(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<MoveBody>,
) -> Response {
    let (Ok(session_id), Ok(project_id)) =
        (Uuid::parse_str(&id), Uuid::parse_str(&body.project_id))
    else {
        return mauvaise_requete("identifiant invalide");
    };
    match s
        .storage
        .sessions
        .move_to_project(session_id, project_id)
        .await
    {
        Ok(()) => {
            Json(serde_json::json!({ "id": id, "project_id": body.project_id })).into_response()
        }
        Err(e) => introuvable(&e.to_string()),
    }
}

/// GET /v1/sessions/{id}/suggest-project — où cette conversation irait bien.
///
/// Répond `{ "project_id": null }` la plupart du temps, et c'est la bonne
/// réponse : une suggestion qui tombe à côté agace plus qu'elle n'aide. Rien
/// n'est déplacé ici — c'est une proposition, le déplacement reste un geste de
/// la personne.
async fn suggest_project(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let Ok(session_id) = Uuid::parse_str(&id) else {
        return mauvaise_requete("identifiant invalide");
    };

    let rien = || Json(serde_json::json!({ "project_id": null })).into_response();

    // Une conversation éphémère ne se range nulle part : elle ne survit pas.
    // Une conversation déjà dans un projet n'a rien à se voir proposer.
    let Ok(sess) = s.storage.sessions.get(session_id).await else {
        return introuvable("conversation inconnue");
    };
    if sess.ephemeral {
        return rien();
    }

    let Ok(projets) = s.storage.projects.list().await else {
        return rien();
    };
    let candidats: Vec<locaryn_agent_runtime::titling::ProjetConnu> = projets
        .iter()
        .filter(|p| p.id != sess.project_id && p.name != "Conversations libres")
        .map(|p| locaryn_agent_runtime::titling::ProjetConnu {
            id: p.id.to_string(),
            name: p.name.clone(),
        })
        .collect();
    if candidats.is_empty() {
        return rien();
    }

    let Some(micro) = locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model)
        .filter(|m| !m.trim().is_empty())
    else {
        return rien();
    };
    let Ok(providers) = s.storage.providers.list().await else {
        return rien();
    };
    let Some(p) = providers.into_iter().find(|p| {
        p.is_active
            && (p.engine == ProviderEngine::LlamaCpp || p.engine == ProviderEngine::OpenAiCompat)
    }) else {
        return rien();
    };

    let echange = match s.storage.messages.list_for_session(session_id).await {
        Ok(msgs) => resume_des_messages(&msgs, 6),
        Err(_) => return rien(),
    };
    if echange.trim().is_empty() {
        return rien();
    }

    let client = reqwest::Client::new();
    let choix = locaryn_agent_runtime::titling::ask_for_project(
        &p.endpoint,
        &client,
        &micro,
        &echange,
        &candidats,
    )
    .await;
    match choix {
        Some(pid) => {
            let nom = projets
                .iter()
                .find(|p| p.id.to_string() == pid)
                .map(|p| p.name.clone());
            Json(serde_json::json!({ "project_id": pid, "project_name": nom })).into_response()
        }
        None => rien(),
    }
}

#[derive(serde::Deserialize)]
struct MergeBody {
    /// La conversation à verser dans celle-ci.
    source_id: String,
}

/// POST /v1/sessions/{id}/merge — réunir deux conversations en une.
///
/// Le modèle des micro-tâches relit les deux fils et en écrit un seul récit,
/// versé dans la conversation d'accueil. La conversation absorbée n'est pas
/// supprimée : elle part aux archives, d'où elle peut ressortir si le résumé
/// a perdu quelque chose. Une fusion ne doit jamais être un geste sans retour.
async fn merge_sessions(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<MergeBody>,
) -> Response {
    let (Ok(accueil_id), Ok(source_id)) = (Uuid::parse_str(&id), Uuid::parse_str(&body.source_id))
    else {
        return mauvaise_requete("identifiant invalide");
    };
    if accueil_id == source_id {
        return mauvaise_requete("une conversation ne se fusionne pas avec elle-même");
    }

    let Some(micro) = locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model)
        .filter(|m| !m.trim().is_empty())
    else {
        return mauvaise_requete(
            "aucun modèle de micro-tâches n'est choisi : la fusion demande un modèle \
             pour relire les deux conversations (Réglages → Assistance)",
        );
    };
    let Ok(providers) = s.storage.providers.list().await else {
        return introuvable("aucun moteur disponible");
    };
    let Some(p) = providers.into_iter().find(|p| {
        p.is_active
            && (p.engine == ProviderEngine::LlamaCpp || p.engine == ProviderEngine::OpenAiCompat)
    }) else {
        return introuvable("aucun moteur actif");
    };

    let (Ok(msgs_accueil), Ok(msgs_source)) = (
        s.storage.messages.list_for_session(accueil_id).await,
        s.storage.messages.list_for_session(source_id).await,
    ) else {
        return introuvable("conversation inconnue");
    };

    let client = reqwest::Client::new();
    let Some(texte) = locaryn_agent_runtime::titling::ask_for_merge(
        &p.endpoint,
        &client,
        &micro,
        &resume_des_messages(&msgs_accueil, 40),
        &resume_des_messages(&msgs_source, 40),
    )
    .await
    else {
        return introuvable("le modèle n'a rien rendu — rien n'a été modifié");
    };

    // Le récit est versé comme un message de l'assistant : il se lit dans le
    // fil, se copie, et se relit plus tard comme n'importe quelle réponse.
    if let Err(e) = s
        .storage
        .messages
        .append(
            accueil_id,
            locaryn_shared_types::MessageRole::Assistant,
            &texte,
        )
        .await
    {
        return introuvable(&e.to_string());
    }
    // Aux archives, pas à la poubelle.
    if let Err(e) = s.storage.sessions.set_archived(source_id, true).await {
        tracing::warn!(error = %e, "conversation fusionnée non archivée");
    }

    Json(serde_json::json!({ "id": id, "archived_source": body.source_id })).into_response()
}

/// Mettre les derniers messages sous une forme lisible par le modèle.
///
/// Les derniers, pas les premiers : ce qui vient d'être dit renseigne mieux
/// sur le sujet d'une conversation que la façon dont elle a commencé.
fn resume_des_messages(msgs: &[locaryn_shared_types::Message], combien: usize) -> String {
    let debut = msgs.len().saturating_sub(combien);
    msgs[debut..]
        .iter()
        .filter(|m| !m.content.trim().is_empty())
        .map(|m| format!("{:?} : {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n")
}

/// GET /v1/projects/{pid}/archived — ce qui a été rangé.
async fn list_archived_sessions(
    State(s): State<Arc<DaemonState>>,
    Path(pid): Path<String>,
) -> Response {
    let Ok(project_id) = Uuid::parse_str(&pid) else {
        return mauvaise_requete("identifiant de projet invalide");
    };
    match s.storage.sessions.list_archived(project_id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => introuvable(&e.to_string()),
    }
}

/// GET /v1/figures — toutes les figures.
async fn list_figures(State(s): State<Arc<DaemonState>>) -> Response {
    match s.storage.figures.list().await {
        Ok(v) => Json(v).into_response(),
        Err(e) => introuvable(&e.to_string()),
    }
}

#[derive(serde::Deserialize)]
struct FigureBody {
    name: String,
    #[serde(default)]
    description: String,
    instructions: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    opening: Option<String>,
    #[serde(default)]
    uses_memory: bool,
    /// Les outils qu'elle a le droit d'appeler. Absents : tout ce que
    /// l'application propose.
    #[serde(default)]
    tools: Option<Vec<String>>,
    /// `user` par défaut : une figure écrite depuis l'interface est le
    /// travail de quelqu'un, et aucune mise à jour d'extension ne l'écrase.
    #[serde(default)]
    source: Option<String>,
}

/// POST /v1/figures — créer, ou remplacer celle du même nom.
async fn save_figure(State(s): State<Arc<DaemonState>>, Json(b): Json<FigureBody>) -> Response {
    match s
        .storage
        .figures
        .upsert(locaryn_storage::figures::NouvelleFigure {
            name: &b.name,
            description: &b.description,
            instructions: &b.instructions,
            model: b.model.as_deref().filter(|m| !m.trim().is_empty()),
            opening: b.opening.as_deref().filter(|o| !o.trim().is_empty()),
            uses_memory: b.uses_memory,
            tools: b.tools.as_deref(),
            source: b.source.as_deref().unwrap_or("user"),
        })
        .await
    {
        Ok(f) => (StatusCode::CREATED, Json(f)).into_response(),
        Err(e) => mauvaise_requete(&e.to_string()),
    }
}

/// DELETE /v1/figures/{id}
async fn remove_figure(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    match s.storage.figures.delete(&id).await {
        Ok(()) => Json(serde_json::json!({ "id": id, "deleted": true })).into_response(),
        Err(e) => introuvable(&e.to_string()),
    }
}

/// Une conversation d'une figure, telle que l'écran la liste :
/// l'identifiant pour la reprendre, le titre, la dernière activité.
#[derive(serde::Serialize)]
struct FigureSessionView {
    id: String,
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_message_at: Option<String>,
}

/// GET /v1/figures/{id}/sessions — ce que cette figure a conversé.
///
/// La liste porte les titres, pas seulement les identifiants : l'écran
/// affiche chaque conversation et la reprend d'un geste.
async fn figure_sessions(State(s): State<Arc<DaemonState>>, Path(id): Path<String>) -> Response {
    let ids = match s.storage.figures.session_ids(&id).await {
        Ok(v) => v,
        Err(e) => return introuvable(&e.to_string()),
    };
    let mut vues = Vec::with_capacity(ids.len());
    for sid in ids {
        let Ok(uuid) = Uuid::parse_str(&sid) else {
            continue;
        };
        if let Ok(session) = s.storage.sessions.get(uuid).await {
            vues.push(FigureSessionView {
                id: session.id.to_string(),
                title: session.title,
                last_message_at: session.last_message_at.map(|t| t.to_rfc3339()),
            });
        }
    }
    Json(vues).into_response()
}

#[derive(serde::Deserialize)]
struct AttachBody {
    /// `null` détache la conversation de sa figure.
    #[serde(default)]
    figure_id: Option<String>,
}

/// POST /v1/sessions/{id}/figure — confier une conversation à une figure.
async fn attach_figure(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(b): Json<AttachBody>,
) -> Response {
    let Ok(session_id) = Uuid::parse_str(&id) else {
        return mauvaise_requete("identifiant de session invalide");
    };
    match s
        .storage
        .figures
        .attach_session(session_id, b.figure_id.as_deref())
        .await
    {
        Ok(()) => Json(serde_json::json!({ "id": id, "figure_id": b.figure_id })).into_response(),
        Err(e) => introuvable(&e.to_string()),
    }
}

fn mauvaise_requete(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": { "code": "bad_request", "message": message }
        })),
    )
        .into_response()
}

fn introuvable(message: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": { "code": "not_found", "message": message }
        })),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct RenameBody {
    title: String,
}

/// POST /v1/sessions/{id}/title — renommer une conversation à la main.
async fn rename_session(
    State(s): State<Arc<DaemonState>>,
    Path(id): Path<String>,
    Json(body): Json<RenameBody>,
) -> Response {
    let Ok(session_id) = Uuid::parse_str(&id) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": { "code": "bad_request", "message": "identifiant de session invalide" }
            })),
        )
            .into_response();
    };
    let titre = body.title.trim();
    if titre.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": { "code": "bad_request", "message": "Un titre vide n'en est pas un." }
            })),
        )
            .into_response();
    }
    match s.storage.sessions.rename_by_user(session_id, titre).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "id": id, "title": titre, "locked": true })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}

/// Ce qui s'ajoute au prompt système, avant toute réponse.
///
/// Deux sources, et l'ordre compte : les consignes de la figure d'abord —
/// c'est le rôle qu'on lui a donné, il prime — puis ce que le service retient
/// de la personne. Une figure qui ne veut pas de cette mémoire le dit, et
/// travaille alors sans rien savoir de son utilisateur : c'est le sens de
/// « travailler à part ».
async fn bloc_systeme(s: &Arc<DaemonState>, session_id: Uuid) -> Option<String> {
    let figure = s
        .storage
        .figures
        .for_session(session_id)
        .await
        .ok()
        .flatten();
    let memoire = match &figure {
        Some(f) if !f.uses_memory => None,
        _ => s.storage.memory.as_system_block(None).await.unwrap_or(None),
    };

    match (figure, memoire) {
        (None, m) => m,
        (Some(f), None) => Some(f.instructions),
        (Some(f), Some(m)) => Some(format!(
            "{}

{m}",
            f.instructions
        )),
    }
}

/// Écouter ce qu'un échange apprend de durable sur la personne, et le retenir.
///
/// Une mémoire qu'il faut remplir soi-même reste vide : personne n'ouvre un
/// formulaire pour déclarer ses préférences. Elles se disent en passant — « fais
/// court », « je travaille en Rust » — et c'est là qu'il faut les entendre.
///
/// Comme le titre : en tâche de fond, avec le modèle des micro-tâches, et
/// seulement si quelqu'un en a désigné un. Une conversation éphémère est
/// ignorée : elle promet de ne rien laisser.
fn spawn_profil_de_l_utilisateur(s: Arc<DaemonState>, session_id: Uuid) {
    tokio::spawn(async move {
        match s.storage.sessions.get(session_id).await {
            Ok(sess) if !sess.ephemeral => {}
            _ => return,
        }

        let Some(micro) = locaryn_config::load(None)
            .ok()
            .and_then(|c| c.assistance.micro_model)
            .filter(|m| !m.trim().is_empty())
        else {
            return;
        };

        // Le dernier aller-retour suffit : c'est là que se disent les
        // préférences, et relire toute la conversation à chaque message
        // coûterait plus que ça ne rapporte.
        let Ok(msgs) = s.storage.messages.list_for_session(session_id).await else {
            return;
        };
        let echange: String = msgs
            .iter()
            .rev()
            .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|m| {
                let qui = match m.role {
                    MessageRole::User => "Personne",
                    _ => "Assistant",
                };
                format!("{qui} : {}", m.content)
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        if echange.trim().is_empty() {
            return;
        }

        let Ok(providers) = s.storage.providers.list().await else {
            return;
        };
        let Some(p) = providers.into_iter().find(|p| {
            p.is_active
                && (p.engine == ProviderEngine::LlamaCpp
                    || p.engine == ProviderEngine::OpenAiCompat)
        }) else {
            return;
        };

        let client = reqwest::Client::new();
        let faits =
            locaryn_agent_runtime::titling::ask_for_profile(&p.endpoint, &client, &micro, &echange)
                .await;
        for f in faits {
            // Le dépôt refuse les doublons : réentendre deux fois la même
            // préférence ne la note pas deux fois.
            match s
                .storage
                .memory
                .remember(None, &f.category, &f.content, "assistant")
                .await
            {
                Ok(_) => tracing::info!(fait = %f.content, "mémoire enrichie"),
                Err(e) => tracing::debug!(error = %e, "fait déjà connu ou non enregistré"),
            }
        }
    });
}

/// Demander au modèle de nommer une conversation, sans faire attendre personne.
///
/// Le titre posé à partir de la première phrase reste tant que le modèle n'a
/// rien donné de meilleur : une conversation nommée approximativement vaut
/// mieux qu'une conversation qui attend son nom.
fn spawn_titre_du_modele(s: Arc<DaemonState>, session_id: Uuid, premiere_demande: String) {
    tokio::spawn(async move {
        // Une conversation éphémère ne porte pas de nom : la nommer, ce serait
        // en garder quelque chose.
        match s.storage.sessions.get(session_id).await {
            Ok(sess) if !sess.ephemeral => {}
            _ => return,
        }

        // Un titre ne se redemande pas à chaque message : seule la première
        // demande le déclenche. À cet instant, elle est le seul message de la
        // conversation.
        match s.storage.messages.list_for_session(session_id).await {
            Ok(msgs) if msgs.len() <= 1 => {}
            _ => return,
        }

        // Le modèle des micro-tâches se choisit dans les réglages, et rien
        // n'est choisi par défaut. Tant qu'aucun n'est désigné, la conversation
        // garde le titre tiré de sa première phrase : mieux vaut un titre
        // approximatif que le gros modèle détourné de son tour pour cinq mots.
        let Some(micro) = locaryn_config::load(None)
            .ok()
            .and_then(|c| c.assistance.micro_model)
            .filter(|m| !m.trim().is_empty())
        else {
            return;
        };

        let Ok(providers) = s.storage.providers.list().await else {
            return;
        };
        let Some(p) = providers.into_iter().find(|p| {
            p.is_active
                && (p.engine == ProviderEngine::LlamaCpp
                    || p.engine == ProviderEngine::OpenAiCompat)
        }) else {
            return;
        };

        let projet = match s.storage.sessions.get(session_id).await {
            Ok(sess) => s
                .storage
                .projects
                .get(sess.project_id)
                .await
                .ok()
                .map(|pr| pr.name),
            Err(_) => None,
        };
        // Le conteneur des conversations libres n'est pas un projet : le
        // nommer au modèle ne ferait que le mettre sur une fausse piste.
        let projet = projet.filter(|n| n != "Conversations libres");

        let client = reqwest::Client::new();
        let modele = micro;
        let demande = locaryn_agent_runtime::titling::TitleRequest {
            first_message: premiere_demande,
            first_reply: None,
            project: projet,
        };
        if let Some(titre) =
            locaryn_agent_runtime::titling::ask_for_title(&p.endpoint, &client, &modele, &demande)
                .await
        {
            if let Err(e) = s.storage.sessions.retitle(session_id, &titre).await {
                tracing::warn!(error = %e, "titre du modèle non enregistré");
            } else {
                tracing::info!(session = %session_id, titre, "conversation nommée par le modèle");
            }
        }
    });
}

/// Simple base64 encoding helper. Uses the `base64` crate if available,
/// otherwise falls back to a minimal implementation.
// L'encodage vit dans `locaryn-shared-types` : le téléphone en a besoin pour
// les mêmes raisons, et deux copies auraient fini par diverger.
pub(crate) use locaryn_shared_types::base64_encode;

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
    /// Modèle à servir. Le drapeau existait côté CLI mais n'était lu par
    /// personne : le moteur redémarrait sur le modèle déjà enregistré, et
    /// `--model` n'avait aucun effet visible.
    #[serde(default)]
    model: Option<String>,
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
    // Le modèle demandé est enregistré avant le démarrage : c'est lui que le
    // superviseur lira pour construire la ligne de commande du moteur.
    if let Some(model) = body.model.as_deref().filter(|m| !m.is_empty()) {
        let endpoint = locaryn_provider_supervisor::default_endpoint(engine).to_string();
        if let Err(e) = s
            .storage
            .providers
            .upsert_local(engine, &endpoint, Some(model.to_string()))
            .await
        {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": { "code": "bad_model", "message": e.to_string() }
                })),
            )
                .into_response();
        }
    }

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
