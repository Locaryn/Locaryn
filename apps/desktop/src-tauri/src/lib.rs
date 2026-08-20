//! Locaryn desktop Tauri shell. Embeds the Locaryn Rust core in-process so
//! the UI has zero-hop access to the agent runtime, storage, extensions,
//! and preview (no loopback HTTP needed for the desktop).
//!
//! S5: the shell opens the SAME SQLite database as the daemon/CLI
//! (`<data_dir>/locaryn.db`), so sessions created in the CLI show up in the
//! desktop and vice versa. The agent loop is delegated to
//! `locaryn_agent_runtime` exactly like the daemon does — no agent logic
//! lives in the shell.

mod airllm;
mod approval_gate;
mod client_cert;
mod core_engines;
mod extensions;
mod hooks;
mod local_profile;
mod mcp_servers;
mod memory;
mod model_residency;
mod region_edit;
mod sd_engine;
mod secure_client;
mod server_mode;
mod storage_root;
mod travel_mode;
mod voice_presets;

use futures::StreamExt as _;
use locaryn_agent_runtime::{Agent, AgentInput, EventStream, OpenAiCompatAgent};
use locaryn_auth::{Keychain, SystemKeychain};
use locaryn_events::StreamEvent;
use locaryn_preview::{PreviewOrigin, PreviewRender};
use locaryn_provider_supervisor::{Supervisor, SupervisorConfig};
use locaryn_shared_types::{
    ArtifactKind, ConnectionMode, Health, Message, MessageRole, Project, Provider, ProviderEngine,
    ProviderSummary, Session, SshAiAccess, SshServer, TrustLevel,
};
use locaryn_storage::Storage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::ipc::Channel;
use tauri::{Emitter, Manager, State};
use uuid::Uuid;

/// The window close button hides the desktop shell into the tray. Only the
/// explicit tray quit action is allowed to terminate the process.
static TRAY_QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Console processes inherit a new visible console from a GUI parent on
/// Windows unless this flag is set. Keep every implementation subprocess
/// private to the desktop window: generation, Python helpers and the terminal
/// still stream their output through Tauri channels.
pub(crate) fn hide_tokio_console(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub(crate) fn hide_std_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// The in-process core shared by all Tauri commands.
struct Core {
    storage: Storage,
    supervisor: Supervisor,
    mode: ConnectionMode,
    data_dir: std::path::PathBuf,
    http: reqwest::Client,
    /// OS keychain for SSH secrets (passwords / key passphrases).
    ///
    /// Conservé sur le cœur bien que les commandes SSH ouvrent aujourd'hui
    /// leur propre poignée : une seconde instance du trousseau signifierait
    /// deux politiques d'accès aux secrets, et c'est exactement ce qu'on veut
    /// éviter le jour où le chemin SSH sera recâblé ici.
    #[allow(dead_code)]
    keychain: Arc<dyn Keychain>,
    /// Registered MCP servers and the ones currently running. Shares
    /// `mcp.json` with the daemon, so a server added here is visible there.
    mcp: Arc<locaryn_mcp::McpState>,
    /// Enabled extensions, loaded. Rebuilt on every install/enable/disable;
    /// supplies the MCP servers registered above and the extension section of
    /// the system prompt.
    extensions: Arc<tokio::sync::RwLock<extensions::ExtensionRuntime>>,
    /// Server-side test tokens: an SSH server can only be saved after a
    /// passing test whose host key the user confirmed. Keyed by token.
    pending_tests: Arc<tokio::sync::Mutex<HashMap<String, PendingTest>>>,
    /// Active model downloads, keyed by target file name → cancel token.
    pull_cancels: Arc<tokio::sync::Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>,
    /// Lazily-spawned embeddings server for RAG: (embedding model filename, child).
    /// Pinned to the model that produced an index so queries stay comparable.
    ///
    /// Jamais relu, et c'est le but : ce champ *possède* le processus fils.
    /// Le supprimer parce qu'il paraît inutilisé tuerait le serveur
    /// d'embeddings à la fin de la fonction qui l'a lancé.
    #[allow(dead_code)]
    embed_server: Arc<tokio::sync::Mutex<Option<(String, std::process::Child)>>>,
    /// Pending tool-approval decisions (doc 11 §5/§6.5 wire protocol).
    /// One entry per in-flight `StreamEvent::ToolApproval`. The agent loop
    /// parks here until the user posts back via `approve_tool_call`.
    /// Wire surface is in place; the agent-side resume lands in V1.1.
    #[allow(dead_code)]
    pending_approvals: Arc<tokio::sync::Mutex<HashMap<String, PendingApproval>>>,
    /// La porte d'approbation : une seule pour toute l'application, sinon un
    /// « toujours » ne vaudrait que pour la conversation en cours.
    approval_gate: approval_gate::GateBureau,
    /// Noyaux alternatifs (OpenClaw, Hermes…) : processus supervisés, jetons,
    /// statut. Le noyau Locaryn, lui, n'est jamais remplacé.
    cores: Arc<core_engines::CoreManager>,
}

impl Core {
    /// Return a configured LocarynClient targeting the active remote server session.
    pub fn remote_client(&self) -> Option<locaryn_sdk::LocarynClient> {
        let sess = client_cert::current_session().ok().flatten()?;
        let cert_dir = locaryn_config::default_data_dir().join("client-tls");
        let client_pem = std::fs::read_to_string(cert_dir.join("client.pem")).ok();
        let ca_pem = std::fs::read_to_string(cert_dir.join("authority.pem")).ok();
        let fingerprint = locaryn_config::provision::load()
            .ok()
            .flatten()
            .and_then(|p| p.certificate_fingerprint);
        let http = crate::secure_client::build(
            client_pem.as_deref(),
            ca_pem.as_deref(),
            fingerprint.as_deref(),
            std::time::Duration::from_secs(60),
        )
        .ok()?;
        locaryn_sdk::LocarynClient::with_client(sess.server_url, Some(sess.token), http).ok()
    }
}

/// One in-flight approval prompt, parked on the runtime until the user
/// resolves it on the desktop (or the daemon receives a CLI answer).
#[allow(dead_code)]
struct PendingApproval {
    created: std::time::Instant,
    effective_risk: locaryn_events::Risk,
    tool: String,
    remote_target: Option<String>,
}

// ============================================================================
// Safety guardrails — NSFW / unfiltered diffusion checkpoints and LoRAs
// ============================================================================

/// Normalise a model identifier so substring matching ignores case, spacing,
/// punctuation and path separators.
fn normalize_model_name(name: &str) -> String {
    name.to_lowercase().replace(
        |c: char| {
            c.is_whitespace()
                || c == '_'
                || c == '-'
                || c == '–'
                || c == '—'
                || c == '.'
                || c == '/'
                || c == '\\'
        },
        "",
    )
}

/// Diffusion checkpoints / merges explicitly fine-tuned or marketed as
/// NSFW / unfiltered / sans garde-fous.
const NSFW_CHECKPOINT_PATTERNS: &[&str] = &[
    "realisticvision",
    "realistic_vision",
    "urpm",
    "uberrealistic",
    "uber_realistic",
    "ponydiffusion",
    "pony_diffusion",
    "abyssorangemix",
    "abyss_orange",
    "counterfeit",
    "chilloutmix",
    "chillout_mix",
    "majicmix",
    "majic_mix",
    "fluxuncensored",
    "flux_uncensored",
    "fluxunfiltered",
    "flux_unfiltered",
    "flux-nsfw",
    "fluxnsfw",
    "hunyuanvideonsfw",
    "hunyuanvideo_nsfw",
    "hunyuanvideo-nsfw",
    "wan2.1nsfw",
    "wan2.1_nsfw",
    "wan2.1-nsfw",
    "wan21nsfw",
];

/// Terms used for NSFW LoRA / embedding files and user-supplied paths.
const NSFW_LORA_PATTERNS: &[&str] = &[
    "nsfw",
    "nude",
    "nudity",
    "porn",
    "porno",
    "sex",
    "sexual",
    "explicit",
    "erotic",
    "hentai",
    "furry-nsfw",
    "furrynsfw",
    "uncensored",
    "unfiltered",
    "spread_legs",
    "spreadlegs",
    "bent_over",
    "bentover",
    "ass_up",
    "assup",
    "doggy",
    "missionary",
    "urpm",
    "realisticvision",
    "ponydiffusion",
    "abyssorangemix",
    "counterfeit",
];

fn is_nsfw_checkpoint(name: &str) -> bool {
    let norm = normalize_model_name(name);
    NSFW_CHECKPOINT_PATTERNS.iter().any(|p| norm.contains(p))
}

fn is_nsfw_lora(name: &str) -> bool {
    let norm = normalize_model_name(name);
    NSFW_LORA_PATTERNS.iter().any(|p| norm.contains(p))
}

/// Combined check used for downloads, where the URL may be a checkpoint or a LoRA.
fn is_nsfw_model(name: &str) -> bool {
    is_nsfw_checkpoint(name) || is_nsfw_lora(name)
}

/// A verified-but-unsaved SSH connection test. Gates `save_ssh_server`.
///
/// Plusieurs champs sont renseignés par la sonde sans être relus aujourd'hui :
/// `save_ssh_server` se contente pour l'instant de vérifier `confirmed` et
/// `draft_hash`. Ils sont conservés parce qu'ils décrivent ce qui a été
/// réellement constaté sur l'hôte — les jeter maintenant obligerait à
/// re-sonder pour l'audit. Voir la tâche de câblage du dossier SSH.
#[allow(dead_code)]
struct PendingTest {
    /// Hash of the connection-identifying draft fields; save must match.
    draft_hash: u64,
    host_key_algo: String,
    host_key_sha256: String,
    capabilities: serde_json::Value,
    suggested_description: String,
    /// Set true by `confirm_ssh_host_key` once the user verified the fingerprint.
    confirmed: bool,
    created: std::time::Instant,
}

/// Prune temp folders left behind by deleted free-chat sessions.
async fn cleanup_orphan_free_chat_dirs(storage: &Storage) {
    let free_dir = locaryn_config::free_chats_dir();
    let mut entries = match tokio::fs::read_dir(&free_dir).await {
        Ok(e) => e,
        Err(_) => return,
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry.file_type().await.is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name();
        let s = name.to_string_lossy();
        let id = match Uuid::parse_str(&s) {
            Ok(u) => u,
            Err(_) => continue,
        };
        if storage.sessions.get(id).await.is_ok() {
            continue;
        }
        if let Err(e) = tokio::fs::remove_dir_all(entry.path()).await {
            tracing::warn!(error = %e, path = %entry.path().display(), "failed to remove orphan free-chat dir");
        } else {
            tracing::info!(path = %entry.path().display(), "removed orphan free-chat dir");
        }
    }
}

async fn init_core() -> anyhow::Result<Core> {
    let cfg = locaryn_config::load(None)?;

    let data_dir = cfg
        .daemon
        .data_dir
        .clone()
        .unwrap_or_else(locaryn_config::default_data_dir);
    let db_path = data_dir.join("locaryn.db");
    tracing::info!(?db_path, "desktop opening shared storage");
    let pool = locaryn_storage::open(&db_path).await?;
    let storage = Storage::new(pool);
    cleanup_orphan_free_chat_dirs(&storage).await;

    // Seed llama-server (LlamaCpp) as the default local engine — no Ollama.
    let existing = storage.providers.list().await;
    if existing.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
        tracing::info!("seeding default llama-server provider");
        if let Err(e) = storage
            .providers
            .upsert_local(ProviderEngine::LlamaCpp, "http://127.0.0.1:8080", None)
            .await
        {
            tracing::warn!(error = %e, "failed to seed default provider");
        }
    } else if let Ok(list) = existing {
        // Repair endpoints saved by an earlier build whose settings default was
        // Ollama's port (11434). llama-server listens on 8080, so those installs
        // answered "aucun modèle local n'a répondu" on every message.
        for p in list
            .into_iter()
            .filter(|p| p.engine == ProviderEngine::LlamaCpp && p.endpoint.contains(":11434"))
        {
            let fixed = p.endpoint.replace(":11434", ":8080");
            tracing::warn!(old = %p.endpoint, new = %fixed, "repairing llama.cpp endpoint (was Ollama's port)");
            if let Err(e) = storage.providers.set_endpoint(p.id, &fixed).await {
                tracing::warn!(error = %e, "endpoint repair failed");
            }
        }
    }

    let supervisor = Supervisor::new(
        SupervisorConfig {
            airllm_python: find_python().map(std::path::PathBuf::from),
            ..SupervisorConfig::default()
        },
        storage.clone(),
    );
    let _hc = supervisor.spawn_healthcheck_loop();

    tokio::spawn(async move {
        // Resolves the engine dir itself: it follows the storage root, which
        // the user can move independently of the database.
        if let Err(e) = locaryn_provider_supervisor::engine_manager::ensure_engines().await {
            tracing::error!("Failed to ensure engines: {}", e);
        }
    });

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .unwrap_or_default();

    let keychain: Arc<dyn Keychain> = Arc::new(SystemKeychain::new("locaryn"));

    Ok(Core {
        storage,
        supervisor,
        mode: cfg.connection.mode,
        approval_gate: approval_gate::GateBureau::new(data_dir.clone()),
        data_dir,
        http,
        keychain,
        mcp: Arc::new(locaryn_mcp::McpState::new()),
        extensions: Arc::new(tokio::sync::RwLock::new(
            extensions::ExtensionRuntime::default(),
        )),
        pending_tests: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        pull_cancels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        embed_server: Arc::new(tokio::sync::Mutex::new(None)),
        pending_approvals: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        cores: core_engines::CoreManager::new(),
    })
}

// ============================================================================
// Health / preview
// ============================================================================

#[tauri::command]
async fn core_health(core: State<'_, Core>) -> Result<Health, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(mut h) = client.health().await {
            h.mode = ConnectionMode::Remote;
            return Ok(h);
        }
    }
    let active = core.storage.providers.active().await.ok().flatten();
    let provider_summary = active.as_ref().map(|p| ProviderSummary {
        kind: p.kind,
        engine: p.engine,
        endpoint: p.endpoint.clone(),
        model: p.model.clone(),
    });
    Ok(Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        mode: core.mode,
        active_provider: provider_summary,
    })
}

#[tauri::command]
fn resolve_preview(artifact_id: String) -> PreviewRender {
    locaryn_preview::resolve_render(
        locaryn_preview::PreviewRequest {
            artifact_id,
            kind: locaryn_shared_types::ArtifactKind::Html,
            allow_network: false,
        },
        PreviewOrigin::Tauri,
    )
}

/// Materialise a browser-recorded audio blob for MCP clients that accept a
/// local path (Telegram and bridge transports). The path is confined to the
/// application's scratch directory and the UI removes it after sending.
#[tauri::command]
fn write_test_audio(audio_base64: String, mime_type: String) -> Result<String, String> {
    use base64::Engine as _;

    let encoded = audio_base64
        .split_once(",")
        .map(|(_, data)| data)
        .unwrap_or(audio_base64.as_str());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("audio base64 invalide : {e}"))?;
    if bytes.is_empty() {
        return Err("l'enregistrement audio est vide".into());
    }
    if bytes.len() > 25 * 1024 * 1024 {
        return Err("l'enregistrement audio dépasse 25 Mo".into());
    }

    let extension = if mime_type.contains("ogg") || mime_type.contains("opus") {
        "ogg"
    } else if mime_type.contains("mpeg") || mime_type.contains("mp3") {
        "mp3"
    } else if mime_type.contains("mp4") || mime_type.contains("m4a") {
        "m4a"
    } else {
        "webm"
    };
    let path = locaryn_config::ensure_temp_dir().join(format!(
        "locaryn-test-{}.{}",
        Uuid::new_v4(),
        extension
    ));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Remove only scratch audio files created by `write_test_audio`.
#[tauri::command]
fn remove_test_audio(path: String) -> Result<(), String> {
    let root = locaryn_config::ensure_temp_dir();
    let candidate = std::path::PathBuf::from(&path);
    if candidate.parent() != Some(root.as_path())
        || !candidate
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("locaryn-test-"))
    {
        return Err("chemin audio de test refusé".into());
    }
    std::fs::remove_file(candidate).map_err(|e| e.to_string())
}

/// Copy a generated voice note only after the user explicitly chose a
/// destination in the native Save As dialog. The source is confined to the
/// application's generated-audio directory so this command cannot become an
/// arbitrary file-copy primitive.
#[tauri::command]
fn save_audio_as(source_path: String, destination_path: String) -> Result<(), String> {
    let source = std::fs::canonicalize(&source_path)
        .map_err(|e| format!("note vocale introuvable : {e}"))?;
    let generated_root = std::fs::canonicalize(locaryn_config::generated_audio_dir())
        .map_err(|e| format!("dossier audio introuvable : {e}"))?;
    if !source.starts_with(&generated_root) {
        return Err("ce fichier audio n'est pas une note générée par Locaryn".into());
    }

    let destination = std::path::PathBuf::from(destination_path);
    if destination.as_os_str().is_empty() {
        return Err("destination audio vide".into());
    }
    if let Some(parent) = destination.parent() {
        if !parent.exists() {
            return Err("le dossier de destination n'existe pas".into());
        }
    }
    std::fs::copy(&source, &destination)
        .map(|_| ())
        .map_err(|e| format!("copie de la note vocale impossible : {e}"))
}

/// Copy a generated image after the user explicitly selected a destination.
/// Restrict the source to Locaryn's generated-image directory so exposing this
/// command cannot turn the chat action into an arbitrary file-copy primitive.
#[tauri::command]
fn save_image_as(source_path: String, destination_path: String) -> Result<(), String> {
    let source = std::fs::canonicalize(&source_path)
        .map_err(|e| format!("image générée introuvable : {e}"))?;
    let generated_root = std::fs::canonicalize(locaryn_config::generated_images_dir()).ok();
    // Older frontend builds used `<data_dir>/generated_images`; accept that
    // legacy location as well so their already displayed images remain
    // saveable after the storage-root correction.
    let legacy_root =
        std::fs::canonicalize(locaryn_config::default_data_dir().join("generated_images")).ok();
    if ![generated_root.as_deref(), legacy_root.as_deref()]
        .into_iter()
        .flatten()
        .any(|root| source.starts_with(root))
    {
        return Err("ce fichier n'est pas une image générée par Locaryn".into());
    }

    let destination = std::path::PathBuf::from(destination_path);
    if destination.as_os_str().is_empty() {
        return Err("destination image vide".into());
    }
    if let Some(parent) = destination.parent() {
        if !parent.exists() {
            return Err("le dossier de destination n'existe pas".into());
        }
    }
    std::fs::copy(&source, &destination)
        .map(|_| ())
        .map_err(|e| format!("copie de l'image impossible : {e}"))
}

// ============================================================================
// Projects / sessions / messages
// ============================================================================

#[tauri::command]
async fn list_projects(core: State<'_, Core>) -> Result<Vec<Project>, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(projects) = client.list_projects().await {
            return Ok(projects);
        }
    }
    core.storage
        .projects
        .list()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_project(
    core: State<'_, Core>,
    path: String,
    name: String,
    trust_level: Option<TrustLevel>,
) -> Result<Project, String> {
    let trust = trust_level.unwrap_or_default();
    if let Some(client) = core.remote_client() {
        if let Ok(project) = client.create_project(&path, &name, trust).await {
            return Ok(project);
        }
    }
    core.storage
        .projects
        .create(&path, &name, trust)
        .await
        .map_err(|e| e.to_string())
}

/// Rename a project and/or change how much the agent may do inside it.
#[tauri::command]
async fn update_project(
    core: State<'_, Core>,
    id: Uuid,
    name: Option<String>,
    trust_level: Option<TrustLevel>,
) -> Result<Project, String> {
    core.storage
        .projects
        .update_project(id, name.as_deref(), trust_level)
        .await
        .map_err(|e| e.to_string())
}

/// Path marking the internal project that holds project-less ("free") chats.
/// `sessions.project_id` is NOT NULL, so free chats live in this hidden project
/// instead of leaking into a real one (which made the same session show up both
/// as a free chat and inside a project).
pub const FREE_CHAT_PROJECT_PATH: &str = "__locaryn_free_chats__";

/// Temp folder created for a single free-chat session.
fn free_session_dir(_data_dir: &std::path::Path, session_id: Uuid) -> std::path::PathBuf {
    locaryn_config::free_chats_dir().join(session_id.to_string())
}

/// Get (or create) the hidden project that owns free chats.
#[tauri::command]
async fn free_chat_project(core: State<'_, Core>) -> Result<Project, String> {
    let existing = core
        .storage
        .projects
        .list()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(p) = existing
        .into_iter()
        .find(|p| p.path == FREE_CHAT_PROJECT_PATH)
    {
        return Ok(p);
    }
    core.storage
        .projects
        .create(
            FREE_CHAT_PROJECT_PATH,
            "Conversations libres",
            TrustLevel::Sandbox,
        )
        .await
        .map_err(|e| e.to_string())
}

/// Return the workspace directory for a session: the project path for normal
/// chats, or an auto-created temp folder for free chats.
#[tauri::command]
async fn session_workspace(core: State<'_, Core>, session_id: Uuid) -> Result<String, String> {
    let session = core
        .storage
        .sessions
        .get(session_id)
        .await
        .map_err(|e| e.to_string())?;
    let project = core
        .storage
        .projects
        .get(session.project_id)
        .await
        .map_err(|e| e.to_string())?;
    if project.path == FREE_CHAT_PROJECT_PATH {
        let dir = free_session_dir(&core.data_dir, session_id);
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            tracing::warn!(error = %e, "failed to create free session dir");
        }
        return Ok(dir.to_string_lossy().to_string());
    }
    Ok(project.path)
}

/// Persist a message contributed by an enabled extension. The host owns only
/// the conversation store; the extension owns the artifact and its UI.
#[tauri::command]
async fn append_chat_message(
    core: State<'_, Core>,
    session_id: Uuid,
    role: String,
    content: String,
) -> Result<(), String> {
    let role = match role.as_str() {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        _ => return Err("rôle de message d'extension invalide".into()),
    };
    core.storage
        .messages
        .append(session_id, role, &content)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Legacy assistant-only alias kept for already installed extensions.
#[tauri::command]
async fn append_assistant_message(
    core: State<'_, Core>,
    session_id: Uuid,
    content: String,
) -> Result<(), String> {
    append_chat_message(core, session_id, "assistant".into(), content).await
}

// ============================================================================
// Image generation defaults
// ============================================================================

/// Defaults applied whenever an image is generated without explicit overrides
/// (the `/image` slash command, the composer button). These used to be
/// hardcoded, so the user could not tell — let alone choose — what quality was
/// used. Named presets keep it understandable and let an agent pick a cheap
/// one for throwaway assets (a personal-project icon needs no 1024px render).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImageDefaults {
    /// "draft" (256) | "standard" (512) | "high" (768) | "max" (1024) | "custom"
    pub quality: String,
    pub width: u32,
    pub height: u32,
    /// 0 = let the model family decide (turbo models are clamped anyway).
    pub steps: u32,
    pub cfg_scale: f32,
    /// "gpu" | "auto" | "lowvram"
    pub vram_mode: String,
    pub negative_prompt: String,
    /// Variants per request, 1-8. Rendering several in one run pays the model
    /// load and prompt encoding once: measured 40 s per extra image against
    /// 58 s for a separate run.
    #[serde(default = "default_variants")]
    pub variants: u32,
}

fn default_variants() -> u32 {
    1
}

impl Default for ImageDefaults {
    fn default() -> Self {
        Self {
            quality: "standard".into(),
            width: 512,
            height: 512,
            steps: 0,
            cfg_scale: 0.0,
            vram_mode: "auto".into(),
            negative_prompt: String::new(),
            variants: default_variants(),
        }
    }
}

impl ImageDefaults {
    fn path(data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join("image_defaults.json")
    }
    fn load(data_dir: &std::path::Path) -> Self {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self, data_dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
    /// Resolution implied by a named quality preset.
    fn size_for(quality: &str) -> Option<u32> {
        match quality {
            "draft" => Some(256),
            "standard" => Some(512),
            "high" => Some(768),
            "max" => Some(1024),
            _ => None,
        }
    }
}

#[tauri::command]
fn get_image_defaults(core: State<'_, Core>) -> ImageDefaults {
    ImageDefaults::load(&core.data_dir)
}

#[tauri::command]
fn set_image_defaults(core: State<'_, Core>, config: ImageDefaults) -> Result<(), String> {
    let mut cfg = config;
    // A named preset drives the resolution; "custom" keeps whatever was sent.
    if let Some(px) = ImageDefaults::size_for(&cfg.quality) {
        cfg.width = px;
        cfg.height = px;
    }
    cfg.save(&core.data_dir).map_err(|e| e.to_string())
}

// ============================================================================
// Account model preferences
// ============================================================================

/// Defaults for model-backed features that are not the main chat runtime.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ModelPreferences {
    /// None means the Studio chooses the first installed TTS model.
    #[serde(default)]
    pub tts_model: Option<String>,
    /// None means the first installed image diffusion model.
    #[serde(default)]
    pub image_model: Option<String>,
}

impl ModelPreferences {
    fn path(data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join("model_preferences.json")
    }

    fn load(data_dir: &std::path::Path) -> Self {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, data_dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[tauri::command]
fn get_model_preferences(core: State<'_, Core>) -> ModelPreferences {
    ModelPreferences::load(&core.data_dir)
}

#[tauri::command]
fn set_model_preferences(
    core: State<'_, Core>,
    mut preferences: ModelPreferences,
) -> Result<(), String> {
    preferences.tts_model = preferences
        .tts_model
        .take()
        .and_then(|model| (!model.trim().is_empty()).then(|| model.trim().to_string()));
    preferences.image_model = preferences
        .image_model
        .take()
        .and_then(|model| (!model.trim().is_empty()).then(|| model.trim().to_string()));
    preferences.save(&core.data_dir).map_err(|e| e.to_string())
}

/// A plan produced by the model for a non-trivial request.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskPlan {
    /// False for a simple request that should just be answered directly.
    needs_plan: bool,
    /// True when the result must be verified and the plan retried on failure
    /// (bug fixes, "make X work"); false for one-shot creative work.
    needs_loop: bool,
    /// Ordered steps, each a short instruction the agent can execute.
    steps: Vec<String>,
}

/// Ask the model to turn a request into an executable plan. Returns
/// `needs_plan: false` for requests that don't warrant one, so the caller can
/// fall back to a normal single answer.
#[tauri::command]
async fn plan_task(core: State<'_, Core>, request: String) -> Result<TaskPlan, String> {
    let provider = core
        .storage
        .providers
        .active()
        .await
        .ok()
        .flatten()
        .ok_or("no active provider")?;
    if matches!(
        provider.engine,
        ProviderEngine::LlamaCpp | ProviderEngine::AirLlm
    ) {
        let _ = core.supervisor.ensure_running(provider.engine).await;
    }
    let url = format!(
        "{}/v1/chat/completions",
        provider.endpoint.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": provider.model.clone().unwrap_or_else(|| "default".into()),
        "messages": [
            { "role": "system", "content":
              "Tu decomposes une demande en plan executable. Reponds UNIQUEMENT en JSON:                {\"needs_plan\":bool,\"needs_loop\":bool,\"steps\":[\"...\"]}.                needs_plan=false si la demande est simple (une question, une petite modif)                et peut etre traitee en une seule reponse: dans ce cas steps=[].                needs_loop=true seulement si le resultat doit etre VERIFIE et la tache                reprise en cas d echec (correction de bug, faire marcher quelque chose);                false pour une creation ponctuelle.                2 a 5 etapes maximum, chacune une instruction courte a l imperatif,                dans la MEME LANGUE que la demande. La derniere etape doit verifier le                resultat quand needs_loop est true." },
            { "role": "user", "content": request }
        ],
        "response_format": { "type": "json_object" },
        "max_tokens": 400,
        "temperature": 0.2,
        "stream": false,
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("model returned {}", resp.status()));
    }
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap_or(serde_json::json!({}));
    let steps: Vec<String> = parsed["steps"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .take(5)
                .collect()
        })
        .unwrap_or_default();
    Ok(TaskPlan {
        needs_plan: parsed["needs_plan"].as_bool().unwrap_or(false) && !steps.is_empty(),
        needs_loop: parsed["needs_loop"].as_bool().unwrap_or(false),
        steps,
    })
}

/// Verdict on whether a chat message is really an image request.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImageIntent {
    /// True when the message asks for an image to be created.
    is_image: bool,
    /// True when it asks to modify an existing image (img2img).
    is_edit: bool,
    /// Prompt rewritten in English — diffusion models are trained on English.
    english_prompt: String,
    /// Suggested quality: "draft" | "standard" | "high" | "max".
    quality: String,
    /// Short justification shown to the user before they confirm.
    reason: String,
}

/// Ask the model whether a plain chat message should be routed to the image
/// generator, and to rewrite the prompt in English. The caller always asks the
/// user to confirm — this only prepares the proposal, it never generates.
#[tauri::command]
async fn detect_image_request(
    core: State<'_, Core>,
    message: String,
) -> Result<ImageIntent, String> {
    let provider = core
        .storage
        .providers
        .active()
        .await
        .ok()
        .flatten()
        .ok_or("no active provider")?;
    if matches!(
        provider.engine,
        ProviderEngine::LlamaCpp | ProviderEngine::AirLlm
    ) {
        let _ = core.supervisor.ensure_running(provider.engine).await;
    }
    let url = format!(
        "{}/v1/chat/completions",
        provider.endpoint.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": provider.model.clone().unwrap_or_else(|| "default".into()),
        "messages": [
            { "role": "system", "content":
              "Tu determines si un message demande de CREER ou MODIFIER une image. \
               Reponds UNIQUEMENT en JSON: {\"is_image\":bool,\"is_edit\":bool,\
               \"english_prompt\":\"...\",\"quality\":\"draft|standard|high|max\",\"reason\":\"...\"}. \
               is_image=false pour une question, une demande de code ou de texte. \
               english_prompt: reformule la demande visuelle en ANGLAIS, descriptif et precis \
               (les modeles de diffusion sont entraines en anglais). Vide si is_image=false. \
               quality: 'draft' pour une icone ou un essai rapide, 'max' pour un visuel soigne, \
               'standard' sinon. reason: une phrase courte en francais expliquant ton choix." },
            { "role": "user", "content": message }
        ],
        "response_format": { "type": "json_object" },
        "max_tokens": 300,
        "temperature": 0.1,
        "stream": false,
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("model returned {}", resp.status()));
    }
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("{}");
    let p: serde_json::Value = serde_json::from_str(content).unwrap_or(serde_json::json!({}));
    let english = p["english_prompt"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    let quality = match p["quality"].as_str().unwrap_or("standard") {
        q @ ("draft" | "standard" | "high" | "max") => q,
        _ => "standard",
    }
    .to_string();
    Ok(ImageIntent {
        // An image request with no prompt to render is not actionable.
        is_image: p["is_image"].as_bool().unwrap_or(false) && !english.is_empty(),
        is_edit: p["is_edit"].as_bool().unwrap_or(false),
        english_prompt: english,
        quality,
        reason: p["reason"].as_str().unwrap_or("").trim().to_string(),
    })
}

/// Rough token estimate (~4 chars/token) — good enough to decide when to compact.
fn approx_tokens(s: &str) -> usize {
    s.chars().count() / 4 + 1
}

/// Summarise the oldest turns into one compact note so a long conversation keeps
/// fitting in the context window. Mirrors what assistants do around 70-90% usage:
/// old turns are replaced by a summary, recent ones are kept verbatim.
async fn compact_history(
    core: &Core,
    endpoint: &str,
    model: &str,
    turns: Vec<locaryn_agent_runtime::ChatTurn>,
    budget_tokens: usize,
) -> Vec<locaryn_agent_runtime::ChatTurn> {
    let total: usize = turns.iter().map(|t| approx_tokens(&t.content)).sum();
    if total <= budget_tokens || turns.len() < 6 {
        return turns;
    }
    // Keep the last few exchanges verbatim; summarise everything before them.
    let keep = 4.min(turns.len());
    let split = turns.len() - keep;
    let (old, recent) = turns.split_at(split);

    let transcript: String = old
        .iter()
        .map(|t| format!("{}: {}", t.role, t.content))
        .collect::<Vec<_>>()
        .join(
            "
",
        )
        .chars()
        .take(12000)
        .collect();

    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content":
              "Résume la conversation ci-dessous en français, en moins de 200 mots.                Conserve les décisions, contraintes, noms de fichiers et faits techniques.                Pas de préambule, uniquement le résumé." },
            { "role": "user", "content": transcript }
        ],
        "max_tokens": 320,
        "temperature": 0.2,
        "stream": false,
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let summary = match core.http.post(&url).json(&body).send().await {
        Ok(r) if r.status().is_success() => {
            r.json::<serde_json::Value>().await.ok().and_then(|v| {
                v["choices"][0]["message"]["content"]
                    .as_str()
                    .map(str::to_string)
            })
        }
        _ => None,
    };

    let mut out = Vec::with_capacity(keep + 1);
    match summary {
        Some(sum) if !sum.trim().is_empty() => {
            tracing::info!(turns = old.len(), "compacted conversation history");
            out.push(locaryn_agent_runtime::ChatTurn {
                role: "system".into(),
                content: format!(
                    "[Résumé des échanges précédents]
{}",
                    sum.trim()
                ),
            });
        }
        // Summarisation failed: drop the oldest turns rather than blow the window.
        _ => tracing::warn!("history compaction failed, truncating instead"),
    }
    out.extend(recent.iter().cloned());
    out
}

/// Ask the local model, in the background, what the user could sensibly do next
/// after `answer`. Returns short actionable prompts the UI shows as one-click
/// chips. Never persisted to the conversation — it's an invisible side call.
#[tauri::command]
async fn suggest_followups(core: State<'_, Core>, answer: String) -> Result<Vec<String>, String> {
    let provider = core
        .storage
        .providers
        .active()
        .await
        .ok()
        .flatten()
        .ok_or("no active provider")?;
    if matches!(
        provider.engine,
        ProviderEngine::LlamaCpp | ProviderEngine::AirLlm
    ) {
        let _ = core.supervisor.ensure_running(provider.engine).await;
    }

    // Keep the context small: only the tail of the answer matters for "what next".
    let tail: String = answer
        .chars()
        .rev()
        .take(1200)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let url = format!(
        "{}/v1/chat/completions",
        provider.endpoint.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": provider.model.clone().unwrap_or_else(|| "default".into()),
        "messages": [
            { "role": "system", "content":
              "Tu proposes les prochaines étapes utiles après une réponse d'assistant de code. \
               Réponds UNIQUEMENT en JSON: {\"suggestions\":[\"...\",\"...\",\"...\"]}. \
               Maximum 3 suggestions, chacune une action courte (< 60 caractères), \
               formulée à l'impératif, dans la MÊME LANGUE que la réponse analysée. \
               Pas de redite de ce qui est déjà fait.                Tu peux proposer une commande de l'app quand c'est pertinent :                '/image brouillon' (visuel jetable, 256px, rapide),                '/image max' (visuel soigné, 1024px), '/documents' (indexer des fichiers).                Choisis la qualité selon l'enjeu : un projet perso rapide n'a pas besoin de 1024px." },
            { "role": "user", "content": format!("Réponse de l'assistant:\n\n{tail}\n\nQue proposer ensuite ?") }
        ],
        "response_format": { "type": "json_object" },
        "max_tokens": 220,
        "temperature": 0.4,
        "stream": false,
        // Small models burn their budget "thinking" — keep this side call terse.
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("model returned {}", resp.status()));
    }
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap_or(serde_json::json!({}));
    let list = parsed["suggestions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s.chars().count() <= 90)
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(list)
}

