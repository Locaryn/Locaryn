//! Locaryn provider supervisor â€” detects, starts, healthchecks, and stops
//! local LLM runtimes (Ollama, llama-server, LM Studio, vLLM) on loopback
//! only.
//!
//! Used as a library by the daemon (in-process) and as a standalone CLI
//! (`locaryn-supervisor`). The supervisor is responsible for:
//!
//! 1. **Auto-spawning** a local runtime when the daemon needs it and it is
//!    not already running (e.g. `ollama serve`).
//! 2. **Healthchecking** the runtime on a configurable interval and updating
//!    the provider status in storage.
//! 3. **Auto-shutting down** the spawned runtime after a configurable idle
//!    period (no agent activity) to free RAM/GPU.
//!
//! ## Security
//!
//! The supervisor only ever binds on **loopback**. It never exposes a local
//! runtime on the network â€” that is the job of the remote-server gateway.

use locaryn_shared_types::{ProviderEngine, ProviderStatus};
use locaryn_storage::Storage;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

pub mod engine_manager;
pub mod extension_engine;
pub mod runtime_install;

pub use extension_engine::ExtensionEngineSpec;

// ============================================================================
// Config
// ============================================================================

/// Tunables for the supervisor. All durations are in seconds.
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// How often to poll each managed engine's health endpoint.
    pub healthcheck_interval: Duration,
    /// Grace period while waiting for a freshly spawned runtime to become
    /// healthy before giving up.
    pub startup_timeout: Duration,
    /// Shut down a spawned runtime after this many seconds with no agent
    /// activity (no `note_activity` calls).
    pub idle_timeout: Duration,
    /// Optional override for the `ollama` binary path. If `None`, the
    /// supervisor searches `PATH` via `which`.
    pub ollama_bin: Option<PathBuf>,
    /// Python interpreter used to run the AirLLM server. If `None`, the
    /// supervisor searches `PATH` for `python`.
    pub airllm_python: Option<PathBuf>,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            healthcheck_interval: Duration::from_secs(15),
            startup_timeout: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(30 * 60), // 30 min
            ollama_bin: None,
            airllm_python: None,
        }
    }
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("le moteur {0:?} ne tourne pas")]
    NotRunning(ProviderEngine),
    #[error("le moteur {0:?} n'a pas démarré en {1:?}")]
    StartupTimeout(ProviderEngine, Duration),
    #[error("démarrage impossible ({0:?}) : {1}")]
    SpawnFailed(ProviderEngine, String),
    #[error("exécutable introuvable : {0}")]
    BinaryNotFound(String),
    #[error("moteur inconnu : {0} — l'extension qui l'apportait n'est plus installée ou active")]
    UnknownEngine(String),
    #[error("storage error: {0}")]
    Storage(#[from] locaryn_storage::StorageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// Runtime state (internal)
// ============================================================================

/// Per-engine runtime state held under the supervisor lock.
#[derive(Debug)]
struct EngineState {
    child: Option<Child>,
    last_activity: Instant,
    /// The endpoint we spawned for (e.g. http://127.0.0.1:11434).
    endpoint: String,
    /// Set to true when we spawned the process ourselves (so we are allowed
    /// to kill it). If the runtime was already running externally, we do NOT
    /// own it and must not kill it.
    owned: bool,
    /// The user asked for this model to stay in memory. The idle timer keeps
    /// running and is still reported, but it no longer kills anything: a
    /// deliberate choice outranks a timeout. Crash recovery still applies —
    /// pinning means "do not evict", not "never restart".
    pinned: bool,
}

impl EngineState {
    fn is_running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => {
                // Poll the child's exit status without awaiting.
                // `try_wait()` returns Ok(Some(status)) if exited.
                c.try_wait().ok().flatten().is_none()
            }
            None => false,
        }
    }
}

// ============================================================================
// Supervisor
// ============================================================================

/// The provider supervisor. Cheaply cloneable (Arc inside).
///
/// The supervisor keeps a handle to `Storage` so it can update
/// `providers.status` as runtimes come and go. Callers (the daemon) use
/// `ensure_running()` before dispatching an agent request, and
/// `note_activity()` after each message to keep the idle timer fresh.
#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<SupervisorInner>,
}

struct SupervisorInner {
    cfg: SupervisorConfig,
    states: Mutex<HashMap<ProviderEngine, EngineState>>,
    storage: Storage,
    http: reqwest::Client,
    /// Moteurs apportés par les extensions installées, par identifiant.
    ///
    /// L'hôte remplit ce registre depuis le registre d'extensions et le
    /// remplace à chaque changement (installation, activation, retrait). Le
    /// superviseur ne lit jamais le disque des extensions : il ne connaît que
    /// ce qu'on lui a donné.
    extension_engines: Mutex<HashMap<String, ExtensionEngineSpec>>,
}

