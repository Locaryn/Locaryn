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
pub mod npu;
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
    ///
    /// llama-server à froid, GPU pleinement déporté (`-ngl 999`) avec un
    /// projecteur de vision, compile ses pipelines Vulkan et relit le modèle
    /// depuis le disque avant de répondre — mesuré à plus de 60 s sur un
    /// portable, alors que 60 s suffit largement une fois le cache chaud.
    /// Verifie le 16/09/2026 contre un orphelin gemma-4-E2B + mmproj qui
    /// avait declenche ce timeout au premier demarrage.
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
            startup_timeout: Duration::from_secs(180),
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
    #[error("le processus du moteur {0:?} s'est arrêté prématurément : {1}")]
    ProcessExited(ProviderEngine, String),
    #[error("démarrage impossible ({0:?}) : {1}")]
    SpawnFailed(ProviderEngine, String),
    #[error("exécutable introuvable : {0}")]
    BinaryNotFound(String),
    #[error("moteur inconnu : {0} — l'extension qui l'apportait n'est plus installée ou active")]
    UnknownEngine(String),
    #[error("le chargement du moteur {0:?} a été interrompu : le modèle a changé entre-temps")]
    Interrompu(ProviderEngine),
    #[error("storage error: {0}")]
    Storage(#[from] locaryn_storage::StorageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// Runtime state (internal)
// ============================================================================

/// Le moteur llama.cpp lancé ici et qui tourne : son modèle, et s'il est sur
/// le NPU ou le processeur. Sert à savoir quoi alléger s'il tombe.
static EN_MARCHE: std::sync::Mutex<Option<(String, bool, bool)>> = std::sync::Mutex::new(None);

/// Plantages en pleine réponse depuis le lancement, par modèle.
static PLANTAGES: std::sync::Mutex<Vec<(String, u8)>> = std::sync::Mutex::new(Vec::new());

/// Combien de fois `modele` est tombé en pleine réponse.
fn plantages_de(modele: &str) -> u8 {
    PLANTAGES
        .lock()
        .map(|v| {
            v.iter()
                .find(|(m, _)| m == modele)
                .map(|(_, n)| *n)
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// Le micro-lot après `n` plantages : 512 → 128 → 32. Un micro-lot plus
/// petit donne au pilote des tâches plus courtes — c'est ce qui évite le délai
/// de Windows (TDR) sur une carte qui affiche aussi le bureau.
fn micro_lot_allege(ubatch: u64, plantages: u8) -> u64 {
    match plantages {
        0 => ubatch,
        1 => ubatch.min(128),
        _ => ubatch.min(32),
    }
}

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
    /// Vrai quand l'instance tournait déjà et a été adoptée sous supervision.
    adopted: bool,
    /// Depuis quand le délai est prolongé parce que le moteur dit travailler
    /// sans que personne de connu ne l'utilise (voir la boucle de santé).
    occupe_depuis: Option<Instant>,
}

impl EngineState {
    fn is_running(&mut self) -> bool {
        if self.adopted {
            return true;
        }
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
    spawn_lock: Mutex<()>,
    /// Les modèles qui ont fait tomber le moteur au démarrage avec le GPU, et
    /// ont tenu sans : leurs lancements suivants passent directement au
    /// processeur plutôt que de rejouer l'échec. Par modèle : un modèle trop
    /// gros pour la carte envoyait auparavant tous les suivants de la session
    /// sur le processeur, sans un octet de VRAM.
    gpu_failed: std::sync::Mutex<std::collections::HashSet<String>>,
    /// Moteurs apportés par les extensions installées, par identifiant.
    ///
    /// L'hôte remplit ce registre depuis le registre d'extensions et le
    /// remplace à chaque changement (installation, activation, retrait). Le
    /// superviseur ne lit jamais le disque des extensions : il ne connaît que
    /// ce qu'on lui a donné.
    extension_engines: Mutex<HashMap<String, ExtensionEngineSpec>>,
    /// Réponses demandées par une personne et pas encore finies. Tant qu'il y
    /// en a, le modèle sert : aucun compte à rebours, aucune éjection.
    occupe_utilisateur: std::sync::atomic::AtomicUsize,
    /// Travail d'arrière-plan en cours (tri de la mémoire pendant l'inactivité).
    /// Il ne compte pas comme une utilisation : le compte à rebours continue,
    /// et le modèle est éjecté à l'échéance même au milieu de ce travail.
    arriere_plan: std::sync::atomic::AtomicUsize,
}

/// Tant qu'il vit, le modèle sert quelqu'un (voir [`Supervisor::utilisation`]).
pub struct Utilisation {
    sup: Supervisor,
    arriere_plan: bool,
}

impl Drop for Utilisation {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        let compteur = if self.arriere_plan {
            &self.sup.inner.arriere_plan
        } else {
            &self.sup.inner.occupe_utilisateur
        };
        compteur.fetch_sub(1, Ordering::SeqCst);
        if !self.arriere_plan {
            // La fin d'une réponse est une utilisation : le délai repart d'ici.
            if let Ok(mut states) = self.sup.inner.states.try_lock() {
                for s in states.values_mut() {
                    s.last_activity = Instant::now();
                }
            }
        }
    }
}

impl Supervisor {
    /// Une personne attend une réponse : jusqu'à ce que le garde soit lâché,
    /// le modèle n'est pas inactif.
    pub fn utilisation(&self) -> Utilisation {
        self.inner
            .occupe_utilisateur
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Utilisation {
            sup: self.clone(),
            arriere_plan: false,
        }
    }

    /// Un travail d'arrière-plan se sert du modèle : il ne repousse pas
    /// l'éjection.
    pub fn arriere_plan(&self) -> Utilisation {
        self.inner
            .arriere_plan
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Utilisation {
            sup: self.clone(),
            arriere_plan: true,
        }
    }

    /// Une réponse demandée par une personne est-elle en cours ?
    pub fn utilisateur_occupe(&self) -> bool {
        self.inner
            .occupe_utilisateur
            .load(std::sync::atomic::Ordering::SeqCst)
            > 0
    }

    /// Un travail d'arrière-plan tourne-t-il ?
    pub fn arriere_plan_actif(&self) -> bool {
        self.inner
            .arriere_plan
            .load(std::sync::atomic::Ordering::SeqCst)
            > 0
    }

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
                spawn_lock: Mutex::new(()),
                gpu_failed: std::sync::Mutex::new(std::collections::HashSet::new()),
                extension_engines: Mutex::new(HashMap::new()),
                occupe_utilisateur: std::sync::atomic::AtomicUsize::new(0),
                arriere_plan: std::sync::atomic::AtomicUsize::new(0),
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
    async fn probe(&self, endpoint: &str, spec: Option<&ExtensionEngineSpec>) -> bool {
        match spec {
            Some(spec) => probe_url(&self.inner.http, &spec.health_url()).await,
            None => healthcheck_engine(&self.inner.http, endpoint).await,
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
        // Un changement de modèle arrête le moteur en plein chargement : ce
        // chargement-là est abandonné, et on recommence avec le modèle choisi.
        for _ in 0..3 {
            match self.ensure_running_une_fois(engine).await {
                Err(SupervisorError::Interrompu(_)) => continue,
                autre => return autre,
            }
        }
        Err(SupervisorError::Interrompu(engine.clone()))
    }

    async fn ensure_running_une_fois(
        &self,
        engine: &ProviderEngine,
    ) -> Result<String, SupervisorError> {
        let _spawn_guard = self.inner.spawn_lock.lock().await;
        let spec = self.extension_engine_spec(engine).await;
        let endpoint = match &spec {
            Some(s) => s.endpoint(),
            None => default_endpoint(engine)
                .ok_or_else(|| SupervisorError::UnknownEngine(engine.as_token()))?
                .to_string(),
        };

        let p = self.inner.storage.providers.active().await.unwrap_or(None);
        let active_model = p.and_then(|p| p.model);

        // Un llama-server déjà là mais qui charge encore son modèle (gros
        // modèle, disque lent, compilation des shaders Vulkan) : l'attendre.
        // Le prendre pour absent faisait lancer un second moteur, qui
        // échouait sur le port tenu par le premier — avec la VRAM déjà
        // occupée et le message « port déjà utilisé ».
        if spec.is_none() && matches!(engine, ProviderEngine::LlamaCpp) {
            let deadline = Instant::now() + self.inner.cfg.startup_timeout;
            while llama_port_state(&self.inner.http, &endpoint).await == LlamaPortState::Loading {
                if Instant::now() >= deadline {
                    break;
                }
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(engine, ProviderStatus::Starting)
                    .await;
                tracing::info!(%endpoint, "llama-server en cours de chargement — attente");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        // Fast path: already healthy and serving the expected model?
        let is_healthy = self.probe(&endpoint, spec.as_ref()).await;
        if is_healthy {
            let model_matches = if matches!(engine, ProviderEngine::LlamaCpp) {
                let same_model = is_llama_server_model_match(
                    &self.inner.http,
                    &endpoint,
                    active_model.as_deref(),
                )
                .await;
                let adoptable = self.llama_adoptable(engine, &endpoint).await;

                // Vérifier si la configuration d'inférence (fenêtre de contexte, etc.) a changé
                let data_dir = locaryn_config::default_data_dir();
                let config_matches =
                    match std::fs::read_to_string(data_dir.join("active_llama_settings.json")) {
                        Ok(raw) => {
                            let active: serde_json::Value =
                                serde_json::from_str(&raw).unwrap_or_default();
                            let wanted_cfg =
                                std::fs::read_to_string(data_dir.join("inference_config.json"))
                                    .ok()
                                    .and_then(|s| {
                                        serde_json::from_str::<serde_json::Value>(&s).ok()
                                    })
                                    .unwrap_or_default();
                            let wanted_ctx = wanted_cfg["context_length"].as_u64().unwrap_or(0);
                            let active_ctx = active["ctx"].as_u64().unwrap_or(0);
                            // Si l'utilisateur a changé la fenêtre de contexte, redémarrer pour l'appliquer
                            wanted_ctx == active_ctx
                        }
                        // Aucun réglage actif enregistré : force le redémarrage pour appliquer les paramètres
                        Err(_) => false,
                    };

                same_model && adoptable && config_matches
            } else {
                true
            };

            if model_matches {
                // Enregistrer le moteur dans les états du superviseur pour qu'il bénéficie
                // de l'idle-shutdown (extinction automatique après inactivité) et d'un arrêt propre.
                let mut states = self.inner.states.lock().await;
                states.entry(engine.clone()).or_insert_with(|| EngineState {
                    endpoint: endpoint.clone(),
                    child: None,
                    last_activity: Instant::now(),
                    pinned: false,
                    owned: true,
                    adopted: true,
                    occupe_depuis: None,
                });
                drop(states);

                // Mark healthy in storage and return.
                let _ = self
                    .inner
                    .storage
                    .providers
                    .set_status_by_engine(engine, ProviderStatus::Healthy)
                    .await;
                return Ok(endpoint);
            } else {
                tracing::info!(?engine, %endpoint, "llama-server needs restart (model or config changed) — restarting with active parameters");
                self.kill_owned(engine).await;
                kill_stray_llama_servers().await;
                if let Some(port) = endpoint_port(&endpoint) {
                    let _ = wait_port_free(port, Duration::from_secs(10)).await;
                }
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

        // Notre moteur est là mais son processus s'est arrêté sans qu'on
        // l'arrête : il est tombé (un arrêt voulu retire son état).
        if matches!(engine, ProviderEngine::LlamaCpp) {
            let tombe = {
                let mut states = self.inner.states.lock().await;
                match states.get_mut(engine) {
                    Some(s) if s.owned && !s.adopted && s.child.is_some() => {
                        let mort = !s.is_running();
                        if mort {
                            s.child = None;
                        }
                        mort
                    }
                    _ => false,
                }
            };
            if tombe {
                self.noter_plantage();
            }
        }

        tracing::info!(%endpoint, ?engine, "engine not running — auto-spawning runtime");

        // Set status to Starting in storage.
        let _ = self
            .inner
            .storage
            .providers
            .set_status_by_engine(engine, ProviderStatus::Starting)
            .await;

        // Le port est encore tenu par un llama-server qui ne répond pas — un
        // orphelin d'une session précédente qui a planté, figé en chargement.
        // L'attendre ne suffit plus : on l'arrête, comme pour un moteur qui
        // sert un autre modèle, au lieu d'échouer sur « port occupé » à
        // chaque message.
        if spec.is_none() && matches!(engine, ProviderEngine::LlamaCpp) {
            if let Some(port) = endpoint_port(&endpoint) {
                if ensure_port_free(port).is_err()
                    && matches!(
                        llama_port_state(&self.inner.http, &endpoint).await,
                        LlamaPortState::Loading | LlamaPortState::Ready
                    )
                {
                    tracing::warn!(%endpoint, "llama-server orphelin sur le port — arrêt avant relance");
                    self.kill_owned(engine).await;
                    kill_stray_llama_servers().await;
                    let _ = wait_port_free(port, Duration::from_secs(10)).await;
                }
            }
        }

        // Premier essai avec le GPU, sauf s'il a déjà fait tomber le moteur
        // dans cette session ; second essai sur le processeur si le moteur
        // meurt au démarrage. Un pilote OpenCL/Vulkan défaillant ne doit pas
        // rendre la conversation impossible.
        let cle_gpu = active_model.clone().unwrap_or_default();
        let mut cpu_only = self
            .inner
            .gpu_failed
            .lock()
            .is_ok_and(|m| m.contains(&cle_gpu));
        // Rechargements pour tenir la marge de prudence (deux au plus).
        let mut corrections_de_marge = 0u8;
        // Snapdragon : le NPU d'abord, tant qu'il n'a pas échoué.
        let mut sur_npu = spec.is_none()
            && matches!(engine, ProviderEngine::LlamaCpp)
            && !cpu_only
            && npu::a_essayer();
        'attempt: loop {
            // Spawn the runtime process: llama-server, the AirLLM Python server,
            // or le serveur décrit par une extension.
            let child = match (&spec, engine) {
                (Some(spec), _) => extension_engine::spawn(spec, active_model.as_deref()).await?,
                (None, ProviderEngine::AirLlm) => {
                    spawn_airllm_server(&self.inner.cfg, active_model.as_deref()).await?
                }
                (None, _) => {
                    match spawn_llama_server(
                        &self.inner.cfg,
                        active_model.as_deref(),
                        &self.inner.http,
                        cpu_only,
                        sur_npu,
                    )
                    .await
                    {
                        Ok(c) => c,
                        // Le moteur NPU ne se télécharge pas (hors ligne, release
                        // absente) : le moteur habituel prend le relais.
                        Err(e) if sur_npu => {
                            npu::noter_echec(&format!("moteur NPU indisponible : {e}"));
                            sur_npu = false;
                            continue 'attempt;
                        }
                        Err(e) => return Err(e),
                    }
                }
            };

            let pid_lance = child.id();
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
                        adopted: false,
                        occupe_depuis: None,
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
            // Au-delà du délai, un llama-server vivant qui répond « loading »
            // charge encore — disque lent, pilote Vulkan qui compile ses
            // shaders, antivirus qui lit les poids. Le tuer à 180 s faisait
            // échouer le chargement au hasard et le relancer de zéro : sur une
            // RX 570, cinq à dix minutes pour un modèle qui finissait par
            // tenir. On le laisse finir, dans cette limite.
            let limite_absolue = Instant::now() + startup_budget.max(Duration::from_secs(20 * 60));
            let mut prolonge = false;
            loop {
                if self.probe(&endpoint, spec.as_ref()).await {
                    let _ = self
                        .inner
                        .storage
                        .providers
                        .set_status_by_engine(engine, ProviderStatus::Healthy)
                        .await;
                    // Sur le NPU, llama.cpp démarre aussi quand le backend
                    // Hexagon n'a pas pu se charger — tout part alors sur le
                    // processeur. Le journal le dit ; on repasse à l'Adreno.
                    if sur_npu {
                        let journal = std::fs::read_to_string(
                            locaryn_config::default_data_dir().join("llama-server.log"),
                        )
                        .unwrap_or_default();
                        if !npu::journal_montre_le_npu(&journal) {
                            let detail = journal
                                .lines()
                                .rfind(|l| l.contains("ggml-hex") || l.contains("HTP"))
                                .unwrap_or("aucune session Hexagon ouverte")
                                .trim()
                                .to_string();
                            npu::noter_echec(&format!("le NPU n'a pas pris le modèle ({detail})"));
                            npu::oublier_actif();
                            self.kill_owned(engine).await;
                            sur_npu = false;
                            continue 'attempt;
                        }
                        let taille = active_model
                            .as_deref()
                            .map(|m| locaryn_config::models_dir().join(m))
                            .and_then(|p| std::fs::metadata(p).ok())
                            .map(|m| m.len())
                            .unwrap_or(0);
                        npu::noter_actif(&npu::constater(
                            &journal,
                            npu::sessions_pour(taille),
                            active_model.as_deref().unwrap_or_default(),
                        ));
                    } else if spec.is_none() && matches!(engine, ProviderEngine::LlamaCpp) {
                        npu::oublier_actif();
                    }
                    tracing::info!(%endpoint, moteur = %engine.as_token(), cpu_only, sur_npu, "moteur en marche");
                    // La prudence choisie, vérifiée sur la VRAM réellement
                    // libre : sinon le contexte est réduit et le modèle
                    // rechargé avec lui.
                    if spec.is_none()
                        && matches!(engine, ProviderEngine::LlamaCpp)
                        && !cpu_only
                        && !sur_npu
                        && corrections_de_marge < 2
                    {
                        let n_ctx = llama_n_ctx(&self.inner.http, &endpoint).await;
                        if let Some((_, memo)) = n_ctx.and_then(contexte_pour_la_marge) {
                            corrections_de_marge += 1;
                            let libre_avant = memo.libre_gb;
                            if let Ok(texte) = serde_json::to_string(&memo) {
                                let _ = std::fs::write(
                                    locaryn_config::default_data_dir().join(FIT_MEMORISE),
                                    texte,
                                );
                            }
                            FIT_EN_COURS
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .take();
                            self.kill_owned(engine).await;
                            attendre_vram_rendue(libre_avant).await;
                            continue 'attempt;
                        }
                    }
                    if spec.is_none() && matches!(engine, ProviderEngine::LlamaCpp) && !cpu_only {
                        enregistrer_fit(&self.inner.http, &endpoint).await;
                    }
                    if cpu_only {
                        if let Ok(mut m) = self.inner.gpu_failed.lock() {
                            m.insert(cle_gpu.clone());
                        }
                    }
                    if spec.is_none() && matches!(engine, ProviderEngine::LlamaCpp) {
                        *EN_MARCHE.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some((cle_gpu.clone(), sur_npu, cpu_only));
                    }
                    return Ok(endpoint);
                }

                // Le moteur lancé ici a été arrêté — changement de modèle —
                // ou remplacé. L'attendre jusqu'au délai gardait le verrou de
                // lancement jusqu'à trois minutes : aucun autre modèle ne
                // pouvait se charger, ni depuis la conversation ni depuis le
                // menu des modèles.
                let toujours_le_notre = {
                    let states = self.inner.states.lock().await;
                    states
                        .get(engine)
                        .and_then(|s| s.child.as_ref())
                        .is_some_and(|c| c.id() == pid_lance || c.id().is_none())
                };
                if !toujours_le_notre {
                    tracing::info!(
                        ?engine,
                        "chargement abandonné : le moteur a été arrêté entre-temps"
                    );
                    return Err(SupervisorError::Interrompu(engine.clone()));
                }

                // Détecter un arrêt immédiat du processus (ex: modèle corrompu ou arguments invalides)
                {
                    let mut states = self.inner.states.lock().await;
                    if let Some(state) = states.get_mut(engine) {
                        if let Some(ref mut child) = state.child {
                            match child.try_wait() {
                                Ok(Some(status)) => {
                                    tracing::warn!(?engine, %status, "le processus du moteur s'est arrêté prématurément");
                                    drop(states);
                                    self.kill_owned(engine).await;
                                    let _ = self
                                        .inner
                                        .storage
                                        .providers
                                        .set_status_by_engine(engine, ProviderStatus::Unhealthy)
                                        .await;

                                    // Tenter d'extraire la cause exacte depuis le fichier journal
                                    let log_detail = if matches!(engine, ProviderEngine::LlamaCpp) {
                                        let log_path = locaryn_config::default_data_dir()
                                            .join("llama-server.log");
                                        std::fs::read_to_string(&log_path).ok().and_then(
                                            |content| {
                                                content
                                                    .lines()
                                                    .rev()
                                                    .find(|l| {
                                                        l.contains("error")
                                                            || l.contains("failed")
                                                            || l.contains("ERROR")
                                                    })
                                                    .map(|l| l.trim().to_string())
                                            },
                                        )
                                    } else {
                                        None
                                    };

                                    let msg = match log_detail {
                                        Some(detail) => format!("{status} ({detail})"),
                                        None => status.to_string(),
                                    };

                                    // Le NPU a refusé (signature non approuvée,
                                    // pilote absent) : le moteur Adreno reprend.
                                    if sur_npu {
                                        npu::noter_echec(&msg);
                                        sur_npu = false;
                                        continue 'attempt;
                                    }
                                    // Un contexte mémorisé qui ne tient plus (la VRAM
                                    // a changé de main) : on l'oublie et on retente
                                    // sur le GPU, en laissant `--fit` choisir.
                                    if spec.is_none()
                                        && matches!(engine, ProviderEngine::LlamaCpp)
                                        && !cpu_only
                                        && oublier_fit()
                                    {
                                        tracing::warn!(cause = %msg, "contexte mémorisé refusé — nouvel essai avec --fit");
                                        continue 'attempt;
                                    }
                                    if spec.is_none()
                                        && matches!(engine, ProviderEngine::LlamaCpp)
                                        && !cpu_only
                                    {
                                        tracing::warn!(cause = %msg, "llama-server est tombé au démarrage avec le GPU — nouvel essai sur le processeur");
                                        cpu_only = true;
                                        continue 'attempt;
                                    }

                                    return Err(SupervisorError::ProcessExited(
                                        engine.clone(),
                                        msg,
                                    ));
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    tracing::debug!(?engine, error = %e, "impossible de vérifier l'état du processus");
                                }
                            }
                        }
                    }
                }

                if Instant::now() >= deadline
                    && Instant::now() < limite_absolue
                    && spec.is_none()
                    && matches!(engine, ProviderEngine::LlamaCpp)
                    && llama_port_state(&self.inner.http, &endpoint).await
                        == LlamaPortState::Loading
                {
                    if !prolonge {
                        prolonge = true;
                        tracing::warn!(
                            ?startup_budget,
                            "llama-server charge encore au-delà du délai — on le laisse finir"
                        );
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
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
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    /// Un llama-server qui répond sur le port peut-il servir tel quel ?
    ///
    /// Oui s'il est le nôtre (lancé par ce superviseur), ou s'il annonce la
    /// build épinglée. Sinon c'est un moteur étranger ou ancien — un orphelin
    /// d'une version précédente, parfois sans GPU : l'adopter faisait tourner
    /// le modèle sur le processeur, sans un octet de VRAM, pendant que
    /// l'application annonçait « chargement du modèle en mémoire ».
    async fn llama_adoptable(&self, engine: &ProviderEngine, endpoint: &str) -> bool {
        let ours = {
            let mut states = self.inner.states.lock().await;
            states
                .get_mut(engine)
                .is_some_and(|s| s.owned && s.is_running())
        };
        if ours {
            return true;
        }
        match llama_build_info(&self.inner.http, endpoint).await {
            Some(build) if build_is_pinned(&build) => true,
            other => {
                tracing::warn!(%endpoint, build = ?other, attendu = LLAMA_BUILD, "llama-server étranger ou ancien sur le port — remplacé par le moteur géré");
                false
            }
        }
    }

    /// Record that the agent used this engine (resets the idle timer).
    pub async fn note_activity(&self, engine: &ProviderEngine) {
        let mut states = self.inner.states.lock().await;
        if let Some(s) = states.get_mut(engine) {
            s.last_activity = Instant::now();
            s.occupe_depuis = None;
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
        self.probe(&endpoint, spec.as_ref()).await
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

    /// Prendre sous supervision un moteur qui tourne déjà sans être inscrit
    /// (resté vivant après un redémarrage de l'application, lancé par le
    /// service). Sans cela, l'inactivité n'était jamais comptée : la barre
    /// affichait « éjection dans 30 min » sans bouger, et le modèle n'était
    /// jamais libéré. Le délai part de maintenant.
    pub async fn adopter_si_absent(&self, engine: &ProviderEngine, endpoint: &str) {
        let mut states = self.inner.states.lock().await;
        states.entry(engine.clone()).or_insert_with(|| {
            tracing::info!(?engine, %endpoint, "moteur déjà en marche adopté pour le délai d'inactivité");
            EngineState {
                endpoint: endpoint.to_string(),
                child: None,
                last_activity: Instant::now(),
                pinned: false,
                owned: true,
                adopted: true,
                occupe_depuis: None,
            }
        });
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
        if matches!(engine, ProviderEngine::LlamaCpp) {
            kill_stray_llama_servers().await;
            if let Some(endpoint) = default_endpoint(engine) {
                if let Some(port) = endpoint_port(endpoint) {
                    let _ = wait_port_free(port, Duration::from_secs(5)).await;
                }
            }
        }
        let _ = self
            .inner
            .storage
            .providers
            .set_status_by_engine(engine, ProviderStatus::Unknown)
            .await;
        Ok(())
    }

    /// Arrêter tous les moteurs supervisés et tuer les llama-server orphelins.
    ///
    /// Appelé à la fermeture de l'application pour libérer la VRAM avant
    /// que le processus ne disparaisse. Non bloquant sur les erreurs :
    /// l'application quitte dans tous les cas.
    pub async fn kill_all_engines(&self) {
        let engines: Vec<ProviderEngine> = {
            let states = self.inner.states.lock().await;
            states.keys().cloned().collect()
        };
        for engine in &engines {
            self.kill_owned(engine).await;
            let _ = self
                .inner
                .storage
                .providers
                .set_status_by_engine(engine, ProviderStatus::Unknown)
                .await;
        }
        kill_stray_llama_servers().await;
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
            let healthy = self.probe(&endpoint, spec.as_ref()).await;
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
                        if matches!(engine, ProviderEngine::LlamaCpp) {
                            self.noter_plantage();
                        }
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
                        None => healthcheck_engine(&self.inner.http, &state.endpoint).await,
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
                    // Une réponse demandée par quelqu'un n'est jamais coupée.
                    if self.utilisateur_occupe() {
                        if idle >= idle_timeout {
                            tracing::info!(
                                ?engine,
                                "délai d'inactivité repoussé : une réponse est en cours"
                            );
                        }
                        state.last_activity = Instant::now();
                        state.occupe_depuis = None;
                        continue;
                    }
                    // Un moteur en pleine génération n'est pas inactif, même
                    // si la requête a commencé il y a plus que le délai : une
                    // longue tâche d'agent (lancée par le service, sans garde)
                    // était coupée en plein travail. Le travail d'arrière-plan
                    // de l'application, lui, ne retient pas le modèle : à
                    // l'échéance, tant pis s'il n'a pas fini.
                    // Prolongé une fois au plus : une requête restée pendue (client
                    // parti, génération qui ne finit pas) gardait le modèle en
                    // mémoire pour toujours, le compte à rebours repartant à
                    // 30 min à chaque échéance.
                    let deja_prolonge = state
                        .occupe_depuis
                        .is_some_and(|d| Instant::now().duration_since(d) >= idle_timeout);
                    if idle >= idle_timeout
                        && !deja_prolonge
                        && !self.arriere_plan_actif()
                        && !extension_specs.contains_key(engine.as_token().as_str())
                        && llama_occupe(&self.inner.http, &state.endpoint).await
                    {
                        tracing::info!(?engine, "délai d'inactivité repoussé : le moteur génère encore (requête hors de l'application)");
                        state.occupe_depuis.get_or_insert_with(Instant::now);
                        state.last_activity = Instant::now();
                        continue;
                    }
                    if deja_prolonge && idle >= idle_timeout {
                        tracing::warn!(
                            ?engine,
                            "moteur toujours occupé après une prolongation : éjection quand même"
                        );
                    }
                    if idle >= idle_timeout {
                        tracing::info!(
                            ?engine,
                            idle_secs = idle.as_secs(),
                            "runtime idle — shutting down"
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
        kill_stray_llama_servers().await;
        if matches!(engine, ProviderEngine::LlamaCpp) {
            let _ = wait_port_free(llama_port(), Duration::from_secs(10)).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }

    /// Kill a spawned child process (owned only). Removes the entry from the
    /// state map.
    /// Le moteur est tombé en pleine réponse (pilote réinitialisé par Windows,
    /// mémoire, NPU). Le prochain lancement s'allège : NPU → Adreno ; sinon un
    /// micro-lot plus petit, puis le processeur au troisième plantage.
    fn noter_plantage(&self) {
        let Some((modele, npu, cpu)) = EN_MARCHE.lock().ok().and_then(|mut e| e.take()) else {
            return;
        };
        if npu {
            npu::noter_echec("le moteur NPU s'est arrêté en pleine réponse");
            return;
        }
        let n = {
            let mut v = PLANTAGES.lock().unwrap_or_else(|e| e.into_inner());
            match v.iter_mut().find(|(m, _)| *m == modele) {
                Some((_, n)) => {
                    *n = n.saturating_add(1);
                    *n
                }
                None => {
                    v.push((modele.clone(), 1));
                    1
                }
            }
        };
        if n >= 3 && !cpu {
            if let Ok(mut m) = self.inner.gpu_failed.lock() {
                m.insert(modele.clone());
            }
            tracing::warn!(
                modele,
                plantages = n,
                "le moteur tombe encore avec le GPU — prochain lancement sur le processeur"
            );
        } else {
            tracing::warn!(
                modele,
                plantages = n,
                "moteur tombé en pleine réponse — prochain lancement avec un micro-lot réduit"
            );
        }
    }

    pub async fn kill_owned(&self, engine: &ProviderEngine) {
        let mut states = self.inner.states.lock().await;
        if let Some(mut state) = states.remove(engine) {
            if state.owned {
                if state.adopted && matches!(engine, ProviderEngine::LlamaCpp) {
                    kill_stray_llama_servers().await;
                }
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

/// Port de `llama-server`, choisi une fois par processus.
///
/// 8080 est aussi le port de la moitié des services qu'on trouve sur une
/// machine. Imposé, il suffisait qu'un autre programme l'occupe pour que le
/// moteur meure au démarrage (« couldn't bind HTTP server socket ») : le
/// modèle ne chargeait jamais, la VRAM ne bougeait pas, et l'application
/// attendait un chargement qui n'aurait pas lieu. Le port est donc le premier
/// de 8080 à 8099 qui soit libre — ou déjà tenu par un llama-server, celui de
/// l'application ou de son service —, à défaut un port donné par le système.
/// `LOCARYN_LLAMA_PORT` l'impose toujours.
pub fn llama_port() -> u16 {
    static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *PORT.get_or_init(|| {
        if let Some(p) = std::env::var("LOCARYN_LLAMA_PORT")
            .ok()
            .and_then(|v| v.trim().parse::<u16>().ok())
            .filter(|p| *p != 0)
        {
            return p;
        }
        let choisi = choisir_port_llama(8080..=8099);
        if choisi != 8080 {
            tracing::warn!(
                port = choisi,
                "port 8080 occupé par un autre programme — llama-server écoutera ailleurs"
            );
        }
        choisi
    })
}

/// Le premier port de `candidats` utilisable par llama-server.
fn choisir_port_llama(candidats: std::ops::RangeInclusive<u16>) -> u16 {
    for p in candidats {
        if port_libre(p) || llama_ecoute(p) {
            return p;
        }
    }
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(8080)
}

/// Personne n'écoute sur ce port, et llama-server pourra l'ouvrir.
///
/// Le seul `bind` sur 127.0.0.1 ne suffit pas : sous Windows, il réussit
/// quand un autre programme écoute sur toutes les adresses (0.0.0.0), alors
/// que llama-server, qui demande le port en exclusivité, sera refusé. Une
/// connexion qui aboutit dit qu'il est pris, quelle que soit l'adresse
/// d'écoute — sans rien ouvrir vers l'extérieur.
fn port_libre(port: u16) -> bool {
    let adresse = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    if std::net::TcpStream::connect_timeout(&adresse, Duration::from_millis(200)).is_ok() {
        return false;
    }
    std::net::TcpListener::bind(adresse).is_ok()
}

/// Un llama-server répond-il sur ce port ? (`/props` porte `build_info`.)
/// Lecture HTTP minimale et bloquante : elle sert au choix du port, avant
/// tout runtime asynchrone.
fn llama_ecoute(port: u16) -> bool {
    use std::io::{Read as _, Write as _};
    let adresse = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut flux) = std::net::TcpStream::connect_timeout(&adresse, Duration::from_millis(200))
    else {
        return false;
    };
    let _ = flux.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = flux.set_write_timeout(Some(Duration::from_millis(800)));
    if flux
        .write_all(b"GET /props HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut corps = Vec::new();
    let _ = flux.take(256 * 1024).read_to_end(&mut corps);
    String::from_utf8_lossy(&corps).contains("\"build_info\"")
}

/// L'adresse locale de llama-server enregistrée ailleurs (base, réglages)
/// doit suivre le port choisi. `Some(nouvelle)` quand `actuelle` désigne le
/// moteur local sur un autre port ; une adresse vers une autre machine n'est
/// jamais touchée.
pub fn realigner_endpoint_llama(actuelle: &str) -> Option<String> {
    let reste = actuelle.trim_end_matches('/').strip_prefix("http://")?;
    let (hote, port) = reste.rsplit_once(':')?;
    if !matches!(hote, "127.0.0.1" | "localhost") {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    let voulu = llama_port();
    (port != voulu).then(|| format!("http://127.0.0.1:{voulu}"))
}

fn llama_endpoint() -> &'static str {
    static ENDPOINT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ENDPOINT.get_or_init(|| format!("http://127.0.0.1:{}", llama_port()))
}

/// Adresse loopback par défaut d'un moteur intégré.
///
/// `None` pour un moteur apporté par une extension : son adresse est celle de
/// son manifeste, et l'inventer ici ferait sonder un port au hasard.
/// Utilisez [`Supervisor::endpoint_for`] pour couvrir les deux cas.
pub fn default_endpoint(e: &ProviderEngine) -> Option<&'static str> {
    match e {
        ProviderEngine::LlamaCpp => Some(llama_endpoint()),
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
pub async fn healthcheck_engine(client: &reqwest::Client, endpoint: &str) -> bool {
    // Tous les moteurs servis ici parlent le dialecte OpenAI : `/v1/models`
    // suffit, et c'est la seule sonde a maintenir. Le repli sur
    // `/api/version`, propre a Ollama, est parti avec lui.
    probe_url(client, &format!("{endpoint}/v1/models")).await
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

/// Les périphériques de calcul que llama-server voit réellement, d'après
/// `--list-devices` : `["Vulkan0: AMD Radeon RX 570 Series (8192 MiB, 7900 MiB free)"]`,
/// ou une liste vide quand il ne voit que le processeur.
///
/// C'est la seule réponse qui compte quand une carte « n'est pas utilisée » :
/// la détection de l'application peut voir la carte, si le pilote Vulkan (ou
/// OpenCL sur Snapdragon) ne l'expose pas au moteur, tout part sur le
/// processeur. `None` quand le moteur n'a pas pu répondre.
pub async fn llama_devices(bin: &std::path::Path) -> Option<Vec<String>> {
    let mut cmd = Command::new(bin);
    cmd.arg("--list-devices")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = bin.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .ok()?
        .ok()?;
    let mut texte = String::from_utf8_lossy(&out.stdout).into_owned();
    texte.push_str(&String::from_utf8_lossy(&out.stderr));
    parse_list_devices(&texte)
}

/// Lit la sortie de `llama-server --list-devices` (b11003) :
///
/// ```text
/// Available devices:
///   Vulkan0: AMD Radeon RX 570 Series (8192 MiB, 7900 MiB free)
/// ```
///
/// `(none)` quand seul le processeur est disponible.
fn parse_list_devices(texte: &str) -> Option<Vec<String>> {
    let debut = texte.find("Available devices:")?;
    Some(
        texte[debut..]
            .lines()
            .skip(1)
            .take_while(|l| l.starts_with(' ') || l.starts_with('\t'))
            .map(str::trim)
            .filter(|l| !l.is_empty() && *l != "(none)" && l.contains(':'))
            .map(str::to_string)
            .collect(),
    )
}

/// La version de llama.cpp que l'application connaît — le pin unique du
/// runtime géré (l'écran de réglages l'affiche, l'installateur le télécharge).
pub const LLAMA_BUILD: &str = "b11003";

/// Installer le moteur d'inférence si la machine ne l'a pas encore.
///
/// Un moteur n'est pas un choix laissé à l'utilisateur : sans lui, aucun
/// modèle ne peut répondre, et lui demander d'aller le chercher dans un écran
/// de réglages revient à livrer une application qui ne marche pas au premier
/// lancement. Il arrive donc avec le reste, au moment où il sert.
pub async fn provision_llama_server(_http: &reqwest::Client) -> Result<PathBuf, SupervisorError> {
    // Le moteur géré, s'il n'est pas celui qui convient à cette machine
    // (build CPU posée sur un Snapdragon, ancienne build d'avant le pin), est
    // mis à niveau. Sans réseau, l'installation existante reste en service.
    let managed = runtime_install::managed_llama_path();
    if managed.is_file() {
        if runtime_install::installed_variant_is_current() {
            return Ok(managed);
        }
        return runtime_install::install_llama_runtime(None).await;
    }
    if let Some(existant) = llama_server_path() {
        return Ok(existant);
    }
    // Un seul chemin d'installation : celui du bouton des réglages. Le second,
    // ici, ne savait extraire que du zip — sous Linux, où llama.cpp publie des
    // `.tar.gz`, l'installation au premier lancement échouait.
    runtime_install::install_llama_runtime(None).await
}

/// Ollama's OpenAI-compatible endpoint ignores `options` silently — the
/// context only follows `num_ctx` on its native `/api/chat` (measured on
/// 0.33.x: a 10k-token prompt is cut at 2048 through /v1, processed in full
/// through /api/chat). Callers that send `num_ctx` must know where to send
/// it, or the setting is decorative.
pub fn ollama_options_hint() -> serde_json::Value {
    serde_json::json!({ "endpoint": "/api/chat", "body_key": "options.num_ctx" })
}

/// Refuse de lancer le moteur si un autre programme tient déjà le port
/// loopback : `TcpListener::bind` réussit seulement si personne n'écoute
/// dessus, et se relâche aussitôt (le `Drop` du listener) pour laisser la
/// place au vrai serveur. Message nommé, plutôt qu'un `bind()` raté côté
/// llama-server dont la sortie peut ne jamais atteindre le disque.
fn ensure_port_free(port: u16) -> Result<(), String> {
    // Une connexion qui aboutit : quelqu'un écoute, peut-être sur toutes les
    // adresses — cas que le seul `bind` sur 127.0.0.1 laissait passer.
    let adresse = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    if std::net::TcpStream::connect_timeout(&adresse, Duration::from_millis(200)).is_ok() {
        return Err(port_error_message(
            port,
            &std::io::Error::from(std::io::ErrorKind::AddrInUse),
        ));
    }
    std::net::TcpListener::bind(adresse)
        .map(|_| ())
        .map_err(|e| port_error_message(port, &e))
}

/// Le message d'un port inutilisable, selon ce que le système a répondu.
///
/// Sous Windows, `llama-server` ouvre son port en `SO_EXCLUSIVEADDRUSE` : un
/// second `bind` y reçoit `WSAEACCES` (10013, « accès interdit ») et non
/// « adresse utilisée ». L'ancien message affirmait alors qu'un *autre
/// programme* tenait le port — c'était en réalité, presque toujours, un
/// llama-server encore en vie (souvent celui qui occupait la VRAM). Le même
/// code sort aussi quand Windows a réservé le port (plages exclues de
/// Hyper-V/WSL).
fn port_error_message(port: u16, e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => format!(
            "le port {port} est tenu en exclusivité par un autre processus — le plus souvent un \
             llama-server resté ouvert (vérifiez le Gestionnaire des tâches) — ou réservé par \
             Windows (`netsh interface ipv4 show excludedportrange protocol=tcp`). Fermez ce \
             processus, ou choisissez un autre port avec la variable LOCARYN_LLAMA_PORT ({e})"
        ),
        _ => format!(
            "port {port} déjà utilisé par un autre programme sur cette machine ({e}) — fermez-le \
             ou libérez le port avant de relancer le moteur"
        ),
    }
}

/// Arrêter tout `llama-server` de la machine : celui d'une session
/// précédente, orphelin, n'appartient à aucun superviseur vivant et garde
/// pourtant le port et la VRAM.
pub async fn kill_stray_llama_servers() {
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW : sans ce drapeau, chaque redémarrage faisait
        // clignoter une console taskkill par-dessus l'application.
        let _ = tokio::process::Command::new("taskkill")
            .args(["/F", "/IM", "llama-server.exe"])
            .creation_flags(0x0800_0000)
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
}

/// Attendre que le port se libère : un moteur qu'on vient d'arrêter le garde
/// encore quelques instants (Windows libère le socket après la fin du
/// processus). Relancer aussitôt échouait sur « port occupé ».
async fn wait_port_free(port: u16, budget: Duration) -> Result<(), String> {
    let deadline = Instant::now() + budget;
    loop {
        match ensure_port_free(port) {
            Ok(()) => return Ok(()),
            Err(e) if Instant::now() >= deadline => return Err(e),
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
}

/// Ce qui répond sur le port de llama-server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlamaPortState {
    /// Un llama-server prêt à répondre.
    Ready,
    /// Un llama-server vivant qui charge encore son modèle : `/health` répond
    /// 503. Le modèle est déjà (en partie) en VRAM.
    Loading,
    /// Quelque chose répond, qui n'est pas un llama-server reconnaissable.
    Other,
    /// Personne n'écoute.
    Absent,
}

/// Sonde `/health`, la seule route que llama-server sert pendant le
/// chargement. `/v1/models` répond 503 tant que le modèle n'est pas prêt :
/// s'y fier faisait prendre un moteur en plein chargement pour un moteur
/// absent, en relancer un second… qui se heurtait au port du premier.
pub async fn llama_port_state(http: &reqwest::Client, endpoint: &str) -> LlamaPortState {
    let resp = http
        .get(format!("{endpoint}/health"))
        .timeout(Duration::from_secs(5))
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) if e.is_connect() => return LlamaPortState::Absent,
        // Un serveur qui ne répond pas dans les temps est occupé (chargement
        // d'un gros modèle depuis un disque lent) : c'est un moteur vivant.
        Err(e) if e.is_timeout() => return LlamaPortState::Loading,
        Err(_) => return LlamaPortState::Other,
    };
    let status = resp.status();
    if status.is_success() {
        return LlamaPortState::Ready;
    }
    let body = resp.text().await.unwrap_or_default().to_ascii_lowercase();
    if status.as_u16() == 503 && body.contains("loading") {
        LlamaPortState::Loading
    } else {
        LlamaPortState::Other
    }
}

/// Un llama-server est-il en train de traiter une requête ? (`/slots` :
/// `is_processing`.) Faux s'il ne répond pas ou n'expose pas ses slots.
async fn llama_occupe(http: &reqwest::Client, endpoint: &str) -> bool {
    let Ok(resp) = http
        .get(format!("{endpoint}/slots"))
        .timeout(Duration::from_secs(3))
        .send()
        .await
    else {
        return false;
    };
    resp.json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.as_array().cloned())
        .is_some_and(|slots| {
            slots
                .iter()
                .any(|s| s.get("is_processing").and_then(|b| b.as_bool()) == Some(true))
        })
}

/// La build d'un llama-server, d'après `/props` (`build_info`, par exemple
/// `b11003-7d6f5d02b` — vérifié sur le binaire b11003).
pub async fn llama_build_info(http: &reqwest::Client, endpoint: &str) -> Option<String> {
    let val: serde_json::Value = http
        .get(format!("{endpoint}/props"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    val.get("build_info")?.as_str().map(str::to_string)
}

/// La build annoncée est-elle celle que l'application installe ?
fn build_is_pinned(build: &str) -> bool {
    build == LLAMA_BUILD || build.starts_with(&format!("{LLAMA_BUILD}-"))
}

/// Le port d'une adresse `http://127.0.0.1:8080`.
fn endpoint_port(endpoint: &str) -> Option<u16> {
    endpoint
        .trim_end_matches('/')
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
}

/// Fichier (dans le dossier de données) où le lanceur note la mémoire vidéo
/// libre juste avant de charger llama-server.
pub const VRAM_AVANT_MOTEUR: &str = "llama_vram_before.json";

/// Contexte minimal que `--fit` peut choisir en profil automatique. En dessous,
/// les outils d'un connecteur ne tiennent plus avec la conversation : mieux
/// vaut déporter une couche que d'offrir 4 096 jetons.
pub const AUTO_FIT_MIN_CTX: u32 = 8192;

/// Les cartes AMD d'avant RDNA (Polaris : RX 470 à 590) : micro-batch réduit.
pub fn is_polaris_or_legacy(gpu_name: Option<&str>) -> bool {
    gpu_name.is_some_and(|name| {
        let n = name.to_lowercase();
        (n.contains("radeon") || n.contains("amd") || n.contains("rx "))
            && [
                "rx 470", "rx 480", "rx 550", "rx 560", "rx 570", "rx 580", "rx 590",
            ]
            .iter()
            .any(|m| n.contains(m))
            || n.contains("polaris")
            || n.contains("ellesmere")
            || n.contains("baffin")
    })
}

/// Le niveau de prudence choisi dans les réglages (`model_caution.json`,
/// écrit par l'application). Intermédiaire par défaut.
pub fn caution_headroom() -> locaryn_llmfit::Headroom {
    let raw =
        std::fs::read_to_string(locaryn_config::default_data_dir().join("model_caution.json"))
            .unwrap_or_default();
    let level = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("level").and_then(|l| l.as_str()).map(str::to_string))
        .unwrap_or_default();
    match level.as_str() {
        "prudent" => locaryn_llmfit::Headroom::Prudent,
        "risque" => locaryn_llmfit::Headroom::Risque,
        _ => locaryn_llmfit::Headroom::Equilibre,
    }
}

/// Ce qui décide du contexte que `--fit` trouve : le même modèle, lancé de la
/// même façon, trouve le même contexte tant que la mémoire libre est la même.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct FitCle {
    model: String,
    taille: u64,
    mmproj: bool,
    fa: String,
    kv: String,
    ubatch: u64,
    marge_mib: u64,
    slots: u64,
    build: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct FitMemorise {
    cle: FitCle,
    n_ctx: u64,
    /// VRAM libre avant ce chargement-là, en Go.
    libre_gb: f64,
}

const FIT_MEMORISE: &str = "llama_fit_cache.json";

/// Le lancement en cours qui attend de connaître le contexte choisi par
/// `--fit` : sa clé, la VRAM libre avant lui, et s'il a réutilisé un contexte
/// mémorisé.
static FIT_EN_COURS: std::sync::Mutex<Option<(FitCle, f64, bool)>> = std::sync::Mutex::new(None);

/// Le contexte trouvé au dernier chargement de ce modèle, s'il vaut encore :
/// mêmes réglages, et au moins autant de VRAM libre (à 150 Mo près).
fn fit_memorise(data_dir: &std::path::Path, cle: &FitCle, libre_gb: f64) -> Option<u64> {
    let raw = std::fs::read_to_string(data_dir.join(FIT_MEMORISE)).ok()?;
    let m: FitMemorise = serde_json::from_str(&raw).ok()?;
    (m.cle == *cle && libre_gb + 0.15 >= m.libre_gb && m.n_ctx >= AUTO_FIT_MIN_CTX as u64)
        .then_some(m.n_ctx)
}

/// La fenêtre que le moteur applique (`/props`).
async fn llama_n_ctx(http: &reqwest::Client, endpoint: &str) -> Option<u64> {
    let v: serde_json::Value = http
        .get(format!("{endpoint}/props"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    v.pointer("/default_generation_settings/n_ctx")
        .and_then(|n| n.as_u64())
}

/// Le moteur répond : noter le contexte que `--fit` a trouvé, pour les
/// chargements suivants.
async fn enregistrer_fit(http: &reqwest::Client, endpoint: &str) {
    let attente = FIT_EN_COURS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    let Some((cle, libre_gb, deja)) = attente else {
        return;
    };
    if deja {
        return;
    }
    let n_ctx = async {
        let v: serde_json::Value = http
            .get(format!("{endpoint}/props"))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        v.pointer("/default_generation_settings/n_ctx")
            .and_then(|n| n.as_u64())
    }
    .await;
    let Some(n_ctx) = n_ctx else { return };
    let data_dir = locaryn_config::default_data_dir();
    let m = FitMemorise {
        cle,
        n_ctx,
        libre_gb,
    };
    if let Ok(texte) = serde_json::to_string(&m) {
        let _ = std::fs::write(data_dir.join(FIT_MEMORISE), texte);
        tracing::info!(contexte = n_ctx, "contexte choisi par --fit mémorisé");
    }
}

/// Ce que la génération ajoute en VRAM au-delà de ce que `--fit` compte
/// (mesuré sur RX 570, Qwen3.5-9B, 135 000 jetons : 1,20 Go libres une fois
/// chargé, 0,82 Go après les premiers jetons), selon la taille de la carte.
fn marge_generation_gb() -> f64 {
    locaryn_llmfit::estimate::marge_generation_gb(locaryn_llmfit::profile().total_vram_gb)
}

/// Le contexte à viser pour que la prudence choisie soit tenue, mesure faite.
///
/// `--fit` (et le contexte qu'il avait trouvé, réutilisé tel quel) promettait
/// la marge du niveau choisi ; mesuré une fois chargé, il en restait bien
/// moins — 1,2 Go puis 0,8 Go au lieu de 1,5 en prudent, jusqu'à l'écran noir.
/// Rend le contexte réduit et la mémorisation qui le portera au prochain
/// chargement, ou `None` si la marge est tenue (ou ne peut pas se mesurer).
fn contexte_pour_la_marge(n_ctx: u64) -> Option<(u64, FitMemorise)> {
    let (cle, libre_avant, _) = FIT_EN_COURS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()?;
    if cle.marge_mib == 0 || n_ctx <= AUTO_FIT_MIN_CTX as u64 {
        return None;
    }
    let libre = locaryn_llmfit::hardware::free_vram_gb()?;
    let cible = cle.marge_mib as f64 / 1024.0 + marge_generation_gb();
    if libre >= cible {
        return None;
    }
    let go = 1024.0 * 1024.0 * 1024.0;
    // Ce que le contexte occupe : tout ce que le moteur a pris, moins ses
    // poids et un socle de tampons.
    let pris = (libre_avant - libre).max(0.0);
    let poids = cle.taille as f64 / go + 0.35;
    let par_jeton = ((pris - poids) / n_ctx as f64).max(1.0e-6);
    let retirer = ((cible - libre) / par_jeton).ceil() as u64;
    let nouveau = (n_ctx.saturating_sub(retirer) / 1024 * 1024).max(AUTO_FIT_MIN_CTX as u64);
    if nouveau >= n_ctx {
        return None;
    }
    tracing::warn!(
        libre_go = libre,
        cible_go = cible,
        contexte = n_ctx,
        nouveau,
        "marge de prudence non tenue une fois chargé — contexte réduit"
    );
    Some((
        nouveau,
        FitMemorise {
            cle,
            n_ctx: nouveau,
            libre_gb: libre_avant,
        },
    ))
}

/// Attendre que la carte ait rendu la mémoire d'un moteur arrêté (jusqu'à 8 s).
async fn attendre_vram_rendue(libre_attendu: f64) {
    for _ in 0..16 {
        match locaryn_llmfit::hardware::free_vram_gb() {
            Some(l) if l + 0.3 >= libre_attendu => return,
            None => return,
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}

/// Le chargement a échoué : le contexte mémorisé ne vaut plus. Vrai s'il en
/// avait été utilisé un — on retente alors avec `--fit`, sur le GPU.
fn oublier_fit() -> bool {
    let attente = FIT_EN_COURS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    let _ = std::fs::remove_file(locaryn_config::default_data_dir().join(FIT_MEMORISE));
    attente.is_some_and(|(_, _, deja)| deja)
}

/// La marge (Mio) que `--fit` doit laisser libre sur une carte dédiée : 5 % de
/// sa mémoire, entre 256 et 512 Mio. `None` : on garde celle de llama.cpp
/// (mémoire unifiée, ou aucune carte connue).
pub fn fit_margin_mib(total_vram_gb: f64, unified: bool) -> Option<u64> {
    if unified || total_vram_gb <= 0.0 {
        return None;
    }
    Some(((total_vram_gb * 1024.0 * 0.05) as u64).clamp(256, 512))
}

/// Le profil automatique : toutes les couches demandées et aucun contexte fixé.
/// llama.cpp choisit alors les deux d'après la mémoire libre.
pub fn is_auto_fit(gpu_layers: i64, context_length: u64) -> bool {
    gpu_layers == -1 && context_length == 0
}

/// Spawn `ollama serve` as a detached child process.
///
/// We set `OLLAMA_HOST=127.0.0.1:11434` to guarantee loopback binding even
/// if the user's environment defaults to something else. stdout/stderr are
/// piped to /dev/null (or NUL on Windows) to avoid the child holding the
/// daemon's terminal.
async fn spawn_llama_server(
    _cfg: &SupervisorConfig,
    active_model: Option<&str>,
    http: &reqwest::Client,
    cpu_only: bool,
    npu: bool,
) -> Result<Child, SupervisorError> {
    let data_dir = locaryn_config::default_data_dir();

    // Load inference config — determines which args to pass.
    let inference_cfg_path = data_dir.join("inference_config.json");
    let inference_cfg: serde_json::Value = std::fs::read_to_string(&inference_cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            serde_json::json!({
                "profile": "auto",
                "gpu_layers": -1,
                "kv_cache_type": "q8_0",
                "context_length": 0,
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
    // Sur Snapdragon, le moteur NPU d'abord (voir `npu`), le moteur Adreno
    // géré en repli.
    let bin = if npu {
        runtime_install::install_npu_runtime().await?
    } else {
        provision_llama_server(http).await?
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

    // Vérifier que le port est libre avant de lancer le moteur. Sans ça, un
    // autre processus déjà dessus (observé avec `wslrelay.exe`, mais tout
    // logiciel loopback ferait pareil) fait échouer le `bind()` de
    // llama-server, qui se ferme aussitôt — souvent avant même d'avoir
    // vidangé la première ligne de son log, puisque stderr redirigé vers un
    // fichier passe en tampon plein bloc côté runtime C au lieu du tampon
    // ligne par ligne d'un terminal. Le journal reste alors vide et
    // l'application n'a plus rien à montrer qu'un chargement qui ne finit
    // jamais. Sonder le port nous-mêmes donne un message net et immédiat.
    wait_port_free(llama_port(), Duration::from_secs(10))
        .await
        .map_err(|e| SupervisorError::SpawnFailed(ProviderEngine::LlamaCpp, e))?;

    // La mémoire vidéo libre juste avant de charger : ce que le moteur prendra
    // se lit ensuite par différence. Windows ne dit pas ce qu'occupe un
    // processus (`nvidia-smi` répond « N/A »), et l'estimation des autres
    // modèles doit savoir ce que ce moteur rendra en se déchargeant.
    // Mesurée maintenant (le profil garde une mesure quelques secondes).
    let libre_avant = locaryn_llmfit::hardware::free_vram_gb()
        .unwrap_or_else(|| locaryn_llmfit::profile().free_vram_gb);
    if let Err(e) = std::fs::write(
        data_dir.join(VRAM_AVANT_MOTEUR),
        format!("{{\"free_vram_gb\":{libre_avant}}}"),
    ) {
        tracing::warn!(erreur = %e, "mémoire vidéo d'avant chargement non enregistrée");
    }

    tracing::info!(
        bin = %bin.display(),
        model = %full_model_path.display(),
        profile = %inference_cfg["profile"].as_str().unwrap_or("auto"),
        "spawning llama-server"
    );

    // Flags verified against llama.cpp b11003 (`--help` capturé le
    // 16/09/2026): unknown flags are a
    // FATAL error for llama-server, so only pass documented ones.
    let mut cmd = Command::new(&bin);
    cmd.arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(llama_port().to_string())
        .arg("-m")
        .arg(&full_model_path);

    // Le gabarit du modèle, quand il en porte un : c'est lui qui place les
    // outils dans l'invite et qui lit les appels en retour. Forcer `chatml`
    // à tous les modèles écrasait le format propre de Gemma, Qwen ou Llama, et
    // un modèle capable d'appeler des outils n'en appelait plus aucun. Le
    // repli `chatml` ne sert qu'aux fichiers sans gabarit.
    // L'en-tête est lu une fois : il sert aussi au plafond de contexte, plus bas.
    let summary = locaryn_llmfit::read_summary(&full_model_path).ok();
    let declares_template = summary
        .as_ref()
        .is_some_and(|sum| !sum.chat_template.trim().is_empty());
    if declares_template {
        cmd.arg("--jinja");
    } else {
        cmd.arg("--chat-template").arg("chatml");
    }

    // Vision: if a matching mmproj file sits next to the model
    // (mmproj-<model>.gguf or a single mmproj-*.gguf in the dir), load it.
    let mmproj = find_mmproj_for(&full_model_path);
    if let Some(mmproj) = &mmproj {
        tracing::info!(mmproj = %mmproj.display(), "loading multimodal projector");
        cmd.arg("--mmproj").arg(mmproj);
        // Les projecteurs à résolution dynamique (Qwen-VL) préparent leur
        // tampon pour 8 464 jetons d'image : sur Vulkan, une partie de leur
        // attention retombe sur le processeur et réclame plusieurs Go de RAM
        // au démarrage, pour une image que personne n'a envoyée. 2 048 jetons
        // gardent une image nette (Qwen en demande 1 024 au minimum).
    }
    // Sur une carte de 8 Go ou moins, le projecteur vision reste côté
    // processeur : ses 0,9 Go de VRAM manquaient au modèle et aux autres
    // applications, pour des images qu'on n'envoie qu'occasionnellement.
    let profil_carte = locaryn_llmfit::profile();
    let vision_sur_processeur = mmproj.is_some()
        && !cpu_only
        && !profil_carte.unified_memory
        && profil_carte.total_vram_gb > 0.0
        && profil_carte.total_vram_gb <= 8.5;
    if vision_sur_processeur {
        cmd.arg("--no-mmproj-offload");
    }
    if mmproj.is_some() {
        // Encodée par le processeur, chaque capture d'écran l'occupait à 75 %
        // (mesuré sur RX 570 pendant un contrôle du navigateur) : 1 024
        // jetons, le minimum de Qwen-VL, divisent ce travail par deux.
        let jetons = if vision_sur_processeur {
            "1024"
        } else {
            "2048"
        };
        cmd.arg("--image-max-tokens").arg(jetons);
    }

    // Détection du GPU et du backend : utilisée pour la flash attention, le
    // micro-batch et la marge laissée libre.
    let profile = locaryn_llmfit::profile();
    let is_polaris_or_legacy = is_polaris_or_legacy(profile.gpu_name.as_deref());
    // L'architecture de la *machine* : l'application x64 tourne émulée sur un
    // Snapdragon, et `cfg!(target_arch)` y répond « x86_64 ». Elle forçait
    // alors la flash attention et le cache V quantifié sur le moteur OpenCL
    // Adreno, qui ne les tient pas : le moteur tombait au chargement.
    let is_arm = cfg!(target_arch = "aarch64") || locaryn_llmfit::hardware::host_is_arm64();

    // Profil automatique (contexte 0, toutes les couches) : llama.cpp choisit
    // le contexte d'après la mémoire libre (`--fit`). Ce choix coûte un
    // chargement d'essai — 22 s sur une RX 570, la VRAM qui monte, retombe et
    // remonte. Le contexte qu'il a trouvé est donc gardé, et les chargements
    // suivants du même modèle le reçoivent directement (`-c`, `--fit off`).
    //
    // Plus de plafond à 8 192 pour les Polaris : il venait d'un cache
    // d'attention sans flash attention, dont les tampons grossissent avec le
    // contexte. Avec la flash attention, mesuré sur RX 570 (Qwen3.5 9B
    // IQ3_XXS) : 129 536 jetons, aucun plantage. Pire, le plafond faisait
    // relancer le moteur à chaque message : 124 928 demandés ≠ 8 192 actifs.
    let gpu_layers = inference_cfg["gpu_layers"].as_i64().unwrap_or(-1);
    let ctx_demande = inference_cfg["context_length"].as_u64().unwrap_or(0);
    let auto = is_auto_fit(gpu_layers, ctx_demande);

    // Flash attention. Sur Vulkan (AMD, Intel, NVIDIA), elle tient : tuilée,
    // elle évite le tampon « clé × requête » qui grossit avec le contexte —
    // c'est lui qui dépassait 2 s par dispatch (TDR Windows) et la VRAM d'une
    // RX 570 dès 64k jetons (2,2 Go réclamés, mesuré). Elle permet aussi de
    // quantifier le cache V. Mesuré sur RX 570, prompt de 4 096 jetons :
    // lecture 75 au lieu de 95 jetons/s, écriture 10,9 au lieu de 8,0.
    // ARM et processeur seul : `auto`, llama.cpp sait ce que le périphérique
    // supporte (OpenCL/Adreno ne l'a pas) — le cache V reste alors en f16.
    let fa_voulue = inference_cfg["flash_attention"].as_bool().unwrap_or(true);
    let fa_mode = if !fa_voulue {
        "off"
    } else if cpu_only || is_arm {
        "auto"
    } else {
        "on"
    };
    cmd.arg("-fa").arg(fa_mode);
    let flash_attention_active = fa_mode == "on";

    // KV cache quantization: -ctk/-ctv. "turbo3" is a legacy label mapped to
    // q4_0. Un cache V quantifié exige la flash attention (sinon : « quantized
    // V cache requires flash_attn to be enabled ») — seulement en `on`.
    let kv_type = match inference_cfg["kv_cache_type"].as_str().unwrap_or("f16") {
        "turbo3" | "turboquant" => "q4_0",
        other => other,
    };
    let kv_quantifie = matches!(
        kv_type,
        "q8_0" | "q5_1" | "q5_0" | "q4_1" | "q4_0" | "iq4_nl"
    );
    // Sur ARM, le moteur OpenCL Adreno ne copie pas vers un cache quantifié :
    // chaque écriture retombait sur le processeur. Le cache reste en f16 — la
    // mémoire y est unifiée, et la place ne manque pas autant qu'en VRAM.
    if kv_quantifie && !is_arm {
        cmd.arg("-ctk").arg(kv_type);
        if flash_attention_active {
            cmd.arg("-ctv").arg(kv_type);
        }
    }

    // Micro-batch. 256 sur Polaris : mesuré sur RX 570, la lecture du prompt
    // passe de 73 (ub 32) à 112 jetons/s, au niveau de 512, sans TDR même sur
    // 4 096 jetons. Le 32 d'avant divisait la vitesse de lecture — 30 à 60 s
    // avant le premier mot.
    let default_ubatch = if is_polaris_or_legacy { 256 } else { 512 };
    let batch = inference_cfg["batch_size"].as_u64().unwrap_or(512).max(1);
    let ubatch = inference_cfg["ubatch_size"]
        .as_u64()
        .unwrap_or(default_ubatch)
        .min(batch);
    // Le moteur est déjà tombé en pleine réponse avec ce modèle : plus léger.
    let plantages = plantages_de(active_model.unwrap_or_default());
    let ubatch = micro_lot_allege(ubatch, plantages);
    if plantages > 0 {
        tracing::warn!(
            plantages,
            ubatch,
            "micro-lot réduit après un plantage du moteur"
        );
    }

    // Un seul emplacement : une seule personne parle au modèle. Sans `-np`,
    // llama.cpp en ouvre quatre, et une requête pouvait atterrir dans un
    // emplacement dont le cache ne contenait pas la conversation — relue en
    // entier. Les requêtes annexes (titre, suggestions) restent bon marché :
    // le cache en RAM (`--cache-ram`) rend la conversation après elles
    // (mesuré : 4,7 s au lieu de 60 s).
    let slots = inference_cfg["parallel_slots"].as_u64().unwrap_or(1).max(1);

    // La marge que `--fit` laisse libre : celle du niveau de prudence choisi,
    // calculée sur la VRAM libre avant le chargement (1,5 Go sur 7 Go libres
    // en prudent, 0,85 en équilibré, 0,53 en risqué). Arrondie au huitième de
    // Go : le contexte mémorisé reste valable d'un chargement à l'autre.
    let marge_mib = if cpu_only {
        None
    } else {
        inference_cfg["fit_margin_mb"].as_u64().or_else(|| {
            if profile.unified_memory || profile.total_vram_gb <= 0.0 {
                return None;
            }
            let reserve = caution_headroom().vram_reserve_gb(libre_avant);
            Some(((reserve * 8.0).round() / 8.0 * 1024.0).round() as u64)
        })
    };

    let cle_fit_taille = std::fs::metadata(&full_model_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let cle_fit = FitCle {
        model: full_model_path.to_string_lossy().into_owned(),
        taille: std::fs::metadata(&full_model_path)
            .map(|m| m.len())
            .unwrap_or(0),
        mmproj: mmproj.is_some(),
        fa: fa_mode.to_string(),
        kv: kv_type.to_string(),
        ubatch,
        marge_mib: marge_mib.unwrap_or(0),
        slots,
        build: if npu {
            format!("{LLAMA_BUILD}-npu")
        } else {
            LLAMA_BUILD.to_string()
        },
    };
    let ctx_memorise = if auto && !cpu_only {
        fit_memorise(&data_dir, &cle_fit, libre_avant)
    } else {
        None
    };
    // Le prochain chargement réussi enregistre ce que `--fit` a trouvé.
    *FIT_EN_COURS.lock().unwrap_or_else(|e| e.into_inner()) = if auto && !cpu_only {
        Some((cle_fit, libre_avant, ctx_memorise.is_some()))
    } else {
        None
    };

    let mut ctx = ctx_demande;
    if let Some(n) = ctx_memorise {
        tracing::info!(
            contexte = n,
            "contexte trouvé au chargement précédent — sans essai"
        );
        cmd.arg("-c").arg(n.to_string()).arg("-fit").arg("off");
        ctx = 0;
    } else {
        if ctx == 0 || is_polaris_or_legacy {
            // En auto, --fit-ctx laisse llama.cpp choisir sans descendre sous
            // le minimum utile. Avec un contexte fixe, c'est un filet : si le
            // modèle et son cache ne tiennent pas, le contexte est réduit au
            // lieu d'un refus de démarrer.
            cmd.arg("--fit-ctx").arg(AUTO_FIT_MIN_CTX.to_string());
        }
        if let Some(marge) = marge_mib {
            // La marge du niveau, plus ce que la génération ajoute ensuite
            // (non compté par --fit) : la vérification après chargement n'a
            // alors presque jamais à recharger.
            let avec_generation = marge + (marge_generation_gb() * 1024.0).round() as u64;
            cmd.arg("--fit-target").arg(avec_generation.to_string());
        }
    }
    // Repli processeur : le pilote GPU a fait tomber le moteur au démarrage
    // (OpenCL d'un Snapdragon, Vulkan d'une vieille carte). `--device none`
    // n'initialise aucun périphérique — vérifié sur le `--help` de b11003.
    if cpu_only {
        cmd.arg("--device").arg("none");
    } else if npu {
        // Le modèle sur le NPU, réparti en autant de sessions Hexagon que sa
        // taille l'exige (environ 2 Go chacune) ; l'Adreno n'est pas utilisé.
        let sessions = npu::sessions_pour(cle_fit_taille);
        cmd.env("GGML_HEXAGON_DEVICES", sessions.to_string());
        cmd.arg("--device").arg(npu::appareils(sessions));
        cmd.arg("-ngl").arg("999");
        tracing::info!(sessions, "moteur lancé sur le NPU Hexagon");
    } else if gpu_layers >= 0 {
        cmd.arg("-ngl").arg(gpu_layers.to_string());
    } else {
        // En mode auto (-1), toutes les couches sur la carte : laissé vide,
        // llama.cpp gardait des couches en RAM (14 Go de RAM, processeur à
        // 100 %). 999 couvre tous les modèles.
        cmd.arg("-ngl").arg("999");
    }

    // Context length. 0 : llama.cpp l'ajuste à la mémoire libre, y compris
    // quand le nombre de couches est imposé.
    if ctx > 0 {
        cmd.arg("-c").arg(ctx.to_string());
    }

    // Suggest to the UI the largest context this model was trained for: a
    // slider that proposes 128k to a 32k model invites a request the engine
    // must quietly clamp. Best effort — the file may not declare it.
    if let Some(sum) = &summary {
        if sum.train_context > 0 {
            let cap = (sum.train_context as f64 * 1.5).round() as u64;
            let _ = std::fs::write(
                data_dir.join("model_ctx_capacity.json"),
                format!("{{\"max_ctx\":{cap}}}"),
            );
        }
    }

    // MoE expert offload to CPU: run very large Mixture-of-Experts models on a
    // modest GPU by keeping expert weights in system RAM while attention stays on
    // the GPU. -1 = all experts on CPU (-cmoe); N>0 = experts of the first N
    // layers (-ncmoe N). Verified present in b11003.
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
    // Modèle sur la carte : les threads du processeur n'ont presque rien à
    // faire pendant la génération, mais par défaut ils attendent le GPU en
    // boucle active (`--poll 50`). Sur une RX 570, llama-server occupait ainsi
    // 75 % du processeur alors que la carte faisait le travail. Sans attente
    // active, ils dorment entre deux lots.
    if !cpu_only {
        cmd.arg("--poll").arg("0");
    }

    cmd.arg("-b").arg(batch.to_string());
    cmd.arg("-ub").arg(ubatch.to_string());

    // mmap : « --no-mmap » n'existe plus depuis ~b11000 (l'option mmap est
    // devenue à valeurs) — la forme actuelle est le mode de chargement
    // `-lm none` (« auto | none | mmap | mlock | mmap+mlock », vérifié sur le
    // --help de b11003). C'est le seul drapeau du spawn qui a changé de forme
    // entre b10088 et b11003.
    // Modèle entier sur une carte dédiée : sans mmap. Mappé, le fichier GGUF
    // restait compté dans la RAM du moteur une fois copié dans la VRAM —
    // mesuré sur RX 570 avec un 9B : 4,1 Go de RAM avec mmap, 0,6 Go sans, et
    // un chargement un peu plus rapide (3,5 s au lieu de 4,3). En mémoire
    // unifiée (Mac, Snapdragon), le mapping évite au contraire une copie.
    let tout_sur_la_carte =
        !cpu_only && gpu_layers < 0 && !profile.unified_memory && profile.total_vram_gb > 0.0;
    if !inference_cfg["use_mmap"].as_bool().unwrap_or(true) || tout_sur_la_carte {
        cmd.arg("-lm").arg("none");
    }

    cmd.arg("-np").arg(slots.to_string());

    // Le cache des conversations en RAM (8 Go par défaut) : sur une machine
    // de 16 Go ou moins, il poussait le système à la pagination. 1 Go garde la
    // conversation en cours et les requêtes annexes (titre, suggestions) ;
    // 2 Go se remplissaient au fil des échanges et se voyaient dans la RAM
    // du moteur.
    if profile.total_ram_gb > 0.0 && profile.total_ram_gb <= 16.5 {
        cmd.arg("-cram").arg("1024");
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
        // Console cachée plutôt que DETACHED_PROCESS : voir extension_engine.
        cmd.creation_flags(locaryn_config::HIDDEN_PROCESS_GROUP);
    }

    // Sans ceci, fermer l'application laissait llama-server tourner : le
    // processus n'était l'enfant de rien qui le tue à sa sortie, gardait le
    // modèle en VRAM indéfiniment, et le superviseur du lancement suivant —
    // sa table d'états repartant vide — ne pouvait plus jamais l'éteindre
    // (« éjecter » n'avait alors aucune prise sur lui).
    cmd.kill_on_drop(true);

    let settings_record = serde_json::json!({
        "model": full_model_path.to_string_lossy(),
        // Le contexte *demandé* (0 = automatique) : c'est lui que l'on compare
        // à la configuration pour savoir s'il faut relancer.
        "ctx": ctx_demande,
        "ctx_memorise": ctx_memorise,
        "ubatch": ubatch,
        "batch": batch,
        "fa": if flash_attention_active { "on" } else { "off" },
        "cpu_only": cpu_only,
    });
    let _ = std::fs::write(
        data_dir.join("active_llama_settings.json"),
        settings_record.to_string(),
    );

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
        cmd.creation_flags(locaryn_config::HIDDEN_PROCESS_GROUP);
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
    let base = stem_sans_quant(&stem);

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    let mut autres_modeles = 0usize;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
            continue;
        };
        if !name.ends_with(".gguf") {
            continue;
        }
        if name.contains("mmproj") {
            if name.contains(&base) {
                return Some(p);
            }
            candidates.push(p);
        } else if p != model_path {
            autres_modeles += 1;
        }
    }
    // Un projecteur isole n'est attribuable qu'a un modele seul dans son
    // dossier : des qu'il y en a plusieurs, le premier mmproj venu appartient
    // tres probablement a un autre (n_embd different, llama-server refuse de
    // demarrer et l'application reste sur « chargement du modele »).
    if candidates.len() == 1 && autres_modeles == 0 {
        return candidates.pop();
    }
    None
}

/// Nom de modele sans sa quantification : « bonsai-27b-q1_0 » -> « bonsai-27b »,
/// « qwen2-vl-2b-instruct-q4_k_m » -> « qwen2-vl-2b-instruct ».
fn stem_sans_quant(stem: &str) -> String {
    const MARQUES: [&str; 9] = [
        "-q1", "-q2", "-q3", "-q4", "-q5", "-q6", "-q8", "-f16", "-bf16",
    ];
    let coupe = MARQUES
        .iter()
        .filter_map(|m| stem.find(m))
        .min()
        .unwrap_or(stem.len());
    stem[..coupe].to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn le_micro_lot_s_allege_a_chaque_plantage() {
        assert_eq!(super::micro_lot_allege(512, 0), 512);
        assert_eq!(super::micro_lot_allege(512, 1), 128);
        assert_eq!(super::micro_lot_allege(512, 2), 32);
        assert_eq!(super::micro_lot_allege(64, 1), 64);
    }

    use super::*;

    #[test]
    fn seul_le_profil_automatique_laisse_llama_cpp_ajuster() {
        assert!(is_auto_fit(-1, 0));
        // Un contexte choisi ou un nombre de couches imposé : on les passe tels quels.
        assert!(!is_auto_fit(-1, 16384));
        assert!(!is_auto_fit(20, 0));
    }

    fn dossier_avec(fichiers: &[&str]) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "locaryn-mmproj-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for f in fichiers {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        dir
    }

    /// Reproduit le blocage observe : `Bonsai-27B-mmproj-Q8_0.gguf` ne commence
    /// pas par `mmproj-`, donc n'etait pas vu, et le mmproj de gemma (seul
    /// candidat) lui etait attribue : llama-server quittait sur « mismatch
    /// between text model and mmproj ».
    #[test]
    fn le_projecteur_d_un_modele_ne_va_pas_a_un_autre() {
        let dir = dossier_avec(&[
            "Bonsai-27B-Q1_0.gguf",
            "Bonsai-27B-mmproj-Q8_0.gguf",
            "gemma-4-E2B-it-Q4_0.gguf",
            "mmproj-gemma-4-E2B-it-Q8_0.gguf",
        ]);
        let bonsai = find_mmproj_for(&dir.join("Bonsai-27B-Q1_0.gguf")).unwrap();
        assert!(bonsai.ends_with("Bonsai-27B-mmproj-Q8_0.gguf"));
        let gemma = find_mmproj_for(&dir.join("gemma-4-E2B-it-Q4_0.gguf")).unwrap();
        assert!(gemma.ends_with("mmproj-gemma-4-E2B-it-Q8_0.gguf"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn un_projecteur_etranger_n_est_pas_attribue_quand_le_dossier_a_plusieurs_modeles() {
        let dir = dossier_avec(&[
            "qwen2.5-3b-instruct-q4_k_m.gguf",
            "gemma-4-E2B-it-Q4_0.gguf",
            "mmproj-gemma-4-E2B-it-Q8_0.gguf",
        ]);
        assert!(find_mmproj_for(&dir.join("qwen2.5-3b-instruct-q4_k_m.gguf")).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn un_projecteur_isole_reste_attribue_a_un_modele_seul() {
        let dir = dossier_avec(&["modele-vl.gguf", "mmproj-autre-nom.gguf"]);
        assert!(find_mmproj_for(&dir.join("modele-vl.gguf")).is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Un port loopback déjà pris (observé avec `wslrelay.exe` squattant
    /// 8080) faisait échouer le `bind()` de llama-server en silence : le
    /// processus se fermait avant d'écrire quoi que ce soit dans son propre
    /// journal, et l'application restait bloquée sur « chargement du modèle »
    /// indéfiniment, sans message. `ensure_port_free` doit détecter ce cas
    /// avant même de lancer le moteur.
    /// Sous Windows, le port d'un llama-server vivant répond 10013
    /// (`PermissionDenied`) : le message ne doit plus accuser un « autre
    /// programme », mais nommer la vraie piste.
    #[test]
    fn un_port_tenu_en_exclusivite_n_est_pas_decrit_comme_un_autre_programme() {
        let refus = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let msg = port_error_message(8080, &refus);
        assert!(msg.contains("llama-server"), "{msg}");
        assert!(msg.contains("LOCARYN_LLAMA_PORT"), "{msg}");

        let pris = std::io::Error::from(std::io::ErrorKind::AddrInUse);
        assert!(port_error_message(8080, &pris).contains("déjà utilisé"));
    }

    #[test]
    fn les_peripheriques_du_moteur_se_lisent() {
        let vulkan = "load_backend: loaded Vulkan backend\nAvailable devices:\n  Vulkan0: AMD Radeon RX 570 Series (8192 MiB, 7900 MiB free)\n";
        assert_eq!(
            parse_list_devices(vulkan),
            Some(vec![
                "Vulkan0: AMD Radeon RX 570 Series (8192 MiB, 7900 MiB free)".to_string()
            ])
        );
        // Sortie réelle de la build CPU b11003 : aucun GPU.
        assert_eq!(
            parse_list_devices("Available devices:\n  (none)\n"),
            Some(vec![])
        );
        assert_eq!(parse_list_devices("erreur"), None);
    }

    /// Une carte de 6 Go gardait 1 Go libre (marge par défaut de `--fit`) :
    /// 0,7 Go jamais utilisés pour le contexte.
    #[test]
    fn la_marge_de_fit_suit_la_carte() {
        assert_eq!(fit_margin_mib(6.0, false), Some(307));
        assert_eq!(fit_margin_mib(4.0, false), Some(256));
        assert_eq!(fit_margin_mib(24.0, false), Some(512));
        // Mémoire unifiée ou pas de carte : la marge de llama.cpp.
        assert_eq!(fit_margin_mib(16.0, true), None);
        assert_eq!(fit_margin_mib(0.0, false), None);
    }

    /// Un moteur qui n'annonce pas la build épinglée n'est pas adopté.
    #[test]
    fn seule_la_build_epinglee_est_adoptee() {
        assert!(build_is_pinned("b11003-7d6f5d02b"));
        assert!(build_is_pinned("b11003"));
        assert!(!build_is_pinned("b10088-1234abc"));
        assert!(!build_is_pinned("b110031-x"));
    }

    /// Le cas du rapport : un autre programme écoute sur le port. Le choix
    /// passe au suivant au lieu de laisser llama-server mourir au démarrage.
    #[test]
    fn un_port_occupe_par_un_autre_programme_est_evite() {
        let occupant = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let pris = occupant.local_addr().unwrap().port();
        assert!(!port_libre(pris), "un port écouté n'est pas libre");
        assert!(!llama_ecoute(pris), "l'occupant n'est pas un llama-server");
        let choisi = choisir_port_llama(pris..=pris);
        assert_ne!(choisi, pris, "le port occupé ne doit pas être choisi");
        assert!(ensure_port_free(pris).is_err());
        drop(occupant);
    }

    /// Une adresse locale suit le port choisi ; une adresse distante, jamais.
    #[test]
    fn l_adresse_locale_du_moteur_suit_le_port() {
        let voulu = llama_port();
        let autre = if voulu == 8080 { 8099 } else { 8080 };
        assert_eq!(
            realigner_endpoint_llama(&format!("http://127.0.0.1:{autre}")),
            Some(format!("http://127.0.0.1:{voulu}"))
        );
        assert_eq!(
            realigner_endpoint_llama(&format!("http://localhost:{autre}/")),
            Some(format!("http://127.0.0.1:{voulu}"))
        );
        assert_eq!(
            realigner_endpoint_llama(&format!("http://127.0.0.1:{voulu}")),
            None
        );
        assert_eq!(realigner_endpoint_llama("http://192.168.1.20:8080"), None);
        assert_eq!(realigner_endpoint_llama("https://api.exemple.fr"), None);
    }

    #[test]
    fn le_port_se_lit_dans_l_adresse_du_moteur() {
        assert_eq!(endpoint_port("http://127.0.0.1:8080"), Some(8080));
        assert_eq!(endpoint_port("http://127.0.0.1:9001/"), Some(9001));
        assert_eq!(endpoint_port("http://localhost"), None);
    }

    /// Un faux llama-server qui répond toujours la même chose à `/health`.
    async fn faux_moteur(statut: &'static str, corps: &'static str) -> String {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut tampon = [0u8; 1024];
                let _ = sock.read(&mut tampon).await;
                let reponse = format!(
                    "HTTP/1.1 {statut}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corps}",
                    corps.len()
                );
                let _ = sock.write_all(reponse.as_bytes()).await;
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// Le cas du rapport : la VRAM est occupée, le moteur charge encore, et
    /// l'application le prenait pour absent.
    #[tokio::test]
    async fn un_moteur_qui_charge_est_reconnu_comme_vivant() {
        let http = reqwest::Client::new();
        let charge = faux_moteur(
            "503 Service Unavailable",
            r#"{"error":{"code":503,"message":"Loading model","type":"unavailable_error"}}"#,
        )
        .await;
        assert_eq!(
            llama_port_state(&http, &charge).await,
            LlamaPortState::Loading
        );

        let pret = faux_moteur("200 OK", r#"{"status":"ok"}"#).await;
        assert_eq!(llama_port_state(&http, &pret).await, LlamaPortState::Ready);

        let autre = faux_moteur("404 Not Found", "nope").await;
        assert_eq!(llama_port_state(&http, &autre).await, LlamaPortState::Other);

        // Un port où personne n'écoute.
        let libre = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = libre.local_addr().unwrap().port();
        drop(libre);
        assert_eq!(
            llama_port_state(&http, &format!("http://127.0.0.1:{port}")).await,
            LlamaPortState::Absent
        );
    }

    /// Un port relâché peu après doit être attendu, pas refusé.
    #[tokio::test]
    async fn le_port_d_un_moteur_arrete_est_attendu() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            drop(listener);
        });
        wait_port_free(port, Duration::from_secs(5))
            .await
            .expect("le port se libère pendant l'attente");
    }

    #[test]
    fn refuse_de_lancer_le_moteur_si_le_port_est_deja_pris() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        let err = ensure_port_free(port).expect_err("le port est occupé par `listener`");
        assert!(
            err.contains(&port.to_string()),
            "le message doit nommer le port en cause : {err}"
        );

        drop(listener);
        ensure_port_free(port).expect("le port est libre une fois le listener relâché");
    }
}