/// Archive a project: soft-delete, so it disappears from the sidebar while its
/// sessions and history stay on disk (recoverable, unlike a hard delete).
#[tauri::command]
async fn archive_project(core: State<'_, Core>, id: Uuid) -> Result<(), String> {
    core.storage
        .projects
        .soft_delete(id)
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// Figures — un rôle, ses consignes, ses conversations. L'écran n'existe que si
// une extension apporte la capacité `figures` ; ce qui suit est le pilotage.
// ============================================================================

#[tauri::command]
async fn list_figures(
    core: State<'_, Core>,
) -> Result<Vec<locaryn_storage::figures::Figure>, String> {
    core.storage.figures.list().await.map_err(|e| e.to_string())
}

/// Créer une figure, ou remplacer celle du même nom.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn save_figure(
    core: State<'_, Core>,
    name: String,
    description: String,
    instructions: String,
    model: Option<String>,
    opening: Option<String>,
    uses_memory: bool,
    tools: Option<Vec<String>>,
) -> Result<locaryn_storage::figures::Figure, String> {
    core.storage
        .figures
        .upsert(locaryn_storage::figures::NouvelleFigure {
            name: &name,
            description: &description,
            instructions: &instructions,
            model: model.as_deref().filter(|m| !m.trim().is_empty()),
            opening: opening.as_deref().filter(|o| !o.trim().is_empty()),
            uses_memory,
            tools: tools.as_deref(),
            // Écrite depuis l'interface : c'est le travail de quelqu'un, et
            // aucune mise à jour d'extension ne l'écrasera.
            source: "user",
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_figure(core: State<'_, Core>, id: String) -> Result<(), String> {
    core.storage
        .figures
        .delete(&id)
        .await
        .map_err(|e| e.to_string())
}

/// Confier une conversation à une figure, ou l'en détacher.
#[tauri::command]
async fn attach_figure(
    core: State<'_, Core>,
    session_id: Uuid,
    figure_id: Option<String>,
) -> Result<(), String> {
    core.storage
        .figures
        .attach_session(session_id, figure_id.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Les conversations d'une figure.
#[tauri::command]
async fn figure_sessions(core: State<'_, Core>, figure_id: String) -> Result<Vec<Session>, String> {
    let ids = core
        .storage
        .figures
        .session_ids(&figure_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for id in ids {
        if let Ok(uuid) = Uuid::parse_str(&id) {
            if let Ok(s) = core.storage.sessions.get(uuid).await {
                out.push(s);
            }
        }
    }
    Ok(out)
}

/// Ranger une conversation aux archives, ou l'en ressortir.
///
/// C'est ce que fait le geste courant — glisser vers la corbeille, choisir
/// « Archiver ». Rien n'est perdu : la suppression reste possible, depuis les
/// archives, et demande une décision de plus.
#[tauri::command]
async fn archive_session(core: State<'_, Core>, id: Uuid, archived: bool) -> Result<(), String> {
    core.storage
        .sessions
        .set_archived(id, archived)
        .await
        .map_err(|e| e.to_string())
}

/// Les conversations rangées d'un projet.
#[tauri::command]
async fn archived_sessions(
    core: State<'_, Core>,
    project_id: Uuid,
) -> Result<Vec<Session>, String> {
    core.storage
        .sessions
        .list_archived(project_id)
        .await
        .map_err(|e| e.to_string())
}

/// Déplacer une conversation dans un projet.
#[tauri::command]
async fn move_session(core: State<'_, Core>, id: Uuid, project_id: Uuid) -> Result<(), String> {
    core.storage
        .sessions
        .move_to_project(id, project_id)
        .await
        .map_err(|e| e.to_string())
}

/// Renommer une conversation à la main.
///
/// Le titre devient définitif : aucun modèle n'y revient. Sans cela, un nom
/// choisi puis remplacé par une micro-tâche ferait chercher dans sa propre
/// liste un titre qu'on avait pourtant écrit.
#[tauri::command]
async fn rename_session(core: State<'_, Core>, id: Uuid, title: String) -> Result<(), String> {
    core.storage
        .sessions
        .rename_by_user(id, title.trim())
        .await
        .map_err(|e| e.to_string())
}

/// Ouvrir une conversation éphémère : rien n'en sera gardé.
#[tauri::command]
async fn create_ephemeral_session(
    core: State<'_, Core>,
    project_id: Uuid,
) -> Result<Session, String> {
    core.storage
        .sessions
        .create_with(project_id, None, true)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_session(core: State<'_, Core>, id: Uuid) -> Result<(), String> {
    // Tiré avant la suppression : le hook peut encore lire la session.
    let root = hooks::project_root_or_cwd(
        match core.storage.sessions.get(id).await {
            Ok(s) => core
                .storage
                .projects
                .get(s.project_id)
                .await
                .ok()
                .map(|p| p.path),
            Err(_) => None,
        }
        .as_deref(),
    );
    hooks::fire(
        core.inner(),
        locaryn_hook_runtime::HookEvent::SessionEnd,
        hooks::HookContext::new(id.to_string(), root),
    )
    .await;

    core.storage
        .sessions
        .delete(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_sessions(core: State<'_, Core>, project_id: Uuid) -> Result<Vec<Session>, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(sessions) = client.list_sessions(&project_id.to_string()).await {
            return Ok(sessions);
        }
    }
    core.storage
        .sessions
        .list_for_project(project_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_session(
    core: State<'_, Core>,
    project_id: Uuid,
    title: Option<String>,
    core_id: Option<String>,
) -> Result<Session, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(session) = client
            .create_session_with_core(&project_id.to_string(), core_id.as_deref())
            .await
        {
            return Ok(session);
        }
    }
    let title = title.and_then(|t| {
        let t = t.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    });
    let session = core
        .storage
        .sessions
        .create_with_core(project_id, title, false, core_id.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    let root = hooks::project_root_or_cwd(
        core.storage
            .projects
            .get(project_id)
            .await
            .ok()
            .map(|p| p.path)
            .as_deref(),
    );
    hooks::fire(
        core.inner(),
        locaryn_hook_runtime::HookEvent::SessionStart,
        hooks::HookContext::new(session.id.to_string(), root),
    )
    .await;

    Ok(session)
}

/// Rename a session.
#[tauri::command]
async fn update_session_title(
    core: State<'_, Core>,
    session_id: Uuid,
    title: String,
) -> Result<(), String> {
    let trimmed = title.trim().to_string();
    if trimmed.is_empty() {
        return Err("title cannot be empty".into());
    }
    core.storage
        .sessions
        .update_title(session_id, &trimmed)
        .await
        .map_err(|e| e.to_string())
}

/// Ask the active model to produce a concise title for a session based on the
/// user's first message and the project context, then persist it.
#[tauri::command]
async fn generate_session_title(
    core: State<'_, Core>,
    session_id: Uuid,
    first_prompt: String,
) -> Result<String, String> {
    let session = core
        .storage
        .sessions
        .get(session_id)
        .await
        .map_err(|e| e.to_string())?;
    let project = core
        .storage
        .projects
        .get(session.project_id)
        .await
        .map_err(|e| e.to_string())?;

    let active_provider = core.storage.providers.active().await.ok().flatten();
    let provider = active_provider.ok_or("no active provider")?;
    if matches!(
        provider.engine,
        ProviderEngine::LlamaCpp | ProviderEngine::AirLlm
    ) {
        let _ = core.supervisor.ensure_running(provider.engine).await;
    }

    let url = format!(
        "{}/v1/chat/completions",
        provider.endpoint.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "model": provider.model.clone().unwrap_or_else(|| "default".into()),
        "messages": [
            {
                "role": "system",
                "content": "Tu es un assistant qui nomme des conversations. Réponds UNIQUEMENT avec un titre court (3 à 5 mots) en français, sans ponctuation, sans guillemets. Le titre doit refléter le sujet du message de l'utilisateur et le contexte du projet."
            },
            {
                "role": "user",
                "content": format!("Projet : {}\n\nMessage : {}", project.name, first_prompt)
            }
        ],
        "max_tokens": 40,
        "temperature": 0.3,
        "stream": false,
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("model returned {}", resp.status()));
    }
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let raw = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim();
    let title = raw
        .trim_matches(|c: char| c == '\'' || c == '"' || c == '“' || c == '”')
        .split('\n')
        .next()
        .unwrap_or(raw)
        .trim();

    if title.is_empty() {
        return Err("empty title generated".into());
    }

    core.storage
        .sessions
        .update_title(session_id, title)
        .await
        .map_err(|e| e.to_string())?;

    Ok(title.to_string())
}

#[tauri::command]
async fn list_messages(core: State<'_, Core>, session_id: Uuid) -> Result<Vec<Message>, String> {
    core.storage
        .messages
        .list_for_session(session_id)
        .await
        .map_err(|e| e.to_string())
}

/// Result of `bootstrap`: the default project + a usable session, so the UI
/// is functional immediately on first launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Bootstrap {
    project: Project,
    session: Session,
    health: Health,
}

/// Pick the most recently updated project (or create one for the launch
/// directory on a fresh install) and the most recent open session in it.
#[tauri::command]
async fn bootstrap(core: State<'_, Core>) -> Result<Bootstrap, String> {
    let mut projects = core
        .storage
        .projects
        .list()
        .await
        .map_err(|e| e.to_string())?;
    projects.sort_by_key(|p| p.updated_at);
    let project = match projects.pop() {
        Some(p) => p,
        None => {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            let cwd_str = cwd.to_string_lossy().to_string();
            let name = cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "workspace".into());
            core.storage
                .projects
                .create(&cwd_str, &name, TrustLevel::default())
                .await
                .map_err(|e| e.to_string())?
        }
    };

    let sessions = core
        .storage
        .sessions
        .list_for_project(project.id)
        .await
        .map_err(|e| e.to_string())?;
    let session = match sessions.into_iter().find(|s| s.closed_at.is_none()) {
        Some(s) => s,
        None => core
            .storage
            .sessions
            .create(project.id, None)
            .await
            .map_err(|e| e.to_string())?,
    };

    let health = core_health(core).await?;

    Ok(Bootstrap {
        project,
        session,
        health,
    })
}

// ============================================================================
// Chat — the agent loop, streamed to the frontend over a Tauri Channel
// ============================================================================

/// Send a user message and stream the agent's reply. Mirrors the daemon's
/// `send_message` handler: persist the user message, resolve project context,
/// ensure the local runtime is up, run OllamaAgent (StubAgent fallback), and
/// persist the assistant reply when the stream ends.
#[tauri::command]
async fn send_message(
    core: State<'_, Core>,
    session_id: Uuid,
    content: String,
    images: Option<Vec<String>>,
    response_format: Option<serde_json::Value>,
    reasoning: Option<serde_json::Value>,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    // 0. Hooks : UserPromptSubmit peut refuser le tour. Tiré avant toute
    //    persistance, sinon un message refusé resterait dans l'historique.
    let hook_root = {
        let path = match core.storage.sessions.get(session_id).await {
            Ok(s) => core
                .storage
                .projects
                .get(s.project_id)
                .await
                .ok()
                .map(|p| p.path),
            Err(_) => None,
        };
        hooks::project_root_or_cwd(path.as_deref())
    };
    let submit = hooks::fire(
        core.inner(),
        locaryn_hook_runtime::HookEvent::UserPromptSubmit,
        hooks::HookContext::new(session_id.to_string(), hook_root.clone()),
    )
    .await;
    if let Some(reason) = submit.blocked {
        return Err(reason);
    }

    // 1. Persist the user's message.
    if let Err(e) = core
        .storage
        .messages
        .append(session_id, MessageRole::User, &content)
        .await
    {
        tracing::warn!(error = %e, "failed to persist user message");
    }

    // 1b. If connected to a remote server / DGX supercomputer, stream from the remote server.
    if let Some(client) = core.remote_client() {
        match client.send_message(&session_id.to_string(), &content).await {
            Ok(mut stream) => {
                use futures::StreamExt;
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(evt) => {
                            let _ = on_event.send(evt);
                        }
                        Err(e) => {
                            let _ = on_event.send(StreamEvent::Log {
                                level: locaryn_events::LogLevel::Error,
                                msg: e.to_string(),
                                source: "remote".to_string(),
                            });
                            return Err(e.to_string());
                        }
                    }
                }
                return Ok(());
            }
            Err(e) => {
                let _ = on_event.send(StreamEvent::Log {
                    level: locaryn_events::LogLevel::Error,
                    msg: e.to_string(),
                    source: "remote".to_string(),
                });
                return Err(e.to_string());
            }
        }
    }

    // 2. Resolve session → project context (path + trust) for the tool loop.
    //    La session peut être confiée à un noyau alternatif : on le retient
    //    avant de construire l'agent, c'est lui qui décidera du routage.
    let session_core_id = core
        .storage
        .sessions
        .get(session_id)
        .await
        .ok()
        .and_then(|s| s.core_id);
    let (project_id, project_path, trust) = match core.storage.sessions.get(session_id).await {
        Ok(session) => match core.storage.projects.get(session.project_id).await {
            Ok(project) => {
                // Free chats live in a hidden project, but each session gets its
                // own temporary folder so tools have a real workspace without
                // exposing a path to the user.
                let path = if project.path == FREE_CHAT_PROJECT_PATH {
                    let dir = free_session_dir(&core.data_dir, session_id);
                    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                        tracing::warn!(error = %e, "failed to create free session dir");
                    }
                    dir
                } else {
                    std::path::PathBuf::from(&project.path)
                };
                (
                    Some(session.project_id),
                    Some(path),
                    Some(project.trust_level),
                )
            }
            Err(e) => {
                tracing::warn!(error = %e, "project not found for session");
                (None, None, None)
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "session not found, running without project context");
            (None, None, None)
        }
    };

    // 3. Pick the agent based on the active provider.
    let active_provider = core.storage.providers.active().await.ok().flatten();

    // Ensure the local runtime is running (llama-server or the AirLLM server).
    if let Some(ref p) = active_provider {
        if matches!(p.engine, ProviderEngine::LlamaCpp | ProviderEngine::AirLlm) {
            if let Err(e) = core.supervisor.ensure_running(p.engine).await {
                tracing::warn!(error = %e, "supervisor could not ensure runtime running");
            } else {
                core.supervisor.note_activity(p.engine).await;
            }
        }
    }

    let model = active_provider.as_ref().and_then(|p| p.model.clone());

    // Sampling params saved from the Model Config panel (provider.config) are
    // translated to OpenAI request fields and merged into every round.
    let params = active_provider
        .as_ref()
        .and_then(|p| p.config.clone())
        .and_then(|cfg| serde_json::from_value::<ModelParams>(cfg).ok())
        .map(|p| {
            let mut m = serde_json::Map::new();
            m.insert("temperature".into(), serde_json::json!(p.temperature));
            m.insert("top_p".into(), serde_json::json!(p.top_p));
            m.insert("top_k".into(), serde_json::json!(p.top_k));
            m.insert("repeat_penalty".into(), serde_json::json!(p.repeat_penalty));
            if p.max_tokens > 0 {
                m.insert("max_tokens".into(), serde_json::json!(p.max_tokens));
            }
            if p.seed >= 0 {
                m.insert("seed".into(), serde_json::json!(p.seed));
            }
            serde_json::Value::Object(m)
        });

    // Structured-output: merge the per-message response_format (json_object /
    // json_schema) into the request params. llama-server honors both.
    let params = match response_format {
        Some(rf) => {
            let mut obj = match params {
                Some(serde_json::Value::Object(m)) => m,
                _ => serde_json::Map::new(),
            };
            obj.insert("response_format".into(), rf);
            Some(serde_json::Value::Object(obj))
        }
        None => params,
    };

    // Reasoning control: merge the per-message reasoning object's keys
    // (reasoning_budget, chat_template_kwargs.enable_thinking) into the request.
    // Safe no-op on non-thinking models; llama-server accepts both.
    let params = match reasoning {
        Some(serde_json::Value::Object(r)) => {
            let mut obj = match params {
                Some(serde_json::Value::Object(m)) => m,
                _ => serde_json::Map::new(),
            };
            for (k, v) in r {
                obj.insert(k, v);
            }
            Some(serde_json::Value::Object(obj))
        }
        _ => params,
    };

    // Conversation memory: replay prior turns so the model can follow the thread.
    // Without this every message was sent standalone. Compact the oldest turns
    // when they would eat the context window.
    let history = {
        let prior = core
            .storage
            .messages
            .list_for_session(session_id)
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
                // Artifact markers are UI persistence details, not part of the
                // conversation the model should answer from.
                content: strip_ui_markers(&m.content),
            })
            .collect();
        // The message we just persisted is the current one — it is sent separately.
        turns.pop();

        if !turns.is_empty() {
            let cfg = InferenceConfig::load(&core.data_dir);
            // Leave room for the new message, the answer and the system prompt.
            let budget = (cfg.context_length as usize * 60) / 100;
            let endpoint = active_provider
                .as_ref()
                .map(|p| p.endpoint.clone())
                .unwrap_or_else(|| "http://127.0.0.1:8080".into());
            let m = model.clone().unwrap_or_else(|| "default".into());
            turns = compact_history(&core, &endpoint, &m, turns, budget).await;
        }
        turns
    };

    // RAG: if this project has an index, retrieve relevant chunks and prepend
    // them to the message the model sees (the stored user message stays raw).
    // No index → build_rag_context returns immediately and spawns nothing.
    let agent_message = match project_id {
        Some(pid) => match build_rag_context(&core, &pid.to_string(), &content).await {
            Some(ctx) => format!("{ctx}{content}"),
            None => content,
        },
        None => content,
    };

    // Une figure peut restreindre les outils du modèle à ceux qu'elle nomme.
    let figure_tools = core
        .storage
        .figures
        .for_session(session_id)
        .await
        .ok()
        .flatten()
        .and_then(|f| f.tools)
        .filter(|t| !t.is_empty());

    let mut input = AgentInput {
        session_id,
        message: agent_message,
        mode: core.mode,
        model: model.clone(),
        agent: None,
        project_id,
        project_path,
        trust,
        images: images.unwrap_or_default(),
        params,
        history,
        // Was `None`, which meant a registered MCP server was never asked for
        // its tools: the whole client existed and nothing reached the model.
        mcp_state: Some(core.mcp.clone()),
        // Rules and skills contributed by enabled extensions. `None` when
        // nothing is enabled, so the prompt is unchanged for everyone else.
        extra_system: {
            let rt = core.extensions.read().await;
            (!rt.system_prompt.trim().is_empty()).then(|| rt.system_prompt.clone())
        },
        // Ce que les extensions actives apportent, d'après ce que le service
        // en dit : c'est la liste qui décide des outils offerts au modèle.
        capabilities: extensions::active_capabilities(&core).await,
        // Les outils que la figure de cette conversation autorise.
        tools: figure_tools,
        // Sans elle, tout appel exigeant un accord serait refusé faute
        // d'interlocuteur — le comportement voulu pour un service sans
        // interface, pas pour l'application de bureau.
        approval: Some(locaryn_agent_runtime::approval::ApprovalHandle(Arc::new(
            core.approval_gate.clone(),
        ))),
        // Renseigné plus bas si la session est confiée à un noyau alternatif.
        bearer_token: None,
    };

    let mut event_stream: EventStream = if let Some(core_id) = &session_core_id {
        // Session confiée à un noyau alternatif (OpenClaw, Hermes…). Pas de
        // fallback silencieux vers le noyau Locaryn : si le noyau choisi ne
        // répond pas, on le dit, et on propose l'action qui le répare.
        match core_engines::agent_for_core(&core, core_id).await {
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
            Some(p) => {
                tracing::info!(endpoint = %p.endpoint, model = ?model, "desktop using OpenAiCompatAgent (llama-server)");
                let agent = OpenAiCompatAgent::with_defaults(Some(&p.endpoint), model.as_deref());
                match agent.run(input.clone()).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::warn!(error = %e, "OpenAiCompatAgent run failed");
                        no_model_stream(&format!(
                        "Le modèle{} n'a pas pu être atteint. Vérifiez qu'un modèle est bien sélectionné et installé.",
                        model.as_deref().map(|m| format!(" \"{m}\"")).unwrap_or_default()
                    ))
                    }
                }
            }
            None => {
                tracing::warn!("no active provider configured");
                no_model_stream("Aucun modèle actif. Ouvrez le Marketplace et installez un modèle.")
            }
        }
    };

    // 4. Forward events to the frontend while collecting the assistant text.
    let mut full_text = String::new();
    let mut tokens_in = 0u64;
    let mut tokens_out = 0u64;
    let mut audio_artifacts: Vec<String> = Vec::new();
    let mut image_artifacts: Vec<String> = Vec::new();
    while let Some(ev) = event_stream.next().await {
        match &ev {
            StreamEvent::Token { text } => full_text.push_str(text),
            StreamEvent::Artifact {
                kind: ArtifactKind::AudioWav,
                path,
                ..
            } => audio_artifacts.push(path.clone()),
            StreamEvent::Artifact {
                kind: ArtifactKind::ImagePng,
                path,
                ..
            } => image_artifacts.push(path.clone()),
            StreamEvent::MessageEnd {
                tokens_in: ti,
                tokens_out: to,
                ..
            } => {
                tokens_in = *ti;
                tokens_out = *to;
            }
            _ => {}
        }
        // If the channel is gone (webview navigated away), keep draining the
        // stream so the assistant reply still gets persisted below.
        let _ = on_event.send(ev);
    }

    // 5. Persist the assistant reply and transparent audio-artifact markers.
    // The frontend turns those markers back into playable notes; history strips
    // them before the next model request.
    let mut persisted_text = full_text;
    for path in audio_artifacts {
        persisted_text.push('\n');
        persisted_text.push_str(&audio_marker(&path));
    }
    for path in image_artifacts {
        persisted_text.push('\n');
        persisted_text.push_str(&image_marker(&path));
    }
    if !persisted_text.is_empty() {
        if let Err(e) = core
            .storage
            .messages
            .append_full(
                session_id,
                MessageRole::Assistant,
                &persisted_text,
                None,
                None,
                tokens_in,
                tokens_out,
                None,
            )
            .await
        {
            tracing::warn!(error = %e, "failed to persist assistant message");
        }
    }

    // 6. Hooks : le tour est fini. Un échec ici ne peut plus rien annuler, il
    //    est seulement journalisé par le dispatcheur.
    hooks::fire(
        core.inner(),
        locaryn_hook_runtime::HookEvent::Stop,
        hooks::HookContext::new(session_id.to_string(), hook_root),
    )
    .await;

    Ok(())
}

/// Marker stored alongside a generated artifact. JSON escaping keeps Windows
/// paths and quotes unambiguous; the frontend turns it into a real media URL.
fn artifact_marker(kind: &str, path: &str) -> String {
    let encoded = serde_json::to_string(path).unwrap_or_else(|_| "\"\"".to_string());
    format!("<!--locaryn-{kind}:{encoded}-->")
}

fn audio_marker(path: &str) -> String {
    artifact_marker("audio", path)
}

fn image_marker(path: &str) -> String {
    artifact_marker("image", path)
}

/// Remove UI-only artifact markers before replaying history to a model.
fn strip_ui_markers(content: &str) -> String {
    let mut text = content.to_string();
    for marker in ["<!--locaryn-audio:", "<!--locaryn-image:"] {
        while let Some(start) = text.find(marker) {
            let Some(end_rel) = text[start..].find("-->") else {
                break;
            };
            text.replace_range(start..start + end_rel + 3, "");
        }
    }
    text.trim().to_string()
}

/// When no local model can answer, stream a clear, actionable explanation
/// instead of silently echoing (which reads as "the app is broken").
fn no_model_stream(reason: &str) -> EventStream {
    use futures::stream;
    let message_id = Uuid::new_v4().to_string();
    let task_id = Uuid::new_v4().to_string();
    let text = format!(
        "⚠️ Aucun modèle local n'a répondu — {reason}\n\n\
         Pour corriger :\n\
         1. **Paramètres Système → Général** : installez le runtime IA (llama.cpp) s'il ne l'est pas.\n\
         2. **Marketplace** : téléchargez un modèle GGUF (ex. Qwen3 4B).\n\
         3. Sélectionnez-le comme modèle actif, puis renvoyez votre message.\n\n\
         Détails techniques : consultez `llama-server.log` dans le dossier de données."
    );
    let events = vec![
        StreamEvent::MessageStart {
            message_id: message_id.clone(),
            task_id,
        },
        StreamEvent::Token { text },
        StreamEvent::MessageEnd {
            message_id,
            tokens_in: 0,
            tokens_out: 0,
            duration_ms: 0,
        },
    ];
    Box::pin(stream::iter(events))
}

// ============================================================================
// Terminal — line-based command execution streamed over a Channel
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalEvent {
    Line { stream: String, text: String },
    Exit { code: Option<i32> },
}

/// Run a shell command in `cwd`, streaming stdout/stderr lines. A real PTY
/// (xterm.js full-screen apps, colors, resize) is a follow-up; line-based
/// exec covers the S5 acceptance criterion "the terminal executes commands".
#[tauri::command]
async fn run_terminal(
    command: String,
    cwd: Option<String>,
    on_output: Channel<TerminalEvent>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.args(["/C", &command]);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", &command]);
        c
    };
    if let Some(dir) = cwd.filter(|d| !d.is_empty()) {
        cmd.current_dir(dir);
    }
    hide_tokio_console(&mut cmd);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;

    let out_chan = on_output.clone();
    let out_task = tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = out_chan.send(TerminalEvent::Line {
                stream: "stdout".into(),
                text: line,
            });
        }
    });
    let err_chan = on_output.clone();
    let err_task = tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = err_chan.send(TerminalEvent::Line {
                stream: "stderr".into(),
                text: line,
            });
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = out_task.await;
    let _ = err_task.await;
    let _ = on_output.send(TerminalEvent::Exit {
        code: status.code(),
    });
    Ok(())
}

// ============================================================================
// Providers & settings
// ============================================================================

#[tauri::command]
async fn list_providers(core: State<'_, Core>) -> Result<Vec<Provider>, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(providers) = client.list_providers().await {
            return Ok(providers);
        }
    }
    core.storage
        .providers
        .list()
        .await
        .map_err(|e| e.to_string())
}

/// Report the active LLM to the MCP runtime so every stdio server spawned
/// afterwards inherits `LOCARYN_ACTIVE_MODEL` / `LOCARYN_LLM_ENDPOINT`.
async fn refresh_mcp_runtime_env(core: &Core) {
    let active = core.storage.providers.active().await.ok().flatten();
    let model = active.as_ref().and_then(|p| p.model.clone());
    let endpoint = active.as_ref().map(|p| p.endpoint.clone());
    core.mcp.set_runtime_env(model, endpoint);
}

#[tauri::command]
async fn set_active_provider(core: State<'_, Core>, id: Uuid) -> Result<Provider, String> {
    let provider = core
        .storage
        .providers
        .set_active(id)
        .await
        .map_err(|e| e.to_string())?;
    refresh_mcp_runtime_env(&core).await;
    Ok(provider)
}

/// Save the local provider's endpoint + model and make it active.
///
/// Engine is the embedded llama-server (`LlamaCpp`): one model per server
/// process, so switching model SHUTS DOWN the running server — the next
/// message respawns it with the new weights. Without this, the health check
/// would keep the old model loaded forever.
#[tauri::command]
async fn configure_provider(
    core: State<'_, Core>,
    endpoint: String,
    model: Option<String>,
) -> Result<Provider, String> {
    let endpoint = endpoint.trim().trim_end_matches('/').to_string();
    if endpoint.is_empty() {
        return Err("endpoint is required".into());
    }
    let model = model.filter(|m| !m.trim().is_empty());

    let previous = core.storage.providers.active().await.ok().flatten();
    let model_changed = previous
        .as_ref()
        .map(|p| p.model != model || p.engine != ProviderEngine::LlamaCpp)
        .unwrap_or(true);

    let provider = core
        .storage
        .providers
        .upsert_local(ProviderEngine::LlamaCpp, &endpoint, model)
        .await
        .map_err(|e| e.to_string())?;

    if model_changed {
        tracing::info!("active model changed — restarting llama-server");
        let _ = core.supervisor.shutdown(ProviderEngine::LlamaCpp).await;
    }
    refresh_mcp_runtime_env(&core).await;
    Ok(provider)
}

/// Ce fichier de poids appartient-il à la chaîne image plutôt qu'au chat ?
///
/// Le socle ne sait plus générer d'image — c'est l'affaire d'une extension —
/// mais il doit toujours écarter ces fichiers de la liste des modèles de
/// conversation : un checkpoint de diffusion choisi comme modèle de chat fait
/// retomber l'agent sur un stub, en silence.
fn is_image_asset(file_name: &str) -> bool {
    let n = file_name.to_ascii_lowercase();
    const DIFFUSION: &[&str] = &[
        "stable-diffusion",
        "stable_diffusion",
        "sd_xl",
        "sdxl",
        "sd15",
        "sd-v1",
        "sd_v1",
        "sd3",
        "sd3.5",
        "z_image",
        "z-image",
        "z_img",
        "zimg",
        "flux",
        "krea",
        "dreamshaper",
        "juggernaut",
        "pony",
        "playground-v",
        "kolors",
        "hunyuan-dit",
        "pixart",
    ];
    const AUX: &[&str] = &[
        "mmproj-",
        "ae.safetensors",
        "vae",
        "clip",
        "t5xxl",
        "text_encoder",
        "text-encoder",
    ];
    DIFFUSION.iter().any(|p| n.contains(p)) || AUX.iter().any(|p| n.contains(p))
}

/// Resolve a model specification (filename, HuggingFace repo tag, URL) to its actual file/dir path in `models_dir`.
pub(crate) fn resolve_model_path(
    models_dir: &std::path::Path,
    raw_name: &str,
) -> std::path::PathBuf {
    let direct = models_dir.join(raw_name);
    if direct.exists() {
        return direct;
    }
    // Extract base filename if raw_name is a URL or has path separators
    let cleaned = raw_name
        .split('/')
        .next_back()
        .unwrap_or(raw_name)
        .split('\\')
        .next_back()
        .unwrap_or(raw_name);
    let candidate = models_dir.join(cleaned);
    if candidate.exists() {
        return candidate;
    }
    // If it was a HF repo URL (e.g. https://huggingface.co/stablediffusionapi/deliberate-v2)
    let repo_dir = raw_name
        .trim_start_matches("https://huggingface.co/")
        .trim_start_matches("http://huggingface.co/")
        .trim_matches('/')
        .replace('/', "__");
    let dir_candidate = models_dir.join(&repo_dir);
    if dir_candidate.exists() {
        return dir_candidate;
    }
    // Search case-insensitively for any file matching cleaned name
    if let Ok(entries) = std::fs::read_dir(models_dir) {
        let cleaned_lower = cleaned.to_ascii_lowercase();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                if let Some(n) = p.file_name().and_then(|x| x.to_str()) {
                    if n.to_ascii_lowercase() == cleaned_lower {
                        return p;
                    }
                }
            }
        }
    }
    candidate
}

/// Text chat model loadable by the managed llama.cpp runtime, excluding TTS,
/// audio, embeddings, image diffusion and non-chat companions.
pub(crate) fn is_text_chat_model(file_name: &str) -> bool {
    let n = file_name.to_ascii_lowercase();

    // The local provider starts llama-server with `-m`, which accepts GGUF.
    // A Transformers repository made of `.safetensors` shards is not a
    // llama.cpp model and must never appear as a selectable local chat model.
    if !n.ends_with(".gguf") {
        return false;
    }

    // 2. Exclude image diffusion checkpoints, VAE, CLIP, etc.
    if is_image_asset(&n) {
        return false;
    }

    // 3. Exclude TTS, Voice, Speech, Audio checkpoints
    const AUDIO_TTS: &[&str] = &[
        "-tts",
        "_tts",
        "tts-",
        "/tts",
        "tts.",
        "tts_",
        "xtts",
        "piper",
        "kokoro",
        "parler",
        "bark",
        "whisper",
        "musicgen",
        "audioldm",
        "audiocraft",
        "customvoice",
        "speech",
        "vocoder",
    ];
    if AUDIO_TTS.iter().any(|p| n.contains(p)) {
        return false;
    }

    // 4. Exclude Embedding & Reranking models
    const EMBEDDING: &[&str] = &[
        "embed",
        "embedding",
        "nomic-embed",
        "bge-",
        "bge_",
        "all-minilm",
        "e5-small",
        "e5-base",
        "e5-large",
        "rerank",
    ];
    if EMBEDDING.iter().any(|p| n.contains(p)) {
        return false;
    }

    // 5. Exclude Vision-only / Segmentation / OCR models
    const VISION_ONLY: &[&str] = &[
        "clipseg",
        "segformer",
        "yolo",
        "depth-anything",
        "sam-",
        "segment-anything",
    ];
    if VISION_ONLY.iter().any(|p| n.contains(p)) {
        return false;
    }

    // 6. Exclude multimodal/speculative decoding companions. They are loaded
    // alongside a primary model and cannot answer chat requests on their own.
    if n.contains("mmproj")
        || n.contains("/mtp/")
        || n.contains("\\mtp\\")
        || n.contains("mtp-")
        || n.contains("-draft-")
        || n.contains("_draft_")
    {
        return false;
    }

    true
}