impl Supervisor {
    /// Create a new supervisor wired to the given storage.
    pub fn new(cfg: SupervisorConfig, storage: Storage) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        Self {
            inner: Arc::new(SupervisorInner {
                cfg,
                states: Mutex::new(HashMap::new()),
                storage,
                http,
                extension_engines: Mutex::new(HashMap::new()),
            }),
        }
    }

    // -- moteurs apportés par les extensions -------------------------------

    /// Remplace la liste des moteurs d'extension connus.
    ///
    /// Appelé par l'hôte au démarrage puis à chaque changement du registre
    /// d'extensions. Remplacer plutôt qu'ajouter est volontaire : une
    /// extension désactivée doit disparaître d'ici, sinon son moteur reste
    /// démarrable alors que l'utilisateur l'a retirée.
    ///
    /// Les processus des moteurs qui disparaissent sont arrêtés — un runtime
    /// dont l'extension n'existe plus ne doit pas continuer à occuper la
    /// mémoire du GPU.
    pub async fn set_extension_engines(&self, specs: Vec<ExtensionEngineSpec>) {
        let nouveaux: HashMap<String, ExtensionEngineSpec> =
            specs.into_iter().map(|s| (s.id.clone(), s)).collect();
        let partis: Vec<String> = {
            let mut registre = self.inner.extension_engines.lock().await;
            let partis = registre
                .keys()
                .filter(|id| !nouveaux.contains_key(*id))
                .cloned()
                .collect();
            *registre = nouveaux;
            partis
        };
        for id in partis {
            let engine = ProviderEngine::Extension(id.clone());
            tracing::info!(moteur = %id, "moteur d'extension retiré — arrêt du runtime");
            self.kill_owned(&engine).await;
        }
    }

    /// La description d'un moteur d'extension, si ce moteur en est un et que
    /// son extension est toujours installée.
    pub async fn extension_engine_spec(
        &self,
        engine: &ProviderEngine,
    ) -> Option<ExtensionEngineSpec> {
        let id = engine.extension_id()?;
        self.inner.extension_engines.lock().await.get(id).cloned()
    }

    /// Tous les moteurs apportés par des extensions, triés par nom affiché —
    /// ce que l'écran des réglages liste à côté des runtimes intégrés.
    pub async fn extension_engines(&self) -> Vec<ExtensionEngineSpec> {
        let mut v: Vec<ExtensionEngineSpec> = self
            .inner
            .extension_engines
            .lock()
            .await
            .values()
            .cloned()
            .collect();
        v.sort_by(|a, b| a.label.cmp(&b.label));
        v
    }

    /// Le point d'entrée HTTP d'un moteur, intégré ou apporté.
    pub async fn endpoint_for(&self, engine: &ProviderEngine) -> Option<String> {
        if let Some(spec) = self.extension_engine_spec(engine).await {
            return Some(spec.endpoint());
        }
        default_endpoint(engine).map(str::to_string)
    }

    /// Sonde un moteur. Un moteur d'extension utilise l'URL de sa propre
    /// sonde ; les autres passent par `/v1/models`.
    async fn probe(
        &self,
        engine: &ProviderEngine,
        endpoint: &str,
        spec: Option<&ExtensionEngineSpec>,
    ) -> bool {
        match spec {
            Some(spec) => probe_url(&self.inner.http, &spec.health_url()).await,
            None => healthcheck_engine(&self.inner.http, engine, endpoint).await,
        }
    }

    // -- public API -------------------------------------------------------

    /// Ensure a local engine is running and healthy. If it is already
    /// running (externally or previously spawned by us), this is a no-op
    /// apart from a quick healthcheck. If it is not running, the supervisor
    /// will spawn it (e.g. `ollama serve`), wait for it to become healthy,
    /// and return.
    ///
    /// Returns the endpoint URL on success.
    pub async fn ensure_running(&self, engine: &ProviderEngine) -> Result<String, SupervisorError> {
        let spec = self.extension_engine_spec(engine).await;
        let endpoint = match &spec {
            Some(s) => s.endpoint(),
            None => default_endpoint(engine)
                .ok_or_else(|| SupervisorError::UnknownEngine(engine.as_token()))?
                .to_string(),
        };

        let p = self.inner.storage.providers.active().await.unwrap_or(None);
        let active_model = p.and_then(|p| p.model);

        // Fast path: already healthy and serving the expected model?
        let is_healthy = self.probe(engine, &endpoint, spec.as_ref()).await;
        if is_healthy {
            let model_matches = if matches!(engine, ProviderEngine::LlamaCpp) {
                is_llama_server_model_match(&self.inner.http, &endpoint, active_model.as_deref())
                    .await
            } else {
                true
            };

            if model_matches {
                // Mark healthy in storage and return.
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(engine, ProviderStatus::Healthy)
                    .await;
                return Ok(endpoint);
            } else {
                tracing::info!(?engine, %endpoint, "llama-server running different model — restarting with active model");
                self.kill_owned(engine).await;
                #[cfg(windows)]
                {
                    let _ = tokio::process::Command::new("taskkill")
                        .args(["/F", "/IM", "llama-server.exe"])
                        .output()
                        .await;
                }
                #[cfg(unix)]
                {
                    let _ = tokio::process::Command::new("pkill")
                        .args(["-9", "-f", "llama-server"])
                        .output()
                        .await;
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        }

        // Not healthy — try to spawn it. Les runtimes gérés sont llama.cpp,
        // AirLLM, et tout moteur qu'une extension décrit assez précisément
        // pour être lancé.
        if spec.is_none() && !matches!(engine, ProviderEngine::LlamaCpp | ProviderEngine::AirLlm) {
            // For non-managed engines, we don't auto-spawn — just report the
            // endpoint and let the caller decide.
            let _ = self
                .inner
                .storage
                .providers
                .set_status_by_engine(engine, ProviderStatus::Unhealthy)
                .await;
            return Err(SupervisorError::NotRunning(engine.clone()));
        }

        tracing::info!(%endpoint, ?engine, "engine not running — auto-spawning runtime");

        // Set status to Starting in storage.
        let _ = self
            .inner
            .storage
            .providers
            .set_status_by_engine(engine, ProviderStatus::Starting)
            .await;

        // Spawn the runtime process: llama-server, the AirLLM Python server,
        // or le serveur décrit par une extension.
        let child = match (&spec, engine) {
            (Some(spec), _) => extension_engine::spawn(spec, active_model.as_deref()).await?,
            (None, ProviderEngine::AirLlm) => {
                spawn_airllm_server(&self.inner.cfg, active_model.as_deref()).await?
            }
            (None, _) => {
                spawn_llama_server(&self.inner.cfg, active_model.as_deref(), &self.inner.http)
                    .await?
            }
        };

        {
            let mut states = self.inner.states.lock().await;
            // A restart must not silently unpin: if the user pinned this
            // engine and the process died, it comes back pinned.
            let pinned = states.get(engine).is_some_and(|s| s.pinned);
            states.insert(
                engine.clone(),
                EngineState {
                    child: Some(child),
                    last_activity: Instant::now(),
                    endpoint: endpoint.clone(),
                    owned: true,
                    pinned,
                },
            );
        }

        // Wait for it to become healthy (poll until startup_timeout). AirLLM's
        // first load converts the layers and can take 10+ minutes, so it gets
        // a much longer startup budget than llama-server.
        // Un premier chargement peut convertir des poids : AirLLM et les
        // moteurs d'extension annoncent leur propre budget, le reste garde
        // celui de la configuration.
        let startup_budget = match (&spec, engine) {
            (Some(spec), _) => spec.startup_timeout(),
            (None, ProviderEngine::AirLlm) => Duration::from_secs(30 * 60),
            (None, _) => self.inner.cfg.startup_timeout,
        };
        let deadline = Instant::now() + startup_budget;
        loop {
            if self.probe(engine, &endpoint, spec.as_ref()).await {
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(engine, ProviderStatus::Healthy)
                    .await;
                tracing::info!(%endpoint, moteur = %engine.as_token(), "moteur en marche");
                return Ok(endpoint);
            }
            if Instant::now() >= deadline {
                // Clean up the dead child.
                self.kill_owned(engine).await;
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(engine, ProviderStatus::Unhealthy)
                    .await;
                return Err(SupervisorError::StartupTimeout(
                    engine.clone(),
                    startup_budget,
                ));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Record that the agent used this engine (resets the idle timer).
    pub async fn note_activity(&self, engine: &ProviderEngine) {
        let mut states = self.inner.states.lock().await;
        if let Some(s) = states.get_mut(engine) {
            s.last_activity = Instant::now();
        }
    }

    /// Check whether an engine is currently healthy (does NOT spawn).
    pub async fn is_healthy(&self, engine: &ProviderEngine) -> bool {
        let spec = self.extension_engine_spec(engine).await;
        let endpoint = match &spec {
            Some(s) => s.endpoint(),
            None => match default_endpoint(engine) {
                Some(e) => e.to_string(),
                None => return false,
            },
        };
        self.probe(engine, &endpoint, spec.as_ref()).await
    }

    /// Keep this engine in memory regardless of the idle timer, or release it
    /// back to the timer's care.
    ///
    /// Unpinning does not unload anything. It restores the ordinary rule —
    /// the engine goes when it has been idle long enough — because a user who
    /// stops pinning a model has not asked for it to disappear this instant.
    pub async fn set_pinned(&self, engine: &ProviderEngine, pinned: bool) {
        let mut states = self.inner.states.lock().await;
        if let Some(s) = states.get_mut(engine) {
            s.pinned = pinned;
            // Unpinning restarts the clock rather than back-dating the
            // eviction: otherwise a long conversation would be evicted the
            // moment it is unpinned.
            if !pinned {
                s.last_activity = Instant::now();
            }
        }
    }

    /// How long an unpinned engine may sit idle before it is unloaded. The
    /// status bar states this rather than leaving the user to discover it.
    pub fn idle_timeout_secs(&self) -> u64 {
        self.inner.cfg.idle_timeout.as_secs()
    }

    /// Whether this engine is pinned in memory.
    pub async fn is_pinned(&self, engine: &ProviderEngine) -> bool {
        let states = self.inner.states.lock().await;
        states.get(engine).is_some_and(|s| s.pinned)
    }

    /// Seconds since the last recorded activity, and whether the engine is
    /// pinned — what the status bar needs to explain itself.
    pub async fn residency(&self, engine: &ProviderEngine) -> Option<(u64, bool, bool)> {
        let mut states = self.inner.states.lock().await;
        let timeout = self.inner.cfg.idle_timeout.as_secs();
        states.get_mut(engine).map(|s| {
            let idle = Instant::now().duration_since(s.last_activity).as_secs();
            (idle.min(timeout), s.pinned, s.is_running())
        })
    }

    /// Manually stop a runtime we own. No-op if we don't own it.
    pub async fn shutdown(&self, engine: &ProviderEngine) -> Result<(), SupervisorError> {
        self.kill_owned(engine).await;
        let _ = self
            .inner
            .storage
            .providers
            .set_status_by_engine(engine, ProviderStatus::Unknown)
            .await;
        Ok(())
    }

    /// Spawn the background healthcheck + idle-shutdown loop. Returns a
    /// `JoinHandle` that the caller can await (or just drop to detach).
    ///
    /// The loop runs forever until the supervisor is dropped or the runtime
    /// task is cancelled.
    pub fn spawn_healthcheck_loop(&self) -> tokio::task::JoinHandle<()> {
        let sup = self.clone();
        tokio::spawn(async move {
            sup.healthcheck_loop().await;
        })
    }

    /// Get a snapshot of the current runtime status for all known engines.
    /// Useful for the daemon's `/v1/supervisor/status` endpoint.
    pub async fn status_snapshot(&self) -> Vec<EngineSnapshot> {
        // Les moteurs intégrés, puis ceux qu'apportent les extensions
        // installées : l'écran des réglages les montre dans une seule liste.
        let mut entrees: Vec<(ProviderEngine, String, Option<ExtensionEngineSpec>)> = [
            ProviderEngine::Ollama,
            ProviderEngine::LlamaCpp,
            ProviderEngine::Lmstudio,
            ProviderEngine::Vllm,
            ProviderEngine::AirLlm,
        ]
        .into_iter()
        .filter_map(|e| {
            let endpoint = default_endpoint(&e)?.to_string();
            Some((e, endpoint, None))
        })
        .collect();
        for spec in self.extension_engines().await {
            entrees.push((spec.engine(), spec.endpoint(), Some(spec)));
        }

        let mut out = Vec::with_capacity(entrees.len());
        for (engine, endpoint, spec) in entrees {
            let (owned, child_alive) = {
                let mut states = self.inner.states.lock().await;
                match states.get_mut(&engine) {
                    Some(s) => (s.owned, s.is_running()),
                    None => (false, false),
                }
            };
            let healthy = self.probe(&engine, &endpoint, spec.as_ref()).await;
            out.push(EngineSnapshot {
                label: spec.as_ref().map(|s| s.label.clone()),
                engine,
                endpoint,
                healthy,
                owned,
                child_alive,
            });
        }
        out
    }

    // -- internal ---------------------------------------------------------

    /// The main loop: every `healthcheck_interval`, poll each owned engine.
    /// If unhealthy, update storage. If idle for longer than `idle_timeout`,
    /// shut it down.
    async fn healthcheck_loop(&self) {
        let interval = self.inner.cfg.healthcheck_interval;
        let idle_timeout = self.inner.cfg.idle_timeout;
        tracing::debug!(?interval, ?idle_timeout, "supervisor loop started");

        loop {
            tokio::time::sleep(interval).await;

            let mut to_shutdown = Vec::new();
            // Photographié hors du verrou des états : la sonde d'un moteur
            // d'extension vit dans l'autre registre.
            let extension_specs: HashMap<String, ExtensionEngineSpec> = self
                .extension_engines()
                .await
                .into_iter()
                .map(|s| (s.engine().as_token(), s))
                .collect();

            {
                let mut states = self.inner.states.lock().await;
                for (engine, state) in states.iter_mut() {
                    // Only manage engines we own.
                    if !state.owned {
                        continue;
                    }

                    // Check if the child process is still alive.
                    if !state.is_running() {
                        tracing::warn!(?engine, "spawned runtime exited unexpectedly");
                        let _ = self
                            .inner
                            .storage
                            .providers
                            .set_status_by_engine(engine, ProviderStatus::Unhealthy)
                            .await;
                        // Remove the dead child entry; it will be re-spawned
                        // on the next ensure_running().
                        state.child = None;
                        continue;
                    }

                    // Healthcheck the HTTP endpoint. Un moteur d'extension a
                    // sa propre sonde ; la lire ici évite de déclarer mort un
                    // serveur qui n'expose pas `/v1/models`.
                    let healthy = match extension_specs.get(engine.as_token().as_str()) {
                        Some(spec) => probe_url(&self.inner.http, &spec.health_url()).await,
                        None => healthcheck_engine(&self.inner.http, engine, &state.endpoint).await,
                    };
                    let new_status = if healthy {
                        ProviderStatus::Healthy
                    } else {
                        ProviderStatus::Starting
                    };
                    let _ = self
                        .inner
                        .storage
                        .providers
                        .set_status_by_engine(engine, new_status)
                        .await;

                    // Idle check. A pinned engine is never evicted: the user
                    // loaded it on purpose and will unload it on purpose.
                    if state.pinned {
                        continue;
                    }
                    let idle = Instant::now().duration_since(state.last_activity);
                    if idle >= idle_timeout {
                        tracing::info!(
                            ?engine,
                            idle_secs = idle.as_secs(),
                            "runtime idle â€” shutting down"
                        );
                        to_shutdown.push(engine.clone());
                    }
                }
            }

            // Shutdown outside the lock to avoid holding it during kill.
            for engine in to_shutdown {
                self.kill_owned(&engine).await;
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(&engine, ProviderStatus::Unknown)
                    .await;
            }
        }
    }

    /// Stop the spawned runtime on purpose: the next `ensure_running()`
    /// restarts it from the configuration now on disk.
    ///
    /// The context window is not a request setting — llama-server sizes its
    /// KV cache at boot. Editing `inference_config.json` while the process
    /// keeps running changes nothing, so "Appliquer" would be a lie. Asking
    /// for a restart is the only honest way a saved context length reaches
    /// the model.
    pub async fn restart_requested(&self, engine: &ProviderEngine) {
        self.kill_owned(engine).await;
        #[cfg(windows)]
        {
            let _ = tokio::process::Command::new("taskkill")
                .args(["/F", "/IM", "llama-server.exe"])
                .output()
                .await;
        }
        #[cfg(unix)]
        {
            let _ = tokio::process::Command::new("pkill")
                .args(["-9", "-f", "llama-server"])
                .output()
                .await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    /// Kill a spawned child process (owned only). Removes the entry from the
    /// state map.
    async fn kill_owned(&self, engine: &ProviderEngine) {
        let mut states = self.inner.states.lock().await;
        if let Some(mut state) = states.remove(engine) {
            if state.owned {
                if let Some(child) = state.child.as_mut() {
                    // Try graceful kill first (SIGTERM on Unix, Kill on Win).
                    #[cfg(unix)]
                    {
                        let pid = child.id();
                        if let Some(pid) = pid {
                            // Send SIGTERM via `kill` command (portable enough).
                            let _ = tokio::process::Command::new("kill")
                                .arg("-TERM")
                                .arg(pid.to_string())
                                .output()
                                .await;
                        }
                    }
                    #[cfg(windows)]
                    {
                        // On Windows, just kill the task tree.
                        let _ = child.kill().await;
                    }

                    // Give it 5s to exit gracefully, then force-kill.
                    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                        Ok(Ok(_status)) => {
                            tracing::info!(?engine, "runtime shut down gracefully");
                        }
                        _ => {
                            let _ = child.kill().await;
                            tracing::warn!(?engine, "runtime force-killed");
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Snapshot type (returned to daemon / CLI)
// ============================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineSnapshot {
    pub engine: ProviderEngine,
    /// Nom affiché quand le moteur vient d'une extension. `None` pour les
    /// runtimes intégrés, dont l'interface connaît déjà le nom.
    pub label: Option<String>,
    pub endpoint: String,
    pub healthy: bool,
    /// True if the supervisor spawned this runtime (and can shut it down).
    pub owned: bool,
    /// True if the spawned child process is still alive.
    pub child_alive: bool,
}

// ============================================================================
// Free functions
// ============================================================================

/// Adresse loopback par défaut d'un moteur intégré.
///
/// `None` pour un moteur apporté par une extension : son adresse est celle de
/// son manifeste, et l'inventer ici ferait sonder un port au hasard.
/// Utilisez [`Supervisor::endpoint_for`] pour couvrir les deux cas.
pub fn default_endpoint(e: &ProviderEngine) -> Option<&'static str> {
    match e {
        ProviderEngine::Ollama => Some("http://127.0.0.1:11434"),
        ProviderEngine::LlamaCpp => Some("http://127.0.0.1:8080"),
        ProviderEngine::Lmstudio => Some("http://127.0.0.1:1234"),
        ProviderEngine::Vllm => Some("http://127.0.0.1:8000"),
        ProviderEngine::OpenAiCompat => Some("http://127.0.0.1:8000"),
        ProviderEngine::AirLlm => Some("http://127.0.0.1:8090"),
        ProviderEngine::Extension(_) => None,
    }
}

/// Une requête `GET` qui répond 2xx : la sonde la plus simple, partagée par
/// tous les chemins de santé.
pub async fn probe_url(client: &reqwest::Client, url: &str) -> bool {
    client
        .get(url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Healthcheck an engine by hitting its HTTP endpoint.
/// Returns `true` if the runtime responds with a 2xx status.
pub async fn healthcheck_engine(
    client: &reqwest::Client,
    engine: &ProviderEngine,
    endpoint: &str,
) -> bool {
    // Try the OpenAI-compatible /v1/models endpoint first (works for all
    // engines). Fall back to Ollama's native /api/version for Ollama.
    if probe_url(client, &format!("{endpoint}/v1/models")).await {
        return true;
    }
    if matches!(engine, ProviderEngine::Ollama) {
        return probe_url(client, &format!("{endpoint}/api/version")).await;
    }
    false
}

/// Vérifie si l'instance llama-server qui tourne actuellement sert bien le
/// modèle demandé, ou si elle sert encore un ancien modèle d'une session précédente.
pub async fn is_llama_server_model_match(
    http: &reqwest::Client,
    endpoint: &str,
    wanted_model: Option<&str>,
) -> bool {
    let Some(wanted) = wanted_model else {
        return true;
    };
    let wanted_clean = wanted.trim();
    if wanted_clean.is_empty() {
        return true;
    }

    let Ok(res) = http.get(format!("{endpoint}/props")).send().await else {
        return false;
    };
    let Ok(val) = res.json::<serde_json::Value>().await else {
        return false;
    };

    let model_alias = val
        .get("model_alias")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let model_path = val.get("model_path").and_then(|v| v.as_str()).unwrap_or("");
    let default_model = val
        .pointer("/default_generation_settings/model")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let w_lower = wanted_clean.to_lowercase();
    let w_fname = std::path::Path::new(&w_lower)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(&w_lower);

    for candidate in [model_alias, model_path, default_model] {
        if candidate.is_empty() {
            continue;
        }
        let c_lower = candidate.trim().to_lowercase();
        let c_fname = std::path::Path::new(&c_lower)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(&c_lower);

        if c_lower == w_lower
            || c_lower.contains(&w_lower)
            || w_lower.contains(&c_lower)
            || c_fname == w_fname
            || c_fname.contains(w_fname)
            || w_fname.contains(c_fname)
        {
            return true;
        }
    }

    false
}

/// Locate a binary on PATH (simple cross-platform `which`).
pub fn which(name: &str) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    let ext = if cfg!(windows) { ".exe" } else { "" };
    for dir in std::env::split_paths(&path_env) {
        let full = dir.join(format!("{name}{ext}"));
        if full.is_file() {
            return Some(full);
        }
    }
    None
}

/// Où se trouve `llama-server`, s'il est là.
///
/// Trois emplacements, dans cet ordre : le runtime géré par l'application
/// (`bin/llama/`), l'ancien dossier plat, puis le chemin du système. Cette
/// fonction est publique parce que l'écran des réglages doit répondre la même
/// chose que le lanceur : il regardait ailleurs, annonçait « non installé »
/// pour un runtime présent, et « installé » pour un runtime que le lanceur ne
/// trouvait pas. Le nom de l'exécutable suit la plateforme — la version
/// codée en dur pour Windows rendait la réponse fausse partout ailleurs.
pub fn llama_server_path() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let bin_root = locaryn_config::bin_dir();
    let managed = bin_root.join("llama").join(exe_name);
    if managed.exists() {
        return Some(managed);
    }
    let legacy = bin_root.join(exe_name);
    if legacy.exists() {
        return Some(legacy);
    }
    which("llama-server")
}

/// La version de llama.cpp que l'application connaît.
pub const LLAMA_BUILD: &str = "b10088";

/// L'archive à récupérer pour cette plateforme, s'il en existe une.
fn llama_release_url() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("https://github.com/ggml-org/llama.cpp/releases/download/b10088/llama-b10088-bin-win-vulkan-x64.zip")
    } else if cfg!(target_os = "linux") {
        Some("https://github.com/ggml-org/llama.cpp/releases/download/b10088/llama-b10088-bin-ubuntu-x64.zip")
    } else {
        None
    }
}

/// Sortir l'archive du moteur dans `bin/llama/`, à plat.
///
/// Les publications de llama.cpp rangent tantôt les fichiers à la racine,
/// tantôt sous `build/bin/`. Ce qui compte est que `llama-server` et les
/// bibliothèques `ggml` finissent côte à côte : sous Windows, l'exécutable ne
/// démarre pas sans ses DLL dans le même dossier.
fn extraire_archive_llama(
    archive: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), SupervisorError> {
    let fichier = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(fichier).map_err(|e| {
        SupervisorError::SpawnFailed(
            ProviderEngine::LlamaCpp,
            format!("archive du moteur illisible : {e}"),
        )
    })?;
    std::fs::create_dir_all(dest)?;
    for index in 0..zip.len() {
        let mut entree = zip.by_index(index).map_err(|e| {
            SupervisorError::SpawnFailed(
                ProviderEngine::LlamaCpp,
                format!("archive du moteur illisible : {e}"),
            )
        })?;
        if entree.is_dir() {
            continue;
        }
        // Un nom qui sortirait du dossier de destination est ignoré, pas
        // réécrit : une archive n'a pas à choisir où elle atterrit.
        let Some(nom) = entree
            .enclosed_name()
            .and_then(|chemin| chemin.file_name().map(|n| n.to_os_string()))
        else {
            continue;
        };
        let cible = dest.join(&nom);
        let mut sortie = std::fs::File::create(&cible)?;
        std::io::copy(&mut entree, &mut sortie)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entree.unix_mode() {
                let _ = std::fs::set_permissions(&cible, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

/// Installer le moteur d'inférence si la machine ne l'a pas encore.
///
/// Un moteur n'est pas un choix laissé à l'utilisateur : sans lui, aucun
/// modèle ne peut répondre, et lui demander d'aller le chercher dans un écran
/// de réglages revient à livrer une application qui ne marche pas au premier
/// lancement. Il arrive donc avec le reste, au moment où il sert.
pub async fn provision_llama_server(http: &reqwest::Client) -> Result<PathBuf, SupervisorError> {
    if let Some(existant) = llama_server_path() {
        return Ok(existant);
    }
    let url = llama_release_url().ok_or_else(|| {
        SupervisorError::BinaryNotFound(
            "llama-server — aucune version prête pour ce système ; installez llama.cpp              (`brew install llama.cpp`) et l'application le trouvera sur le chemin"
                .into(),
        )
    })?;

    let bin_root = locaryn_config::bin_dir();
    let runtime_dir = bin_root.join("llama");
    std::fs::create_dir_all(&runtime_dir)?;
    let archive = bin_root.join("llama-runtime.zip");

    tracing::info!(%url, "moteur d'inférence absent — installation");
    let reponse = http.get(url).send().await.map_err(|e| {
        SupervisorError::SpawnFailed(
            ProviderEngine::LlamaCpp,
            format!("téléchargement du moteur impossible : {e}"),
        )
    })?;
    if !reponse.status().is_success() {
        return Err(SupervisorError::SpawnFailed(
            ProviderEngine::LlamaCpp,
            format!("téléchargement du moteur : HTTP {}", reponse.status()),
        ));
    }
    let octets = reponse.bytes().await.map_err(|e| {
        SupervisorError::SpawnFailed(
            ProviderEngine::LlamaCpp,
            format!("téléchargement du moteur interrompu : {e}"),
        )
    })?;
    std::fs::write(&archive, &octets)?;

    let extraction = extraire_archive_llama(&archive, &runtime_dir);
    let _ = std::fs::remove_file(&archive);
    extraction?;

    llama_server_path().ok_or_else(|| {
        SupervisorError::SpawnFailed(
            ProviderEngine::LlamaCpp,
            "l'archive du moteur ne contenait pas llama-server".into(),
        )
    })
}

/// Ollama's OpenAI-compatible endpoint ignores `options` silently — the
/// context only follows `num_ctx` on its native `/api/chat` (measured on
/// 0.33.x: a 10k-token prompt is cut at 2048 through /v1, processed in full
/// through /api/chat). Callers that send `num_ctx` must know where to send
/// it, or the setting is decorative.
pub fn ollama_options_hint() -> serde_json::Value {
    serde_json::json!({ "endpoint": "/api/chat", "body_key": "options.num_ctx" })
}

/// Spawn `ollama serve` as a detached child process.
///
/// We set `OLLAMA_HOST=127.0.0.1:11434` to guarantee loopback binding even
/// if the user's environment defaults to something else. stdout/stderr are
/// piped to /dev/null (or NUL on Windows) to avoid the child holding the
/// daemon's terminal.
#[allow(dead_code)]
async fn spawn_ollama(cfg: &SupervisorConfig) -> Result<Child, SupervisorError> {
    let bin = cfg
        .ollama_bin
        .clone()
        .or_else(|| which("ollama"))
        .ok_or_else(|| SupervisorError::BinaryNotFound("ollama".into()))?;

    tracing::info!(bin = %bin.display(), "spawning ollama serve");

    let mut cmd = Command::new(&bin);
    cmd.arg("serve")
        .env("OLLAMA_HOST", "127.0.0.1:11434")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // On Windows, CREATE_NO_WINDOW + DETACHED_PROCESS to avoid a console
    // popup and detach from the daemon's process group.
    // On Unix, the child inherits null stdio (set above) which is sufficient
    // for V1 â€” the idle timer and explicit shutdown handle cleanup.
    // (V1.1: add `nix` crate for proper setsid detachment on Unix.)
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW = 0x08000000
        // DETACHED_PROCESS  = 0x00000008
        // tokio::process::Command has an inherent creation_flags() method
        // on Windows â€” no trait import needed.
        cmd.creation_flags(0x08000008);
    }

    cmd.spawn()
        .map_err(|e| SupervisorError::SpawnFailed(ProviderEngine::Ollama, e.to_string()))
}

async fn spawn_llama_server(
    _cfg: &SupervisorConfig,
    active_model: Option<&str>,
    http: &reqwest::Client,
) -> Result<Child, SupervisorError> {
    let data_dir = locaryn_config::default_data_dir();

    // Load inference config — determines which args to pass.
    let inference_cfg_path = data_dir.join("inference_config.json");
    let inference_cfg: serde_json::Value = std::fs::read_to_string(&inference_cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "profile": "balanced",
                "gpu_layers": -1,
                "kv_cache_type": "q8_0",
                "context_length": 8192,
                "flash_attention": true,
                "cpu_threads": 0,
                "batch_size": 512,
                "use_mmap": true,
                "parallel_slots": 1,
                "draft_model_path": ""
            })
        });

    // Binary resolution: prefer the managed runtime (data_dir/bin/llama —
    // installed/updated by the app, pinned modern build), then the legacy
    // flat bin dir, then PATH.
    // Absent : on l'installe, plutôt que d'échouer en demandant à quelqu'un
    // d'aller le chercher. C'est une dépendance de l'application, pas une
    // option.
    let bin = provision_llama_server(http).await?;

    let model_name = active_model.unwrap_or("model.gguf");
    let model_file = if model_name.starts_with("http") {
        model_name.split('/').next_back().unwrap_or("model.gguf")
    } else {
        model_name
    };
    let models_dir = locaryn_config::models_dir();
    let full_model_path = if std::path::Path::new(model_file).is_absolute() {
        std::path::PathBuf::from(model_file)
    } else {
        models_dir.join(model_file)
    };

    if !full_model_path.exists() {
        return Err(SupervisorError::SpawnFailed(
            ProviderEngine::LlamaCpp,
            format!(
                "fichier de poids introuvable — {}",
                full_model_path.display()
            ),
        ));
    }

    let is_gguf = full_model_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"));
    if !full_model_path.is_file() || !is_gguf {
        return Err(SupervisorError::SpawnFailed(
            ProviderEngine::LlamaCpp,
            format!(
                "Unsupported model format for llama.cpp: {}. Install a GGUF model; Transformers .safetensors repositories cannot be passed to llama-server -m.",
                full_model_path.display()
            ),
        ));
    }

    tracing::info!(
        bin = %bin.display(),
        model = %full_model_path.display(),
        profile = %inference_cfg["profile"].as_str().unwrap_or("balanced"),
        "spawning llama-server"
    );

    // Flags verified against current llama.cpp (b10088): unknown flags are a
    // FATAL error for llama-server, so only pass documented ones.
    let mut cmd = Command::new(&bin);
    cmd.arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("8080")
        .arg("-m")
        .arg(&full_model_path)
        .arg("--chat-template")
        .arg("chatml");

    // Vision: if a matching mmproj file sits next to the model
    // (mmproj-<model>.gguf or a single mmproj-*.gguf in the dir), load it.
    if let Some(mmproj) = find_mmproj_for(&full_model_path) {
        tracing::info!(mmproj = %mmproj.display(), "loading multimodal projector");
        cmd.arg("--mmproj").arg(mmproj);
    }

    // GPU layers: -1 = all.
    let gpu_layers = inference_cfg["gpu_layers"].as_i64().unwrap_or(-1);
    if gpu_layers == -1 {
        cmd.arg("-ngl").arg("999");
    } else {
        cmd.arg("-ngl").arg(gpu_layers.to_string());
    }

    // KV cache quantization: the real flags are -ctk/-ctv (NOT --kv-cache-type,
    // which does not exist and would abort startup). "turbo3" is a legacy label
    // from a fake "3-bit TurboQuant" — mainline llama.cpp has no 3-bit KV, so we
    // map it to the real maximal compression (4-bit q4_0).
    let kv_type = match inference_cfg["kv_cache_type"].as_str().unwrap_or("f16") {
        "turbo3" | "turboquant" => "q4_0",
        other => other,
    };
    if matches!(
        kv_type,
        "q8_0" | "q5_1" | "q5_0" | "q4_1" | "q4_0" | "iq4_nl"
    ) {
        cmd.arg("-ctk").arg(kv_type).arg("-ctv").arg(kv_type);
    }

    // Context length.
    let ctx = inference_cfg["context_length"].as_u64().unwrap_or(8192);
    cmd.arg("-c").arg(ctx.to_string());

    // Suggest to the UI the largest context this model was trained for: a
    // slider that proposes 128k to a 32k model invites a request the engine
    // must quietly clamp. Best effort — the file may not declare it.
    if let Ok(sum) = locaryn_llmfit::read_summary(&full_model_path) {
        if sum.train_context > 0 {
            let cap = (sum.train_context as f64 * 1.5).round() as u64;
            let _ = std::fs::write(
                data_dir.join("model_ctx_capacity.json"),
                format!("{{\"max_ctx\":{cap}}}"),
            );
        }
    }

    // Flash attention takes a value (on|off|auto). If the user disabled it, pass
    // off. Otherwise, when the KV cache is quantized we force it on, because a
    // quantized V cache requires Flash Attention in llama.cpp; else leave auto.
    let kv_is_quantized = matches!(
        kv_type,
        "q8_0" | "q5_1" | "q5_0" | "q4_1" | "q4_0" | "iq4_nl"
    );
    if !inference_cfg["flash_attention"].as_bool().unwrap_or(true) {
        cmd.arg("-fa").arg("off");
    } else if kv_is_quantized {
        cmd.arg("-fa").arg("on");
    }

    // MoE expert offload to CPU: run very large Mixture-of-Experts models on a
    // modest GPU by keeping expert weights in system RAM while attention stays on
    // the GPU. -1 = all experts on CPU (-cmoe); N>0 = experts of the first N
    // layers (-ncmoe N). Verified present in b10088.
    match inference_cfg["n_cpu_moe"].as_i64().unwrap_or(0) {
        0 => {}
        n if n < 0 => {
            cmd.arg("-cmoe");
        }
        n => {
            cmd.arg("-ncmoe").arg(n.to_string());
        }
    }

    // Distributed inference: spread model layers across networked ggml-rpc-server
    // workers (host:port,host:port). Empty = single machine. (--rpc)
    if let Some(rpc) = inference_cfg["rpc_servers"].as_str() {
        let rpc = rpc.trim();
        if !rpc.is_empty() {
            cmd.arg("--rpc").arg(rpc);
        }
    }

    // LoRA adapters: preload each at scale 1.0 (--lora, one flag per adapter to
    // stay safe with Windows paths that contain ':'). Their scale can then be
    // changed live via the /lora-adapters endpoint. Skip missing files so a stale
    // path can never make startup FATAL.
    if let Some(adapters) = inference_cfg["lora_adapters"].as_array() {
        for a in adapters {
            if let Some(path) = a.as_str() {
                let path = path.trim();
                if !path.is_empty() && std::path::Path::new(path).exists() {
                    cmd.arg("--lora").arg(path);
                } else if !path.is_empty() {
                    tracing::warn!(adapter = %path, "LoRA adapter file missing, skipping");
                }
            }
        }
    }

    // CPU threads: default to logical cores - 2 to prevent crawling on 1 thread.
    let default_threads = std::thread::available_parallelism()
        .map(|n| (n.get() as u64).saturating_sub(2).max(1))
        .unwrap_or(4);
    let threads = inference_cfg["cpu_threads"].as_u64().unwrap_or(0);
    if threads > 0 {
        cmd.arg("-t").arg(threads.to_string());
    } else {
        cmd.arg("-t").arg(default_threads.to_string());
    }

    // Batch size.
    let batch = inference_cfg["batch_size"].as_u64().unwrap_or(512);
    cmd.arg("-b").arg(batch.to_string());

    // mmap.
    if !inference_cfg["use_mmap"].as_bool().unwrap_or(true) {
        cmd.arg("--no-mmap");
    }

    // Parallel slots.
    let slots = inference_cfg["parallel_slots"].as_u64().unwrap_or(1);
    if slots > 1 {
        cmd.arg("-np").arg(slots.to_string());
    }

    // Speculative decoding draft model (current flag: -md).
    if let Some(draft) = inference_cfg["draft_model_path"].as_str() {
        if !draft.is_empty() {
            cmd.arg("-md").arg(draft);
        }
    }

    // Log to a file so startup failures are diagnosable from the app.
    let log_path = data_dir.join("llama-server.log");
    let log_file = std::fs::File::create(&log_path).ok();
    cmd.stdin(Stdio::null());
    match log_file {
        Some(f) => {
            let f2 = f.try_clone().ok();
            cmd.stdout(Stdio::from(f));
            match f2 {
                Some(f2) => cmd.stderr(Stdio::from(f2)),
                None => cmd.stderr(Stdio::null()),
            };
        }
        None => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }

    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000008);
    }

    // Sans ceci, fermer l'application laissait llama-server tourner : le
    // processus n'était l'enfant de rien qui le tue à sa sortie, gardait le
    // modèle en VRAM indéfiniment, et le superviseur du lancement suivant —
    // sa table d'états repartant vide — ne pouvait plus jamais l'éteindre
    // (« éjecter » n'avait alors aucune prise sur lui).
    cmd.kill_on_drop(true);

    cmd.spawn()
        .map_err(|e| SupervisorError::SpawnFailed(ProviderEngine::LlamaCpp, e.to_string()))
}

/// AirLLM OpenAI-compatible server (embedded Python). Speaks the
/// `/v1/models` + `/v1/chat/completions` (SSE streaming) protocol so the
/// existing agent client works unchanged. Runs huge models on small GPUs via
/// AirLLM layer-by-layer offloading.
const AIRLLM_SERVER_PY: &str = r#"#!/usr/bin/env python
"""AirLLM OpenAI-compatible server - /v1/models + /v1/chat/completions (SSE)."""
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading

ap = argparse.ArgumentParser()
ap.add_argument("--model", required=True)
ap.add_argument("--port", type=int, default=8090)
ap.add_argument("--max-tokens", type=int, default=2048)
args = ap.parse_args()

MODEL_ID = args.model
MAX_NEW = args.max_tokens


def log(*a):
    print(*a, flush=True)


log("[airllm] importing torch + AirLLM (first import can be slow)...")
from airllm import AutoModel  # noqa: E402
from transformers import AutoTokenizer  # noqa: E402

log(f"[airllm] loading tokenizer {MODEL_ID}")
tokenizer = AutoTokenizer.from_pretrained(MODEL_ID)

log(f"[airllm] loading model {MODEL_ID} (first run converts layers, be patient)...")
model = AutoModel.from_pretrained(MODEL_ID)
log(f"[airllm] model ready - serving on port {args.port}")

gen_lock = threading.Lock()


def run_generate(messages, temperature, max_tokens):
    prompt = tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
    inputs = tokenizer(prompt, return_tensors="pt")
    n_new = max(1, min(max_tokens or MAX_NEW, MAX_NEW))
    do_sample = temperature is not None and temperature > 0
    temp = temperature if (temperature is not None and temperature > 0) else 1.0
    with gen_lock:
        out = model.generate(
            inputs.input_ids,
            max_new_tokens=n_new,
            do_sample=do_sample,
            temperature=temp,
            top_p=0.9,
            repetition_penalty=1.05,
        )
    new_ids = out[0][inputs.input_ids.shape[1]:]
    return tokenizer.decode(new_ids, skip_special_tokens=True)


def chunk_text(text, size=24):
    for i in range(0, len(text), size):
        yield text[i:i + size]


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def _send(self, code, ctype, body=None):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Access-Control-Allow-Origin", "*")
        if body is not None:
            self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body is not None:
            self.wfile.write(body)

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "POST, GET, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, Authorization")
        self.end_headers()

    def do_GET(self):
        if self.path.rstrip("/") == "/v1/models":
            payload = {
                "object": "list",
                "data": [{"id": MODEL_ID, "object": "model", "created": 0, "owned_by": "airllm"}],
            }
            self._send(200, "application/json", json.dumps(payload).encode())
            return
        self._send(404, "application/json", b'{"error":{"message":"not found"}}')

    def do_POST(self):
        if self.path.rstrip("/") != "/v1/chat/completions":
            self._send(404, "application/json", b'{"error":{"message":"not found"}}')
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            req = json.loads(self.rfile.read(length) or b"{}")
        except Exception as e:
            self._send(400, "application/json", json.dumps({"error": {"message": str(e)}}).encode())
            return
        messages = req.get("messages") or []
        temperature = req.get("temperature")
        max_tokens = req.get("max_tokens")
        stream = bool(req.get("stream", False))
        try:
            text = run_generate(messages, temperature, max_tokens)
        except Exception as e:
            self._send(500, "application/json", json.dumps({"error": {"message": str(e)}}).encode())
            return
        if not stream:
            payload = {
                "id": "chatcmpl-airllm",
                "object": "chat.completion",
                "created": 0,
                "model": MODEL_ID,
                "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
            }
            self._send(200, "application/json", json.dumps(payload).encode())
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        try:
            def ev(obj):
                self.wfile.write(b"data: " + json.dumps(obj).encode() + b"\n\n")
                self.wfile.flush()
            ev({"id": "chatcmpl-airllm", "object": "chat.completion.chunk", "created": 0,
                "model": MODEL_ID,
                "choices": [{"index": 0, "delta": {"role": "assistant", "content": ""}, "finish_reason": None}]})
            for piece in chunk_text(text):
                ev({"choices": [{"index": 0, "delta": {"content": piece}, "finish_reason": None}]})
            ev({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


srv = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
log(f"[airllm] listening on http://127.0.0.1:{args.port}")
srv.serve_forever()
"#;

/// Spawn the AirLLM Python server (OpenAI-compatible) for the active model.
async fn spawn_airllm_server(
    cfg: &SupervisorConfig,
    active_model: Option<&str>,
) -> Result<Child, SupervisorError> {
    let model = active_model.unwrap_or("Qwen/Qwen2.5-3B-Instruct");
    let data_dir = locaryn_config::default_data_dir();
    let script_path = data_dir.join("airllm_server.py");
    std::fs::write(&script_path, AIRLLM_SERVER_PY).map_err(|e| {
        SupervisorError::SpawnFailed(ProviderEngine::AirLlm, format!("write server script: {e}"))
    })?;

    let python = cfg
        .airllm_python
        .clone()
        .or_else(|| which("python"))
        .ok_or_else(|| SupervisorError::BinaryNotFound("python".into()))?;

    let hf = locaryn_config::hf_cache_dir();
    let temp = locaryn_config::ensure_temp_dir();

    tracing::info!(model = %model, python = %python.display(), "spawning AirLLM server");

    let mut cmd = Command::new(&python);
    cmd.arg(&script_path)
        .arg("--model")
        .arg(model)
        .arg("--port")
        .arg("8090")
        .env("HF_HOME", &hf)
        .env("TRANSFORMERS_NO_TF", "1")
        .env("USE_TF", "0")
        .env("TF_CPP_MIN_LOG_LEVEL", "3")
        .env("TMPDIR", &temp)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .stdin(Stdio::null());

    let log_path = data_dir.join("airllm-server.log");
    let log_file = std::fs::File::create(&log_path).ok();
    match log_file {
        Some(f) => {
            let f2 = f.try_clone().ok();
            cmd.stdout(Stdio::from(f));
            match f2 {
                Some(f2) => cmd.stderr(Stdio::from(f2)),
                None => cmd.stderr(Stdio::null()),
            };
        }
        None => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }

    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000008);
    }

    // Même raison que pour llama-server : sans ceci, un AirLLM resté en
    // mémoire après la fermeture de l'application n'était plus rattrapable.
    cmd.kill_on_drop(true);

    cmd.spawn()
        .map_err(|e| SupervisorError::SpawnFailed(ProviderEngine::AirLlm, e.to_string()))
}

/// Find the multimodal projector that belongs to `model_path`, if any:
/// prefer an `mmproj-*` file sharing the model's stem prefix, else a single
/// unambiguous `mmproj-*.gguf` in the same directory.
///
/// Public parce que c'est aussi la reponse a « ce modele accepte-t-il des
/// images ». Le lancement passe `--mmproj` si et seulement si cette fonction
/// trouve quelque chose : l'interface doit se fonder sur le meme fichier, sans
/// quoi elle proposerait de joindre des images que le moteur ne recevrait pas.
/// Elle ne lit que le dossier, donc elle repond aussi quand rien n'est charge.
pub fn find_mmproj_for(model_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let dir = model_path.parent()?;
    let stem = model_path.file_stem()?.to_string_lossy().to_lowercase();
    // Base model name without the quant suffix, e.g. "qwen2-vl-2b-instruct".
    let base = stem
        .split("-q4")
        .next()
        .and_then(|s| s.split("-q5").next())
        .and_then(|s| s.split("-q6").next())
        .and_then(|s| s.split("-q8").next())
        .and_then(|s| s.split("-f16").next())
        .unwrap_or(&stem)
        .to_string();

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
            continue;
        };
        if name.starts_with("mmproj-") && name.ends_with(".gguf") {
            if name.contains(&base) {
                return Some(p);
            }
            candidates.push(p);
        }
    }
    if candidates.len() == 1 {
        return candidates.pop();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// « Installer le moteur » téléchargeait l'archive et s'arrêtait là, en
    /// annonçant une réussite. L'écran disait installé, le chat répondait
    /// « exécutable introuvable », et le journal conseillé pour comprendre
    /// n'existait pas — il n'est écrit qu'au premier démarrage du moteur.
    #[test]
    fn l_archive_du_moteur_est_mise_a_plat() {
        let base = std::env::temp_dir().join(format!(
            "locaryn_runtime_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let archive = base.join("llama.zip");

        // Les publications de llama.cpp rangent tantôt à la racine, tantôt
        // sous `build/bin/` : les deux doivent aboutir au même dossier.
        let fichier = std::fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(fichier);
        let options: zip::write::SimpleFileOptions = Default::default();
        for chemin in [
            "llama-server.exe",
            "ggml-base.dll",
            "build/bin/ggml-vulkan.dll",
        ] {
            zip.start_file(chemin, options).unwrap();
            zip.write_all(b"binaire").unwrap();
        }
        zip.finish().unwrap();

        let dest = base.join("llama");
        extraire_archive_llama(&archive, &dest).unwrap();
        for nom in ["llama-server.exe", "ggml-base.dll", "ggml-vulkan.dll"] {
            assert!(
                dest.join(nom).is_file(),
                "{nom} devrait être extrait à plat"
            );
        }
        assert!(
            !dest.join("build").exists(),
            "l'arborescence de l'archive ne doit pas être recopiée : sous Windows \
             l'exécutable ne démarre pas sans ses DLL à côté de lui"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
