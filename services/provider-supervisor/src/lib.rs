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
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            healthcheck_interval: Duration::from_secs(15),
            startup_timeout: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(30 * 60), // 30 min
            ollama_bin: None,
        }
    }
}

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("engine {0:?} is not running")]
    NotRunning(ProviderEngine),
    #[error("engine {0:?} failed to start within {1:?}")]
    StartupTimeout(ProviderEngine, Duration),
    #[error("spawn failed for {0:?}: {1}")]
    SpawnFailed(ProviderEngine, String),
    #[error("binary not found on PATH: {0}")]
    BinaryNotFound(String),
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
            }),
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
    pub async fn ensure_running(
        &self,
        engine: ProviderEngine,
    ) -> Result<String, SupervisorError> {
        let endpoint = default_endpoint(engine).to_string();

        // Fast path: already healthy?
        if healthcheck_engine(&self.inner.http, engine, &endpoint).await {
            // Mark healthy in storage and return.
            let _ = self
                .inner
                .storage
                .providers
                .set_status_by_engine(engine, ProviderStatus::Healthy)
                .await;
            return Ok(endpoint);
        }

        // Not healthy — try to spawn it (only LlamaCpp is auto-spawned in V1).
        if engine != ProviderEngine::LlamaCpp {
            // For non-managed engines, we don't auto-spawn — just report the
            // endpoint and let the caller decide.
            let _ = self
                .inner
                .storage
                .providers
                .set_status_by_engine(engine, ProviderStatus::Unhealthy)
                .await;
            return Err(SupervisorError::NotRunning(engine));
        }

        tracing::info!(%endpoint, ?engine, "engine not running — auto-spawning llama-server");
        
        let p = self.inner.storage.providers.active().await.unwrap_or(None);
        let active_model = p.and_then(|p| p.model);

        // Set status to Starting in storage.
        let _ = self
            .inner
            .storage
            .providers
            .set_status_by_engine(engine, ProviderStatus::Starting)
            .await;

        // Spawn llama-server process.
        let child = spawn_llama_server(&self.inner.cfg, active_model.as_deref()).await?;

        {
            let mut states = self.inner.states.lock().await;
            states.insert(
                engine,
                EngineState {
                    child: Some(child),
                    last_activity: Instant::now(),
                    endpoint: endpoint.clone(),
                    owned: true,
                },
            );
        }

        // Wait for it to become healthy (poll until startup_timeout).
        let deadline = Instant::now() + self.inner.cfg.startup_timeout;
        loop {
            if healthcheck_engine(&self.inner.http, engine, &endpoint).await {
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(engine, ProviderStatus::Healthy)
                    .await;
                tracing::info!(%endpoint, "ollama is now healthy");
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
                    engine,
                    self.inner.cfg.startup_timeout,
                ));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Record that the agent used this engine (resets the idle timer).
    pub async fn note_activity(&self, engine: ProviderEngine) {
        let mut states = self.inner.states.lock().await;
        if let Some(s) = states.get_mut(&engine) {
            s.last_activity = Instant::now();
        }
    }

    /// Check whether an engine is currently healthy (does NOT spawn).
    pub async fn is_healthy(&self, engine: ProviderEngine) -> bool {
        let endpoint = default_endpoint(engine).to_string();
        healthcheck_engine(&self.inner.http, engine, &endpoint).await
    }

    /// Manually stop a runtime we own. No-op if we don't own it.
    pub async fn shutdown(&self, engine: ProviderEngine) -> Result<(), SupervisorError> {
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
        let engines = [
            ProviderEngine::Ollama,
            ProviderEngine::LlamaCpp,
            ProviderEngine::Lmstudio,
            ProviderEngine::Vllm,
        ];
        let mut states = self.inner.states.lock().await;
        let mut out = Vec::with_capacity(engines.len());
        for e in engines {
            let endpoint = default_endpoint(e).to_string();
            let (owned, child_alive) = match states.get_mut(&e) {
                Some(s) => (s.owned, s.is_running()),
                None => (false, false),
            };
            let healthy = healthcheck_engine(&self.inner.http, e, &endpoint).await;
            out.push(EngineSnapshot {
                engine: e,
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

            {
                let mut states = self.inner.states.lock().await;
                for (&engine, state) in states.iter_mut() {
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

                    // Healthcheck the HTTP endpoint.
                    let healthy =
                        healthcheck_engine(&self.inner.http, engine, &state.endpoint).await;
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

                    // Idle check.
                    let idle = Instant::now().duration_since(state.last_activity);
                    if idle >= idle_timeout {
                        tracing::info!(
                            ?engine,
                            idle_secs = idle.as_secs(),
                            "runtime idle â€” shutting down"
                        );
                        to_shutdown.push(engine);
                    }
                }
            }

            // Shutdown outside the lock to avoid holding it during kill.
            for engine in to_shutdown {
                let _ = self.kill_owned(engine).await;
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(engine, ProviderStatus::Unknown)
                    .await;
            }
        }
    }

    /// Kill a spawned child process (owned only). Removes the entry from the
    /// state map.
    async fn kill_owned(&self, engine: ProviderEngine) {
        let mut states = self.inner.states.lock().await;
        if let Some(mut state) = states.remove(&engine) {
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
                    match tokio::time::timeout(
                        Duration::from_secs(5),
                        child.wait(),
                    )
                    .await
                    {
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

/// Default loopback endpoint per engine.
pub fn default_endpoint(e: ProviderEngine) -> &'static str {
    match e {
        ProviderEngine::Ollama => "http://127.0.0.1:11434",
        ProviderEngine::LlamaCpp => "http://127.0.0.1:8080",
        ProviderEngine::Lmstudio => "http://127.0.0.1:1234",
        ProviderEngine::Vllm => "http://127.0.0.1:8000",
        ProviderEngine::OpenAiCompat => "http://127.0.0.1:8000",
    }
}

/// Healthcheck an engine by hitting its HTTP endpoint.
/// Returns `true` if the runtime responds with a 2xx status.
pub async fn healthcheck_engine(
    client: &reqwest::Client,
    engine: ProviderEngine,
    endpoint: &str,
) -> bool {
    // Try the OpenAI-compatible /v1/models endpoint first (works for all
    // engines). Fall back to Ollama's native /api/version for Ollama.
    let url = format!("{endpoint}/v1/models");
    if client
        .get(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
    {
        return true;
    }
    if matches!(engine, ProviderEngine::Ollama) {
        let v = format!("{endpoint}/api/version");
        return client
            .get(&v)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
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
) -> Result<Child, SupervisorError> {
    let data_dir = locaryn_config::default_data_dir();

    // Load inference config — determines which args to pass.
    let inference_cfg_path = data_dir.join("inference_config.json");
    let inference_cfg: serde_json::Value = std::fs::read_to_string(&inference_cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({
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
        }));

    // Binary resolution: prefer the managed runtime (data_dir/bin/llama —
    // installed/updated by the app, pinned modern build), then the legacy
    // flat bin dir, then PATH.
    let exe_name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let bin_root = locaryn_config::bin_dir();
    let managed = bin_root.join("llama").join(exe_name);
    let legacy = bin_root.join(exe_name);
    let bin = if managed.exists() {
        managed
    } else if legacy.exists() {
        tracing::warn!(
            "using legacy llama-server from data_dir/bin — install the managed runtime \
             (Settings → System) for an up-to-date build"
        );
        legacy
    } else if let Some(p) = which("llama-server") {
        p
    } else {
        return Err(SupervisorError::BinaryNotFound(
            "llama-server (install it from Settings → System → Runtime)".into(),
        ));
    };

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
            format!("Model not found: {}", full_model_path.display()),
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
        // Jinja chat templating: required for OpenAI-style tool calling
        // (default-on in recent server builds; explicit is harmless).
        .arg("--jinja");

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
    if matches!(kv_type, "q8_0" | "q5_1" | "q5_0" | "q4_1" | "q4_0" | "iq4_nl") {
        cmd.arg("-ctk").arg(kv_type).arg("-ctv").arg(kv_type);
    }

    // Context length.
    let ctx = inference_cfg["context_length"].as_u64().unwrap_or(8192);
    cmd.arg("-c").arg(ctx.to_string());

    // Flash attention takes a value (on|off|auto). If the user disabled it, pass
    // off. Otherwise, when the KV cache is quantized we force it on, because a
    // quantized V cache requires Flash Attention in llama.cpp; else leave auto.
    let kv_is_quantized = matches!(kv_type, "q8_0" | "q5_1" | "q5_0" | "q4_1" | "q4_0" | "iq4_nl");
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

    // CPU threads.
    let threads = inference_cfg["cpu_threads"].as_u64().unwrap_or(0);
    if threads > 0 {
        cmd.arg("-t").arg(threads.to_string());
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

    cmd.spawn()
        .map_err(|e| SupervisorError::SpawnFailed(ProviderEngine::LlamaCpp, e.to_string()))
}

/// Find the multimodal projector that belongs to `model_path`, if any:
/// prefer an `mmproj-*` file sharing the model's stem prefix, else a single
/// unambiguous `mmproj-*.gguf` in the same directory.
fn find_mmproj_for(model_path: &std::path::Path) -> Option<std::path::PathBuf> {
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