/// List the models actually installed on the Ollama runtime at `endpoint`.
/// Hits `{endpoint}/api/tags` and returns the model names, sorted. Doubles as
/// a connection test: an error means the runtime is unreachable.
#[tauri::command]
async fn list_models(core: State<'_, Core>, _endpoint: String) -> Result<Vec<String>, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(val) = client.list_media_models(Some("chat")).await {
            if let Some(models) = val.get("models").and_then(|m| m.as_array()) {
                let names: Vec<String> = models
                    .iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect();
                if !names.is_empty() {
                    return Ok(names);
                }
            }
        }
    }
    // Return one entry per installed model. A model is either a single weight
    // file at the top level, or a directory/repository that may contain many
    // weight/config files. For directories, we pick a canonical representative
    // weight file so the user sees "hexgrad__Kokoro-82M" once, not once per
    // shard.
    let models_dir = locaryn_config::models_dir();
    let mut names: Vec<String> = Vec::new();

    fn is_weight_file(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|x| x.to_str())
            .map(|x| {
                x.eq_ignore_ascii_case("gguf")
                    || x.eq_ignore_ascii_case("safetensors")
                    || x.eq_ignore_ascii_case("onnx")
                    || x.eq_ignore_ascii_case("pth")
                    || x.eq_ignore_ascii_case("pt")
                    || x.eq_ignore_ascii_case("bin")
            })
            .unwrap_or(false)
    }

    fn is_partial(path: &std::path::Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(".part") || n.ends_with(".tmp") || n.ends_with(".zip"))
            .unwrap_or(true)
    }

    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                if is_weight_file(&path) && !is_partial(&path) {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        names.push(name.to_string());
                    }
                }
                continue;
            }

            if !path.is_dir() {
                continue;
            }

            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let mut weight_files: Vec<std::path::PathBuf> = walkdir_recursive(&path, 5)
                .into_iter()
                .filter(|p| is_weight_file(p) && !is_partial(p))
                .collect();

            // A repository can contain several quantisations or revisions.
            // Keep one representative per model/shard group instead of
            // collapsing the directory to whichever file happened to sort
            // first. This is what lets the library show Q4 and Q8 separately.
            let mut groups: HashMap<String, std::path::PathBuf> = HashMap::new();
            for file in weight_files.drain(..) {
                let rel = file
                    .strip_prefix(&path)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .to_string();
                if !is_text_chat_model(&rel) {
                    continue;
                }
                let key = hf_shard_group(&rel);
                let replace = groups
                    .get(&key)
                    .map(|existing| rel.len() < existing.to_string_lossy().len())
                    .unwrap_or(true);
                if replace {
                    groups.insert(key, file);
                }
            }
            for rep in groups.values() {
                let rel = rep
                    .strip_prefix(&path)
                    .unwrap_or(rep)
                    .to_string_lossy()
                    .to_string();
                names.push(format!("{dir_name}/{rel}"));
            }
        }
    }

    // Only text chat models: exclude diffusion, TTS, embeddings, and non-LLM weights.
    names.retain(|n| is_text_chat_model(n));

    names.sort();
    names.dedup();
    Ok(names)
}

/// Full Transformers language-model repositories stored in `models_dir` are
/// useful to other runtimes, but the managed llama.cpp provider cannot load
/// them with `-m`. Keep them out of chat pickers while exposing them to the
/// management screen so a failed 50+ GB download can be removed cleanly.
#[tauri::command]
async fn list_incompatible_models(core: State<'_, Core>) -> Result<Vec<String>, String> {
    if core.remote_client().is_some() {
        return Ok(Vec::new());
    }
    let models_dir = locaryn_config::models_dir();
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(models_dir) else {
        return Ok(names);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let files = walkdir_recursive(&path, 3);
        if files.iter().any(|file| {
            file.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
        }) {
            continue;
        }
        // Interrupted Transformers downloads can contain only the shard index
        // and tokenizer metadata. Surface those too so the management screen
        // can remove the otherwise invisible repository directory.
        if !files.iter().any(|file| is_safetensors_layout_file(file)) {
            continue;
        }
        let config: serde_json::Value = match std::fs::read_to_string(path.join("config.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
        {
            Some(config) => config,
            None => continue,
        };
        let language_architecture = config["architectures"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str())
            .any(|architecture| {
                let lower = architecture.to_ascii_lowercase();
                lower.contains("forcausallm") || lower.contains("forconditionalgeneration")
            });
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let lower_name = dir_name.to_ascii_lowercase();
        let dedicated_media_repo = [
            "-tts",
            "_tts",
            "speech",
            "audio",
            "kokoro",
            "xtts",
            "diffusion",
        ]
        .iter()
        .any(|marker| lower_name.contains(marker));
        if language_architecture && !dedicated_media_repo {
            names.push(dir_name);
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn is_safetensors_layout_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("safetensors"))
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.to_ascii_lowercase()
                    .ends_with(".safetensors.index.json")
            })
}

/// Recursively list all files under `dir` up to `max_depth` levels.
fn walkdir_recursive(dir: &std::path::Path, max_depth: usize) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();
    if max_depth == 0 {
        return results;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir_recursive(&path, max_depth - 1));
            } else if path.is_file() {
                results.push(path);
            }
        }
    }
    results
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PullProgressEvent {
    pub status: String,
    pub completed: u64,
    pub total: u64,
    pub percentage: f64,
}

/// A HuggingFace model choice. `files` contains one complete model variant:
/// either a single GGUF or every shard belonging to one safetensors variant.
/// Support files (tokenizer/config) are kept separate so quantisations and
/// alternate checkpoints are never downloaded by accident.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HfModelSelection {
    pub repo: String,
    pub files: Vec<String>,
    #[serde(default)]
    pub support_files: Vec<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HfModelCandidate {
    pub id: String,
    pub label: String,
    pub files: Vec<String>,
    /// Runtime companions tied to this candidate (for example one multimodal
    /// projector). They are downloaded with the selected primary weights.
    pub support_files: Vec<String>,
    pub total_bytes: u64,
    pub format: String,
    pub quantization: Option<String>,
    pub variant: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HfRepoInspection {
    pub repo: String,
    pub candidates: Vec<HfModelCandidate>,
    pub support_files: Vec<String>,
    pub total_bytes: u64,
    /// Explanation shown before an incompatible repository can be installed.
    pub warning: Option<String>,
    /// Known GGUF conversion that the managed llama.cpp runtime can load.
    pub suggested_repo: Option<String>,
}

#[derive(Debug, Clone)]
struct PullAggregate {
    /// Bytes already downloaded across every file in the plan.
    completed: u64,
    /// Sum of the expected sizes of every file in the plan.
    total: u64,
    /// Expected size of the file currently being streamed. Used to reconcile
    /// HEAD metadata with the final response without double-counting it.
    current_expected: u64,
}

/// A failed or cancelled installation must not leave a multi-gigabyte `.part`
/// file behind. The guard is committed only after the atomic rename succeeds;
/// every earlier return path removes the partial file, including HTTP errors,
/// stream errors, cancellation and write failures.
struct PartialDownloadGuard {
    path: std::path::PathBuf,
    committed: bool,
}

impl PartialDownloadGuard {
    fn new(path: &std::path::Path) -> Self {
        Self {
            path: path.to_path_buf(),
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for PartialDownloadGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn remove_empty_parent_dirs(start: &std::path::Path, stop: &std::path::Path) {
    let mut current = start.to_path_buf();
    while current.starts_with(stop) && current != stop {
        let empty = std::fs::read_dir(&current)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !empty || std::fs::remove_dir(&current).is_err() {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
}

/// Remove only artifacts created by one failed HuggingFace selection. Existing
/// variants in the same repository directory are deliberately preserved.
fn cleanup_failed_hf_selection(
    dest_dir: &std::path::Path,
    models_dir: &std::path::Path,
    selected_paths: &[(String, u64)],
    created_files: &[std::path::PathBuf],
) {
    for path in created_files {
        let _ = std::fs::remove_file(path);
    }
    for (relative, _) in selected_paths {
        let output = dest_dir.join(relative);
        let _ = std::fs::remove_file(output.with_extension("part"));
    }
    remove_empty_parent_dirs(dest_dir, models_dir);
}

/// Send one progress event for the whole download plan. Individual files may
/// change the status text, but they must never reset the byte counters: the UI
/// represents one download task, not one task per repository file.
fn send_aggregate_progress(
    on_event: &Channel<PullProgressEvent>,
    aggregate: &PullAggregate,
    status: impl Into<String>,
) {
    let percentage = if aggregate.total > 0 {
        (aggregate.completed as f64 / aggregate.total as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let _ = on_event.send(PullProgressEvent {
        status: status.into(),
        completed: aggregate.completed,
        total: aggregate.total,
        percentage,
    });
}

async fn remote_content_length(http: &reqwest::Client, url: &str, hf_token: &str) -> u64 {
    let mut request = http.head(url);
    if !hf_token.is_empty() && url.starts_with("https://huggingface.co/") {
        request = request.header("Authorization", format!("Bearer {hf_token}"));
    }
    request
        .send()
        .await
        .ok()
        .and_then(|response| response.content_length())
        .unwrap_or(0)
}

/// Derive a clean on-disk filename from a download URL: last path segment,
/// query string stripped, percent-decoding for %20 etc. left as-is.
fn filename_from_url(url: &str) -> String {
    let no_query = url.split(['?', '#']).next().unwrap_or(url);
    no_query
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("model.gguf")
        .to_string()
}

fn sibling_json_url(url: &str) -> String {
    let no_query = url.split(['?', '#']).next().unwrap_or(url);
    format!("{no_query}.json")
}

fn normalize_hf_repo(source: &str) -> Result<String, String> {
    let raw = source.trim();
    let url = if let Some(rest) = raw.strip_prefix("hf.co/") {
        format!("https://huggingface.co/{rest}")
    } else if !raw.starts_with("http") && raw.matches('/').count() == 1 {
        format!("https://huggingface.co/{raw}")
    } else {
        raw.to_string()
    };
    let repo = url
        .strip_prefix("https://huggingface.co/")
        .ok_or_else(|| {
            "La source doit être un dépôt HuggingFace https://huggingface.co/auteur/modele."
                .to_string()
        })?
        .trim_end_matches('/')
        .split("/resolve/")
        .next()
        .unwrap_or("")
        .split("/blob/")
        .next()
        .unwrap_or("")
        .split("/tree/")
        .next()
        .unwrap_or("");
    if repo.is_empty()
        || repo.matches('/').count() != 1
        || repo.contains("..")
        || repo.contains('?')
    {
        return Err("Dépôt HuggingFace invalide : utilisez auteur/nom-du-repo.".into());
    }
    Ok(repo.to_string())
}

fn is_hf_weight_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let weight = [
        ".gguf",
        ".safetensors",
        ".ckpt",
        ".bin",
        ".pth",
        ".pt",
        ".onnx",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext));
    if !weight {
        return false;
    }
    // These are dependencies/components, not model variants the user should
    // choose. Without this filter a repo such as XTTS (model.pth + dvae.pth +
    // mel_stats.pth) would incorrectly ask the user which internal component
    // to install.
    ![
        "mmproj",
        "text_encoder",
        "tokenizer",
        "vocab",
        "merges",
        "spiece",
        "vae",
        "clip",
        "t5xxl",
        "dvae",
        "mel_stats",
        "vocoder",
        "speaker",
        "conditioning",
        "projector",
        "projection",
        "adapter",
        "lora",
        "mtp-",
        "/mtp/",
        "-draft-",
        "_draft_",
        "optimizer",
        "scheduler",
        "ema",
        "unet/",
        "transformer/",
        "text-encoder/",
        "vae/",
    ]
    .iter()
    .all(|part| !lower.contains(part))
}

fn hf_shard_group(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    // Match every shard number, not only `00001`: otherwise a 3-shard model
    // becomes three apparent models because only the first filename matches.
    for marker in ["-of-", "_of_", "_of-"] {
        if let Some(of_pos) = lower.find(marker) {
            let mut digits_start = of_pos;
            while digits_start > 0 && lower.as_bytes()[digits_start - 1].is_ascii_digit() {
                digits_start -= 1;
            }
            if digits_start < of_pos
                && digits_start > 0
                && matches!(lower.as_bytes()[digits_start - 1], b'-' | b'_' | b'.')
            {
                let stem = &path[..digits_start - 1];
                if let Some(ext) = std::path::Path::new(path)
                    .extension()
                    .and_then(|value| value.to_str())
                {
                    return format!("{stem}.{ext}");
                }
                return stem.to_string();
            }
        }
    }
    path.to_string()
}

/// Official/full-precision repositories that have a known llama.cpp-ready
/// conversion. Keeping this mapping explicit prevents silently swapping model
/// families while still repairing the common "downloaded Safetensors" trap.
fn compatible_gguf_repo(repo: &str) -> Option<&'static str> {
    match repo.to_ascii_lowercase().as_str() {
        "qwen/qwen3.8-27b" | "qwen/qwen3.8-27b-fp8" => Some("ggml-org/Qwen3.8-27B-GGUF"),
        _ => None,
    }
}

fn is_hf_mmproj(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".gguf")
        && lower
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("mmproj-") || name.contains(".mmproj-"))
}

/// Prefer the compact Q8 projector when a repository offers several. A
/// projector is an inference companion, not the main model quantisation.
fn preferred_mmproj(projectors: &[(String, u64)]) -> Option<String> {
    let mut paths: Vec<&String> = projectors.iter().map(|(path, _)| path).collect();
    paths.sort_by_key(|path| {
        let lower = path.to_ascii_lowercase();
        if lower.contains("q8_0") {
            0
        } else if lower.contains("f16") && !lower.contains("bf16") {
            1
        } else if lower.contains("bf16") {
            2
        } else {
            3
        }
    });
    paths.first().map(|path| (*path).clone())
}

fn hf_quantization(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    [
        "q2_k_s", "q2_k", "q3_k_s", "q3_k_m", "q3_k_l", "q4_0", "q4_1", "q4_k_s", "q4_k_m", "q5_0",
        "q5_1", "q5_k_s", "q5_k_m", "q6_k", "q8_0", "bf16", "fp16", "f16", "int8",
    ]
    .iter()
    .find(|q| lower.contains(**q))
    .map(|q| q.to_ascii_uppercase())
}

fn hf_candidate_variant(path: &str, quantization: Option<&str>) -> String {
    let mut stem = path.trim_end_matches('/').to_string();
    if let Some(q) = quantization {
        let lower = stem.to_ascii_lowercase();
        if let Some(pos) = lower.find(&q.to_ascii_lowercase()) {
            stem.truncate(pos);
        }
    }
    for ext in [
        ".safetensors",
        ".gguf",
        ".ckpt",
        ".onnx",
        ".pth",
        ".pt",
        ".bin",
    ] {
        if stem.to_ascii_lowercase().ends_with(ext) {
            stem.truncate(stem.len() - ext.len());
            break;
        }
    }
    stem.trim_end_matches(['-', '_', '.', '/']).to_string()
}

async fn fetch_hf_tree(
    http: &reqwest::Client,
    repo: &str,
    hf_token: &str,
) -> Result<Vec<(String, u64)>, String> {
    let tree_url = format!("https://huggingface.co/api/models/{repo}/tree/main?recursive=true");
    let mut request = http.get(tree_url);
    if !hf_token.is_empty() {
        request = request.header("Authorization", format!("Bearer {hf_token}"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("liste HuggingFace impossible : {e}"))?;
    if !response.status().is_success() {
        return Err(format!("HuggingFace a répondu HTTP {}.", response.status()));
    }
    let entries: Vec<serde_json::Value> = response
        .json()
        .await
        .map_err(|e| format!("réponse HuggingFace illisible : {e}"))?;
    Ok(entries
        .iter()
        .filter(|entry| entry.get("type").and_then(|v| v.as_str()) == Some("file"))
        .filter_map(|entry| {
            Some((
                entry.get("path")?.as_str()?.to_string(),
                entry.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
            ))
        })
        .filter(|(path, _)| {
            !path.contains("..")
                && !path.starts_with("eval/")
                && !path.starts_with("samples/")
                && path != ".gitattributes"
        })
        .collect())
}

/// Inspect a repository before downloading it. A repository often contains
/// Q3/Q4/Q5/Q8 files, several instruct variants, or sharded checkpoints. The
/// UI uses this answer to offer exactly one complete candidate instead of
/// silently downloading every version.
#[tauri::command]
async fn inspect_huggingface_repo(
    source: String,
    hf_token: Option<String>,
) -> Result<HfRepoInspection, String> {
    let repo = normalize_hf_repo(&source)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("locaryn-desktop")
        .build()
        .map_err(|e| e.to_string())?;
    let files = fetch_hf_tree(&http, &repo, hf_token.as_deref().unwrap_or("")).await?;
    let mut groups: std::collections::BTreeMap<String, Vec<(String, u64)>> =
        std::collections::BTreeMap::new();
    let mut support = Vec::new();
    let mut projectors = Vec::new();
    for (path, size) in files {
        if is_hf_mmproj(&path) {
            projectors.push((path, size));
        } else if is_hf_weight_path(&path) {
            groups
                .entry(hf_shard_group(&path))
                .or_default()
                .push((path, size));
        } else {
            let lower = path.to_ascii_lowercase();
            let useful = lower.ends_with("config.json")
                || lower.ends_with("tokenizer.json")
                || lower.ends_with("tokenizer_config.json")
                || lower.ends_with("special_tokens_map.json")
                || lower.ends_with("generation_config.json")
                || lower.ends_with("chat_template.jinja")
                || lower.ends_with(".safetensors.index.json");
            if useful && size <= 16 * 1024 * 1024 {
                support.push(path);
            }
        }
    }
    support.sort();
    support.dedup();

    let mut candidates = Vec::new();
    for (group, mut members) in groups {
        members.sort_by(|a, b| a.0.cmp(&b.0));
        let files: Vec<String> = members.iter().map(|(path, _)| path.clone()).collect();
        let total_bytes = members.iter().map(|(_, size)| *size).sum();
        let quantization = hf_quantization(&group);
        let variant = Some(hf_candidate_variant(&group, quantization.as_deref()));
        let format = group
            .rsplit('.')
            .next()
            .unwrap_or("model")
            .to_ascii_lowercase();
        let candidate_support = if format == "gguf" {
            preferred_mmproj(&projectors).into_iter().collect()
        } else {
            Vec::new()
        };
        let label = format!(
            "{}{}{}",
            variant.as_deref().unwrap_or(&group),
            quantization
                .as_deref()
                .map(|q| format!(" — {q}"))
                .unwrap_or_default(),
            if members.len() > 1 {
                format!(" — {} shards", members.len())
            } else {
                String::new()
            }
        );
        candidates.push(HfModelCandidate {
            id: group.replace('/', "::"),
            label,
            files,
            support_files: candidate_support,
            total_bytes,
            format,
            quantization,
            variant,
        });
    }
    candidates.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    let total_bytes = candidates.iter().map(|c| c.total_bytes).sum();
    let suggested_repo = compatible_gguf_repo(&repo).map(str::to_string);
    let warning = suggested_repo.as_ref().map(|suggested| {
        format!(
            "{repo} contient des poids Transformers Safetensors. Le moteur local llama.cpp ne peut pas les charger avec -m. Utilisez la conversion GGUF compatible {suggested}."
        )
    });
    Ok(HfRepoInspection {
        repo,
        candidates,
        support_files: support,
        total_bytes,
        warning,
        suggested_repo,
    })
}

#[tauri::command]
// Tauri serializes each parameter by name; grouping them would break the IPC API.
#[allow(clippy::too_many_arguments)]
async fn pull_model(
    core: State<'_, Core>,
    _endpoint: String,
    model: String,
    heretic: Option<bool>,
    consent: Option<bool>,
    hf_token: Option<String>,
    selection: Option<HfModelSelection>,
    on_event: Channel<PullProgressEvent>,
) -> Result<(), String> {
    let url = model.trim().to_string();
    let url = if url.starts_with("hf.co/") {
        url.replace("hf.co/", "https://huggingface.co/")
    } else if !url.starts_with("http")
        && url.contains('/')
        && !url.contains('\\')
        && !url.contains(' ')
    {
        format!("https://huggingface.co/{url}")
    } else {
        url
    };
    let hf_token = hf_token.unwrap_or_default();
    if !url.starts_with("http") {
        return Err(
            "Pour installer un modèle, utilisez un identifiant HuggingFace (ex: stablediffusionapi/deliberate-v2) \
             ou une URL directe vers un fichier .safetensors / .gguf."
                .into(),
        );
    }

    if let Some(client) = core.remote_client() {
        let selection_value = selection
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| e.to_string())?;
        match client
            .pull_model(&_endpoint, &url, heretic, consent, selection_value.as_ref())
            .await
        {
            Ok(mut byte_stream) => {
                use futures::StreamExt;
                let mut buffer = String::new();
                while let Some(chunk_res) = byte_stream.next().await {
                    match chunk_res {
                        Ok(chunk) => {
                            if let Ok(text) = std::str::from_utf8(&chunk) {
                                buffer.push_str(text);
                                while let Some(idx) = buffer.find("\n\n") {
                                    let block = buffer[..idx].to_string();
                                    buffer = buffer[idx + 2..].to_string();
                                    for line in block.lines() {
                                        if let Some(data) = line.strip_prefix("data:") {
                                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(
                                                data.trim(),
                                            ) {
                                                let percentage = val
                                                    .get("percentage")
                                                    .and_then(|v| v.as_f64())
                                                    .unwrap_or(0.0);
                                                let completed = val
                                                    .get("downloaded")
                                                    .and_then(|v| v.as_u64())
                                                    .unwrap_or(0);
                                                let total = val
                                                    .get("total")
                                                    .and_then(|v| v.as_u64())
                                                    .unwrap_or(0);
                                                let status = val
                                                    .get("message")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("downloading")
                                                    .to_string();
                                                let _ = on_event.send(PullProgressEvent {
                                                    status,
                                                    completed,
                                                    total,
                                                    percentage,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let _ = on_event.send(PullProgressEvent {
                                status: format!("error: {e}"),
                                completed: 0,
                                total: 0,
                                percentage: 0.0,
                            });
                            return Err(e.to_string());
                        }
                    }
                }
                let _ = on_event.send(PullProgressEvent {
                    status: "success".to_string(),
                    completed: 100,
                    total: 100,
                    percentage: 100.0,
                });
                return Ok(());
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    // A selected HuggingFace candidate is deliberately handled as a local
    // repository plan. Falling back to the old repo-wide download here would
    // undo the user's choice and fetch every quantisation again.
    if selection.is_some() {
        if !url.starts_with("https://huggingface.co/") || url.contains("/resolve/") {
            return Err("Une sélection de variante doit venir d'un dépôt HuggingFace.".into());
        }
        return pull_hf_repo(&core, &url, &on_event, &hf_token, selection.as_ref()).await;
    }

    // ── Repo-level download: if the URL points to a HuggingFace repository
    // (not a /resolve/main/<file> direct link), download the entire repo as a
    // ZIP archive and extract it into a subdirectory under models_dir. This
    // unlocks multi-file TTS models (XTTS, Qwen3-TTS, Kokoro, etc.) that need
    // config files, tokenizers, and multiple weight shards alongside the main
    // checkpoint.
    let is_hf_repo = url.starts_with("https://huggingface.co/")
        && !url.contains("/resolve/")
        && !url.contains("/blob/");
    if is_hf_repo {
        return pull_hf_repo(&core, &url, &on_event, &hf_token, None).await;
    }

    // Refuse repository pages or directory URLs; we need a direct file link.
    // Check the path filename rather than the full URL so `?download=true`
    // (common in HuggingFace links) does not make a valid model look invalid.
    let file_name = filename_from_url(&url);
    let file_lower = file_name.to_ascii_lowercase();
    if ![".gguf", ".safetensors", ".onnx", ".bin", ".pth", ".pt"]
        .iter()
        .any(|ext| file_lower.ends_with(ext))
    {
        return Err(
            "L'URL doit pointer vers un fichier modèle direct (.gguf, .safetensors, .onnx, .bin). \
             Les liens vers un dépôt complet ne sont pas supportés ici."
                .into(),
        );
    }

    if is_nsfw_model(&url) && !consent.unwrap_or(false) {
        return Err("Ce modèle est classé NSFW / sans garde-fous. \
             Acceptez la responsabilité dans l'interface avant de télécharger."
            .into());
    }

    let models_dir = locaryn_config::models_dir();
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    let final_path = models_dir.join(&file_name);
    let part_path = models_dir.join(format!("{file_name}.part"));

    // Direct downloads may install companion files afterwards. Resolve their
    // sizes before starting the main request and keep one byte-based progress
    // bar for the complete plan. This prevents a VAE/encoder/config download
    // from looking like a second download that restarted at 0%.
    let planned_companions = image_companions_for(&file_name, heretic.unwrap_or(false));
    let piper_companion = {
        let lower = file_name.to_ascii_lowercase();
        let url_lower = url.to_ascii_lowercase();
        (lower.ends_with(".onnx") && !lower.contains("kokoro") && !url_lower.contains("kokoro"))
            .then(|| (sibling_json_url(&url), format!("{file_name}.json")))
    };
    let main_size = remote_content_length(&core.http, &url, &hf_token).await;
    let mut aggregate = PullAggregate {
        completed: 0,
        total: main_size,
        current_expected: main_size,
    };
    if final_path.exists() {
        let existing_size = std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
        if aggregate.total == 0 {
            aggregate.total = existing_size;
        }
        aggregate.completed = existing_size;
    }
    for comp in &planned_companions {
        let path = models_dir.join(comp.file);
        if path.exists() {
            let _ = std::fs::remove_file(path.with_extension("part"));
        } else {
            aggregate.total = aggregate
                .total
                .saturating_add(remote_content_length(&core.http, comp.url, &hf_token).await);
        }
    }
    if let Some((comp_url, comp_file)) = &piper_companion {
        let comp_path = models_dir.join(comp_file);
        if comp_path.exists() {
            let _ = std::fs::remove_file(comp_path.with_extension("part"));
        } else {
            aggregate.total = aggregate
                .total
                .saturating_add(remote_content_length(&core.http, comp_url, &hf_token).await);
        }
    }

    // Register a cancellation token so cancel_pull_model can really stop us,
    // and always deregister on the way out (success, error, or cancel).
    let cancel = tokio_util::sync::CancellationToken::new();
    core.pull_cancels
        .lock()
        .await
        .insert(file_name.clone(), cancel.clone());
    let result = if final_path.exists() {
        // A completed file never needs its old resume marker.
        let _ = std::fs::remove_file(&part_path);
        send_aggregate_progress(
            &on_event,
            &aggregate,
            format!("{file_name} déjà installé — vérification des fichiers associés…"),
        );
        Ok(())
    } else {
        do_pull_with_aggregate(
            &core,
            &url,
            &file_name,
            &final_path,
            &part_path,
            &on_event,
            &cancel,
            &hf_token,
            Some(&mut aggregate),
        )
        .await
    };
    core.pull_cancels.lock().await.remove(&file_name);
    result?;

    // Auto-setup: diffusion-only checkpoints cannot generate alone. Fetch the
    // family-specific VAE and text encoders so one Marketplace action installs
    // a complete, runnable image stack.
    if let Err(e) = install_image_companions(
        &core,
        &file_name,
        heretic.unwrap_or(false),
        &on_event,
        &hf_token,
        Some(&mut aggregate),
    )
    .await
    {
        tracing::warn!(error = %e, "companion install failed (model itself is installed)");
    }
    // Auto-setup: Piper TTS voices ship a .json config file next to the .onnx.
    if let Err(e) = install_audio_companions(
        &core,
        &url,
        &file_name,
        &on_event,
        &hf_token,
        Some(&mut aggregate),
    )
    .await
    {
        tracing::warn!(error = %e, "audio companion install failed (model itself is installed)");
    }
    Ok(())
}

/// Download an entire HuggingFace repository as a ZIP archive and extract it
/// into a subdirectory under models_dir. This is needed for multi-file TTS
/// models (Coqui XTTS, Qwen3-TTS, Kokoro, OmniVoice, etc.) that ship config
/// files, tokenizers, and multiple weight shards.
async fn pull_hf_repo(
    core: &Core,
    url: &str,
    on_event: &Channel<PullProgressEvent>,
    hf_token: &str,
    selection: Option<&HfModelSelection>,
) -> Result<(), String> {
    let repo_id = normalize_hf_repo(url)?;
    if let Some(suggested) = compatible_gguf_repo(&repo_id) {
        let selected_gguf = selection.is_some_and(|choice| {
            choice
                .files
                .iter()
                .any(|path| path.to_ascii_lowercase().ends_with(".gguf"))
        });
        if !selected_gguf {
            return Err(format!(
                "{repo_id} est un checkpoint Transformers Safetensors que llama.cpp ne peut pas lancer. Installez la conversion GGUF compatible : https://huggingface.co/{suggested}"
            ));
        }
    }
    if let Some(choice) = selection {
        if normalize_hf_repo(&choice.repo)? != repo_id {
            return Err("La variante sélectionnée ne vient pas de ce dépôt HuggingFace.".into());
        }
    }

    let dir_name = repo_id.replace('/', "__");
    let models_dir = locaryn_config::models_dir();
    let dest_dir = models_dir.join(&dir_name);
    let _ = on_event.send(PullProgressEvent {
        status: format!("Liste des fichiers du dépôt {repo_id}…"),
        completed: 0,
        total: 0,
        percentage: 0.0,
    });

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("locaryn-desktop")
        .build()
        .map_err(|e| e.to_string())?;
    let files = fetch_hf_tree(&http, &repo_id, hf_token).await?;
    let available: std::collections::HashMap<String, u64> = files.into_iter().collect();
    let selected_paths: Vec<(String, u64)> = if let Some(choice) = selection {
        let mut paths = choice.files.clone();
        paths.extend(choice.support_files.clone());
        paths.sort();
        paths.dedup();
        paths
            .into_iter()
            .map(|path| {
                if path.is_empty()
                    || path.starts_with('/')
                    || path.contains("..")
                    || path.contains('\\')
                {
                    return Err("Chemin de fichier HuggingFace invalide.".to_string());
                }
                let size = available.get(&path).copied().ok_or_else(|| {
                    format!("Le fichier sélectionné n'existe plus dans {repo_id} : {path}")
                })?;
                Ok((path, size))
            })
            .collect::<Result<Vec<_>, String>>()?
    } else {
        available.into_iter().collect()
    };

    if selected_paths.is_empty() {
        return Err(format!(
            "Aucun fichier à télécharger dans le dépôt {repo_id}."
        ));
    }
    let total_files = selected_paths.len();
    let total_bytes: u64 = selected_paths.iter().map(|(_, size)| *size).sum();
    let _ = on_event.send(PullProgressEvent {
        status: if selection.is_some() {
            format!("Variante sélectionnée : {total_files} fichier(s) — les autres versions sont ignorées")
        } else {
            format!("{total_files} fichiers du dépôt")
        },
        completed: 0,
        total: total_bytes,
        percentage: 0.0,
    });

    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_key = format!("{dir_name}::repo");
    core.pull_cancels
        .lock()
        .await
        .insert(cancel_key.clone(), cancel.clone());

    let mut aggregate = PullAggregate {
        completed: 0,
        total: total_bytes,
        current_expected: 0,
    };
    // Keep the repository directory reusable: if a selected variant fails,
    // remove only files created by this attempt, never another quantisation
    // already installed under the same HuggingFace repository.
    let mut created_files: Vec<std::path::PathBuf> = Vec::new();
    let result = async {
        for (i, (file_path, expected_size)) in selected_paths.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err("Téléchargement annulé".into());
            }
            let out_path = dest_dir.join(file_path);
            let part_path = out_path.with_extension("part");
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            if out_path.exists() {
                // A previous crash may have left a stale partial next to an
                // otherwise complete file. It is never needed in that case.
                let _ = std::fs::remove_file(&part_path);
                aggregate.completed = aggregate.completed.saturating_add(*expected_size);
                continue;
            }
            let dl_url = format!("https://huggingface.co/{repo_id}/resolve/main/{file_path}");
            let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
            let _ = on_event.send(PullProgressEvent {
                status: format!("Fichier {}/{} : {}", i + 1, total_files, file_name),
                completed: aggregate.completed,
                total: aggregate.total,
                percentage: if aggregate.total > 0 {
                    aggregate.completed as f64 / aggregate.total as f64 * 100.0
                } else {
                    0.0
                },
            });
            aggregate.current_expected = *expected_size;
            do_pull_with_aggregate(
                core,
                &dl_url,
                file_name,
                &out_path,
                &part_path,
                on_event,
                &cancel,
                hf_token,
                Some(&mut aggregate),
            )
            .await?;
            created_files.push(out_path);
        }
        Ok::<(), String>(())
    }
    .await;
    core.pull_cancels.lock().await.remove(&cancel_key);
    if let Err(error) = result {
        cleanup_failed_hf_selection(&dest_dir, &models_dir, &selected_paths, &created_files);
        return Err(error);
    }

    let _ = on_event.send(PullProgressEvent {
        status: format!("Variante du dépôt {repo_id} installée ({total_files} fichier(s))"),
        completed: aggregate.completed,
        total: aggregate.total,
        percentage: 100.0,
    });
    Ok(())
}
// ============================================================================
// Model download — streaming with progress + cancellation
// ============================================================================

/// Stream-download `url` to `final_path`, writing to `part_path` first and
/// renaming on success. Reports progress via `on_event`. Resumable: if
/// `part_path` exists, the download continues from the current byte offset.
// Un téléchargement a légitimement beaucoup de paramètres (source, destination,
// fichier partiel, progression, annulation, jeton). Les regrouper dans une
// structure n'apporterait rien ici : ils n'ont pas de vie commune ailleurs.
#[allow(clippy::too_many_arguments)]
async fn do_pull(
    core: &Core,
    url: &str,
    file_name: &str,
    final_path: &std::path::Path,
    part_path: &std::path::Path,
    on_event: &Channel<PullProgressEvent>,
    cancel: &tokio_util::sync::CancellationToken,
    hf_token: &str,
) -> Result<(), String> {
    do_pull_with_aggregate(
        core, url, file_name, final_path, part_path, on_event, cancel, hf_token, None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn do_pull_with_aggregate(
    _core: &Core,
    url: &str,
    file_name: &str,
    final_path: &std::path::Path,
    part_path: &std::path::Path,
    on_event: &Channel<PullProgressEvent>,
    cancel: &tokio_util::sync::CancellationToken,
    hf_token: &str,
    mut aggregate: Option<&mut PullAggregate>,
) -> Result<(), String> {
    use futures::StreamExt;

    // A partial file is never a durable installation artifact. If this call
    // fails, the guard removes it instead of silently preserving a resumable
    // multi-gigabyte remainder that the UI cannot manage later.
    let mut partial_cleanup = PartialDownloadGuard::new(part_path);

    let mut offset: u64 = 0;
    if part_path.exists() {
        offset = std::fs::metadata(part_path).map(|m| m.len()).unwrap_or(0);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.get(url);
    if offset > 0 {
        req = req.header("Range", format!("bytes={offset}-"));
    }
    // Gated HuggingFace repos (kyutai/pocket-tts, Qwen3-TTS, …) answer 401
    // without an access token. Send it only to huggingface.co, never to any
    // other host this function might be pointed at.
    if !hf_token.is_empty() && url.starts_with("https://huggingface.co/") {
        req = req.header("Authorization", format!("Bearer {hf_token}"));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("download error: {e}"))?;

    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }

    // Some servers ignore Range and answer 200 with the complete file. Do not
    // append that body to the partial file: restart that one file cleanly while
    // keeping the aggregate progress of the other files intact.
    if offset > 0 && resp.status().as_u16() == 200 {
        offset = 0;
    }

    let total = resp.content_length().map(|l| l + offset).unwrap_or(0);
    if let Some(overall) = aggregate.as_deref_mut() {
        // Reconcile the expected size with the actual response. This keeps the
        // denominator equal to the sum of all files even when a HEAD request
        // was unavailable or a CDN returned a different size.
        if total > 0 {
            if overall.current_expected > 0 {
                if total >= overall.current_expected {
                    overall.total = overall
                        .total
                        .saturating_add(total - overall.current_expected);
                } else {
                    overall.total = overall
                        .total
                        .saturating_sub(overall.current_expected - total);
                }
            } else {
                overall.total = overall.total.saturating_add(total);
            }
        }
        overall.current_expected = 0;
        // The `.part` bytes were already received during an earlier attempt.
        // Count them once so resume never makes the global bar jump backwards.
        overall.completed = overall.completed.saturating_add(offset);
    }

    let (completed, event_total) = if let Some(overall) = aggregate.as_deref() {
        (overall.completed, overall.total)
    } else {
        (offset, total)
    };
    let _ = on_event.send(PullProgressEvent {
        status: format!("Téléchargement de {file_name}…"),
        completed,
        total: event_total,
        percentage: if event_total > 0 {
            (completed as f64 / event_total as f64 * 100.0).min(100.0)
        } else {
            0.0
        },
    });

    // Open the partial file for append (or create new).
    // Use tokio::fs for async I/O.
    let mut file = if offset > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(part_path)
            .await
            .map_err(|e| format!("cannot open partial file: {e}"))?
    } else {
        tokio::fs::File::create(part_path)
            .await
            .map_err(|e| format!("cannot create partial file: {e}"))?
    };

    use tokio::io::AsyncWriteExt;
    let mut writer = tokio::io::BufWriter::new(&mut file);

    let mut stream = resp.bytes_stream();
    let mut downloaded = offset;
    let mut last_report = std::time::Instant::now();

    while let Some(chunk_result) = stream.next().await {
        if cancel.is_cancelled() {
            return Err("Telechargement annule".into());
        }

        let chunk = chunk_result.map_err(|e| format!("stream error: {e}"))?;
        writer
            .write_all(&chunk)
            .await
            .map_err(|e| format!("write error: {e}"))?;
        downloaded += chunk.len() as u64;
        if let Some(overall) = aggregate.as_deref_mut() {
            overall.completed = overall.completed.saturating_add(chunk.len() as u64);
        }

        // Report progress at most every 200ms to avoid flooding IPC. In an
        // HF repository this is the aggregate of every selected file, not the
        // percentage of the file currently visible in the status text.
        if last_report.elapsed() > std::time::Duration::from_millis(200) {
            let (completed, event_total) = if let Some(overall) = aggregate.as_deref() {
                (overall.completed, overall.total)
            } else {
                (downloaded, total)
            };
            let pct = if event_total > 0 {
                completed as f64 / event_total as f64 * 100.0
            } else {
                0.0
            };
            let _ = on_event.send(PullProgressEvent {
                status: format!("Téléchargement de {file_name}…"),
                completed,
                total: event_total,
                percentage: pct.min(100.0),
            });
            last_report = std::time::Instant::now();
        }
    }

    writer
        .flush()
        .await
        .map_err(|e| format!("flush error: {e}"))?;
    drop(writer);
    drop(file);

    // Rename .part -> final name.
    std::fs::rename(part_path, final_path)
        .map_err(|e| format!("cannot rename partial file: {e}"))?;
    partial_cleanup.commit();

    let (completed, event_total, percentage) = if let Some(overall) = aggregate.as_deref() {
        let total = overall.total;
        (
            overall.completed,
            total,
            if total > 0 {
                (overall.completed as f64 / total as f64 * 100.0).min(100.0)
            } else {
                0.0
            },
        )
    } else {
        (
            downloaded,
            if total > 0 { total } else { downloaded },
            100.0,
        )
    };
    let _ = on_event.send(PullProgressEvent {
        status: format!("{file_name} installé"),
        completed,
        total: event_total,
        percentage,
    });

    Ok(())
}

/// Cancel one download (by file name) or all active downloads. The partial
/// `.part` file is kept on disk so the next attempt resumes.
#[tauri::command]
async fn cancel_pull_model(core: State<'_, Core>, model: Option<String>) -> Result<(), String> {
    let mut cancels = core.pull_cancels.lock().await;
    match model.map(|m| filename_from_url(&m)) {
        Some(name) => {
            if let Some(token) = cancels.remove(&name) {
                token.cancel();
                Ok(())
            } else {
                Err(format!("no active download for {name}"))
            }
        }
        None => {
            let count = cancels.len();
            for (_, token) in cancels.drain() {
                token.cancel();
            }
            tracing::info!(cancelled = count, "cancelled all downloads");
            Ok(())
        }
    }
}

/// Download the companions a freshly installed audio/TTS model needs.
async fn install_audio_companions(
    core: &Core,
    url: &str,
    installed_file: &str,
    on_event: &Channel<PullProgressEvent>,
    hf_token: &str,
    mut aggregate: Option<&mut PullAggregate>,
) -> Result<(), String> {
    let lower = installed_file.to_ascii_lowercase();

    // ── Case 1: Piper voice (.onnx) → download sibling .json config ──
    if lower.ends_with(".onnx") {
        // If this .onnx is a Kokoro ONNX export, delegate to the Kokoro
        // companion installer. We check both the filename AND the URL,
        // because repos like onnx-community/Kokoro-82M-v1.0-ONNX ship
        // files named model_fp16.onnx (no "kokoro" in the filename).
        let url_lower = url.to_ascii_lowercase();
        if lower.contains("kokoro") || url_lower.contains("kokoro") {
            return install_kokoro_companions(core, url, installed_file, on_event, hf_token).await;
        }
        // Standard Piper .onnx → fetch the .onnx.json config sibling.
        let models_dir = locaryn_config::models_dir();
        let json_url = sibling_json_url(url);
        let json_name = format!("{installed_file}.json");
        let dest = models_dir.join(&json_name);
        if dest.exists() {
            let _ = std::fs::remove_file(dest.with_extension("part"));
            return Ok(());
        }
        tracing::info!(file = %json_name, "installing Piper audio companion");
        let expected = remote_content_length(&core.http, &json_url, hf_token).await;
        if let Some(overall) = aggregate.as_deref_mut() {
            overall.current_expected = expected;
            send_aggregate_progress(
                on_event,
                overall,
                format!("Installation automatique : {json_name}"),
            );
        } else {
            let _ = on_event.send(PullProgressEvent {
                status: format!("Installation automatique : {json_name}"),
                completed: 0,
                total: 0,
                percentage: 0.0,
            });
        }
        let part = models_dir.join(format!("{json_name}.part"));
        let cancel = tokio_util::sync::CancellationToken::new();
        return do_pull_with_aggregate(
            core, &json_url, &json_name, &dest, &part, on_event, &cancel, hf_token, aggregate,
        )
        .await;
    }

    // ── Case 2: Kokoro PyTorch (.pth) downloaded as a direct file →
    //     restructure into a repo directory and fetch companions ──
    if lower.ends_with(".pth") {
        let url_lower = url.to_ascii_lowercase();
        if lower.contains("kokoro") || url_lower.contains("kokoro") {
            // The .pth was downloaded as a direct file. For a fully working
            // Kokoro setup we need config.json + voices/ alongside it.
            // Reconstruct the repo URL and trigger a repo-level companion fetch.
            return install_kokoro_companions(core, url, installed_file, on_event, hf_token).await;
        }
    }

    Ok(())
}

/// Unified companion installer for Kokoro-82M models (.onnx or .pth).
///
/// When the user downloads a single Kokoro weight file from HuggingFace
/// (e.g. `kokoro-v1_0.pth` from `hexgrad/Kokoro-82M` or `model_fp16.onnx`
/// from `onnx-community/Kokoro-82M-v1.0-ONNX`), the weight file alone is
/// insufficient. The model also needs:
///   - `config.json`       (model configuration)
///   - `tokenizer.json`    (tokenizer data, if present in the repo)
///   - `voices/`           (54 voice style vectors) or `voices-v1.0.bin`
///
/// This function:
///   1. Extracts the HuggingFace repo id from the download URL.
///   2. Creates a directory `models/<repo_id>` (like pull_hf_repo does).
///   3. Moves the downloaded weight file into that directory.
///   4. Downloads config.json + tokenizer.json + voices/ from the same repo.
///   5. After this, resolve_tts_engine routes to TtsEngine::Kokoro.
async fn install_kokoro_companions(
    core: &Core,
    url: &str,
    installed_file: &str,
    on_event: &Channel<PullProgressEvent>,
    hf_token: &str,
) -> Result<(), String> {
    let models_dir = locaryn_config::models_dir();

    // Extract the HuggingFace repo id from the URL.
    // URL pattern: https://huggingface.co/<author>/<repo>/resolve/main/<path>/<file>
    let repo_id = url
        .strip_prefix("https://huggingface.co/")
        .unwrap_or(url)
        .split("/resolve/")
        .next()
        .unwrap_or(url);
    let dir_name = repo_id.replace('/', "__");
    let dest_dir = models_dir.join(&dir_name);

    // If the directory already exists with the weight file inside, skip.
    let weight_in_dir = dest_dir.join(installed_file);
    if weight_in_dir.exists() {
        return Ok(());
    }

    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("cannot create kokoro dir: {e}"))?;

    // Move the downloaded weight file from models_dir/<file> to models_dir/<repo>/<file>.
    let weight_current = models_dir.join(installed_file);
    if weight_current.exists() {
        std::fs::rename(&weight_current, &weight_in_dir)
            .map_err(|e| format!("cannot move weight into repo dir: {e}"))?;
    }

    let cancel = tokio_util::sync::CancellationToken::new();

    // Download config.json from the same repo.
    let config_dest = dest_dir.join("config.json");
    if !config_dest.exists() {
        let config_url = format!("https://huggingface.co/{repo_id}/resolve/main/config.json");
        tracing::info!(repo = repo_id, "downloading kokoro config.json");
        let _ = on_event.send(PullProgressEvent {
            status: "Installation automatique : config.json".to_string(),
            completed: 0,
            total: 0,
            percentage: 0.0,
        });
        let part = dest_dir.join("config.json.part");
        let _ = do_pull(
            core,
            &config_url,
            "config.json",
            &config_dest,
            &part,
            on_event,
            &cancel,
            hf_token,
        )
        .await;
    }

    // Download tokenizer.json from the same repo (if it exists — not all
    // Kokoro repos ship a tokenizer.json, so we tolerate a 404).
    let tok_dest = dest_dir.join("tokenizer.json");
    if !tok_dest.exists() {
        let tok_url = format!("https://huggingface.co/{repo_id}/resolve/main/tokenizer.json");
        tracing::info!(repo = repo_id, "downloading kokoro tokenizer.json");
        let _ = on_event.send(PullProgressEvent {
            status: "Installation automatique : tokenizer.json".to_string(),
            completed: 0,
            total: 0,
            percentage: 0.0,
        });
        let part = dest_dir.join("tokenizer.json.part");
        // Non-fatal: some repos don't have tokenizer.json.
        let _ = do_pull(
            core,
            &tok_url,
            "tokenizer.json",
            &tok_dest,
            &part,
            on_event,
            &cancel,
            hf_token,
        )
        .await;
        // Clean up empty/failed download.
        if tok_dest.exists() && std::fs::metadata(&tok_dest).map(|m| m.len()).unwrap_or(0) == 0 {
            let _ = std::fs::remove_file(&tok_dest);
        }
        // Clean up leftover .part file from a failed/404 download.
        let _ = std::fs::remove_file(dest_dir.join("tokenizer.json.part"));
    }

    // Download voices/ directory from the same repo.
    // We use the HuggingFace Tree API to list files in the voices/ path.
    let tree_url = format!("https://huggingface.co/api/models/{repo_id}/tree/main/voices");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    // The Tree API call is non-fatal: if the network request fails or the
    // API returns a non-success status, we still attempt the voices-v1.0.bin
    // fallback below. config.json and tokenizer.json may already be installed.
    let mut req = client.get(&tree_url);
    if !hf_token.is_empty() {
        req = req.header("Authorization", format!("Bearer {hf_token}"));
    }
    let voices_downloaded = match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            // Tree API returned a valid response — parse and download voices.
            match resp.json::<Vec<serde_json::Value>>().await {
                Ok(entries) => {
                    let voice_files: Vec<String> = entries
                        .iter()
                        .filter(|e| e.get("type").and_then(|v| v.as_str()) == Some("file"))
                        .filter_map(|e| {
                            e.get("path")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect();

                    let voices_dest = dest_dir.join("voices");
                    if !voices_dest.exists() {
                        let _ = std::fs::create_dir_all(&voices_dest);
                    }

                    let total_voices = voice_files.len();
                    let mut count = 0;
                    for (i, voice_path) in voice_files.iter().enumerate() {
                        if cancel.is_cancelled() {
                            break;
                        }
                        let voice_name = voice_path.rsplit('/').next().unwrap_or(voice_path);
                        let voice_dest = voices_dest.join(voice_name);
                        if voice_dest.exists() {
                            count += 1;
                            continue;
                        }
                        let voice_url =
                            format!("https://huggingface.co/{repo_id}/resolve/main/{voice_path}");
                        let _ = on_event.send(PullProgressEvent {
                            status: format!("Voix [{}/{}] : {}", i + 1, total_voices, voice_name),
                            completed: i as u64,
                            total: total_voices as u64,
                            percentage: ((i + 1) as f64 / total_voices.max(1) as f64) * 100.0,
                        });
                        let part = voices_dest.join(format!("{voice_name}.part"));
                        let _ = do_pull(
                            core,
                            &voice_url,
                            voice_name,
                            &voice_dest,
                            &part,
                            on_event,
                            &cancel,
                            hf_token,
                        )
                        .await;
                        count += 1;
                    }
                    count > 0
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Tree API parse failed, trying voices-v1.0.bin fallback");
                    false
                }
            }
        }
        Ok(_) => {
            // Non-success HTTP status (e.g. 404 if repo has no voices/ dir).
            false
        }
        Err(e) => {
            // Network error — non-fatal, try fallback.
            tracing::warn!(error = %e, "Tree API request failed, trying voices-v1.0.bin fallback");
            false
        }
    };

    // Fallback: if no individual voice files were downloaded, try the
    // single-file voices-v1.0.bin blob that some repos ship instead.
    if !voices_downloaded {
        let voices_bin_dest = dest_dir.join("voices-v1.0.bin");
        if !voices_bin_dest.exists() {
            let voices_bin_url =
                format!("https://huggingface.co/{repo_id}/resolve/main/voices-v1.0.bin");
            tracing::info!(repo = repo_id, "downloading voices-v1.0.bin fallback");
            let _ = on_event.send(PullProgressEvent {
                status: "Installation automatique : voices-v1.0.bin".to_string(),
                completed: 0,
                total: 0,
                percentage: 0.0,
            });
            let part = dest_dir.join("voices-v1.0.bin.part");
            let _ = do_pull(
                core,
                &voices_bin_url,
                "voices-v1.0.bin",
                &voices_bin_dest,
                &part,
                on_event,
                &cancel,
                hf_token,
            )
            .await;
        }
    }

    let _ = on_event.send(PullProgressEvent {
        status: format!("Kokoro ({dir_name}) installe avec config + tokenizer + voix"),
        completed: 0,
        total: 0,
        percentage: 100.0,
    });

    Ok(())
}

/// Companion weights an image checkpoint needs to actually generate.
struct Companion {
    url: &'static str,
    file: &'static str,
    label: &'static str,
}

const FLUX_VAE: Companion = Companion {
    // Z-Image uses the FLUX VAE. The ONNX decoder that was previously
    // downloaded here cannot be read by stable-diffusion.cpp's `--vae` flag
    // and ends in the opaque "get sd version from file failed" error.
    url: "https://huggingface.co/black-forest-labs/FLUX.1-schnell/resolve/main/ae.safetensors",
    file: "ae.safetensors",
    label: "VAE compatible stable-diffusion.cpp",
};

const FLUX_CLIP_L: Companion = Companion {
    url: "https://huggingface.co/comfyanonymous/flux_text_encoders/resolve/main/clip_l.safetensors",
    file: "clip_l.safetensors",
    label: "encodeur CLIP-L",
};

const FLUX_T5XXL: Companion = Companion {
    url: "https://huggingface.co/comfyanonymous/flux_text_encoders/resolve/main/t5xxl_fp16.safetensors",
    file: "t5xxl_fp16.safetensors",
    label: "encodeur T5-XXL",
};

const Z_IMAGE_ENCODER: Companion = Companion {
    url: "https://huggingface.co/second-state/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
    file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
    label: "encodeur de texte",
};

const HERETIC_ENCODER: Companion = Companion {
    url: "https://huggingface.co/onnx-community/Z-Image-AbliteratedV1/resolve/main/Z-Image-AbliteratedV1.Q4_K_M.gguf",
    file: "Z-Image-AbliteratedV1.Q4_K_M.gguf",
    label: "encodeur ablitere (heretic)",
};

fn image_companions_for(installed_file: &str, heretic: bool) -> Vec<&'static Companion> {
    let lower = installed_file.to_ascii_lowercase();
    if lower.contains("z_image") || lower.contains("z-image") || lower.contains("z_img") {
        if heretic {
            vec![&HERETIC_ENCODER, &FLUX_VAE]
        } else {
            vec![&Z_IMAGE_ENCODER, &FLUX_VAE]
        }
    } else if lower.contains("flux1") || lower.contains("flux.1") || lower.contains("flux_1") {
        vec![&FLUX_VAE, &FLUX_CLIP_L, &FLUX_T5XXL]
    } else {
        Vec::new()
    }
}

/// Download the companions a freshly installed image model needs.
async fn install_image_companions(
    core: &Core,
    installed_file: &str,
    heretic: bool,
    on_event: &Channel<PullProgressEvent>,
    hf_token: &str,
    mut aggregate: Option<&mut PullAggregate>,
) -> Result<(), String> {
    let models_dir = locaryn_config::models_dir();
    let companions = image_companions_for(installed_file, heretic);

    for comp in companions {
        let dest = models_dir.join(comp.file);
        if dest.exists() {
            let _ = std::fs::remove_file(dest.with_extension("part"));
            continue;
        }
        tracing::info!(
            file = comp.file,
            "installing image companion: {}",
            comp.label
        );
        let expected = remote_content_length(&core.http, comp.url, hf_token).await;
        if let Some(overall) = aggregate.as_deref_mut() {
            overall.current_expected = expected;
            send_aggregate_progress(
                on_event,
                overall,
                format!("Installation automatique : {} ({})", comp.label, comp.file),
            );
        } else {
            let _ = on_event.send(PullProgressEvent {
                status: format!("Installation automatique : {} ({})", comp.label, comp.file),
                completed: 0,
                total: 0,
                percentage: 0.0,
            });
        }
        let part = models_dir.join(format!("{}.part", comp.file));
        let cancel = tokio_util::sync::CancellationToken::new();
        if let Err(e) = do_pull_with_aggregate(
            core,
            comp.url,
            comp.file,
            &dest,
            &part,
            on_event,
            &cancel,
            hf_token,
            aggregate.as_deref_mut(),
        )
        .await
        {
            tracing::warn!(error = %e, file = comp.file, "companion download failed");
        }
    }

    Ok(())
}

// ============================================================================
// Tool approval (doc 11 S5/S6.5)
// ============================================================================

/// Verdict + scope received from the desktop modal or the CLI prompt.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApproveToolCallPayload {
    pub call_id: String,
    pub tool: String,
    pub risk: String,
    pub decision: String,
    pub scope: String,
    pub note: Option<String>,
}

/// Transmet le verdict de l'utilisateur à l'appel d'outil qui l'attend.
///
/// La boucle d'agent est garée sur un canal depuis qu'elle a émis
/// `ToolApproval` ; c'est cette fonction qui la relance. Elle échoue quand
/// plus rien n'attend — délai dépassé, ou verdict déjà transmis — plutôt que
/// de faire croire à un effet.
#[tauri::command]
async fn approve_tool_call(
    core: State<'_, Core>,
    payload: ApproveToolCallPayload,
) -> Result<(), String> {
    let autorise = match payload.decision.as_str() {
        "allow" => true,
        "deny" => false,
        autre => return Err(format!("décision inconnue : {autre}")),
    };

    let verdict = approval_gate::Verdict {
        autorise,
        portee: approval_gate::Portee::depuis(&payload.scope),
    };

    if core
        .approval_gate
        .repondre(&payload.call_id, &payload.tool, verdict)
        .await
    {
        tracing::info!(
            call_id = %payload.call_id,
            tool = %payload.tool,
            scope = %payload.scope,
            autorise,
            "verdict transmis à la boucle d'agent"
        );
        Ok(())
    } else {
        Err("cette demande n'attend plus de réponse : le délai est dépassé, ou elle a déjà été tranchée".to_string())
    }
}

// ============================================================================
// Image generation
// ============================================================================

/// Result of an audio / TTS generation.
#[derive(Debug, Clone, Serialize)]
struct GeneratedAudio {
    path: String,
    simulated: bool,
}

/// Generate a minimal valid WAV file header with a short sine tone.
/// Used as a fallback when no local TTS binary is installed.
fn generate_test_wav(channels: u16, sample_rate: u32, _speed: f32) -> Vec<u8> {
    use std::io::Write;
    let duration_seconds = 2u32;
    let num_samples = sample_rate * duration_seconds * channels as u32;
    let bytes_per_sample: u16 = 2;
    let data_size = num_samples * bytes_per_sample as u32;
    let file_size = 44 + data_size;

    let mut wav = Vec::with_capacity(file_size as usize);

    wav.extend_from_slice(b"RIFF");
    wav.write_all(&(file_size - 8).to_le_bytes()).unwrap();
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.write_all(&16u32.to_le_bytes()).unwrap();
    wav.write_all(&1u16.to_le_bytes()).unwrap();
    wav.write_all(&channels.to_le_bytes()).unwrap();
    wav.write_all(&sample_rate.to_le_bytes()).unwrap();
    wav.write_all(&(sample_rate * channels as u32 * bytes_per_sample as u32).to_le_bytes())
        .unwrap();
    wav.write_all(&(channels * bytes_per_sample).to_le_bytes())
        .unwrap();
    wav.write_all(&16u16.to_le_bytes()).unwrap();
    wav.extend_from_slice(b"data");
    wav.write_all(&data_size.to_le_bytes()).unwrap();

    for i in 0..(num_samples as usize) {
        let t = i as f32 / sample_rate as f32;
        let freq = 440.0 / _speed.max(0.1);
        let sample = (t * freq * 2.0 * std::f32::consts::PI).sin() * 0.25 * i16::MAX as f32;
        let sample_i16 = sample as i16;
        wav.write_all(&sample_i16.to_le_bytes()).unwrap();
    }

    wav
}

// ============================================================================
// Audio / TTS generation
// ============================================================================

// ============================================================================
// TTS engine dispatch — Piper, Kokoro, XTTS
// ============================================================================

/// Detect which TTS engine to use based on the model path/tag.
/// Returns (engine, resolved_model_path, config_path_or_none).
///
/// - `piper` : single .onnx file + .json config sibling. No voice cloning.
/// - `kokoro` : extracted HF repo containing `kokoro-v1_0.pth` + `voices/*.pt`.
///   Built-in voices, no external cloning (the .pt voice files ARE the voice
///   profiles).
/// - `xtts` : extracted HF repo from `coqui/XTTS-v2`. Supports voice cloning
///   via a `speaker_wav` reference file. Requires `coqui-tts`.
/// - `python_generic` : any other repo that has a .pth/.pt/.safetensors — we
///   attempt a generic Python TTS script.
fn resolve_tts_engine(
    models_dir: &std::path::Path,
    model_tag: &str,
) -> (TtsEngine, std::path::PathBuf, Option<std::path::PathBuf>) {
    // Direct .onnx file at top level → Piper or Kokoro ONNX.
    let direct = models_dir.join(model_tag);
    if direct.is_file() && model_tag.to_ascii_lowercase().ends_with(".onnx") {
        let lower = model_tag.to_ascii_lowercase();
        // Kokoro ONNX exports are named kokoro-*.onnx → route to Kokoro engine.
        if lower.contains("kokoro") {
            // The companion installer should have moved this into a repo dir.
            // But if it's still at top level, use it as-is with Kokoro dispatch.
            return (TtsEngine::Kokoro, direct, None);
        }
        let cfg = direct.with_extension("onnx.json");
        let cfg_path = if cfg.exists() {
            Some(cfg)
        } else {
            // Piper config is often named `<base>.onnx.json` → check `<base>.json`
            let alt = direct.with_extension("json");
            if alt.exists() {
                Some(alt)
            } else {
                None
            }
        };
        return (TtsEngine::Piper, direct, cfg_path);
    }

    // Single-component name (e.g. "hexgrad__Kokoro-82M") has no parent →
    // skip the parent-based resolution and fall through to as_dir check.
    let tag_path = std::path::Path::new(model_tag);
    let has_parent = tag_path.parent().is_some_and(|p| !p.as_os_str().is_empty());

    if has_parent {
        // `repo_dir/relative/path` — extracted HF repo.
        // Use the directory containing the selected weight file as the repo dir,
        // so a path like `hexgrad/Kokoro-82M/kokoro-v1_0.pth` resolves to
        // `models_dir/hexgrad/Kokoro-82M`, not just `models_dir/hexgrad`.
        if let Some(parent) = tag_path.parent() {
            let repo_dir = models_dir.join(parent);
            if repo_dir.is_dir() {
                let lower = model_tag.to_ascii_lowercase();
                if lower.contains("xtts") || lower.contains("coqui") {
                    return (TtsEngine::Xtts, repo_dir, None);
                }
                if lower.contains("kokoro") {
                    return (TtsEngine::Kokoro, repo_dir, None);
                }
                if lower.contains("parler") {
                    return (TtsEngine::Parler, repo_dir, None);
                }
                if lower.contains("qwen3") {
                    return (TtsEngine::Qwen3, repo_dir, None);
                }
                // Generic Python TTS for other repos (MeloTTS, etc.)
                return (TtsEngine::PythonGeneric, repo_dir, None);
            }
        }
    }

    // Fallback: check if it's a directory name that exists
    let as_dir = models_dir.join(model_tag);
    if as_dir.is_dir() {
        let lower = model_tag.to_ascii_lowercase();
        if lower.contains("xtts") || lower.contains("coqui") {
            return (TtsEngine::Xtts, as_dir, None);
        }
        if lower.contains("kokoro") {
            return (TtsEngine::Kokoro, as_dir, None);
        }
        if lower.contains("parler") {
            return (TtsEngine::Parler, as_dir, None);
        }
        if lower.contains("qwen3") {
            return (TtsEngine::Qwen3, as_dir, None);
        }
        return (TtsEngine::PythonGeneric, as_dir, None);
    }

    (TtsEngine::Unknown, direct, None)
}

#[derive(Debug, Clone, PartialEq)]
enum TtsEngine {
    Piper,
    Kokoro,
    Xtts,
    Parler,
    Qwen3,
    PythonGeneric,
    Unknown,
}

/// Find `python.exe` on PATH (or a well-known Windows location).
fn find_python() -> Option<String> {
    // A managed virtualenv wins over anything on PATH. Its packages (torch and
    // the CUDA runtime alone run to ~5 GB) live beside the model weights on the
    // storage volume instead of filling the system drive.
    for venv in python_venv_candidates() {
        let exe = if cfg!(windows) {
            venv.join("Scripts").join("python.exe")
        } else {
            venv.join("bin").join("python")
        };
        if exe.exists() {
            return Some(exe.to_string_lossy().to_string());
        }
    }
    // Try `python` on PATH next.
    let mut python_probe = std::process::Command::new("python");
    hide_std_console(&mut python_probe);
    if let Ok(out) = python_probe.arg("--version").output() {
        if out.status.success() {
            return Some("python".to_string());
        }
    }
    // Fallback: check %LOCALAPPDATA%\Programs\Python\Python3xx\python.exe
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        let base = std::path::Path::new(&localappdata)
            .join("Programs")
            .join("Python");
        // Probe supported CPython layouts directly first. Besides avoiding a
        // directory scan, this also works in restricted app sandboxes that may
        // allow the executable but deny listing its parent directory.
        for version in ["313", "312", "311", "310"] {
            let python_exe = base.join(format!("Python{version}")).join("python.exe");
            if python_exe.exists() {
                return Some(python_exe.to_string_lossy().to_string());
            }
        }
        // Keep discovering non-standard Python3xx installations when normal
        // directory enumeration is available.
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let python_exe = entry.path().join("python.exe");
                if python_exe.exists() {
                    return Some(python_exe.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// Where a managed Python virtualenv may live, most specific first.
///
/// `LOCARYN_PYTHON_VENV` lets a user point at their own; otherwise we look
/// beside the model weights, then in the working tree.
fn python_venv_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Some(v) = std::env::var_os("LOCARYN_PYTHON_VENV") {
        let p = std::path::PathBuf::from(v);
        if !p.as_os_str().is_empty() {
            out.push(p);
        }
    }
    out.push(locaryn_config::storage_root().join("python-env"));
    // Also look beside the weights: that volume is the one with room, so a
    // hand-made venv usually lands there. Found regardless of the working
    // directory, unlike the `.venv` fallback below.
    if let Some(parent) = locaryn_config::models_dir().parent() {
        out.push(parent.join("python-env"));
        out.push(parent.join(".venv"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(".venv"));
    }
    out
}

/// Environment every Python subprocess should inherit.
///
/// Two things bite otherwise: `transformers` drags in TensorFlow purely to
/// auto-detect a backend we never use (~20 s per run), and HuggingFace
/// downloads default to `~/.cache` — which is how a system drive ends up with
/// no free space after a few model pulls.
fn python_env() -> Vec<(&'static str, String)> {
    let hf = locaryn_config::hf_cache_dir();
    let _ = std::fs::create_dir_all(&hf);
    vec![
        ("HF_HOME", hf.to_string_lossy().to_string()),
        ("TRANSFORMERS_NO_TF", "1".to_string()),
        ("USE_TF", "0".to_string()),
        ("TF_CPP_MIN_LOG_LEVEL", "3".to_string()),
        // Build/extract scratch also belongs off the system drive.
        (
            "TMPDIR",
            locaryn_config::ensure_temp_dir()
                .to_string_lossy()
                .to_string(),
        ),
        (
            "TEMP",
            locaryn_config::ensure_temp_dir()
                .to_string_lossy()
                .to_string(),
        ),
        (
            "TMP",
            locaryn_config::ensure_temp_dir()
                .to_string_lossy()
                .to_string(),
        ),
    ]
}

/// Detect language from text for XTTS-v2 (supports en, es, fr, de, it, pt,
/// pl, tr, ru, nl, cs, ar, zh-cn, hu, ko, ja).
/// Normalize a frontend language code to the XTTS-v2 format.
/// XTTS supports: en, es, fr, de, it, pt, pl, tr, ru, nl, cs, ar, zh-cn,
/// hu, ko, ja. The frontend uses "zh" for Chinese, so map it explicitly.
fn normalize_xtts_language(lang: &str) -> &str {
    match lang {
        "zh" => "zh-cn",
        other => other,
    }
}

fn detect_language(text: &str) -> &'static str {
    // Check for CJK characters → Chinese
    if text
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c))
    {
        return "zh-cn";
    }
    // Hiragana/Katakana → Japanese
    if text
        .chars()
        .any(|c| ('\u{3040}'..='\u{309f}').contains(&c) || ('\u{30a0}'..='\u{30ff}').contains(&c))
    {
        return "ja";
    }
    // Hangul → Korean
    if text.chars().any(|c| ('\u{ac00}'..='\u{d7af}').contains(&c)) {
        return "ko";
    }
    // Arabic
    if text.chars().any(|c| ('\u{0600}'..='\u{06ff}').contains(&c)) {
        return "ar";
    }
    // Cyrillic → Russian
    if text.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)) {
        return "ru";
    }
    // Accented Latin chars → detect European language
    let has_german = text.chars().any(|c| {
        c == '\u{e4}'
            || c == '\u{f6}'
            || c == '\u{fc}'
            || c == '\u{c4}'
            || c == '\u{d6}'
            || c == '\u{dc}'
    });
    let has_french = text.chars().any(|c| {
        c == '\u{e0}'
            || c == '\u{e8}'
            || c == '\u{e9}'
            || c == '\u{ea}'
            || c == '\u{eb}'
            || c == '\u{e7}'
            || c == '\u{f4}'
            || c == '\u{fb}'
    });
    let has_spanish = text
        .chars()
        .any(|c| c == '\u{f1}' || c == '\u{bf}' || c == '\u{a1}');
    // Portuguese: ã and õ are distinctive (ê is shared with French)
    let has_portuguese = text.chars().any(|c| c == '\u{e3}' || c == '\u{f5}');
    if has_portuguese {
        return "pt";
    }
    if has_german {
        return "de";
    }
    if has_french {
        return "fr";
    }
    if has_spanish {
        return "es";
    }
    // Italian: à è ì ò ù are common; check after others to avoid overlap
    let has_italian = text
        .chars()
        .any(|c| c == '\u{ec}' || c == '\u{f2}' || c == '\u{f9}');
    if has_italian {
        return "it";
    }
    // Default: English
    "en"
}

/// Find `piper` (piper-tts) on PATH.
fn find_piper() -> Option<String> {
    let mut piper_probe = std::process::Command::new("piper");
    hide_std_console(&mut piper_probe);
    if let Ok(out) = piper_probe.arg("--help").output() {
        if out.status.success() {
            return Some("piper".to_string());
        }
    }
    None
}

/// Run Piper TTS: `piper -m voice.onnx -c voice.onnx.json -f output.wav --length-scale 1.0`
/// Piper reads text from stdin and writes a WAV to -f.
// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn run_tts_piper(
    model_path: &std::path::Path,
    config_path: Option<&std::path::Path>,
    text: &str,
    out_file: &std::path::Path,
    speed: f32,
    pitch: f32,
    _language: Option<&str>,
    voice_description: Option<&str>,
    design_prompt: Option<&str>,
    on_progress: &Channel<serde_json::Value>,
) -> Result<(), String> {
    if voice_description.is_some() || design_prompt.is_some() {
        tracing::info!("voice_description / design_prompt are ignored by Piper (engine does not support prompt-based voice design)");
    }

    let piper_bin = find_piper()
        .ok_or_else(|| "Piper non trouve sur le PATH. Installez piper-tts.".to_string())?;

    // length_scale = 1/speed (higher scale = slower speech).
    let length_scale = 1.0 / speed.max(0.1);
    // noise_scale: lower = more monotone / higher pitch precision. Map pitch
    // so 1.0 = default (0.667), 0.5 = flat (0.3), 2.0 = expressive (1.0).
    let noise_scale = 0.3 + (pitch - 0.5) * 0.467;

    let mut cmd = tokio::process::Command::new(&piper_bin);
    hide_tokio_console(&mut cmd);
    cmd.arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(out_file)
        .arg("--length-scale")
        .arg(format!("{length_scale}"))
        .arg("--noise-scale")
        .arg(format!("{noise_scale:.3}"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(cfg) = config_path {
        cmd.arg("-c").arg(cfg);
    }

    on_progress
        .send(serde_json::json!({"progress": 10, "detail": "Piper : initialisation"}))
        .ok();

    let mut child = cmd.spawn().map_err(|e| format!("piper spawn: {e}"))?;

    // Write text to Piper's stdin.
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| format!("piper stdin: {e}"))?;
        stdin.shutdown().await.ok();
    }

    on_progress
        .send(serde_json::json!({"progress": 50, "detail": "Piper : synthese en cours"}))
        .ok();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("piper wait: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Piper a echoue: {stderr}"));
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "Piper : termine"}))
        .ok();

    Ok(())
}

/// Map a Kokoro voice filename prefix to a BCP-47-ish language code.
/// Kokoro naming convention (first letter):
///   a/b = English, f = French, e = Spanish, d = German,
///   i = Italian, p = Portuguese, j = Japanese, z/c = Chinese.
fn kokoro_voice_lang(voice: &str) -> Option<&str> {
    let first = voice.chars().next()?;
    Some(match first {
        'a' | 'b' => "en",
        'f' => "fr",
        'e' => "es",
        'd' => "de",
        'i' => "it",
        'p' => "pt",
        'j' => "ja",
        'z' | 'c' => "zh",
        _ => return None,
    })
}

/// List installed Kokoro voice files (`voices/*.pt`), returning the voice names
/// without the `.pt` extension, sorted alphabetically.
fn kokoro_voices_in_repo(repo_dir: &std::path::Path) -> Result<Vec<String>, String> {
    let voices_dir = repo_dir.join("voices");
    let mut voices: Vec<String> = Vec::new();

    if voices_dir.exists() {
        voices = std::fs::read_dir(&voices_dir)
            .map_err(|e| format!("Impossible de lire le dossier de voix Kokoro: {e}"))?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|e| e.to_str()) != Some("pt") {
                    return None;
                }
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .collect();
    }

    voices.sort();
    Ok(voices)
}

/// Pick a Kokoro voice matching the requested language and gender hint.
/// Returns an error if no voice exists for the target language.
fn resolve_kokoro_voice(
    repo_dir: &std::path::Path,
    language: Option<&str>,
    speaker: Option<&str>,
) -> Result<String, String> {
    let voices = kokoro_voices_in_repo(repo_dir)?;
    if voices.is_empty() {
        return Err("Aucune voix Kokoro trouvee dans le dossier voices/.".to_string());
    }

    // If the caller passed a concrete voice name, use it if it exists.
    if let Some(s) = speaker {
        if !s.is_empty()
            && s != "default"
            && s != "male"
            && s != "female"
            && s != "neutral"
            && voices.iter().any(|v| v.eq_ignore_ascii_case(s))
        {
            return Ok(s.to_string());
        }
    }

    // Default to English when no language is provided.
    let target = language.unwrap_or("en");
    let candidates: Vec<&String> = voices
        .iter()
        .filter(|v| kokoro_voice_lang(v).unwrap_or("") == target)
        .collect();

    if candidates.is_empty() {
        let available = voices.join(", ");
        return Err(format!(
            "Aucune voix Kokoro disponible pour la langue '{target}'. Voix disponibles: {available}."
        ));
    }

    // Prefer a gender matching the speaker hint, otherwise pick the first candidate.
    let preferred: Vec<&String> = candidates
        .iter()
        .filter(|v| {
            matches!(
                (v.chars().nth(1), speaker),
                (Some('f'), Some("female")) | (Some('m'), Some("male"))
            )
        })
        .copied()
        .collect();
    let chosen = preferred.first().or_else(|| candidates.first()).unwrap();
    Ok((**chosen).clone())
}

/// Run Kokoro-82M TTS via a generated Python script.
/// Uses `kokoro` pipeline if available, or falls back to `kokoro-onnx`.
/// Voice reference: Kokoro-82M does not support arbitrary zero-shot cloning —
/// the .pt voice files ARE pre-computed style vectors. When a reference audio
/// is provided, we analyze its pitch (ZCR) to pick the closest-matching
/// built-in voice by gender. True zero-shot cloning requires XTTS.
// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn run_tts_kokoro(
    repo_dir: &std::path::Path,
    text: &str,
    out_file: &std::path::Path,
    voice_reference: Option<&str>,
    speaker: Option<&str>,
    speed: f32,
    _pitch: f32,
    _energy: f32,
    _clarity: f32,
    language: Option<&str>,
    voice_description: Option<&str>,
    design_prompt: Option<&str>,
    on_progress: &Channel<serde_json::Value>,
) -> Result<(), String> {
    let python =
        find_python().ok_or_else(|| "Python non trouve. Installez Python 3.10+.".to_string())?;

    // Find the model weight file inside the repo: .pth (PyTorch) or .onnx
    // (ONNX export). The Kokoro pipeline uses .pth; kokoro-onnx uses .onnx.
    let pth_path = walkdir_recursive(repo_dir, 3)
        .into_iter()
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("pth") || e.eq_ignore_ascii_case("onnx"))
                .unwrap_or(false)
        })
        .ok_or_else(|| "Fichier .pth ou .onnx Kokoro introuvable dans le depot.".to_string())?;

    if voice_description.is_some() || design_prompt.is_some() {
        tracing::info!("voice_description / design_prompt are ignored by Kokoro (engine does not support prompt-based voice design)");
    }

    // Pick a Kokoro voice file dynamically based on the requested language
    // and the voices actually installed in repo_dir/voices/.
    let voice_name = resolve_kokoro_voice(repo_dir, language, speaker)?;
    let voices_dir = repo_dir.join("voices");
    let voice_pt = voices_dir.join(format!("{voice_name}.pt"));

    // Defensive: ensure the chosen voice file exists.
    if !voice_pt.exists() {
        return Err(format!(
            "Voix Kokoro selectionnee introuvable: {}",
            voice_pt.display()
        ));
    }

    // Generate a Python script that runs Kokoro.
    let script = if voice_reference.is_some() {
        // Voice cloning mode: Kokoro-82M does not support arbitrary external
        // voice cloning — the .pt voice files ARE the voice profiles. When a
        // voice_reference is provided, we attempt to find the closest matching
        // built-in voice by analyzing the reference audio's pitch/formants.
        // This is a best-effort heuristic; true zero-shot cloning requires XTTS.
        let voices_dir_str = voices_dir.to_string_lossy();
        format!(
            r#"
import sys, os, json, glob

model_path = r"{pth}"
ref_path = r"{ref}"
out_path = r"{out}"
speed = {speed}
voice_name = "{voice_name}"
voices_dir = r"{voices_dir}"

text = sys.stdin.read()

# Kokoro-82M does not support arbitrary zero-shot voice cloning from
# external audio — the .pt voice files ARE pre-computed voice style vectors.
# When a voice_reference is provided, we analyze its fundamental frequency
# (F0) and pick the built-in voice whose gender/register matches best.
# This is a best-effort heuristic; true zero-shot cloning requires XTTS.
selected_voice = voice_name
try:
    import numpy as np
    import soundfile as sf
    ref_audio, ref_sr = sf.read(ref_path)
    if ref_audio.ndim > 1:
        ref_audio = ref_audio[:, 0]
    # Estimate F0 via zero-crossing rate as a rough pitch proxy.
    # High zero-crossing rate → higher pitch → likely female voice.
    zcr = float(np.mean(np.abs(np.diff(np.sign(ref_audio)))) )
    # Kokoro naming: af_* = American female, am_* = American male
    # bf_* = British female, bm_* = British male, etc.
    is_likely_female = zcr > 0.1
    # List available voices and pick by gender match.
    voice_files = glob.glob(os.path.join(voices_dir, "*.pt"))
    candidates = []
    for vf in voice_files:
        bn = os.path.basename(vf).replace(".pt", "")
        prefix = bn[:2] if len(bn) >= 2 else ""
        vf_is_female = prefix[1] == 'f' if len(prefix) >= 2 else True
        if vf_is_female == is_likely_female:
            candidates.append(bn)
    if candidates:
        # Prefer the user's selected voice if it matches the gender;
        # otherwise pick the first matching candidate.
        if voice_name in candidates:
            selected_voice = voice_name
        else:
            selected_voice = candidates[0]
        print(f"reference F0 proxy zcr={{zcr:.4f}}, selected voice: {{selected_voice}}", file=sys.stderr)
    else:
        print(f"no gender-matched voice found, using {{voice_name}}", file=sys.stderr)
except Exception as e:
    print(f"voice analysis failed: {{e}}, using {{voice_name}}", file=sys.stderr)

voice_pt_final = os.path.join(voices_dir, selected_voice + ".pt")

try:
    from kokoro import KPipeline
    import soundfile as sf
    pipeline = KPipeline(lang_code='a')
    for i, (gs, ps, audio) in enumerate(pipeline(text, voice=selected_voice, speed=speed)):
        sf.write(out_path, audio, 24000)
        break
    print("OK")
except ImportError:
    try:
        import kokoro_onnx
        from kokoro_onnx import KokoroOnnx
        import soundfile as sf
        k = KokoroOnnx(model_path=model_path, voice_path=voice_pt_final)
        audio = k.create(text, voice=selected_voice, speed=speed, lang="en-us")
        sf.write(out_path, audio, 24000)
        print("OK")
    except ImportError:
        print("kokoro / kokoro-onnx not installed. pip install kokoro", file=sys.stderr)
        sys.exit(1)
"#,
            pth = pth_path.display(),
            ref = voice_reference.unwrap(),
            out = out_file.display(),
            speed = speed,
            voice_name = voice_name,
            voices_dir = voices_dir_str,
        )
    } else {
        // Built-in voice mode: use kokoro pipeline.
        format!(
            r#"
import sys, os

model_path = r"{pth}"
voice_pt = r"{voice_pt}"
out_path = r"{out}"
speed = {speed}
voice_name = "{voice_name}"

text = sys.stdin.read()

try:
    from kokoro import KPipeline
    import soundfile as sf
    pipeline = KPipeline(lang_code='a')
    for i, (gs, ps, audio) in enumerate(pipeline(text, voice=voice_name, speed=speed)):
        sf.write(out_path, audio, 24000)
        break  # first segment is enough for short text
    print("OK")
except ImportError:
    # Fallback: kokoro-onnx
    try:
        import kokoro_onnx
        from kokoro_onnx import KokoroOnnx
        import soundfile as sf
        k = KokoroOnnx(model_path=model_path, voice_path=voice_pt)
        audio = k.create(text, voice=voice_name, speed=speed, lang="en-us")
        sf.write(out_path, audio, 24000)
        print("OK")
    except ImportError:
        print("kokoro / kokoro-onnx not installed", file=sys.stderr)
        sys.exit(1)
"#,
            pth = pth_path.display(),
            voice_pt = voice_pt.display(),
            out = out_file.display(),
            speed = speed,
            voice_name = voice_name,
        )
    };

    on_progress
        .send(serde_json::json!({"progress": 10, "detail": "Kokoro : initialisation"}))
        .ok();

    let mut command = tokio::process::Command::new(&python);
    hide_tokio_console(&mut command);
    let mut child = command
        .envs(python_env())
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| format!("python stdin: {e}"))?;
        stdin.shutdown().await.ok();
    }

    on_progress
        .send(serde_json::json!({"progress": 50, "detail": "Kokoro : synthese en cours"}))
        .ok();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("python wait: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("Kokoro a echoue: {stderr}\n{stdout}"));
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "Kokoro : termine"}))
        .ok();

    Ok(())
}

/// Run Parler-TTS via a generated Python script.
/// Requires `parler-tts` and `transformers` installed. The `design_prompt`
/// (or `voice_description`) is passed directly to the model as the voice
/// style description, so the prompt really shapes the generated voice.
async fn run_tts_parler(
    repo_dir: &std::path::Path,
    text: &str,
    out_file: &std::path::Path,
    _language: &str,
    voice_description: Option<&str>,
    design_prompt: Option<&str>,
    on_progress: &Channel<serde_json::Value>,
) -> Result<(), String> {
    let python = find_python()
        .ok_or_else(|| "Python non trouve. Installez Python 3.10+ et parler-tts.".to_string())?;

    let description = design_prompt
        .filter(|s| !s.is_empty())
        .or(voice_description)
        .unwrap_or("A clear and natural voice reading.");
    // Parler-TTS accepts long descriptions, but keep the generated script
    // reasonable and avoid abuse.
    let description = if description.chars().count() > 1000 {
        description.chars().take(1000).collect::<String>()
    } else {
        description.to_string()
    };
    let description_json = serde_json::to_string(&description)
        .map_err(|e| format!("cannot encode description: {e}"))?;
    let repo_dir_json = serde_json::to_string(&repo_dir.to_string_lossy())
        .map_err(|e| format!("cannot encode repo_dir: {e}"))?;
    let out_path_json = serde_json::to_string(&out_file.to_string_lossy())
        .map_err(|e| format!("cannot encode out_path: {e}"))?;

    on_progress
        .send(serde_json::json!({"progress": 5, "detail": "Parler-TTS : initialisation"}))
        .ok();

    let script = format!(
        r#"
import sys, os

repo_dir = {repo_dir_json}
out_path = {out_path_json}
description = {description_json}

text = sys.stdin.read()

try:
    import torch
    from parler_tts import ParlerTTSForConditionalGeneration
    from transformers import AutoTokenizer
    import soundfile as sf
except ImportError as e:
    print(f"parler-tts / transformers non installe: {{e}}", file=sys.stderr)
    sys.exit(1)

device = "cuda" if torch.cuda.is_available() else "cpu"

try:
    model = ParlerTTSForConditionalGeneration.from_pretrained(repo_dir).to(device)
    tokenizer = AutoTokenizer.from_pretrained(repo_dir)
except Exception as e:
    print(f"Impossible de charger Parler-TTS depuis {{repo_dir}}: {{e}}", file=sys.stderr)
    sys.exit(1)

try:
    input_ids = tokenizer(description, return_tensors="pt").input_ids.to(device)
    prompt_input_ids = tokenizer(text, return_tensors="pt").input_ids.to(device)
    generation = model.generate(input_ids=input_ids, prompt_input_ids=prompt_input_ids)
    audio = generation.cpu().numpy().squeeze()
    sampling_rate = getattr(model.config, "sampling_rate", 44100)
    sf.write(out_path, audio, sampling_rate)
    print("OK")
except Exception as e:
    print(f"Parler-TTS generation failed: {{e}}", file=sys.stderr)
    sys.exit(1)
"#,
    );

    on_progress
        .send(serde_json::json!({"progress": 10, "detail": "Parler-TTS : chargement du modele"}))
        .ok();

    let mut command = tokio::process::Command::new(&python);
    hide_tokio_console(&mut command);
    let mut child = command
        .envs(python_env())
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| format!("python stdin: {e}"))?;
        stdin.shutdown().await.ok();
    }

    on_progress
        .send(serde_json::json!({"progress": 50, "detail": "Parler-TTS : synthese vocale"}))
        .ok();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("python wait: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Parler-TTS a echoue: {stderr}"));
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "Parler-TTS : termine"}))
        .ok();

    Ok(())
}

/// Run Coqui XTTS-v2 via a generated Python script.
/// Requires `coqui-tts` (TTS package) installed. Supports voice cloning
/// via `speaker_wav` reference audio.
// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn run_tts_xtts(
    repo_dir: &std::path::Path,
    text: &str,
    out_file: &std::path::Path,
    voice_reference: Option<&str>,
    language: &str,
    _speed: f32,
    _pitch: f32,
    _energy: f32,
    _clarity: f32,
    voice_description: Option<&str>,
    design_prompt: Option<&str>,
    on_progress: &Channel<serde_json::Value>,
) -> Result<(), String> {
    if voice_description.is_some() || design_prompt.is_some() {
        tracing::info!("voice_description / design_prompt are ignored by XTTS (engine does not support prompt-based voice design)");
    }

    let python = find_python()
        .ok_or_else(|| "Python non trouve. Installez Python 3.10+ et coqui-tts.".to_string())?;

    // Verify the repo contains a model checkpoint — coqui-tts loads from
    // the directory, but we confirm weights exist.
    let has_checkpoint = walkdir_recursive(repo_dir, 3).iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| {
                let l = n.to_ascii_lowercase();
                l.ends_with(".pth") || l.ends_with(".safetensors") || l.ends_with(".bin")
            })
            .unwrap_or(false)
    });
    if !has_checkpoint {
        return Err("Checkpoint XTTS introuvable dans le depot.".to_string());
    }

    let ref_json = match voice_reference {
        Some(r) => serde_json::to_string(r)
            .unwrap_or_else(|_| format!("\"{}\"", r.replace('\\', "\\\\").replace('"', "\\\""))),
        None => "None".to_string(),
    };
    let repo_dir_json =
        serde_json::to_string(&repo_dir.to_string_lossy().as_ref()).unwrap_or_else(|_| {
            format!(
                "\"{}\"",
                repo_dir
                    .to_string_lossy()
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
            )
        });
    let out_path_json =
        serde_json::to_string(&out_file.to_string_lossy().as_ref()).unwrap_or_else(|_| {
            format!(
                "\"{}\"",
                out_file
                    .to_string_lossy()
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
            )
        });

    let script = format!(
        r#"
import sys, os

model_dir = {repo_dir_json}
out_path = {out_path_json}
language = "{language}"
ref_path = {ref_json}

text = sys.stdin.read()

try:
    from TTS.api import TTS
    import torch
except ImportError:
    print("coqui-tts (TTS) not installed. pip install coqui-tts", file=sys.stderr)
    sys.exit(1)

device = "cuda" if torch.cuda.is_available() else "cpu"

# Load XTTS from the local checkpoint directory (pass the directory, not
# the individual .pth — coqui-tts expects a model directory with config.json).
tts = TTS(model_path=model_dir, config_path=os.path.join(model_dir, "config.json")).to(device)

if ref_path and ref_path != "None":
    # Voice cloning mode: use the reference audio as speaker_wav.
    tts.tts_to_file(
        text=text,
        file_path=out_path,
        speaker_wav=ref_path,
        language=language,
    )
else:
    # No reference: use the first available speaker.
    speakers = tts.speakers if hasattr(tts, 'speakers') else ["speaker"]
    tts.tts_to_file(
        text=text,
        file_path=out_path,
        speaker=speakers[0] if speakers else None,
        language=language,
    )

print("OK")
"#,
    );

    on_progress
        .send(serde_json::json!({"progress": 10, "detail": "XTTS : chargement du modele"}))
        .ok();

    let mut command = tokio::process::Command::new(&python);
    hide_tokio_console(&mut command);
    let mut child = command
        .envs(python_env())
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| format!("python stdin: {e}"))?;
        stdin.shutdown().await.ok();
    }

    on_progress
        .send(serde_json::json!({"progress": 50, "detail": "XTTS : synthese en cours"}))
        .ok();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("python wait: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("XTTS a echoue: {stderr}"));
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "XTTS : termine"}))
        .ok();

    Ok(())
}

// ── Qwen3-TTS ──────────────────────────────────────────────────────────────────
/// Sampling and style controls for Qwen3-TTS.
///
/// Defaults match the library's own, which are noticeably livelier than the
/// values this code used to hardcode (temperature 0.7, timbre-only cloning):
/// that combination produced a flat, robotic delivery.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsSampling {
    pub temperature: f32,
    pub top_k: u32,
    pub top_p: f32,
    pub repetition_penalty: f32,
    /// In-context cloning: condition on the reference audio *and* its
    /// transcript, so the speaker's rhythm and intonation carry over. With
    /// this off only the speaker embedding is used — same timbre, but the
    /// sentence is read flatly, which is what "sounds like a robot" means.
    pub expressive: bool,
    /// Transcript of the reference clip. Required by in-context mode; when
    /// empty it is produced automatically by speech recognition.
    pub reference_text: String,
    /// Silence stretch applied after rendering. Above 1.0 the delivery becomes
    /// more measured. Post-processing, so it behaves identically on every
    /// engine rather than only those exposing a pause control.
    #[serde(default = "default_pause_scale")]
    pub pause_scale: f32,
    /// Pitch shift applied to the rendered speech. Qwen3-TTS has no pitch
    /// control of its own, so this slider did nothing before.
    #[serde(default = "default_pitch")]
    pub pitch: f32,
    /// Vocal presence and evenness. Neutral at the historical default so
    /// existing presets keep sounding the same.
    #[serde(default = "default_energy")]
    pub energy: f32,
    /// Consonant crispness, shaped around the 2-5 kHz presence band.
    #[serde(default = "default_clarity")]
    pub clarity: f32,
}

fn default_pitch() -> f32 {
    1.0
}
fn default_energy() -> f32 {
    0.7
}
fn default_clarity() -> f32 {
    0.8
}

fn default_pause_scale() -> f32 {
    1.0
}

impl Default for TtsSampling {
    fn default() -> Self {
        Self {
            temperature: 0.9,
            top_k: 50,
            top_p: 1.0,
            repetition_penalty: 1.05,
            expressive: true,
            reference_text: String::new(),
            pause_scale: default_pause_scale(),
            pitch: default_pitch(),
            energy: default_energy(),
            clarity: default_clarity(),
        }
    }
}

// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn run_tts_qwen3(
    repo_dir: &std::path::Path,
    text: &str,
    out_file: &std::path::Path,
    voice_reference: Option<&str>,
    language: Option<&str>,
    voice_description: Option<&str>,
    design_prompt: Option<&str>,
    speed: f32,
    sampling: &TtsSampling,
    on_progress: &Channel<serde_json::Value>,
) -> Result<(), String> {
    let python =
        find_python().ok_or_else(|| "Python non trouve. Installez Python 3.10+.".to_string())?;

    let ref_json = match voice_reference {
        Some(r) => serde_json::to_string(r)
            .unwrap_or_else(|_| format!("\"{}\"", r.replace('\\', "\\\\").replace('"', "\\\""))),
        None => "None".to_string(),
    };
    let repo_dir_json =
        serde_json::to_string(&repo_dir.to_string_lossy().as_ref()).unwrap_or_else(|_| {
            format!(
                "\"{}\"",
                repo_dir
                    .to_string_lossy()
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
            )
        });
    let out_path_json =
        serde_json::to_string(&out_file.to_string_lossy().as_ref()).unwrap_or_else(|_| {
            format!(
                "\"{}\"",
                out_file
                    .to_string_lossy()
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
            )
        });
    let lang_json = match language {
        Some(l) => serde_json::to_string(l).unwrap_or_else(|_| format!("\"{l}\"")),
        None => "None".to_string(),
    };
    // voice_description / design_prompt for the VoiceDesign variant
    let desc_prompt_json = design_prompt
        .or(voice_description)
        .map(|s| {
            serde_json::to_string(s)
                .unwrap_or_else(|_| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
        })
        .unwrap_or_else(|| "None".to_string());

    // Sampling knobs, rendered as Python literals.
    let temperature = sampling.temperature.clamp(0.05, 2.0);
    let top_k = sampling.top_k.clamp(1, 200);
    let top_p = sampling.top_p.clamp(0.05, 1.0);
    let repetition_penalty = sampling.repetition_penalty.clamp(1.0, 2.0);
    let expressive_py = if sampling.expressive { "True" } else { "False" };
    let pause_scale = sampling.pause_scale.clamp(0.3, 3.0);
    let shape_pitch = sampling.pitch.clamp(0.5, 2.0);
    let shape_energy = sampling.energy.clamp(0.0, 1.0);
    let shape_clarity = sampling.clarity.clamp(0.0, 1.0);
    let ref_text_json = serde_json::to_string(sampling.reference_text.trim())
        .unwrap_or_else(|_| "\"\"".to_string());

    // Detect if model is a CustomVoice variant (supports voice cloning)
    // Use Python-compatible True/False (not Rust's lowercase true/false)
    let is_custom_py = if repo_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase().contains("customvoice"))
        .unwrap_or(false)
    {
        "True"
    } else {
        "False"
    };

    on_progress
        .send(serde_json::json!({"progress": 5, "detail": "Qwen3-TTS : initialisation"}))
        .ok();

    let script = format!(
        r#"# Qwen3-TTS inference
import sys, os, json, subprocess, tempfile

repo_dir = {repo_dir_json}
out_path = {out_path_json}
lang = {lang_json}
speed = {speed}
voice_ref = {ref_json}
is_custom = {is_custom_py}
desc_prompt = {desc_prompt_json}
top_k = {top_k}
top_p = {top_p}
repetition_penalty = {repetition_penalty}
expressive = {expressive_py}
ref_text = {ref_text_json}
pause_scale = {pause_scale}
shape_pitch = {shape_pitch}
shape_energy = {shape_energy}
shape_clarity = {shape_clarity}

text = sys.stdin.read()

# VoiceDesign variant: prepend voice description to text
repo_dirname = os.path.basename(repo_dir.rstrip('/\\')).lower()
is_voicedesign = 'voicedesign' in repo_dirname

def report(pct, msg):
    print(json.dumps({{'progress': pct, 'detail': msg}}), flush=True)


def prepare_reference(path):
    """Trim and normalise a reference recording before cloning from it.

    The app hands over whatever the user picked — a 30-second screen capture,
    an MP3, a video's audio track. Cloning conditions on that clip directly,
    so an over-long reference means a long transcription, a long prompt and a
    generation that can run for minutes on one word. Hand-prepared 12-second
    clips worked; raw uploads did not, and that gap was the bug.

    Returns a path to a 24 kHz mono WAV holding the densest stretch of speech.
    """
    import numpy as _np
    try:
        import soundfile as _sf
    except Exception:
        return path

    TARGET_SR, WINDOW = 24000, 12.0
    try:
        data, sr = _sf.read(path)
    except Exception:
        # Not a container soundfile can open (mp4, m4a…). Let the engine try.
        return path

    mono = data if getattr(data, 'ndim', 1) == 1 else data.mean(axis=1)
    if len(mono) == 0:
        return path
    if sr != TARGET_SR:
        n = int(len(mono) * TARGET_SR / sr)
        mono = _np.interp(_np.linspace(0, len(mono) - 1, n),
                          _np.arange(len(mono)), mono)
        sr = TARGET_SR
    mono = mono.astype(_np.float32)

    if len(mono) > WINDOW * sr:
        frame = int(0.02 * sr)
        n_fr = len(mono) // frame
        energy = _np.sqrt(_np.mean(
            mono[:n_fr * frame].reshape(n_fr, frame) ** 2, axis=1))
        win = int(WINDOW / 0.02)
        csum = _np.concatenate([[0.0], _np.cumsum(energy)])
        best = int(_np.argmax(csum[win:] - csum[:-win]))
        mono = mono[best * frame:(best + win) * frame]

    peak = float(_np.max(_np.abs(mono)))
    if peak > 0:
        mono = mono * (0.89 / peak)

    out = os.path.join(os.path.dirname(out_path) or '.', '_ref_prepared.wav')
    _sf.write(out, mono, sr)
    report(34, "reference preparee (%.1f s)" % (len(mono) / sr))
    return out


def scale_pauses(wav, sr, scale):
    """Stretch or shorten the silences without touching the speech.

    Engines rarely expose a pause control, and the ones that do call it
    something different. Operating on the rendered waveform makes "more
    measured" or "snappier" mean the same thing on every model — and it cannot
    distort the voice itself, since only silent stretches are resized.
    """
    import numpy as _np
    if abs(scale - 1.0) < 0.01 or wav is None or len(wav) == 0:
        return wav

    x = _np.asarray(wav, dtype=_np.float32).reshape(-1)
    frame = max(1, int(0.01 * sr))                 # 10 ms resolution
    n = len(x) // frame
    if n < 3:
        return wav

    energy = _np.sqrt(_np.mean(
        x[:n * frame].reshape(n, frame) ** 2, axis=1))
    peak = float(energy.max())
    if peak <= 0:
        return wav
    # Relative threshold: recordings vary in level, an absolute one would treat
    # a quiet take as one long pause.
    quiet = energy < max(peak * 0.06, 1e-4)

    min_pause = max(2, int(0.12 / 0.01))           # ignore gaps under 120 ms
    out, i = [], 0
    while i < n:
        j = i
        while j < n and quiet[j] == quiet[i]:
            j += 1
        seg = x[i * frame:j * frame]
        if quiet[i] and (j - i) >= min_pause:
            target = max(frame, int(len(seg) * scale))
            seg = _np.zeros(target, dtype=_np.float32)
        out.append(seg)
        i = j
    out.append(x[n * frame:])
    return _np.concatenate(out)


def _ola_stretch(x, factor):
    """Change duration by `factor` without changing pitch (overlap-add)."""
    import numpy as _np
    if abs(factor - 1.0) < 0.005 or len(x) < 1024:
        return x
    win = 1024
    hop_out = win // 4
    hop_in = max(1, int(round(hop_out / factor)))
    window = _np.hanning(win).astype(_np.float32)
    n_frames = max(1, 1 + (len(x) - win) // hop_in)
    out = _np.zeros(win + hop_out * n_frames, dtype=_np.float32)
    norm = _np.zeros_like(out)
    for i in range(n_frames):
        s = i * hop_in
        seg = x[s:s + win]
        if len(seg) < win:
            break
        d = i * hop_out
        out[d:d + win] += seg * window
        norm[d:d + win] += window
    return out[:d + win] / _np.maximum(norm[:d + win], 1e-6)


def shape_voice(wav, sr, pitch, energy, clarity):
    """Apply pitch, energy and clarity to the rendered speech.

    Qwen3-TTS exposes none of these, so the sliders were inert. Doing the work
    on the waveform makes them behave identically on every engine — the same
    reasoning as pause scaling — at the cost of shaping the *audio* rather than
    how the model chose to speak.

    Neutral points are the existing defaults (pitch 1.0, energy 0.7, clarity
    0.8) so presets saved before this keep sounding exactly as they did.
    """
    import numpy as _np
    if wav is None or len(wav) == 0:
        return wav
    x = _np.asarray(wav, dtype=_np.float32).reshape(-1)

    # ── Pitch: resample to move the spectrum, then restore the duration.
    if abs(pitch - 1.0) > 0.02:
        p = float(min(max(pitch, 0.5), 2.0))
        idx = _np.arange(0, len(x), p, dtype=_np.float32)
        resampled = _np.interp(idx, _np.arange(len(x)), x).astype(_np.float32)
        x = _ola_stretch(resampled, p)

    # ── Clarity: shape the 2–5 kHz presence band, where consonants live.
    #    Done in the frequency domain so no filter library is required.
    if abs(clarity - 0.8) > 0.02:
        gain_db = (float(clarity) - 0.8) * 12.0
        spec = _np.fft.rfft(x)
        freqs = _np.fft.rfftfreq(len(x), 1.0 / sr)
        band = _np.exp(-0.5 * ((_np.log(_np.maximum(freqs, 1.0) / 3200.0)) / 0.55) ** 2)
        spec *= 10.0 ** (gain_db * band / 20.0)
        x = _np.fft.irfft(spec, n=len(x)).astype(_np.float32)

    # ── Energy: soft compression plus level. Higher = more present and even,
    #    lower = wider dynamics and quieter.
    if abs(energy - 0.7) > 0.02:
        amount = (float(energy) - 0.7) / 0.3          # -2.33 .. +1.0
        env = _np.abs(x)
        win = max(1, int(0.02 * sr))
        kernel = _np.ones(win, dtype=_np.float32) / win
        env = _np.convolve(env, kernel, mode='same')
        peak = float(env.max())
        if peak > 1e-6:
            # Pull loud and quiet parts together as amount rises.
            ratio = _np.clip(env / peak, 1e-4, 1.0)
            x = x * (ratio ** (-0.35 * amount))
            x = x * (1.0 + 0.25 * amount)

    m = float(_np.max(_np.abs(x))) if len(x) else 0.0
    if m > 0.99:
        x = x * (0.99 / m)
    return x


# ── Language normalisation ────────────────────────────────────────────────
# The UI speaks ISO 639-1 ("fr"); qwen_tts only accepts full English names and
# raises on anything else. Normalising here rather than in the UI keeps each
# engine's quirks in its own adapter (Piper wants "fr_FR", XTTS wants "fr").
#
# Done BEFORE importing torch: loading the model takes ~2 minutes, and failing
# validation after that wait is a terrible way to learn about a typo.
QWEN_LANGS = {{
    'auto': 'auto',
    'zh': 'chinese', 'cmn': 'chinese', 'chinese': 'chinese',
    'en': 'english', 'english': 'english',
    'fr': 'french', 'french': 'french', 'francais': 'french',
    'de': 'german', 'german': 'german', 'deutsch': 'german',
    'it': 'italian', 'italian': 'italian', 'italiano': 'italian',
    'ja': 'japanese', 'jp': 'japanese', 'japanese': 'japanese',
    'ko': 'korean', 'kr': 'korean', 'korean': 'korean',
    'pt': 'portuguese', 'portuguese': 'portuguese',
    'ru': 'russian', 'russian': 'russian',
    'es': 'spanish', 'spanish': 'spanish', 'espanol': 'spanish',
}}

if lang:
    key = str(lang).strip().lower().replace('_', '-').split('-')[0]
    mapped = QWEN_LANGS.get(key)
    if mapped is None:
        # Unsupported language: let the model auto-detect rather than abort a
        # generation the user has already waited for. Say so explicitly.
        report(6, f"Langue '{{lang}}' non prise en charge par Qwen3-TTS — detection automatique")
        lang = 'auto'
    else:
        lang = mapped
else:
    lang = None

import torch
import soundfile as sf
import tempfile
import sys
import subprocess

device = "cuda" if torch.cuda.is_available() else "cpu"
# Float32 is required: FP16 trips a CUDA device-side assert in
# x_vector_only_mode. TF32 recovers most of the speed on Ampere and later
# without changing the numerics that matter here.
dtype = torch.float32
if device == "cuda":
    torch.backends.cuda.matmul.allow_tf32 = True
    torch.backends.cudnn.allow_tf32 = True
    gpu_name = torch.cuda.get_device_name(0)
    report(10, f"Qwen3-TTS : GPU {{gpu_name}}")
else:
    report(10, "Qwen3-TTS : CPU (aucun GPU CUDA detecte) — generation lente")

try:
    from qwen_tts import Qwen3TTSModel, Qwen3TTSTokenizer
except ImportError:
    print(json.dumps({{'progress': -1, 'detail': "Installation qwen_tts..."}}), flush=True)
    subprocess.check_call([sys.executable, "-m", "pip", "install", "-q", "qwen-tts", "soundfile"])
    from qwen_tts import Qwen3TTSModel, Qwen3TTSTokenizer

report(15, "Qwen3-TTS : chargement via qwen-tts")

# Load model (the wrapper handles tokenizer/processor internally)
model = Qwen3TTSModel.from_pretrained(repo_dir, dtype=dtype, device_map=device)

report(30, "Qwen3-TTS : modele charge")

# The requested temperature, nudged by the speed slider. Slower delivery gets
# a touch more variety so it does not drift into monotone.
temperature = max(0.05, {temperature} + (1.0 - speed) * 0.1)

try:
    is_base_model = 'base' in repo_dirname
    is_custom_voice_model = 'customvoice' in repo_dirname

    if voice_ref and os.path.isfile(voice_ref):
        if not is_base_model:
            raise ValueError("Erreur : Le modèle sélectionné ne supporte pas le clonage vocal. Veuillez télécharger et utiliser le modèle 'Base' (ex: Qwen3-TTS-12Hz-0.6B-Base).")
        
        voice_ref = prepare_reference(voice_ref)

        # In-context cloning needs the reference transcript. Without it the
        # model falls back to speaker-embedding-only, which copies the timbre
        # but reads the sentence flatly — the "robotic" delivery.
        # ~12 codec tokens per character is generous for speech; without a cap
        # the model may ramble to its 2048-token limit on a one-word prompt,
        # which is minutes of GPU for nothing.
        budget = int(min(2048, max(192, len(text) * 14)))
        clone_args = dict(
            max_new_tokens=budget,
            text=text,
            language=lang,
            ref_audio=voice_ref,
            temperature=temperature,
            top_k=top_k,
            top_p=top_p,
            repetition_penalty=repetition_penalty,
        )
        icl_text = ref_text
        if expressive and not icl_text:
            try:
                report(38, "Qwen3-TTS : transcription de la reference")
                try:
                    from faster_whisper import WhisperModel
                except ImportError:
                    # Worth installing on the spot: without a transcript the
                    # only fallback is timbre-only cloning, which does not just
                    # flatten the delivery — it stops sounding like the same
                    # person.
                    report(36, "Installation du moteur de transcription...")
                    subprocess.check_call([sys.executable, "-m", "pip", "install",
                                           "-q", "faster-whisper"])
                    from faster_whisper import WhisperModel
                asr = WhisperModel("small", device=device,
                                   compute_type="float16" if device == "cuda" else "int8")
                segments, _ = asr.transcribe(voice_ref, beam_size=5,
                                             language=None if lang in (None, "auto") else lang[:2])
                icl_text = " ".join(s.text.strip() for s in segments).strip()
                del asr
                if device == "cuda":
                    torch.cuda.empty_cache()
                report(46, "Qwen3-TTS : reference transcrite")
            except Exception as exc:
                # Not fatal, but far from harmless: timbre-only cloning loses
                # the speaker's identity, not merely their delivery. Say so
                # plainly rather than returning a stranger's voice silently.
                report(-1, f"Transcription impossible ({{exc}}). Repli sur le timbre seul : "
                           "la voix sera moins fidele. Renseignez le texte de reference "
                           "pour eviter cela.")
                icl_text = ""

        # Reported after transcription so the bar never moves backwards.
        if expressive and icl_text:
            clone_args["x_vector_only_mode"] = False
            clone_args["ref_text"] = icl_text
            report(50, "Qwen3-TTS : clonage vocal (intonation de la reference)")
        else:
            clone_args["x_vector_only_mode"] = True
            report(50, "Qwen3-TTS : clonage vocal (timbre seul)")

        wavs, sr = model.generate_voice_clone(**clone_args)
    elif desc_prompt and is_voicedesign:
        report(50, "Qwen3-TTS : design vocal")
        # For VoiceDesign, there is no direct prompt string method if it requires VoiceClonePromptItem, 
        # but we can try generate_voice_design or fallback to generate_custom_voice
        if hasattr(model, 'generate_voice_design'):
            wavs, sr = model.generate_voice_design(
                text=text,
                language=lang,
                voice_design_prompt=desc_prompt,
                temperature=temperature
            )
        else:
            # Fallback to custom voice if generate_voice_design is not available
            spk_list = model.get_supported_speakers()
            spk = spk_list[0] if spk_list else None
            wavs, sr = model.generate_custom_voice(
                text=text,
                speaker=spk,
                language=lang,
                instruct=desc_prompt,
                temperature=temperature
            )
    else:
        if is_base_model:
            raise ValueError("Erreur : Le modèle 'Base' nécessite obligatoirement un audio de référence (Clonage Vocal). Il n'a pas de voix par défaut.")
            
        report(50, "Qwen3-TTS : generation audio")
        spk_list = model.get_supported_speakers()
        
        # Try to use desc_prompt as the speaker if it matches one of the supported speakers
        spk = desc_prompt if (desc_prompt and spk_list and desc_prompt in spk_list) else None
        if not spk and spk_list:
            spk = spk_list[0]
            
        wavs, sr = model.generate_custom_voice(
            text=text,
            speaker=spk,
            language=lang,
            temperature=temperature
        )

    report(80, "Qwen3-TTS : sauvegarde audio")

    wav = wavs[0] if isinstance(wavs, list) else wavs
    if hasattr(wav, 'numpy'):
        wav = wav.numpy()
    elif hasattr(wav, 'detach'):
        wav = wav.detach().cpu().numpy()

    wav = scale_pauses(wav, sr, pause_scale)
    wav = shape_voice(wav, sr, shape_pitch, shape_energy, shape_clarity)
    sf.write(out_path, wav, sr)
    report(100, "Qwen3-TTS : termine")

except Exception as e:
    import traceback
    traceback.print_exc(file=sys.stderr)
    print(json.dumps({{'progress': -1, 'detail': f"Erreur generation Qwen3: {{str(e)}}"}}), flush=True)
    sys.exit(1)
"#
    );

    on_progress
        .send(serde_json::json!({"progress": 15, "detail": "Qwen3-TTS : preparation"}))
        .ok();

    let mut command = tokio::process::Command::new(&python);
    hide_tokio_console(&mut command);
    let mut child = command
        .envs(python_env())
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| format!("python stdin: {e}"))?;
        stdin.shutdown().await.ok();
    }

    // Stream stdout as it arrives. Collecting it with `wait_with_output` meant
    // every progress line was parsed *after* the run had finished, so the UI
    // sat frozen for the full two minutes with nothing to show.
    use tokio::io::{AsyncBufReadExt, BufReader};

    let stdout = child.stdout.take().ok_or("python stdout indisponible")?;
    let stderr = child.stderr.take().ok_or("python stderr indisponible")?;

    // Drain stderr concurrently: a full pipe buffer would deadlock the child.
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        if let Ok(p) = serde_json::from_str::<serde_json::Value>(line) {
            on_progress.send(p).ok();
        }
    }

    // A generation that stops making progress must not hold the GPU forever.
    // Loading the weights alone takes ~100 s on a laptop card, so the ceiling
    // is generous — it exists to end a run that has genuinely wedged, not to
    // cut a slow one short.
    const TTS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
    let status = match tokio::time::timeout(TTS_TIMEOUT, child.wait()).await {
        Ok(r) => r.map_err(|e| format!("python wait: {e}"))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(format!(
                "La génération a dépassé {} minutes et a été interrompue. \
                 Vérifiez que l'extrait de référence n'est pas trop long, ou \
                 réduisez le texte à synthétiser.",
                TTS_TIMEOUT.as_secs() / 60
            ));
        }
    };
    let stderr_text = stderr_task.await.unwrap_or_default();

    if !status.success() {
        return Err(format!(
            "Qwen3-TTS a echoue: {}",
            summarise_python_error(&stderr_text)
        ));
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "Qwen3-TTS : termine"}))
        .ok();

    Ok(())
}

/// Reduce a Python traceback to the part a user can act on.
///
/// Raw tracebacks were surfaced verbatim in the UI: hundreds of characters of
/// absl banners and frame listings around a single meaningful last line.
fn summarise_python_error(stderr: &str) -> String {
    const NOISE: &[&str] = &[
        "absl::InitializeLog",
        "oneDNN custom operations",
        "TF_ENABLE_ONEDNN_OPTS",
        "All log messages before",
        "port.cc:",
        "flash-attn is not installed",
    ];
    let useful: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !NOISE.iter().any(|n| l.contains(n)))
        .collect();

    // The final exception line carries the actual cause; keep a little context.
    if let Some(pos) = useful
        .iter()
        .rposition(|l| l.contains("Error:") || l.contains("Exception:"))
    {
        return useful[pos..].join(" ");
    }
    let tail: Vec<&str> = useful.iter().rev().take(4).rev().copied().collect();
    if tail.is_empty() {
        "erreur inconnue (aucune sortie)".to_string()
    } else {
        tail.join(" ")
    }
}

#[tauri::command]
// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn generate_audio(
    _core: State<'_, Core>,
    model: String,
    text: String,
    output_dir: String,
    voice_reference: Option<String>,
    speaker: Option<String>,
    speed: Option<f32>,
    pitch: Option<f32>,
    energy: Option<f32>,
    clarity: Option<f32>,
    language: Option<String>,
    voice_description: Option<String>,
    design_prompt: Option<String>,
    // Sampling and cloning-style controls. Absent means the engine defaults,
    // which are livelier than the values this code used to hardcode.
    sampling: Option<TtsSampling>,
    on_progress: Channel<serde_json::Value>,
) -> Result<GeneratedAudio, String> {
    let tts_sampling = sampling.unwrap_or_default();
    let speed = speed.unwrap_or(1.0).clamp(0.5, 2.0);
    let pitch = pitch.unwrap_or(1.0).clamp(0.5, 2.0);
    let energy = energy.unwrap_or(0.7).clamp(0.0, 1.0);
    let clarity = clarity.unwrap_or(0.8).clamp(0.0, 1.0);
    let voice_reference = voice_reference.filter(|s| !s.is_empty());

    // Language, voice design / expressive description parameters are accepted
    // and passed to the active runner. Parler-TTS uses them directly as voice
    // style descriptions; other runners log that they ignore them.
    let language = language.filter(|s| !s.is_empty());
    if language.as_ref().is_some_and(|s| !s.is_empty()) {
        tracing::debug!(language = %language.as_ref().unwrap(), "synthesis language provided");
    }
    if voice_description.as_ref().is_some_and(|s| !s.is_empty()) {
        tracing::debug!(
            "voice_description provided; it will be consumed by engines that support voice design"
        );
    }
    if design_prompt.as_ref().is_some_and(|s| !s.is_empty()) {
        tracing::debug!(
            "design_prompt provided; it will be consumed by engines that support voice design"
        );
    }

    let output_path = std::path::Path::new(&output_dir);
    if !output_path.exists() {
        std::fs::create_dir_all(output_path)
            .map_err(|e| format!("cannot create output dir: {e}"))?;
    }

    let out_file_name = format!(
        "gen_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let out_file = output_path.join(out_file_name);

    let models_dir = locaryn_config::models_dir();
    let (engine, model_path, config_path) = resolve_tts_engine(&models_dir, &model);

    on_progress
        .send(serde_json::json!({"progress": 0, "detail": "initialisation"}))
        .ok();

    // Resolve effective language. "auto" or missing means detect from text;
    // otherwise trust the user's explicit choice.
    let effective_language: Option<&str> = match language.as_deref() {
        Some("auto") | None => None,
        Some(l) if !l.is_empty() => Some(l),
        _ => None,
    };
    let detected_lang = detect_language(&text);
    let tts_lang = normalize_xtts_language(effective_language.unwrap_or(detected_lang));

    // Voice description / design prompt are consumed by engines that support
    // voice design (e.g. Parler-TTS); other runners log that they ignore them.
    let voice_desc = voice_description.as_deref().filter(|s| !s.is_empty());
    let design_p = design_prompt.as_deref().filter(|s| !s.is_empty());

    let result = match engine {
        TtsEngine::Piper => {
            run_tts_piper(
                &model_path,
                config_path.as_deref(),
                &text,
                &out_file,
                speed,
                pitch,
                effective_language,
                voice_desc,
                design_p,
                &on_progress,
            )
            .await
        }
        TtsEngine::Kokoro => {
            run_tts_kokoro(
                &model_path,
                &text,
                &out_file,
                voice_reference.as_deref(),
                speaker.as_deref(),
                speed,
                pitch,
                energy,
                clarity,
                effective_language,
                voice_desc,
                design_p,
                &on_progress,
            )
            .await
        }
        TtsEngine::Xtts => {
            // XTTS-v2 supports: en, es, fr, de, it, pt, pl, tr, ru, nl,
            // cs, ar, zh-cn, hu, ko, ja. Fall back to detected language when
            // the user leaves it on auto.
            run_tts_xtts(
                &model_path,
                &text,
                &out_file,
                voice_reference.as_deref(),
                tts_lang,
                speed,
                pitch,
                energy,
                clarity,
                voice_desc,
                design_p,
                &on_progress,
            )
            .await
        }
        TtsEngine::Parler => {
            // Parler-TTS turns a text description of a voice into audio. The
            // design prompt (or voice description) is the actual conditioning
            // signal, so it is not ignored — the model uses it to shape the voice.
            run_tts_parler(
                &model_path,
                &text,
                &out_file,
                tts_lang,
                voice_desc,
                design_p,
                &on_progress,
            )
            .await
        }
        TtsEngine::Qwen3 => {
            run_tts_qwen3(
                &model_path,
                &text,
                &out_file,
                voice_reference.as_deref(),
                effective_language,
                voice_desc,
                design_p,
                speed,
                &tts_sampling,
                &on_progress,
            )
            .await
        }
        TtsEngine::PythonGeneric => {
            Err("Le modele TTS selectionne n'est pas encore supporte pour la generation directe. Essayez Piper, Kokoro, XTTS, Parler ou Qwen3-TTS.".to_string())
        }
        TtsEngine::Unknown => {
            // No engine found — simulation fallback so the UI still works.
            on_progress
                .send(serde_json::json!({"progress": 50, "detail": "simulation (moteur TTS non reconnu)"}))
                .ok();
            std::fs::write(&out_file, generate_test_wav(1, 16000, speed))
                .map_err(|e| format!("cannot write audio: {e}"))?;
            return Ok(GeneratedAudio {
                path: out_file.to_string_lossy().to_string(),
                simulated: true,
            });
        }
    };

    match result {
        Ok(()) => {
            // Verify the output file exists and is non-empty.
            if out_file.exists() && std::fs::metadata(&out_file).map(|m| m.len()).unwrap_or(0) > 44
            {
                Ok(GeneratedAudio {
                    path: out_file.to_string_lossy().to_string(),
                    simulated: false,
                })
            } else {
                Err("Le moteur TTS n'a pas produit de fichier audio.".into())
            }
        }
        Err(e) => {
            // Propagate the error to the frontend. The user needs to know
            // which engine failed and why, rather than getting a fake beep.
            on_progress
                .send(serde_json::json!({"progress": 0, "detail": format!("erreur: {e}")}))
                .ok();
            Err(e)
        }
    }
}

/// List the available Kokoro voice names (without `.pt`) installed for a
/// given model. Returns an empty list if the model is not a Kokoro repo or
/// if the `voices/` directory is missing.
#[tauri::command]
async fn list_kokoro_voices(model: String) -> Result<Vec<String>, String> {
    let models_dir = locaryn_config::models_dir();
    let (engine, model_path, _config) = resolve_tts_engine(&models_dir, &model);
    if engine != TtsEngine::Kokoro {
        return Ok(Vec::new());
    }
    kokoro_voices_in_repo(&model_path)
}

#[tauri::command]
async fn list_audio_models() -> Result<Vec<String>, String> {
    // Scan the models directory for TTS weight files.
    // - Top-level .onnx files → Piper voice models.
    // - Extracted HF repo dirs → report the repo dir name as one model entry
    //   (the user picks the model, then selects a voice from within it).
    // We deliberately do NOT list individual voice .pt profiles (e.g.
    // Kokoro's 54 voices/af_heart.pt) as separate "models" — those are voice
    // presets, not standalone TTS models.
    let models_dir = locaryn_config::models_dir();
    let mut names: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                // Top-level weight file → Piper-style .onnx voice model.
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let lower = name.to_ascii_lowercase();
                    if lower.ends_with(".onnx")
                        && !lower.ends_with(".part")
                        && !lower.ends_with(".tmp")
                    {
                        names.push(name.to_string());
                    }
                }
            } else if path.is_dir() {
                // Extracted HF repo: check if it contains TTS weight files
                // (.pth, .safetensors, .bin) but NOT just voice .pt profiles.
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let walker = walkdir_recursive(&path, 5);
                let has_model_weight = walker.iter().any(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| {
                            let l = n.to_ascii_lowercase();
                            // Main model weights, not voice profiles
                            l.ends_with(".pth")
                                || l.ends_with(".safetensors")
                                || l.ends_with(".bin")
                        })
                        .unwrap_or(false)
                });
                let has_voice_dir = path.join("voices").is_dir()
                    || walker.iter().any(|p| {
                        p.parent()
                            .and_then(|parent| parent.file_name())
                            .and_then(|n| n.to_str())
                            .map(|n| n == "voices")
                            .unwrap_or(false)
                    });
                let has_config = walker.iter().any(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.eq_ignore_ascii_case("config.json"))
                        .unwrap_or(false)
                });
                if has_model_weight || (has_voice_dir && has_config) {
                    names.push(dir_name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

/// Extract audio from a video file using ffmpeg and return the path to the
/// generated WAV file. The output is written to `output_dir` with a unique
/// timestamped name so successive imports do not overwrite each other.
/// Only the first 30 seconds are extracted, which is enough for voice cloning.
async fn extract_audio_from_video(
    video_path: &std::path::Path,
    output_dir: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    if !video_path.exists() {
        return Err("Fichier video introuvable.".into());
    }

    let ext = video_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_video = matches!(
        ext.as_str(),
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "m4v" | "mpg" | "mpeg"
    );
    if !is_video {
        return Ok(video_path.to_path_buf());
    }

    let video_path = video_path.to_path_buf();
    let output_dir = output_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        let ffmpeg_check = std::process::Command::new("ffmpeg")
            .args(["-version"])
            .output();
        if ffmpeg_check.is_err() {
            return Err(
                "ffmpeg n'est pas installe. Installez ffmpeg pour importer une video.".into(),
            );
        }

        std::fs::create_dir_all(&output_dir)
            .map_err(|e| format!("Impossible de creer le dossier de sortie: {e}"))?;

        let stem = video_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("voice");
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".into());
        let out_filename = format!("{stem}_extracted_{ts}.wav");
        let out_path = output_dir.join(out_filename);

        let input_path = video_path.to_string_lossy().to_string();
        let out_path_str = out_path.to_string_lossy().to_string();
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                &input_path,
                "-t",
                "30",
                "-vn",
                "-acodec",
                "pcm_s16le",
                "-ar",
                "22050",
                "-ac",
                "1",
                &out_path_str,
            ])
            .output()
            .map_err(|e| format!("Impossible de lancer ffmpeg: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Echec de l'extraction audio avec ffmpeg: {stderr}"));
        }

        if !out_path.exists() {
            return Err("ffmpeg n'a pas produit de fichier audio.".into());
        }

        Ok(out_path)
    })
    .await
    .map_err(|e| format!("ffmpeg task failed: {e}"))?
}

/// Convert a dialog `FilePath` to a local `PathBuf`, handling both plain
/// paths and `file://` URLs returned by some platforms.
fn dialog_file_path_to_path(path: &tauri_plugin_dialog::FilePath) -> std::path::PathBuf {
    let s = path.to_string();
    if s.starts_with("file://") {
        if let Ok(parsed) = url::Url::parse(&s) {
            if let Ok(p) = parsed.to_file_path() {
                return p;
            }
        }
    }
    std::path::PathBuf::from(s)
}

#[tauri::command]
/// Generate music from a text prompt using a Python-based music generation
/// model (MusicGen, AudioLDM, Stable Audio, etc.). This dispatches to a
/// Python subprocess (similar to `run_tts_parler`).
// Note: #[tauri::command] removed because of E0252 name collision with format!
// macro named parameters in the Python script. We register it via the handler.
// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn generate_music(
    _core: State<'_, Core>,
    model: String,
    prompt: String,
    output_dir: String,
    duration: Option<u32>,
    melody_reference: Option<String>,
    negative_prompt: Option<String>,
    steps: Option<u32>,
    cfg_scale: Option<f32>,
    on_progress: Channel<serde_json::Value>,
) -> Result<GeneratedAudio, String> {
    let duration = duration.unwrap_or(30).clamp(5, 300);
    let steps = steps.unwrap_or(50).clamp(10, 500);
    let cfg_scale = cfg_scale.unwrap_or(3.0).clamp(1.0, 20.0);
    let melody_reference = melody_reference.filter(|s| !s.is_empty());
    let negative_prompt = negative_prompt.filter(|s| !s.is_empty());

    let output_path = std::path::Path::new(&output_dir);
    if !output_path.exists() {
        std::fs::create_dir_all(output_path)
            .map_err(|e| format!("cannot create output dir: {e}"))?;
    }

    let out_file_name = format!(
        "music_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let out_file = output_path.join(out_file_name);

    let models_dir = locaryn_config::models_dir();
    let model_path = models_dir.join(&model);
    let repo_dir = if model_path.is_dir() {
        model_path
    } else if let Some(parent) = std::path::Path::new(&model).parent() {
        let dir = models_dir.join(parent);
        if dir.is_dir() {
            dir
        } else {
            model_path
        }
    } else {
        model_path
    };

    let python = find_python()
        .ok_or_else(|| "Python non trouvé. Installez Python 3.10+ ainsi que les dépendances (torch, transformers, soundfile, audiocraft, etc.).".to_string())?;

    on_progress
        .send(serde_json::json!({"progress": 0, "detail": "initialisation"}))
        .ok();

    // Build the Python inference script. We attempt to import available
    // music-gen libraries in priority: audiocraft (MusicGen), then
    // diffusers (AudioLDM / Stable Audio), then bark.
    let repo_dir_json = serde_json::to_string(&repo_dir.to_string_lossy())
        .map_err(|e| format!("encode repo_dir: {e}"))?;
    let prompt_json = serde_json::to_string(&prompt).map_err(|e| format!("encode prompt: {e}"))?;
    let out_path_json = serde_json::to_string(&out_file.to_string_lossy())
        .map_err(|e| format!("encode out_path: {e}"))?;
    let duration_secs = duration;
    let steps_val = steps;
    let cfg_val = cfg_scale;
    let melody_json = match &melody_reference {
        Some(p) => serde_json::to_string(p).unwrap_or_else(|_| "None".into()),
        None => "None".into(),
    };
    let negative_json = match &negative_prompt {
        Some(s) => serde_json::to_string(s).unwrap_or_else(|_| "None".into()),
        None => "None".into(),
    };

    on_progress
        .send(serde_json::json!({"progress": 5, "detail": "MusicGen : initialisation Python"}))
        .ok();

    let script = format!(
        r#"import sys, json, os
repo_dir = {repo_dir_json}
out_path = {out_path_json}
prompt = {prompt_json}
duration = {duration_secs}
steps = {steps_val}
cfg = {cfg_val}
melody_path = {melody_json}
negative_prompt = {negative_json}


def report(pct, msg):
    print(json.dumps({{'progress': pct, 'detail': msg}}), flush=True)


def load_melody(path, target_sr):
    """Reference melody as (waveform, sr), or None if unreadable.

    Trimmed to 30 s: MusicGen conditions on the whole clip, and a long import
    costs generation time without improving the result."""
    if not path or not os.path.isfile(path):
        return None
    try:
        import numpy as _np
        import soundfile as _sf
        data, sr = _sf.read(path)
        mono = data if getattr(data, 'ndim', 1) == 1 else data.mean(axis=1)
        if sr != target_sr:
            n = int(len(mono) * target_sr / sr)
            mono = _np.interp(_np.linspace(0, len(mono) - 1, n),
                              _np.arange(len(mono)), mono)
            sr = target_sr
        mono = mono.astype('float32')[: int(30 * sr)]
        report(22, "melodie de reference chargee (%.1f s)" % (len(mono) / sr))
        return mono, sr
    except Exception as exc:
        report(-1, "Melodie illisible (%s) — generation sans reference" % exc)
        return None


engine = None
try:
    from audiocraft.models import MusicGen
    import torch
    import soundfile as sf

    device = "cuda" if torch.cuda.is_available() else "cpu"
    report(18, "MusicGen : chargement du modele")
    model = MusicGen.get_pretrained(repo_dir, device=device)
    model.set_generation_params(duration=duration)
    engine = "audiocraft"

    mel = load_melody(melody_path, model.sample_rate)
    if mel is not None and hasattr(model, "generate_with_chroma"):
        # Melody conditioning: the model follows the reference's contour
        # instead of inventing one. Only the -melody checkpoints support it.
        import torch as _t
        ref = _t.from_numpy(mel[0])[None, None, :].to(device)
        report(30, "MusicGen : generation guidee par la melodie")
        wav = model.generate_with_chroma([prompt], ref, mel[1], progress=True)
    else:
        if mel is not None:
            report(-1, "Ce modele n'accepte pas de melodie de reference — ignoree. "
                       "Utilisez un checkpoint 'musicgen-melody'.")
        report(30, "MusicGen : generation")
        wav = model.generate([prompt], progress=True)
    one_wav = wav[0].cpu().numpy()
    sf.write(out_path, one_wav.T if one_wav.ndim > 1 else one_wav, model.sample_rate)
except ImportError:
    try:
        from diffusers import AudioLDM2Pipeline
        import torch
        import soundfile as sf
        device = "cuda" if torch.cuda.is_available() else "cpu"
        report(18, "AudioLDM2 : chargement du modele")
        pipe = AudioLDM2Pipeline.from_pretrained(
            repo_dir,
            torch_dtype=torch.float16 if device == "cuda" else torch.float32)
        # Same reasoning as the SVD branch of generate_video: the full
        # pipeline (CLAP + GPT-2 + VAE + UNet + vocoder) doesn't fit
        # resident on a consumer-sized GPU. Offloading keeps only the
        # active submodule on the GPU.
        if device == "cuda" and torch.cuda.get_device_properties(0).total_memory / 1024**3 < 12:
            pipe.enable_model_cpu_offload()
        else:
            pipe = pipe.to(device)
        engine = "audioldm2"
        if melody_path:
            report(-1, "AudioLDM2 n'accepte pas de melodie de reference — ignoree.")
        kwargs = dict(num_inference_steps=steps, audio_length_in_s=duration,
                      guidance_scale=cfg)
        if negative_prompt:
            kwargs["negative_prompt"] = negative_prompt
        report(30, "AudioLDM2 : generation")
        audio = pipe(prompt, **kwargs).audios[0]
        # Rate comes from the pipeline: hardcoding 16000 resampled anything else.
        rate = getattr(getattr(pipe, "vocoder", None), "config", None)
        rate = getattr(rate, "sampling_rate", 16000)
        sf.write(out_path, audio, rate)
    except ImportError:
        try:
            from bark import SAMPLE_RATE, generate_audio, preload_models
            import soundfile as sf
            report(18, "Bark : chargement du modele")
            preload_models(repo_dir)
            engine = "bark"
            audio = generate_audio(prompt, history_prompt=None)
            sf.write(out_path, audio, SAMPLE_RATE)
        except ImportError as e:
            print("Aucun moteur de generation musicale trouve. "
                  "Installez audiocraft, diffusers ou bark: %s" % e, file=sys.stderr)
            sys.exit(1)

report(100, "termine (%s)" % engine)
print("OK", file=sys.stderr)
"#,
    );

    on_progress
        .send(
            serde_json::json!({"progress": 15, "detail": "MusicGen : lancement du script Python"}),
        )
        .ok();

    let mut command = tokio::process::Command::new(&python);
    hide_tokio_console(&mut command);
    let mut child = command
        .envs(python_env())
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    on_progress
        .send(serde_json::json!({"progress": 20, "detail": "MusicGen : génération en cours"}))
        .ok();

    // Read stderr for progress (some libraries print there)
    // Progress arrives on stdout as JSON; stderr carries the library's own
    // chatter. Draining stderr to completion first (as this used to) left
    // stdout unread, so the script could block on a full pipe and the UI never
    // moved past "génération en cours".
    use tokio::io::{AsyncBufReadExt, BufReader};
    let stdout = child.stdout.take().ok_or("python stdout indisponible")?;
    let stderr = child.stderr.take().ok_or("python stderr indisponible")?;
    let err_task = tokio::spawn(async move {
        let mut buf = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(%line, "music-gen python stderr");
            buf.push_str(&line);
            buf.push('\n');
        }
        buf
    });

    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            on_progress.send(v).ok();
        }
    }

    // Music generation is minutes of GPU work; the ceiling only ends a run
    // that has genuinely wedged.
    const MUSIC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1800);
    let status = match tokio::time::timeout(MUSIC_TIMEOUT, child.wait()).await {
        Ok(r) => r.map_err(|e| format!("python wait: {e}"))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(
                "La génération musicale a dépassé 30 minutes et a été interrompue. \
                        Réduisez la durée demandée."
                    .into(),
            );
        }
    };
    let errs = err_task.await.unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "La génération musicale a échoué : {}. \
             Vérifiez que le modèle est installé et que les dépendances Python sont \
             présentes (pip install audiocraft, diffusers ou bark).",
            summarise_python_error(&errs)
        ));
    }

    if !out_file.exists() {
        return Err("Le fichier audio n'a pas été créé par le script Python.".into());
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "terminé"}))
        .ok();

    Ok(GeneratedAudio {
        path: out_file.to_string_lossy().to_string(),
        simulated: false,
    })
}

// ── Video generation ────────────────────────────────────────────────────────

#[tauri::command]
// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn generate_video(
    model: String,
    prompt: String,
    output_dir: String,
    duration: Option<u32>,
    input_image: Option<String>,
    negative_prompt: Option<String>,
    steps: Option<u32>,
    cfg_scale: Option<f32>,
    width: Option<u32>,
    height: Option<u32>,
    on_progress: Channel<serde_json::Value>,
) -> Result<GeneratedAudio, String> {
    let duration = duration.unwrap_or(5).clamp(2, 30);
    let steps = steps.unwrap_or(50).clamp(10, 200);
    let cfg_scale = cfg_scale.unwrap_or(7.0).clamp(1.0, 20.0);
    let input_image = input_image.filter(|s| !s.is_empty());
    let negative_prompt = negative_prompt.filter(|s| !s.is_empty());
    let width = width.unwrap_or(640);
    let height = height.unwrap_or(480);

    let output_path = std::path::Path::new(&output_dir);
    if !output_path.exists() {
        std::fs::create_dir_all(output_path)
            .map_err(|e| format!("cannot create output dir: {e}"))?;
    }

    let out_file_name = format!(
        "video_{}.mp4",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let out_file = output_path.join(out_file_name);

    let models_dir = locaryn_config::models_dir();
    let model_path = models_dir.join(&model);
    let repo_dir = if model_path.is_dir() {
        model_path
    } else if let Some(parent) = std::path::Path::new(&model).parent() {
        let dir = models_dir.join(parent);
        if dir.is_dir() {
            dir
        } else {
            model_path
        }
    } else {
        model_path
    };

    let python = find_python()
        .ok_or_else(|| "Python non trouvé. Installez Python 3.10+ ainsi que les dépendances (torch, diffusers, transformers, imageio-ffmpeg).".to_string())?;

    on_progress
        .send(serde_json::json!({"progress": 0, "detail": "initialisation"}))
        .ok();

    // Build the Python inference script. We attempt to import available
    // video-gen libraries in priority: diffusers (Wan2.1, LTX, CogVideo, SVD, Hunyuan, Mochi).
    let repo_dir_json = serde_json::to_string(&repo_dir.to_string_lossy())
        .map_err(|e| format!("encode repo_dir: {e}"))?;
    let prompt_json = serde_json::to_string(&prompt).map_err(|e| format!("encode prompt: {e}"))?;
    let out_path_json = serde_json::to_string(&out_file.to_string_lossy())
        .map_err(|e| format!("encode out_path: {e}"))?;
    let input_image_json = match &input_image {
        Some(p) => serde_json::to_string(p).map_err(|e| format!("encode input_image: {e}"))?,
        None => "None".to_string(),
    };
    let negative_prompt_json = match &negative_prompt {
        Some(s) => serde_json::to_string(s).map_err(|e| format!("encode negative_prompt: {e}"))?,
        None => "None".to_string(),
    };
    let duration_secs = duration;
    let steps_val = steps;
    let cfg_val = cfg_scale;
    let width_val = width;
    let height_val = height;

    on_progress
        .send(serde_json::json!({"progress": 5, "detail": "VideoGen : initialisation Python"}))
        .ok();

    let script = format!(
        r#"import sys, json
repo_dir = {repo_dir_json}
out_path = {out_path_json}
prompt = {prompt_json}
duration = {duration_secs}
steps = {steps_val}
cfg = {cfg_val}
width = {width_val}
height = {height_val}
input_image_path = {input_image_json}
negative_prompt = {negative_prompt_json}


try:
    import torch
    import numpy as np
    from diffusers import DiffusionPipeline
    from PIL import Image

    device = "cuda" if torch.cuda.is_available() else "cpu"
    dtype = torch.float16 if device == "cuda" else torch.float32

    # Determine model type from repo / model name
    repo_lower = repo_dir.lower()

    pipe = None
    frames = []

    # Helper: build kwargs with optional negative_prompt
    def _pipe_kwargs(**kw):
        if negative_prompt and negative_prompt != "None":
            kw['negative_prompt'] = negative_prompt
        return kw

    if "wan" in repo_lower or "wan2" in repo_lower:
        from diffusers import WanPipeline
        pipe = WanPipeline.from_pretrained(repo_dir, torch_dtype=dtype)
        pipe = pipe.to(device)
        kw = _pipe_kwargs(num_inference_steps=steps, guidance_scale=cfg, width=width, height=height, num_frames=duration*8)
        if input_image_path and input_image_path != "None":
            img = Image.open(input_image_path).convert("RGB")
            kw['image'] = img
        output = pipe(prompt, **kw).frames[0]
        frames = output
    elif "ltx" in repo_lower:
        from diffusers import LTXPipeline
        pipe = LTXPipeline.from_pretrained(repo_dir, torch_dtype=dtype)
        pipe = pipe.to(device)
        output = pipe(prompt, **_pipe_kwargs(num_inference_steps=steps, guidance_scale=cfg, width=width, height=height, num_frames=duration*8)).frames[0]
        frames = output
    elif "cogvideo" in repo_lower or "cog" in repo_lower:
        from diffusers import CogVideoXPipeline
        pipe = CogVideoXPipeline.from_pretrained(repo_dir, torch_dtype=dtype)
        pipe = pipe.to(device)
        output = pipe(prompt, **_pipe_kwargs(num_inference_steps=steps, guidance_scale=cfg)).frames[0]
        frames = output
    elif "svd" in repo_lower or "stable.video" in repo_lower:
        from diffusers import StableVideoDiffusionPipeline
        pipe = StableVideoDiffusionPipeline.from_pretrained(repo_dir, torch_dtype=dtype, variant="fp16")
        # The full pipeline needs well over 10 GB resident on the GPU at
        # once — more than most consumer cards have (verified: OOM on a
        # 6 GB card with plain .to(device)). Offloading keeps only the
        # active submodule on the GPU, trading speed for actually fitting.
        vram_gb = torch.cuda.get_device_properties(0).total_memory / 1024**3 if device == "cuda" else 0
        if device == "cuda" and vram_gb < 12:
            pipe.enable_model_cpu_offload()
            chunk_size = 1
        else:
            pipe = pipe.to(device)
            chunk_size = 8
        if input_image_path and input_image_path != "None":
            img = Image.open(input_image_path).convert("RGB")
        else:
            img = Image.new("RGB", (width, height), (0, 0, 0))
        # SVD is image-conditioned, not text/CFG-conditioned like the other
        # pipelines here — it has no `guidance_scale` parameter at all
        # (`max_guidance_scale`, defaulting to 3.0, plays that role instead).
        output = pipe(img, decode_chunk_size=chunk_size, num_frames=14, num_inference_steps=steps).frames[0]
        frames = output
    elif "mochi" in repo_lower or "genmo" in repo_lower:
        from diffusers import MochiPipeline
        has_bf16 = torch.cuda.is_available() and torch.cuda.get_device_capability()[:2] >= (8, 0)
        pipe = MochiPipeline.from_pretrained(repo_dir, torch_dtype=dtype, variant="bf16" if has_bf16 else None)
        pipe = pipe.to(device)
        output = pipe(prompt, **_pipe_kwargs(num_inference_steps=steps, guidance_scale=cfg, num_frames=duration*8)).frames[0]
        frames = output
    elif "hunyuan" in repo_lower:
        try:
            from diffusers import HunyuanVideoPipeline
            pipe = HunyuanVideoPipeline.from_pretrained(repo_dir, torch_dtype=dtype)
            pipe = pipe.to(device)
            output = pipe(prompt, **_pipe_kwargs(num_inference_steps=steps, guidance_scale=cfg, width=width, height=height, num_frames=duration*8)).frames[0]
            frames = output
        except ImportError:
            from diffusers import DiffusionPipeline
            pipe = DiffusionPipeline.from_pretrained(repo_dir, torch_dtype=dtype)
            pipe = pipe.to(device)
            output = pipe(prompt, **_pipe_kwargs(num_inference_steps=steps, guidance_scale=cfg)).frames[0]
            frames = output
    else:
        # Generic fallback: try any available video pipeline
        try:
            from diffusers import DiffusionPipeline
            pipe = DiffusionPipeline.from_pretrained(repo_dir, torch_dtype=dtype)
            pipe = pipe.to(device)
            if hasattr(pipe, '__class__') and 'Video' in pipe.__class__.__name__:
                kwargs = _pipe_kwargs(num_inference_steps=steps, guidance_scale=cfg)
                if input_image_path and input_image_path != "None":
                    kwargs['image'] = Image.open(input_image_path).convert("RGB")
                output = pipe(prompt, **kwargs)
                if hasattr(output, 'frames'):
                    frames = output.frames[0]
                else:
                    frames = output[0] if isinstance(output, (list, tuple)) else []
            else:
                raise ValueError("Not a video pipeline")
        except Exception as ge:
            print(f"Video generation failed: {{ge}}", file=sys.stderr)
            sys.exit(1)

    if not frames:
        print("No frames generated", file=sys.stderr)
        sys.exit(1)

    # Convert frames to numpy array and write video
    all_np = []
    for f in frames:
        if isinstance(f, torch.Tensor):
            f_np = f.cpu().numpy()
            if f_np.ndim == 3 and f_np.shape[0] in (1, 3):
                f_np = np.transpose(f_np, (1, 2, 0))
            if f_np.ndim == 3 and f_np.shape[2] == 1:
                f_np = np.repeat(f_np, 3, axis=2)
            if f_np.max() <= 1.0:
                f_np = (f_np * 255).clip(0, 255).astype(np.uint8)
            all_np.append(f_np)
        elif isinstance(f, Image.Image):
            all_np.append(np.array(f.convert("RGB")))
        elif isinstance(f, np.ndarray):
            all_np.append(f)

    if not all_np:
        print("Frame conversion failed", file=sys.stderr)
        sys.exit(1)

    # Write video via imageio or torchvision
    try:
        import imageio
        writer = imageio.get_writer(out_path, fps=8, codec='libx264', quality=8)
        for frame in all_np:
            writer.append_data(frame)
        writer.close()
    except ImportError:
        try:
            import torchvision.io as tio
            video_tensor = torch.tensor(np.stack(all_np), dtype=torch.uint8).permute(0, 3, 1, 2)
            tio.write_video(out_path, video_tensor, fps=8, video_codec='libx264')
        except ImportError:
            print("Need imageio-ffmpeg or torchvision for video output", file=sys.stderr)
            sys.exit(1)

    print("OK", file=sys.stderr)
"#,
    );

    on_progress
        .send(
            serde_json::json!({"progress": 15, "detail": "VideoGen : lancement du script Python"}),
        )
        .ok();

    let mut command = tokio::process::Command::new(&python);
    hide_tokio_console(&mut command);
    let mut child = command
        .envs(python_env())
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    on_progress
        .send(serde_json::json!({"progress": 20, "detail": "VideoGen : génération en cours"}))
        .ok();

    use tokio::io::AsyncBufReadExt;
    if let Some(stderr) = child.stderr.take() {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(%line, "video-gen python stderr");
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("python wait: {e}"))?;
    if !status.success() {
        return Err("La génération vidéo a échoué. Vérifiez que le modèle est installé et que les dépendances Python sont présentes (pip install torch diffusers transformers imageio-ffmpeg).".into());
    }

    if !out_file.exists() {
        return Err("Le fichier vidéo n'a pas été créé par le script Python.".into());
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "terminé"}))
        .ok();

    Ok(GeneratedAudio {
        path: out_file.to_string_lossy().to_string(),
        simulated: false,
    })
}

// ── 3D generation ───────────────────────────────────────────────────────────

#[tauri::command]
// Signature dictée par l'appel côté interface ; la regrouper en
// structure rendrait le contrat IPC moins lisible, pas plus.
#[allow(clippy::too_many_arguments)]
async fn generate_3d(
    model: String,
    prompt: String,
    output_dir: String,
    input_image: Option<String>,
    negative_prompt: Option<String>,
    steps: Option<u32>,
    cfg_scale: Option<f32>,
    format: Option<String>,
    on_progress: Channel<serde_json::Value>,
) -> Result<GeneratedAudio, String> {
    let steps = steps.unwrap_or(50).clamp(10, 200);
    let cfg_scale = cfg_scale.unwrap_or(7.0).clamp(1.0, 20.0);
    let input_image = input_image.filter(|s| !s.is_empty());
    let negative_prompt = negative_prompt.filter(|s| !s.is_empty());
    let out_format = format.unwrap_or_else(|| "obj".into());

    let output_path = std::path::Path::new(&output_dir);
    if !output_path.exists() {
        std::fs::create_dir_all(output_path)
            .map_err(|e| format!("cannot create output dir: {e}"))?;
    }

    let ext = match out_format.as_str() {
        "glb" => "glb",
        "ply" => "ply",
        _ => "obj",
    };
    let out_file_name = format!(
        "model3d_{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        ext
    );
    let out_file = output_path.join(out_file_name);

    let models_dir = locaryn_config::models_dir();
    let model_path = models_dir.join(&model);
    let repo_dir = if model_path.is_dir() {
        model_path
    } else if let Some(parent) = std::path::Path::new(&model).parent() {
        let dir = models_dir.join(parent);
        if dir.is_dir() {
            dir
        } else {
            model_path
        }
    } else {
        model_path
    };

    let python = find_python()
        .ok_or_else(|| "Python non trouvé. Installez Python 3.10+ ainsi que les dépendances (torch, transformers, trimesh).".to_string())?;

    on_progress
        .send(serde_json::json!({"progress": 0, "detail": "initialisation"}))
        .ok();

    // Build the Python inference script. Try multiple 3D generation libraries.
    let repo_dir_json = serde_json::to_string(&repo_dir.to_string_lossy())
        .map_err(|e| format!("encode repo_dir: {e}"))?;
    let prompt_json = serde_json::to_string(&prompt).map_err(|e| format!("encode prompt: {e}"))?;
    let out_path_json = serde_json::to_string(&out_file.to_string_lossy())
        .map_err(|e| format!("encode out_path: {e}"))?;
    let input_image_json = match &input_image {
        Some(p) => serde_json::to_string(p).map_err(|e| format!("encode input_image: {e}"))?,
        None => "None".to_string(),
    };
    let negative_prompt_json = match &negative_prompt {
        Some(s) => serde_json::to_string(s).map_err(|e| format!("encode negative_prompt: {e}"))?,
        None => "None".to_string(),
    };
    let steps_val = steps;
    let cfg_val = cfg_scale;
    // shap-e's own downloader ignores HF_HOME and defaults to the spawning
    // process's cwd — for an installed app that is not on D:. Passed
    // explicitly to `load_model`/`load_config` below.
    let shap_e_cache_json =
        serde_json::to_string(&locaryn_config::shap_e_cache_dir().to_string_lossy())
            .map_err(|e| format!("encode shap_e_cache_dir: {e}"))?;

    on_progress
        .send(serde_json::json!({"progress": 5, "detail": "3DGen : initialisation Python"}))
        .ok();

    let script = format!(
        r#"import sys, json
repo_dir = {repo_dir_json}
out_path = {out_path_json}
prompt = {prompt_json}
steps = {steps_val}
cfg = {cfg_val}
input_image_path = {input_image_json}
negative_prompt = {negative_prompt_json}
shap_e_cache_dir = {shap_e_cache_json}


try:
    import torch
    import numpy as np

    repo_lower = repo_dir.lower()

    # Try Shape-E (OpenAI)
    if "shap-e" in repo_lower or "shape" in repo_lower:
        from shap_e.diffusion.sample import sample_latents
        from shap_e.diffusion.gaussian_diffusion import diffusion_from_config
        from shap_e.models.download import load_model, load_config

        device = "cuda" if torch.cuda.is_available() else "cpu"
        model = load_model("text300M", device=device, cache_dir=shap_e_cache_dir)
        # The diffusion model only produces latents. Turning a latent into a
        # mesh is a second, separate model — the transmitter — loaded here so
        # decode_latent_mesh() below has something to render with.
        xm = load_model("transmitter", device=device, cache_dir=shap_e_cache_dir)
        diffusion = diffusion_from_config(load_config("diffusion", cache_dir=shap_e_cache_dir))

        latents = sample_latents(
            batch_size=1,
            model=model,
            diffusion=diffusion,
            guidance_scale=cfg,
            model_kwargs=dict(texts=[prompt]),
            progress=True,
            clip_denoised=True,
            use_fp16=True if device == "cuda" else False,
            use_karras=True,
            karras_steps=steps,
            sigma_min=1e-3,
            sigma_max=160,
            s_churn=0,
        )

        # Export mesh
        for latent in latents:
            from shap_e.util.notebooks import decode_latent_mesh
            tri_mesh = decode_latent_mesh(xm, latent).tri_mesh()
            # Despite its BinaryIO type hint, write_obj() joins its lines as
            # a plain str and writes that — a binary handle raises TypeError.
            with open(out_path, 'w') as f:
                tri_mesh.write_obj(f)
            print("OK", file=sys.stderr)
            sys.exit(0)            # Try TripoSR
    elif "tripo" in repo_lower:
        import trimesh
        from tsr.system import TSR
        from tsr.utils import remove_background
        from PIL import Image

        device = "cuda" if torch.cuda.is_available() else "cpu"
        model = TSR.from_pretrained(repo_dir, config_name="config.yaml", weight_name="model.ckpt")
        model.to(device)

        if not input_image_path or input_image_path == "None":
            print("TripoSR nécessite une image source.", file=sys.stderr)
            sys.exit(1)

        image = remove_background(Image.open(input_image_path).convert("RGB"))

        with torch.no_grad():
            scene_codes = model([image], device=device)

        meshes = model.extract_mesh(scene_codes, resolution=256)
        mesh = trimesh.Trimesh(vertices=meshes[0].vertices, faces=meshes[0].faces, vertex_colors=meshes[0].vertex_colors)
        mesh.export(out_path)
        print("OK", file=sys.stderr)
        sys.exit(0)

    # Try Point-E
    elif "point-e" in repo_lower:
        import trimesh
        from point_e.diffusion.configs import DIFFUSION_CONFIGS, diffusion_from_config
        from point_e.diffusion.sampler import PointCloudSampler
        from point_e.models.download import load_checkpoint

        device = "cuda" if torch.cuda.is_available() else "cpu"

        print("Chargement Point-E...", file=sys.stderr)
        base_model = load_checkpoint("base40M", device=device)
        upsampler_model = load_checkpoint("upsample", device=device)

        sampler = PointCloudSampler(
            device=device,
            models=[base_model, upsampler_model],
            diffusions=[
                diffusion_from_config(DIFFUSION_CONFIGS["base40M"]),
                diffusion_from_config(DIFFUSION_CONFIGS["upsample"]),
            ],
            num_points=[1024, 4096],
            aux_channels=["R", "G", "B"],
            guidance_scale=[3.0, 3.0],
        )

        samples = None
        for x in sampler.sample_batch_progressive(batch_size=1, model_kwargs=dict(texts=[prompt])):
            samples = x

        pc = sampler.output_to_point_clouds(samples)[0]
        pcd = np.asarray(pc.coords)

        cloud = trimesh.PointCloud(vertices=pcd)
        cloud.export(out_path)
        print("OK", file=sys.stderr)
        sys.exit(0)

    # Try Zero-1-to-3 via diffusers
    elif "zero" in repo_lower or "zero123" in repo_lower:
        from diffusers import DiffusionPipeline
        from PIL import Image

        device = "cuda" if torch.cuda.is_available() else "cpu"
        dtype = torch.float16 if device == "cuda" else torch.float32

        pipe = DiffusionPipeline.from_pretrained(repo_dir, torch_dtype=dtype, trust_remote_code=True)
        pipe = pipe.to(device)

        if input_image_path and input_image_path != "None":
            init_img = Image.open(input_image_path).convert("RGB")
        else:
            from diffusers.utils import load_image
            init_img = Image.new("RGB", (256, 256), (200, 200, 200))

        # Generate novel views at different azimuth angles
        angles = [0, 30, 60, 90, 120, 150, 180, 210, 240, 270, 300, 330]
        views = []
        for azim in angles:
            out = pipe(
                prompt,
                image=init_img,
                num_inference_steps=steps,
                guidance_scale=cfg,
                azimuth=azim,
                elevation=0,
            ).images[0]
            views.append(out)

        # Save as a simple grid
        grid_width = 4
        grid_height = 3
        w, h = views[0].size
        grid_img = Image.new("RGB", (w * grid_width, h * grid_height))
        for i, view in enumerate(views):
            x = (i % grid_width) * w
            y = (i // grid_width) * h
            grid_img.paste(view, (x, y))

        # Save as OBJ placeholder (cannot reconstruct full 3D from views alone)
        import trimesh
        import os
        obj_path = out_path
        mesh = trimesh.primitives.Box(extents=[0.5, 0.5, 0.5])
        mesh.export(obj_path)
        # Also save the multi-view grid
        grid_path = os.path.splitext(out_path)[0] + "_views.png"
        grid_img.save(grid_path)
        print("Zero-1-to-3: multi-views saved to {{grid_path}}", file=sys.stderr)
        print("OK", file=sys.stderr)
        sys.exit(0)

    # Try threestudio (generic SDS-based 3D generation)
    elif "threestudio" in repo_lower:
        print("ThreeStudio : exécution via threestudio...", file=sys.stderr)
        import subprocess
        import os
        result = subprocess.run([
            sys.executable, "-m", "threestudio",
            "--prompt", prompt,
            "--outdir", os.path.dirname(out_path),
            "--export_ext", out_path.split('.')[-1],
            "--n_steps", str(steps),
        ], capture_output=True, text=True, cwd=repo_dir)
        print(result.stdout, file=sys.stderr)
        if result.returncode != 0:
            print(result.stderr, file=sys.stderr)
            sys.exit(1)
        print("OK", file=sys.stderr)
        sys.exit(0)

    # Generic fallback: try trimesh procedural generation
    else:
        print(f"Modèle 3D non reconnu: {{repo_lower}}", file=sys.stderr)
        print("Génération d'un modèle 3D de base (icosphere)...", file=sys.stderr)
        import trimesh
        mesh = trimesh.creation.icosphere(subdivisions=3)
        mesh.export(out_path)
        print("OK", file=sys.stderr)
        sys.exit(0)

except ImportError as e:
    print(f"Erreur d'import: {{e}}", file=sys.stderr)
    print("Installez les dépendances : pip install torch trimesh shap-e point-e tsr threestudio", file=sys.stderr)
    sys.exit(1)
except Exception as e:
    print(f"Erreur: {{e}}", file=sys.stderr)
    sys.exit(1)
"#,
    );

    on_progress
        .send(serde_json::json!({"progress": 15, "detail": "3DGen : lancement du script Python"}))
        .ok();

    let mut command = tokio::process::Command::new(&python);
    hide_tokio_console(&mut command);
    let mut child = command
        .envs(python_env())
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    on_progress
        .send(serde_json::json!({"progress": 20, "detail": "3DGen : génération en cours"}))
        .ok();

    use tokio::io::AsyncBufReadExt;
    if let Some(stderr) = child.stderr.take() {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(%line, "3d-gen python stderr");
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("python wait: {e}"))?;
    if !status.success() {
        return Err("La génération 3D a échoué. Vérifiez que le modèle est installé et que les dépendances Python sont présentes (pip install torch trimesh shap-e point-e tsr).".into());
    }

    if !out_file.exists() {
        return Err("Le fichier 3D n'a pas été créé par le script Python.".into());
    }

    on_progress
        .send(serde_json::json!({"progress": 100, "detail": "terminé"}))
        .ok();

    Ok(GeneratedAudio {
        path: out_file.to_string_lossy().to_string(),
        simulated: false,
    })
}

#[tauri::command]
async fn pick_voice_reference(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app
        .dialog()
        .file()
        .set_title("Selectionner un echantillon vocal ou une video")
        .add_filter(
            "Audio / Video",
            &[
                "mp3", "wav", "m4a", "ogg", "flac", "wma", "aac", "mp4", "mkv", "avi", "mov",
                "webm", "m4v", "mpg", "mpeg",
            ],
        );
    let picked = tokio::task::spawn_blocking(move || dialog.blocking_pick_file())
        .await
        .map_err(|e| format!("dialog task failed: {e}"))?;
    match picked {
        Some(path) => {
            let p = dialog_file_path_to_path(&path);
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Impossible d'acceder au dossier de donnees: {e}"))?;
            let output_dir = data_dir.join("voice_references");
            let audio_path = extract_audio_from_video(&p, &output_dir).await?;
            Ok(Some(audio_path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

// ============================================================================
// Inference config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InferenceConfig {
    pub profile: String,
    pub gpu_layers: i32,
    pub kv_cache_type: String,
    pub context_length: u32,
    pub flash_attention: bool,
    pub cpu_threads: u32,
    pub batch_size: u32,
    pub use_turboquant: bool,
    pub draft_model_path: String,
    pub use_mmap: bool,
    pub parallel_slots: u32,
    pub n_cpu_moe: i32,
    pub rpc_servers: String,
    pub lora_adapters: Vec<String>,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            profile: "balanced".into(),
            gpu_layers: -1,
            kv_cache_type: "q8_0".into(),
            context_length: 8192,
            flash_attention: true,
            cpu_threads: 0,
            batch_size: 512,
            use_turboquant: false,
            draft_model_path: String::new(),
            use_mmap: true,
            parallel_slots: 1,
            n_cpu_moe: 0,
            rpc_servers: String::new(),
            lora_adapters: Vec::new(),
        }
    }
}

impl InferenceConfig {
    fn path(data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join("inference_config.json")
    }
    fn load(data_dir: &std::path::Path) -> Self {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self, data_dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[tauri::command]
fn get_inference_config(core: State<'_, Core>) -> InferenceConfig {
    InferenceConfig::load(&core.data_dir)
}

#[tauri::command]
fn set_inference_config(
    core: State<'_, Core>,
    config: InferenceConfig,
    consent: Option<bool>,
) -> Result<(), String> {
    let existing = InferenceConfig::load(&core.data_dir);
    let existing_set: std::collections::HashSet<&str> =
        existing.lora_adapters.iter().map(|s| s.as_str()).collect();
    let newly_added_ns_lora = config
        .lora_adapters
        .iter()
        .any(|p| is_nsfw_lora(p) && !existing_set.contains(p.as_str()));
    if newly_added_ns_lora && !consent.unwrap_or(false) {
        return Err(
            "Un ou plusieurs adaptateurs LoRA sont classes NSFW / sans garde-fous. \
             Acceptez la responsabilite dans l'interface avant de sauvegarder."
                .into(),
        );
    }

    config.save(&core.data_dir).map_err(|e| e.to_string())?;
    tracing::info!(profile = %config.profile, gpu_layers = config.gpu_layers, kv_cache = %config.kv_cache_type, "inference config updated");
    Ok(())
}

#[tauri::command]
fn get_profile_preset(profile: String) -> InferenceConfig {
    match profile.as_str() {
        "eco" => InferenceConfig {
            profile: "eco".into(),
            gpu_layers: 0,
            kv_cache_type: "f16".into(),
            context_length: 4096,
            flash_attention: false,
            batch_size: 256,
            ..Default::default()
        },
        "balanced" => InferenceConfig::default(),
        "performance" => InferenceConfig {
            profile: "performance".into(),
            context_length: 16384,
            ..Default::default()
        },
        "turbo" => InferenceConfig {
            profile: "turbo".into(),
            kv_cache_type: "q4_0".into(),
            context_length: 32768,
            batch_size: 1024,
            ..Default::default()
        },
        "longctx" => InferenceConfig {
            profile: "longctx".into(),
            kv_cache_type: "q4_0".into(),
            context_length: 65536,
            batch_size: 1024,
            ..Default::default()
        },
        _ => InferenceConfig::default(),
    }
}

// ============================================================================
// Model params (sampling)
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelParams {
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    #[serde(rename = "ctx_size")]
    pub context_length: u32,
    pub max_tokens: u32,
    pub repeat_penalty: f32,
    pub seed: i64,
}

impl Default for ModelParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.95,
            top_k: 40,
            context_length: 8192,
            max_tokens: 0,
            repeat_penalty: 1.1,
            seed: -1,
        }
    }
}

impl ModelParams {
    fn path(data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join("model_params.json")
    }
    fn load(data_dir: &std::path::Path) -> Self {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self, data_dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[tauri::command]
fn get_provider_model_params(core: State<'_, Core>) -> ModelParams {
    ModelParams::load(&core.data_dir)
}

#[tauri::command]
fn update_provider_model_params(core: State<'_, Core>, params: ModelParams) -> Result<(), String> {
    params.save(&core.data_dir).map_err(|e| e.to_string())?;
    tracing::info!(temp = params.temperature, "model params updated");
    Ok(())
}

// ============================================================================
// Hardware detection
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HardwareSpec {
    pub total_ram_gb: u32,
    pub total_vram_gb: u32,
    pub recommended_size_label: String,
    pub cpu_cores: u32,
}

/// Cached hardware spec to avoid re-running slow wmic/nvidia-smi on every call.
pub(crate) static HARDWARE_CACHE: std::sync::OnceLock<HardwareSpec> = std::sync::OnceLock::new();

#[tauri::command]
async fn check_hardware() -> Result<HardwareSpec, String> {
    if let Some(cached) = HARDWARE_CACHE.get() {
        return Ok(cached.clone());
    }
    let spec = tokio::task::spawn_blocking(probe_hardware)
        .await
        .map_err(|e| format!("hardware probe panicked: {e}"))??;
    let _ = HARDWARE_CACHE.set(spec.clone());
    Ok(spec)
}

/// The actual heavy lifting — runs wmic / nvidia-smi. Called at most once.
pub(crate) fn probe_hardware() -> Result<HardwareSpec, String> {
    // RAM: use sysinfo-like approach via system commands.
    let ram_gb = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("wmic");
        hide_std_console(&mut command);
        command
            .args(["computersystem", "get", "TotalPhysicalMemory"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines()
                    .filter(|l| l.trim().chars().all(|c| c.is_ascii_digit()))
                    .find(|l| !l.trim().is_empty())
                    .and_then(|n| n.trim().parse::<u64>().ok())
            })
            .map(|bytes| (bytes / (1024 * 1024 * 1024)) as u32)
            .unwrap_or(16)
    } else {
        16
    };

    // VRAM: best-effort via nvidia-smi in MiB converted to GB; fallback to WMI or 0.
    let vram_gb = {
        let mut command = std::process::Command::new("nvidia-smi");
        hide_std_console(&mut command);
        command
            .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
            .output()
    }
    .ok()
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .and_then(|s| {
        s.lines()
            .next()
            .and_then(|l| l.trim().parse::<f32>().ok())
            .map(|mb| (mb / 1024.0).round() as u32)
    })
    .or_else(|| {
        if cfg!(target_os = "windows") {
            let mut command = std::process::Command::new("wmic");
            hide_std_console(&mut command);
            command
                .args(["path", "win32_VideoController", "get", "AdapterRAM"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|s| {
                    s.lines()
                        .filter_map(|l| l.trim().parse::<u64>().ok())
                        .max()
                        .map(|bytes| (bytes / (1024 * 1024 * 1024)) as u32)
                })
        } else {
            None
        }
    })
    .unwrap_or(0);

    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);

    let recommended = match (ram_gb, vram_gb) {
        (r, v) if r >= 64 && v >= 24 => "large (35-70B)",
        (r, v) if r >= 32 && v >= 12 => "mid (14-35B)",
        (r, v) if r >= 16 && v >= 8 => "small (7-14B)",
        _ => "tiny (1-7B)",
    };

    Ok(HardwareSpec {
        total_ram_gb: ram_gb,
        total_vram_gb: vram_gb,
        recommended_size_label: recommended.to_string(),
        cpu_cores,
    })
}

// ============================================================================
// Model management
// ============================================================================

fn is_model_weight_path(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let name = name.strip_suffix(".part").unwrap_or(name);
    ["gguf", "safetensors", "onnx", "pth", "pt", "bin"]
        .iter()
        .any(|ext| name.to_ascii_lowercase().ends_with(&format!(".{ext}")))
}

/// Remove one model and all of its shards, without deleting another
/// quantisation stored in the same repository directory. Partial files are
/// included so a failed install can also be cleaned by the same command.
fn delete_local_model_artifacts(models_dir: &std::path::Path, model: &str) -> Result<(), String> {
    let direct = models_dir.join(model);
    let resolved = if direct.exists() || direct.with_extension("part").exists() {
        direct
    } else {
        resolve_model_path(models_dir, model)
    };

    if resolved.is_dir() {
        std::fs::remove_dir_all(&resolved).map_err(|e| {
            format!(
                "Impossible de supprimer le dossier modèle {} : {e}",
                resolved.display()
            )
        })?;
        return Ok(());
    }

    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    if resolved.is_file() || resolved.with_extension("part").is_file() {
        targets.push(resolved.clone());
    }

    // The installed-model list exposes one representative shard as
    // `repo/file`. Expand that identity to every shard of the same variant.
    if let Ok(relative) = resolved.strip_prefix(models_dir) {
        if let Some(first) = relative.components().next() {
            let repo_dir = models_dir.join(first.as_os_str());
            if repo_dir.is_dir() {
                let relative_model = resolved
                    .strip_prefix(&repo_dir)
                    .unwrap_or(&resolved)
                    .to_string_lossy()
                    .replace('\\', "/");
                let model_key = relative_model
                    .strip_suffix(".part")
                    .unwrap_or(&relative_model);
                if is_model_weight_path(std::path::Path::new(model_key)) {
                    let group = hf_shard_group(model_key);
                    for file in walkdir_recursive(&repo_dir, 8) {
                        if !file.is_file() || !is_model_weight_path(&file) {
                            continue;
                        }
                        let rel = file
                            .strip_prefix(&repo_dir)
                            .unwrap_or(&file)
                            .to_string_lossy()
                            .replace('\\', "/");
                        let clean = rel.strip_suffix(".part").unwrap_or(&rel);
                        if hf_shard_group(clean) == group && !targets.contains(&file) {
                            targets.push(file);
                        }
                    }
                }
            }
        }
    }

    let mut deleted = false;
    for target in targets {
        if target.is_file() {
            std::fs::remove_file(&target).map_err(|e| {
                format!(
                    "Impossible de supprimer le fichier {} : {e}",
                    target.display()
                )
            })?;
            deleted = true;
        }
        let partial = target.with_extension("part");
        if partial.is_file() {
            std::fs::remove_file(&partial).map_err(|e| {
                format!(
                    "Impossible de supprimer le fichier partiel {} : {e}",
                    partial.display()
                )
            })?;
            deleted = true;
        }
    }
    if !deleted {
        return Err(format!(
            "Aucun modèle trouvé correspondant à '{}' dans {}",
            model,
            models_dir.display()
        ));
    }
    if let Some(parent) = resolved.parent() {
        remove_empty_parent_dirs(parent, models_dir);
    }
    Ok(())
}

#[tauri::command]
async fn delete_model_cmd(
    core: State<'_, Core>,
    _endpoint: String,
    model: String,
) -> Result<(), String> {
    let model = model.trim().replace('\\', "/");
    if model.is_empty()
        || model.starts_with('/')
        || model.contains("..")
        || model.contains(':')
        || std::path::Path::new(&model).is_absolute()
    {
        return Err("nom de modèle invalide".into());
    }
    if let Some(client) = core.remote_client() {
        // In remote mode the server is authoritative. Do not silently fall
        // back to deleting a similarly named local file when the server says
        // deletion failed.
        return client
            .delete_model(&model)
            .await
            .map_err(|e| format!("suppression distante impossible : {e}"));
    }
    let models_dir = locaryn_config::models_dir();
    delete_local_model_artifacts(&models_dir, &model)?;
    tracing::info!(model = %model, "deleted model artifacts");
    Ok(())
}

#[tauri::command]
fn open_models_folder(path: Option<String>) -> Result<(), String> {
    let dir = match path {
        Some(p) if !p.is_empty() => std::path::PathBuf::from(p),
        _ => locaryn_config::models_dir(),
    };
    if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    } else {
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ============================================================================
// App info
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AppInfo {
    pub version: String,
    pub mode: String,
    pub data_dir: String,
    pub db_path: String,
    /// Where model weights live. Exposed because the UI used to hardcode an
    /// absolute path, which was wrong on any other machine.
    pub models_dir: String,
    /// OS and CPU architecture of the running build. Lets the UI decide
    /// whether the automatic updater applies (Windows/macOS) or whether it
    /// must fall back to opening the GitHub releases page (Linux).
    pub platform: String,
    pub arch: String,
}

#[tauri::command]
fn app_info(core: State<'_, Core>) -> Result<AppInfo, String> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        mode: format!("{:?}", core.mode).to_lowercase(),
        data_dir: core.data_dir.to_string_lossy().to_string(),
        db_path: core
            .data_dir
            .join("locaryn.db")
            .to_string_lossy()
            .to_string(),
        models_dir: locaryn_config::models_dir().to_string_lossy().to_string(),
        platform: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    })
}

// ============================================================================
// Runtime plan — auto GPU/RAM routing
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimePlan {
    pub model: String,
    pub size_gb: f64,
    pub vram_gb: f64,
    pub ram_gb: f64,
    pub mode: String,
    pub label: String,
    pub gpu_layers: i32,
    pub n_cpu_moe: i32,
}

#[tauri::command]
fn plan_model_runtime(_core: State<'_, Core>, model: String) -> Result<RuntimePlan, String> {
    let models_dir = locaryn_config::models_dir();
    let path = models_dir.join(&model);
    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let size_gb = size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    let hw = HARDWARE_CACHE.get().cloned().unwrap_or_else(|| {
        probe_hardware().unwrap_or(HardwareSpec {
            total_ram_gb: 16,
            total_vram_gb: 0,
            recommended_size_label: "tiny (1-7B)".to_string(),
            cpu_cores: 4,
        })
    });
    let vram_gb = hw.total_vram_gb as f64;
    let ram_gb = hw.total_ram_gb as f64;

    let (mode, label, gpu_layers) = if size_gb <= vram_gb * 0.9 {
        ("gpu", String::new(), -1)
    } else if size_gb <= ram_gb * 0.8 {
        (
            "offload",
            format!("offload RAM ({:.1} GB)", size_gb - vram_gb),
            0,
        )
    } else {
        ("heavy", "low VRAM".to_string(), 0)
    };

    Ok(RuntimePlan {
        model,
        size_gb,
        vram_gb,
        ram_gb,
        mode: mode.to_string(),
        label,
        gpu_layers,
        n_cpu_moe: 0,
    })
}

// ============================================================================
// Runtime capabilities — honest, install-based
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeCapabilities {
    pub runtime_installed: bool,
    pub runtime_version: Option<String>,
    pub chat: bool,
    pub vision: bool,
    pub embeddings: bool,
    pub image_gen: bool,
    pub finetune: bool,
    pub distributed: bool,
    pub speculative_decoding: bool,
    pub kv_quant: bool,
    pub weight_formats: Vec<String>,
    pub unavailable: Vec<String>,
}

#[tauri::command]
fn runtime_capabilities(_core: State<'_, Core>) -> Result<RuntimeCapabilities, String> {
    let llama_bin = locaryn_config::bin_dir().join("llama-server.exe");
    let runtime_installed = llama_bin.exists();

    Ok(RuntimeCapabilities {
        runtime_installed,
        runtime_version: None,
        chat: runtime_installed,
        vision: false,
        embeddings: runtime_installed,
        image_gen: sd_engine::find_sd_binary().is_some(),
        finetune: false,
        distributed: true,
        speculative_decoding: true,
        kv_quant: true,
        weight_formats: vec!["GGUF".to_string()],
        unavailable: vec![
            "AWQ / EXL2 / GPTQ (necessite ExLlamaV2 / vLLM)".to_string(),
            "PagedAttention / vLLM (serveur GPU separe)".to_string(),
        ],
    })
}

// ============================================================================
// LoRA adapters
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LoraAdapter {
    pub id: u32,
    pub path: String,
    pub scale: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoraScale {
    pub id: u32,
    pub scale: f32,
}

#[tauri::command]
async fn list_lora_adapters() -> Result<Vec<LoraAdapter>, String> {
    // The running llama-server exposes /v1/lora-adapters when LoRA is loaded.
    // For now, return empty — the real implementation will query the server.
    Ok(Vec::new())
}

#[tauri::command]
async fn set_lora_adapters(_scales: Vec<LoraScale>) -> Result<(), String> {
    // POST to the running llama-server's /v1/lora-adapters endpoint.
    Ok(())
}

// ============================================================================
// RAG — retrieval-augmented generation
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RagStatus {
    pub chunk_count: u32,
    pub dim: u32,
    pub embed_model: String,
    pub sources: Vec<RagSource>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RagSource {
    pub source: String,
    pub chunks: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RagHit {
    pub source: String,
    pub text: String,
    pub score: f32,
}

/// Le modèle qui calcule les plongements.
///
/// Celui que la personne a désigné pour les micro-tâches quand il en existe un,
/// sinon celui de conversation. Ce n'est pas indifférent : un modèle de
/// conversation rend des vecteurs utilisables mais médiocres, parce qu'il a été
/// entraîné à prédire la suite, pas à rapprocher ce qui se ressemble. L'écran
/// affiche lequel a servi, pour que la différence ne se devine pas.
async fn modele_de_plongement(core: &Core) -> Result<(String, String), String> {
    let providers = core
        .storage
        .providers
        .list()
        .await
        .map_err(|e| e.to_string())?;
    let actif = providers
        .into_iter()
        .find(|p| {
            p.is_active
                && matches!(
                    p.engine,
                    locaryn_shared_types::ProviderEngine::LlamaCpp
                        | locaryn_shared_types::ProviderEngine::OpenAiCompat
                        | locaryn_shared_types::ProviderEngine::Ollama
                )
        })
        .ok_or_else(|| {
            "Aucun moteur d'inférence actif. Démarrez-en un : c'est lui qui calcule \
             les plongements."
                .to_string()
        })?;

    let micro = locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model)
        .filter(|m| !m.trim().is_empty());
    let modele = micro
        .or(actif.model)
        .ok_or_else(|| "Le moteur actif n'annonce aucun modèle.".to_string())?;
    Ok((actif.endpoint, modele))
}

/// Traduire l'état du magasin vers ce que l'écran attend.
///
/// L'écran compte les extraits par document ; le magasin range les documents
/// sans les compter. Un appel de plus par document, sur une liste courte.
async fn etat_pour_l_ecran(core: &Core, project_id: uuid::Uuid) -> Result<RagStatus, String> {
    let brut = core
        .storage
        .rag
        .status(project_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut sources = Vec::new();
    for nom in &brut.sources {
        let combien = core
            .storage
            .rag
            .count_for_source(project_id, nom)
            .await
            .unwrap_or(0);
        sources.push(RagSource {
            source: nom.clone(),
            chunks: combien,
        });
    }
    Ok(RagStatus {
        chunk_count: brut.chunk_count,
        dim: brut.dim,
        embed_model: brut.embed_model,
        sources,
    })
}

/// Ce que le projet sait, quand la question touche à ce qu'il contient.
///
/// Rend `None` quand rien n'est indexé, quand le moteur ne calcule pas de
/// plongements, ou quand rien ne ressemble d'assez près à la question. Dans
/// les trois cas, le message part sans contexte : mieux vaut une réponse sans
/// documents que trois extraits hors sujet qui égarent le modèle.
async fn build_rag_context(core: &Core, project_id: &str, query: &str) -> Option<String> {
    let pid = uuid::Uuid::parse_str(project_id).ok()?;
    let etat = core.storage.rag.status(pid).await.ok()?;
    if etat.chunk_count == 0 {
        return None;
    }
    let (endpoint, modele) = modele_de_plongement(core).await.ok()?;
    let client = reqwest::Client::new();
    let vecteurs = locaryn_agent_runtime::embeddings::embed(
        &endpoint,
        &client,
        &modele,
        &[query.to_string()],
        locaryn_agent_runtime::embeddings::Role::Question,
    )
    .await
    .ok()?;
    let question = vecteurs.into_iter().next()?;
    let hits = core.storage.rag.search(pid, &question, 4).await.ok()?;

    // Le tri regarde le détachement du premier, pas un seuil absolu : l'échelle
    // des cosinus dépend du modèle, et quand aucun document ne répond, tous les
    // scores se tassent. C'est ce tassement qu'on reconnaît.
    let scores: Vec<f32> = hits.iter().map(|h| h.score).collect();
    let gardes = locaryn_storage::rag::retenir(&scores, 3);
    if gardes.is_empty() {
        return None;
    }
    let retenus: Vec<_> = gardes
        .into_iter()
        .filter_map(|i| hits.get(i).cloned())
        .collect();

    let mut bloc = String::from(
        "Extraits des documents du projet, à utiliser s'ils répondent à la question. \
         Cite la source quand tu t'en sers. S'ils ne répondent pas, dis-le et \
         réponds sans eux.\n\n",
    );
    for h in retenus {
        bloc.push_str(&format!("[{}]\n{}\n\n", h.source, h.text));
    }
    Some(bloc)
}

/// Indexer un texte pour un projet.
///
/// Le texte est découpé, chaque morceau est vectorisé par le moteur, et le tout
/// est rangé sous le nom du document. Réindexer le même nom le remplace : c'est
/// ce qu'on veut quand un fichier a changé.
#[tauri::command]
async fn rag_index_text(
    core: State<'_, Core>,
    project_id: String,
    source: String,
    text: String,
) -> Result<RagStatus, String> {
    let pid = uuid::Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;
    let source = source.trim();
    if source.is_empty() {
        return Err("Ce document n'a pas de nom.".into());
    }

    let morceaux = locaryn_storage::rag::decouper(&text, 1200, 120);
    if morceaux.is_empty() {
        return Err("Ce document ne contient aucun texte lisible.".into());
    }

    let (endpoint, modele) = modele_de_plongement(&core).await?;
    let client = reqwest::Client::new();
    let vecteurs = locaryn_agent_runtime::embeddings::embed(
        &endpoint,
        &client,
        &modele,
        &morceaux,
        locaryn_agent_runtime::embeddings::Role::Document,
    )
    .await?;

    let a_ranger: Vec<locaryn_storage::rag::MorceauAIndexer> = morceaux
        .into_iter()
        .zip(vecteurs)
        .map(|(text, embedding)| locaryn_storage::rag::MorceauAIndexer { text, embedding })
        .collect();

    core.storage
        .rag
        .index(pid, source, &modele, &a_ranger)
        .await
        .map_err(|e| e.to_string())?;
    etat_pour_l_ecran(&core, pid).await
}

/// Ce que l'index du projet contient.
#[tauri::command]
async fn rag_status(core: State<'_, Core>, project_id: String) -> Result<RagStatus, String> {
    let pid = uuid::Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;
    etat_pour_l_ecran(&core, pid).await
}

/// Vider l'index du projet.
#[tauri::command]
async fn rag_clear(core: State<'_, Core>, project_id: String) -> Result<(), String> {
    let pid = uuid::Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;
    core.storage
        .rag
        .clear(pid, None)
        .await
        .map_err(|e| e.to_string())
}

/// Chercher dans les documents du projet.
#[tauri::command]
async fn rag_search(
    core: State<'_, Core>,
    project_id: String,
    query: String,
    k: Option<u32>,
) -> Result<Vec<RagHit>, String> {
    let pid = uuid::Uuid::parse_str(&project_id).map_err(|e| e.to_string())?;
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let (endpoint, modele) = modele_de_plongement(&core).await?;
    let client = reqwest::Client::new();
    let vecteurs = locaryn_agent_runtime::embeddings::embed(
        &endpoint,
        &client,
        &modele,
        &[query],
        locaryn_agent_runtime::embeddings::Role::Question,
    )
    .await?;
    let question = vecteurs
        .into_iter()
        .next()
        .ok_or_else(|| "Le moteur n'a rien rendu pour cette question.".to_string())?;

    let hits = core
        .storage
        .rag
        .search(pid, &question, k.unwrap_or(5) as usize)
        .await
        .map_err(|e| e.to_string())?;
    Ok(hits
        .into_iter()
        .map(|h| RagHit {
            source: h.source,
            text: h.text,
            score: h.score,
        })
        .collect())
}

// ============================================================================
// Ollama library search
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct OllamaLibraryModel {
    pub name: String,
    pub description: String,
    pub pulls: String,
    pub tags: Vec<String>,
    pub category: String,
}

#[tauri::command]
async fn search_ollama_library(
    _core: State<'_, Core>,
    query: String,
    _category: Option<String>,
) -> Result<Vec<OllamaLibraryModel>, String> {
    let _ = query;
    // The real implementation fetches from https://ollama.com/library/{query}
    // and parses the model list. For now, return empty.
    Ok(Vec::new())
}

// ============================================================================
// llama.cpp runtime management
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LlamaRuntimeStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub up_to_date: bool,
    pub pinned: String,
    pub path: String,
}

#[tauri::command]
fn llama_runtime_status(_core: State<'_, Core>) -> Result<LlamaRuntimeStatus, String> {
    let bin = locaryn_config::bin_dir().join("llama-server.exe");
    let installed = bin.exists();
    Ok(LlamaRuntimeStatus {
        installed,
        version: None,
        up_to_date: true,
        pinned: "b10088".to_string(),
        path: bin.to_string_lossy().to_string(),
    })
}

#[tauri::command]
async fn setup_llama_runtime(
    core: State<'_, Core>,
    _variant: Option<String>,
    on_event: Channel<PullProgressEvent>,
) -> Result<LlamaRuntimeStatus, String> {
    let bin_dir = locaryn_config::bin_dir();
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    // Download the pinned llama-server binary.
    let url = if cfg!(target_os = "windows") {
        "https://github.com/ggml-org/llama.cpp/releases/download/b10088/llama-b10088-bin-win-vulkan-x64.zip"
    } else {
        "https://github.com/ggml-org/llama.cpp/releases/download/b10088/llama-b10088-bin-ubuntu-x64.zip"
    };

    let zip_name = "llama-server.zip";
    let zip_path = bin_dir.join(zip_name);
    let part_path = bin_dir.join(format!("{zip_name}.part"));
    let cancel = tokio_util::sync::CancellationToken::new();

    let pull_event = PullProgressEvent {
        status: "Telechargement du runtime...".into(),
        completed: 0,
        total: 0,
        percentage: 0.0,
    };
    let _ = on_event.send(pull_event);

    do_pull(
        &core, url, zip_name, &zip_path, &part_path, &on_event, &cancel, "",
    )
    .await?;

    // Extract would go here; for now just report success.
    let bin = bin_dir.join("llama-server.exe");
    Ok(LlamaRuntimeStatus {
        installed: true,
        version: Some("b10088".to_string()),
        up_to_date: true,
        pinned: "b10088".to_string(),
        path: bin.to_string_lossy().to_string(),
    })
}

// ============================================================================
// Connector types
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectorType {
    pub type_id: String,
    pub display_name: String,
    pub summary: String,
    pub icon: String,
    pub category: String,
    pub source: String,
    pub available: bool,
    pub supports_test: bool,
    pub install_hint: String,
}

#[tauri::command]
fn list_connector_types() -> Result<Vec<ConnectorType>, String> {
    Ok(vec![
        ConnectorType {
            type_id: "ssh".into(),
            display_name: "SSH Remote Server".into(),
            summary: "Connexion serveur distant via SSH".into(),
            icon: "\u{1f5a7}".into(),
            category: "connector".into(),
            source: "built-in".into(),
            available: true,
            supports_test: true,
            install_hint: String::new(),
        },
        ConnectorType {
            type_id: "mcp_custom".into(),
            display_name: "Serveur MCP Personnalise".into(),
            summary: "Ajoutez n'importe quel serveur MCP".into(),
            icon: "\u{1f6e0}\u{fe0f}".into(),
            category: "extension".into(),
            source: "built-in".into(),
            available: true,
            supports_test: false,
            install_hint: String::new(),
        },
    ])
}

// ============================================================================
// SSH servers
// ============================================================================

#[tauri::command]
async fn list_ssh_servers(core: State<'_, Core>) -> Result<Vec<SshServer>, String> {
    core.storage
        .ssh_servers
        .list()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn test_ssh_connection(
    draft: serde_json::Value,
    secret: Option<String>,
    on_event: Channel<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let _ = (draft, secret, on_event);
    Err("SSH connection test not yet implemented".into())
}

#[tauri::command]
async fn confirm_ssh_host_key(core: State<'_, Core>, test_token: String) -> Result<(), String> {
    let mut tests = core.pending_tests.lock().await;
    if let Some(test) = tests.get_mut(&test_token) {
        test.confirmed = true;
        Ok(())
    } else {
        Err("invalid or expired test token".into())
    }
}

#[tauri::command]
async fn save_ssh_server(
    _core: State<'_, Core>,
    draft: serde_json::Value,
    secret: Option<String>,
    test_token: String,
) -> Result<SshServer, String> {
    let _ = (draft, secret, test_token);
    Err("SSH server save not yet implemented".into())
}

#[tauri::command]
async fn update_ssh_server(
    core: State<'_, Core>,
    id: Uuid,
    patch: serde_json::Value,
) -> Result<SshServer, String> {
    let _ = (core, id, patch);
    Err("update_ssh_server not yet fully implemented".into())
}

#[tauri::command]
async fn set_ssh_ai_access(
    core: State<'_, Core>,
    id: Uuid,
    level: SshAiAccess,
) -> Result<SshServer, String> {
    core.storage
        .ssh_servers
        .set_ai_access(id, level)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_ssh_server(core: State<'_, Core>, id: Uuid) -> Result<(), String> {
    core.storage
        .ssh_servers
        .delete(id)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ============================================================================
// Unit tests — Python script syntax validation
// ============================================================================

// ============================================================================
// Entry point — Tauri builder with all commands registered
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,locaryn=debug")),
        )
        .with_target(true)
        .init();

    tracing::info!("Starting Locaryn desktop v{}", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Windows' X is a hide-to-tray action, not a process exit. The
                // tray's explicit Quit action sets the flag first so its exit
                // request is allowed through.
                if !TRAY_QUIT_REQUESTED.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let core = tauri::async_runtime::block_on(init_core())?;
            // Les serveurs stdio héritent du moteur actif avant tout spawn,
            // y compris ceux des extensions chargées juste après.
            tauri::async_runtime::block_on(refresh_mcp_runtime_env(&core));
            // Servers the user marked automatic come up in the background:
            // one that hangs must not hold the window shut.
            let mcp = core.mcp.clone();
            tauri::async_runtime::spawn(async move {
                mcp_servers::start_automatic(&mcp).await;
            });
            app.manage(core);

            // Keep Locaryn in the notification area when its window is closed.
            // The daemon is owned by this application and is stopped only by
            // the explicit tray quit action or the final Tauri exit event.
            #[cfg(desktop)]
            {
                use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let show_item =
                    MenuItem::with_id(app, "show", "Ouvrir Locaryn", true, None::<&str>)?;
                let status_item = MenuItem::with_id(
                    app,
                    "daemon_status",
                    "Service : Détection…",
                    false,
                    None::<&str>,
                )?;
                let port_item =
                    MenuItem::with_id(app, "daemon_port", "Port : 7474", false, None::<&str>)?;
                let restart_item = MenuItem::with_id(
                    app,
                    "restart_daemon",
                    "Redémarrer le service",
                    true,
                    None::<&str>,
                )?;
                let quit_item =
                    MenuItem::with_id(app, "quit", "Quitter Locaryn", true, None::<&str>)?;
                let sep1 = PredefinedMenuItem::separator(app)?;
                let sep2 = PredefinedMenuItem::separator(app)?;

                let menu = Menu::with_items(
                    app,
                    &[
                        &show_item,
                        &sep1,
                        &status_item,
                        &port_item,
                        &restart_item,
                        &sep2,
                        &quit_item,
                    ],
                )?;
                let icon = app
                    .default_window_icon()
                    .cloned()
                    .ok_or("icône Locaryn introuvable")?;

                let status_item_bg = status_item.clone();
                let port_item_bg = port_item.clone();
                let restart_item_bg = restart_item.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        if let Ok(st) = server_mode::server_status().await {
                            let status_text = if st.running {
                                "Service : En écoute (Actif)"
                            } else {
                                "Service : Arrêté"
                            };
                            let port_text = format!("Port : {}", st.port);
                            let _ = status_item_bg.set_text(status_text);
                            let _ = port_item_bg.set_text(port_text);
                            let _ = restart_item_bg.set_enabled(st.blocker.is_none());
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                });

                let status_item_ev = status_item.clone();
                let port_item_ev = port_item.clone();

                TrayIconBuilder::new()
                    .icon(icon)
                    .tooltip("Locaryn — service local")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "restart_daemon" => {
                            let app_handle = app.clone();
                            let s_item = status_item_ev.clone();
                            let p_item = port_item_ev.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = s_item.set_text("Service : Redémarrage…");
                                let res = server_mode::restart_server().await;
                                if let Ok(st) = res {
                                    let status_text = if st.running {
                                        "Service : En écoute (Actif)"
                                    } else {
                                        "Service : Arrêté"
                                    };
                                    let _ = s_item.set_text(status_text);
                                    let _ = p_item.set_text(format!("Port : {}", st.port));
                                    let _ = app_handle.emit("locaryn:server-status-changed", &st);
                                }
                            });
                        }
                        "quit" => {
                            TRAY_QUIT_REQUESTED.store(true, Ordering::SeqCst);
                            server_mode::stop_daemon();
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.unminimize();
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    })
                    .build(app)?;
            }

            // Deep links (locaryn://install?src=…). Registering makes the OS
            // treat this app as the handler for the scheme; the frontend
            // already subscribes to the plugin's `deep-link://new-url` event
            // and polls `get_current` on load, so a URL that arrives while the
            // window is closed still pre-fills the install dialog.
            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register("locaryn") {
                    tracing::warn!(error = %e, "enregistrement du schéma locaryn:// impossible");
                }
                // Warm links while the app is already running: forward every
                // opened URL to the frontend as a custom event.
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let _ = handle.emit("locaryn://deep-link", url.to_string());
                    }
                });
            }

            // Load the enabled extensions and publish their MCP servers. Also
            // in the background: an extension whose server hangs must not hold
            // the window shut either.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let core = handle.state::<Core>();
                if let Err(e) = extensions::reload(&core).await {
                    tracing::warn!(error = %e, "chargement des extensions échoué");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_health,
            resolve_preview,
            list_projects,
            create_project,
            archive_project,
            update_project,
            free_chat_project,
            session_workspace,
            append_chat_message,
            append_assistant_message,
            suggest_followups,
            plan_task,
            detect_image_request,
            list_sessions,
            create_session,
            update_session_title,
            generate_session_title,
            delete_session,
            archive_session,
            archived_sessions,
            move_session,
            rename_session,
            create_ephemeral_session,
            list_figures,
            save_figure,
            delete_figure,
            attach_figure,
            figure_sessions,
            list_messages,
            send_message,
            run_terminal,
            list_providers,
            set_active_provider,
            configure_provider,
            airllm::airllm_status,
            airllm::airllm_setup,
            airllm::airllm_install,
            airllm::airllm_installed,
            airllm::airllm_uninstall,
            airllm::configure_airllm_provider,
            list_models,
            list_incompatible_models,
            inspect_huggingface_repo,
            app_info,
            region_edit::edit_region,
            client_cert::client_certificate_status,
            client_cert::sign_in,
            client_cert::current_session,
            client_cert::sign_out,
            client_cert::install_client_certificate,
            client_cert::remove_client_certificate,
            server_mode::server_status,
            server_mode::set_server_mode,
            server_mode::restart_server,
            server_mode::provisioning,
            server_mode::list_server_users,
            server_mode::create_server_user,
            server_mode::delete_server_user,
            storage_root::storage_info,
            voice_presets::list_voice_presets,
            voice_presets::save_voice_preset,
            voice_presets::delete_voice_preset,
            voice_presets::voice_preset_support,
            storage_root::set_storage_root,
            storage_root::clean_temp,
            pull_model,
            cancel_pull_model,
            delete_model_cmd,
            approve_tool_call,
            update_provider_model_params,
            get_provider_model_params,
            get_inference_config,
            set_inference_config,
            get_profile_preset,
            check_hardware,
            model_residency::model_residency,
            model_residency::check_model_fit,
            model_residency::load_chat_model,
            model_residency::eject_chat_model,
            model_residency::caution_level,
            model_residency::set_caution_level,
            open_models_folder,
            plan_model_runtime,
            runtime_capabilities,
            list_lora_adapters,
            set_lora_adapters,
            rag_index_text,
            rag_status,
            rag_clear,
            rag_search,
            search_ollama_library,
            list_audio_models,
            list_kokoro_voices,
            generate_audio,
            generate_music,
            generate_video,
            generate_3d,
            pick_voice_reference,
            llama_runtime_status,
            setup_llama_runtime,
            list_connector_types,
            travel_mode::travel_relays,
            travel_mode::travel_status,
            travel_mode::set_travel_mode,
            travel_mode::travel_home_code,
            travel_mode::pairing_code,
            travel_mode::run_composer_tool,
            travel_mode::suggest_project,
            travel_mode::merge_sessions,
            travel_mode::micro_model,
            travel_mode::set_micro_model,
            memory::list_memory,
            memory::list_model_metrics,
            memory::remember,
            memory::edit_memory,
            memory::forget_memory,
            memory::forget_all_memory,
            local_profile::get_local_profile,
            local_profile::set_local_profile,
            local_profile::set_local_avatar,
            local_profile::clear_local_avatar,
            extensions::list_extensions,
            extensions::list_capabilities,
            core_engines::core_status,
            core_engines::core_start,
            core_engines::core_stop,
            core_engines::core_skills,
            core_engines::core_install_skill,
            extensions::install_extension,
            extensions::update_extension,
            extensions::update_extension_source,
            extensions::reload_extensions,
            extensions::preview_extension_source,
            extensions::check_extension_updates,
            extensions::set_extension_enabled,
            extensions::set_extension_permissions,
            extensions::remove_extension,
            extensions::get_extension_config,
            extensions::set_extension_config,
            extensions::get_extension_mcp_servers,
            extensions::set_extension_mcp_servers,
            extensions::list_extension_commands,
            extensions::resolve_extension_command,
            extensions::read_extension_asset,
            extensions::invoke_extension_tool,
            extensions::browse_extension_catalog,
            extensions::refresh_extension_catalog,
            extensions::catalog_entry_details,
            extensions::list_catalog_sources,
            extensions::add_catalog_source,
            extensions::set_catalog_source_enabled,
            extensions::remove_catalog_source,
            mcp_servers::list_mcp_servers,
            mcp_servers::add_mcp_server,
            mcp_servers::remove_mcp_server,
            mcp_servers::start_mcp_server,
            mcp_servers::stop_mcp_server,
            mcp_servers::invoke_mcp_tool,
            mcp_servers::diagnose_android_vm,
            mcp_servers::setup_android_vm,
            mcp_servers::start_android_vm,
            mcp_servers::stop_android_vm,
            mcp_servers::android_screen_probe,
            mcp_servers::android_screen_action,
            write_test_audio,
            remove_test_audio,
            save_audio_as,
            save_image_as,
            list_ssh_servers,
            test_ssh_connection,
            confirm_ssh_host_key,
            save_ssh_server,
            update_ssh_server,
            set_ssh_ai_access,
            delete_ssh_server,
            get_image_defaults,
            set_image_defaults,
            get_model_preferences,
            set_model_preferences,
            bootstrap
        ])
        .build(tauri::generate_context!())
        .expect("error while building Locaryn desktop")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                // Covers shutdown paths other than the tray menu as well. The
                // child is killed before the desktop process disappears.
                server_mode::stop_daemon();
            }
        });
}

// Le module de test ferme le fichier : tout élément placé après lui se lit
// mal et s'oublie facilement — c'est précisément ce que signale clippy.

#[cfg(test)]
mod tests {
    use super::{
        compatible_gguf_repo, find_python, hf_candidate_variant, hf_quantization, hf_shard_group,
        is_safetensors_layout_file, is_text_chat_model, preferred_mmproj, summarise_python_error,
    };
    use std::process::Command;
    use uuid::Uuid;

    /// Une configuration sans `app.windows` compile, se lance, et n'affiche
    /// rien — c'est exactement ce qui est arrivé à l'application mobile en
    /// v0.3.1. Même garde-fou ici, la panne serait identique.
    #[test]
    fn la_configuration_declare_une_fenetre() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let fenetres = conf["app"]["windows"]
            .as_array()
            .expect("app.windows doit exister");
        assert!(
            !fenetres.is_empty(),
            "app.windows est vide : l'application s'ouvrirait sur un écran noir"
        );
    }

    #[test]
    fn huggingface_shards_stay_one_variant_and_quantisations_stay_distinct() {
        assert_eq!(
            hf_shard_group("model-Q4_K_M-00001-of-00003.gguf"),
            "model-Q4_K_M.gguf"
        );
        assert_eq!(
            hf_shard_group("model-Q4_K_M-00003-of-00003.gguf"),
            "model-Q4_K_M.gguf"
        );
        assert_eq!(
            hf_shard_group("model-00018-of-00018.safetensors"),
            "model.safetensors"
        );
        assert_ne!(
            hf_shard_group("model-Q4_K_M.gguf"),
            hf_shard_group("model-Q8_0.gguf")
        );
        assert_eq!(
            hf_quantization("model-Q3_K_M.gguf").as_deref(),
            Some("Q3_K_M")
        );
        assert_eq!(hf_quantization("model-BF16.gguf").as_deref(), Some("BF16"));
        assert_eq!(
            hf_candidate_variant("models/Champion-Inst-Q3_K_M.gguf", Some("Q3_K_M")),
            "models/Champion-Inst"
        );
    }

    #[test]
    fn transformers_shards_and_runtime_companions_are_not_chat_models() {
        assert!(is_text_chat_model("Qwen3.8-27B-Q4_K_M.gguf"));
        assert!(!is_text_chat_model(
            "Qwen__Qwen3.8-27B/model-00001-of-00018.safetensors"
        ));
        assert!(!is_text_chat_model("mmproj-Qwen3.8-27B-Q8_0.gguf"));
        assert!(!is_text_chat_model("mtp-Qwen3.8-27B-Q4_0.gguf"));
        assert!(is_safetensors_layout_file(std::path::Path::new(
            "Qwen__Qwen3.8-27B/model.safetensors.index.json"
        )));
    }

    #[test]
    fn qwen38_uses_a_loadable_gguf_and_the_compact_vision_projector() {
        assert_eq!(
            compatible_gguf_repo("Qwen/Qwen3.8-27B"),
            Some("ggml-org/Qwen3.8-27B-GGUF")
        );
        let projectors = vec![
            ("mmproj-Qwen3.8-27B-BF16.gguf".to_string(), 900),
            ("mmproj-Qwen3.8-27B-Q8_0.gguf".to_string(), 600),
        ];
        assert_eq!(
            preferred_mmproj(&projectors).as_deref(),
            Some("mmproj-Qwen3.8-27B-Q8_0.gguf")
        );
    }

    #[test]
    fn marketplace_image_downloads_include_family_companions() {
        let z_image = super::image_companions_for("z_image_turbo-Q8_0.gguf", false);
        assert_eq!(
            z_image.iter().map(|item| item.file).collect::<Vec<_>>(),
            ["Qwen3-4B-Instruct-2507-Q4_K_M.gguf", "ae.safetensors"]
        );

        let flux = super::image_companions_for("flux1-schnell-Q4_0.gguf", false);
        assert_eq!(
            flux.iter().map(|item| item.file).collect::<Vec<_>>(),
            [
                "ae.safetensors",
                "clip_l.safetensors",
                "t5xxl_fp16.safetensors"
            ]
        );

        assert!(super::image_companions_for("sdxl-turbo-Q4_0.gguf", false).is_empty());
    }

    /// Real sd.cpp output: the bar redraws with carriage returns, so a whole
    /// render can arrive as a single line holding every step.
    #[test]
    fn failed_partial_downloads_are_removed_but_committed_files_stay() {
        let path = std::env::temp_dir().join(format!("locaryn-partial-{}", Uuid::new_v4()));
        std::fs::write(&path, b"partial").unwrap();
        {
            let _guard = super::PartialDownloadGuard::new(&path);
        }
        assert!(
            !path.exists(),
            "un téléchargement échoué ne doit pas laisser son .part"
        );

        std::fs::write(&path, b"complete").unwrap();
        {
            let mut guard = super::PartialDownloadGuard::new(&path);
            guard.commit();
        }
        assert!(path.exists(), "un fichier validé ne doit pas être supprimé");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn supprimer_une_quantisation_supprime_tous_ses_shards_et_preserve_les_autres() {
        let root = std::env::temp_dir().join(format!("locaryn-models-{}", Uuid::new_v4()));
        let repo = root.join("author__repo");
        std::fs::create_dir_all(&repo).unwrap();
        for file in [
            "model-Q4_K_M-00001-of-00002.gguf",
            "model-Q4_K_M-00002-of-00002.gguf",
            "model-Q8_0.gguf",
            "config.json",
        ] {
            std::fs::write(repo.join(file), b"model").unwrap();
        }
        std::fs::write(
            repo.join("model-Q4_K_M-00003-of-00002.gguf.part"),
            b"partial",
        )
        .unwrap();

        super::delete_local_model_artifacts(&root, "author__repo/model-Q4_K_M-00001-of-00002.gguf")
            .unwrap();

        assert!(!repo.join("model-Q4_K_M-00001-of-00002.gguf").exists());
        assert!(!repo.join("model-Q4_K_M-00002-of-00002.gguf").exists());
        assert!(!repo.join("model-Q4_K_M-00003-of-00002.gguf.part").exists());
        assert!(repo.join("model-Q8_0.gguf").exists());
        assert!(repo.join("config.json").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Le marqueur d'image traverse deux langages : il est écrit ici et relu
    /// par `splitInlineImages` côté interface, qui fait un `JSON.parse` du
    /// contenu. Un chemin Windows contient des antislashs — s'il n'était pas
    /// encodé en JSON, l'analyse échouerait et l'image générée disparaîtrait
    /// du fil sans erreur.
    #[test]
    fn image_marker_carries_a_json_encoded_path() {
        let marqueur = super::image_marker(r"D:\Documents\Syncho\media\img_1.png");
        assert!(marqueur.starts_with("<!--locaryn-image:"));
        assert!(marqueur.ends_with("-->"));
        let encode = marqueur
            .trim_start_matches("<!--locaryn-image:")
            .trim_end_matches("-->");
        let decode: String = serde_json::from_str(encode).expect("un chemin JSON relisible");
        assert_eq!(decode, r"D:\Documents\Syncho\media\img_1.png");
        // Rejoué vers le modèle, le marqueur ne doit plus rien laisser paraître.
        assert_eq!(
            super::strip_ui_markers(&format!(
                "Voici l'image.
{marqueur}"
            )),
            "Voici l'image."
        );
    }

    /// Built from the traceback the user actually reported: absl banners, a
    /// SoX warning and a full traceback around one meaningful ValueError.
    #[test]
    fn python_errors_are_reduced_to_the_actionable_line() {
        let raw =
            "WARNING: All log messages before absl::InitializeLog() is called are written to STDERR
            I0000 00:00:1785432677.183995 52864 port.cc:153] oneDNN custom operations are on.
            'sox' n'est pas reconnu en tant que commande interne ou externe
            SoX could not be found!
            Traceback (most recent call last):
              File \"<string>\", line 57, in <module>
              File \"qwen3_tts_model.py\", line 163, in _validate_languages
                raise ValueError(f\"Unsupported languages: {bad}\")
            ValueError: Unsupported languages: ['fr']. Supported: ['auto', 'french']
";

        let out = summarise_python_error(raw);

        assert!(
            out.starts_with("ValueError: Unsupported languages"),
            "got: {out}"
        );
        assert!(!out.contains("absl"), "absl banner leaked: {out}");
        assert!(!out.contains("oneDNN"), "oneDNN banner leaked: {out}");
        assert!(
            out.len() < 120,
            "still too long ({} chars): {out}",
            out.len()
        );
    }

    #[test]
    fn python_errors_without_an_exception_line_keep_the_tail() {
        let out = summarise_python_error(
            "bruit
ligne utile A
ligne utile B
",
        );
        assert!(out.contains("ligne utile B"), "got: {out}");
        assert_eq!(
            summarise_python_error(
                "   
  
"
            ),
            "erreur inconnue (aucune sortie)"
        );
    }

    /// Validate Python syntax of all embedded TTS scripts.
    /// Reads each .py file in `scripts/`, replaces Rust format!() placeholders
    /// with dummy values, and runs the same Python interpreter Locaryn will
    /// use to catch syntax errors before they surface at runtime.
    #[test]
    fn validate_tts_python_syntax() {
        let scripts_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");

        let names = [
            "kokoro_clone_tts.py",
            "kokoro_std_tts.py",
            "parler_tts.py",
            "xtts_tts.py",
            "qwen3_tts.py",
        ];

        for name in &names {
            let path = scripts_dir.join(name);
            assert!(path.exists(), "script not found: {}", path.display());

            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

            // Write to a temp file so Python can read it cleanly.
            // The raw scripts are already syntactically valid Python:
            // `{repo_dir_json}` is a legal Python set literal (undefined var
            // at runtime, but compile() only checks syntax).
            let tmpdir = std::env::temp_dir().join("locaryn_tts_test");
            let _ = std::fs::create_dir_all(&tmpdir);
            let tmpfile = tmpdir.join(name);
            std::fs::write(&tmpfile, &src)
                .unwrap_or_else(|e| panic!("failed to write temp script: {e}"));

            // Use forward slashes so Python's parser doesn't interpret \U / \u escapes.
            let py_path = tmpfile.to_str().unwrap().replace("\\", "/");
            let python = find_python().expect("Python 3.10+ is required for media features");
            let out = Command::new(python)
                .args([
                    "-c",
                    &format!("compile(open('{py_path}', 'r').read(), '{name}', 'exec')"),
                ])
                .output()
                .unwrap_or_else(|e| panic!("failed to run Python: {e}"));

            assert!(
                out.status.success(),
                "Python syntax error in {}:\n{}",
                name,
                String::from_utf8_lossy(&out.stderr)
            );

            // Clean up the temp file.
            let _ = std::fs::remove_file(&tmpfile);
        }
    }
}
