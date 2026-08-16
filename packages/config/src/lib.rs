//! Locaryn configuration, merged across scopes:
//!   1. defaults (in code)
//!   2. global:   `~/.locaryn/config.toml`
//!   3. workspace: `<project>/.locaryn/config.toml`
//!   4. env vars:  `LOCARYN_*` (highest priority)
//!
//! V1 skeleton uses JSON-ish parsing via serde_json to avoid a toml dep in
//! the MVP. V1.1 switches to `toml`.

pub mod mtls;
pub mod provision;

use locaryn_shared_types::ConnectionMode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub connection: ConnectionConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub remote: Option<RemoteConfig>,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    #[serde(default = "default_mode")]
    pub mode: ConnectionMode,
    #[serde(default = "default_local_url")]
    pub local_url: String,
    #[serde(default)]
    pub remote_url: Option<String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            local_url: default_local_url(),
            remote_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    #[serde(default = "default_daemon_port")]
    pub port: u16,
    #[serde(default = "default_daemon_bind")]
    pub bind: String,
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    /// TLS certificate and key, in PEM. Both or neither: naming only one is
    /// refused rather than silently downgraded to plain HTTP.
    /// When the daemon is exposed and neither is set, a self-signed pair is
    /// generated once and reused.
    #[serde(default)]
    pub tls_cert: Option<String>,
    #[serde(default)]
    pub tls_key: Option<String>,
    /// Demand a client certificate signed by this server's authority.
    ///
    /// Off by default and never enabled by an update: switching it on stops
    /// every existing client until each has been issued a certificate, which
    /// has to be a decision rather than a surprise.
    #[serde(default)]
    pub require_client_cert: bool,
    /// Ask the router to forward this port, making the daemon reachable from
    /// outside the local network.
    ///
    /// Refused unless `require_client_cert` is on: publishing a service to the
    /// internet behind only a password is not defensible.
    #[serde(default)]
    pub open_router_port: bool,
    /// Travel mode: open an outbound tunnel through a relay, so this machine
    /// is reachable from anywhere without touching the router.
    ///
    /// `"cloudflare"`, `"ngrok"` or `"devtunnel"`. None means off, which is
    /// the default — nothing reaches the internet unless it was asked for.
    #[serde(default)]
    pub travel: Option<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            port: default_daemon_port(),
            bind: default_daemon_bind(),
            data_dir: None,
            tls_cert: None,
            tls_key: None,
            require_client_cert: false,
            open_router_port: false,
            travel: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub server_url: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub tls: TlsConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlsConfig {
    #[serde(default)]
    pub ca_path: Option<PathBuf>,
    #[serde(default)]
    pub client_cert: Option<PathBuf>,
    #[serde(default)]
    pub client_key: Option<PathBuf>,
    #[serde(default = "default_tls_insecure")]
    pub allow_insecure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub json: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            json: false,
        }
    }
}

fn default_mode() -> ConnectionMode {
    ConnectionMode::Auto
}
fn default_local_url() -> String {
    "http://127.0.0.1:7474".to_string()
}
fn default_daemon_port() -> u16 {
    7474
}
fn default_daemon_bind() -> String {
    "127.0.0.1".to_string()
}
fn default_tls_insecure() -> bool {
    false
}
fn default_log_level() -> String {
    "info".to_string()
}

// ============================================================================
// Paths
// ============================================================================

/// `~/.locaryn`, or the `~/.lochor` of an install predating the rename.
///
/// The project was called Lochor before it was called Locaryn. Returning the
/// new path unconditionally would leave those installs staring at an empty
/// application: same disk, same data, invisible. So the legacy directory wins
/// when it is the only one present — the pointer file, the config and the
/// database it designates all keep resolving.
///
/// Nothing is moved. A rename of the product is not a reason to rewrite a
/// user's home directory behind their back, and a half-finished move is worse
/// than none. Once `~/.locaryn` exists, it takes precedence for good.
pub fn global_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let current = home.join(".locaryn");
    if current.exists() {
        return current;
    }
    let legacy = home.join(".lochor");
    if legacy.is_dir() {
        return legacy;
    }
    current
}

/// `~/.locaryn/config.toml`
pub fn global_config_path() -> PathBuf {
    global_dir().join("config.toml")
}

/// `~/.locaryn/data` — the built-in location, used only when no storage root
/// has been configured. Never call this directly: go through [`storage_root`]
/// so the user's choice is honoured.
fn builtin_data_dir() -> PathBuf {
    global_dir().join("data")
}

/// `~/.locaryn/storage.json` — a tiny pointer file naming the real storage root.
///
/// It deliberately stays next to the config on the home drive: it is a few
/// bytes, and it must be readable before anything else (the database itself
/// lives under the root it designates).
pub fn storage_pointer_path() -> PathBuf {
    global_dir().join("storage.json")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoragePointer {
    #[serde(default)]
    root: Option<PathBuf>,
}

/// Root directory holding everything bulky: model weights, engine binaries,
/// the database and scratch files. Configurable because a full system drive
/// is the normal case — weights alone run to tens of gigabytes.
///
/// Resolution order:
/// 1. `LOCARYN_STORAGE_ROOT` env var,
/// 2. the `storage.json` pointer written by [`set_storage_root`],
/// 3. `~/.locaryn/data`.
pub fn storage_root() -> PathBuf {
    configured_storage_root().unwrap_or_else(builtin_data_dir)
}

/// The explicitly chosen root, or `None` when the user never chose one.
///
/// Both sources of an explicit choice are handled here so every derived path
/// agrees: resolving the env var separately once left `models_dir` on the
/// legacy location while the engines had already followed the new root.
pub fn configured_storage_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LOCARYN_STORAGE_ROOT") {
        let p = PathBuf::from(dir);
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }
    let raw = std::fs::read_to_string(storage_pointer_path()).ok()?;
    let ptr: StoragePointer = serde_json::from_str(&raw).ok()?;
    ptr.root.filter(|p| !p.as_os_str().is_empty())
}

/// Persist a new storage root. Moving the existing data is the caller's job —
/// this only records the choice.
pub fn set_storage_root(root: Option<&Path>) -> std::io::Result<()> {
    let dir = global_dir();
    std::fs::create_dir_all(&dir)?;
    let ptr = StoragePointer {
        root: root.map(|p| p.to_path_buf()),
    };
    let json = serde_json::to_string_pretty(&ptr)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(storage_pointer_path(), json)
}

/// Where the database and the small JSON settings live.
///
/// Deliberately *not* under [`storage_root`]: the database is a few megabytes,
/// it is held open for the whole session, and relocating an open SQLite file
/// (plus its WAL) while the app runs risks corrupting it. Only the bulky,
/// closed-file data is relocatable.
pub fn default_data_dir() -> PathBuf {
    builtin_data_dir()
}

/// Engine binaries (llama-server, sd). Hundreds of megabytes, so they follow
/// the storage root.
pub fn bin_dir() -> PathBuf {
    storage_root().join("bin")
}

/// Attachments of chats that belong to no project. User-supplied images and
/// audio, so unbounded in size — follows the storage root.
pub fn free_chats_dir() -> PathBuf {
    storage_root().join("free_chats")
}

/// Volume that should absorb bulky, regenerable data: scratch files and
/// downloaded model caches.
///
/// When the user configured a root, that is the answer. Otherwise follow the
/// weights: if the models already live on a data volume, its caches belong
/// there too. Defaulting these to the home directory is how a system drive
/// ends up with no free space after a handful of model pulls.
fn heavy_data_root() -> PathBuf {
    if let Some(root) = configured_storage_root() {
        return root;
    }
    if let Some(parent) = models_dir().parent() {
        if parent.is_dir() {
            return parent.to_path_buf();
        }
    }
    builtin_data_dir()
}

/// Scratch space for intermediate files (img2img inputs, decoded uploads,
/// conversion buffers). Deliberately *not* the OS temp dir: those files reach
/// hundreds of megabytes and would land on the system drive.
pub fn temp_dir() -> PathBuf {
    heavy_data_root().join("locaryn_tmp")
}

/// Où vont les images produites par les modèles.
///
/// Elles sont grosses et refabricables — un mégaoctet et demi la pièce, et
/// rien n'oblige à les garder. Elles atterrissaient dans le dossier de données
/// du système : quelques dizaines d'images suffisaient à ronger le disque C:
/// d'une machine dont tous les poids étaient pourtant sur un autre volume.
pub fn generated_images_dir() -> PathBuf {
    heavy_data_root().join("generated_images")
}

/// Où vont les voix produites. Même raisonnement que les images.
pub fn generated_audio_dir() -> PathBuf {
    heavy_data_root().join("generated_audio")
}

/// Where HuggingFace downloads land. A single inpainting pipeline is ~2 GB and
/// a diffusion checkpoint several more, so this must not default to `~/.cache`.
pub fn hf_cache_dir() -> PathBuf {
    heavy_data_root().join("hf_cache")
}

/// [`temp_dir`], created if missing. Falls back to the OS temp dir if the
/// configured root is not writable, so a bad setting degrades instead of
/// breaking generation outright.
pub fn ensure_temp_dir() -> PathBuf {
    let dir = temp_dir();
    match std::fs::create_dir_all(&dir) {
        Ok(()) => dir,
        Err(e) => {
            tracing::warn!(
                "temp dir {} unusable ({e}); falling back to the system temp dir",
                dir.display()
            );
            std::env::temp_dir()
        }
    }
}

/// `<project>/.locaryn/config.toml`
pub fn workspace_config_path(project: &Path) -> PathBuf {
    project.join(".locaryn").join("config.toml")
}

/// Where model weights live. Resolution order:
/// 1. `LOCARYN_MODELS_DIR` env var (explicit override),
/// 2. `<storage root>/models` when the user configured a root — an explicit
///    choice always wins over the legacy guess below,
/// 3. an existing legacy models folder that already holds downloaded weights
///    (kept so early users don't lose multi-GB downloads),
/// 4. `<storage root>/models`.
pub fn models_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LOCARYN_MODELS_DIR") {
        let p = PathBuf::from(dir);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    if let Some(root) = configured_storage_root() {
        return root.join("models");
    }
    // Legacy location used by early builds — keep it if it already has models.
    let legacy = PathBuf::from(r"D:\Documents\Syncho\models");
    if legacy.is_dir() {
        let has_weights = std::fs::read_dir(&legacy)
            .map(|entries| {
                entries.flatten().any(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| {
                            let x = x.to_ascii_lowercase();
                            x == "gguf" || x == "safetensors"
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if has_weights {
            return legacy;
        }
    }
    default_data_dir().join("models")
}

// ============================================================================
// Loading & merging
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config read error: {0}")]
    Io(#[from] std::io::Error),
    #[error("config parse error: {0}")]
    Parse(String),
}

/// Load and merge configuration across all scopes, then apply env overrides.
pub fn load(project: Option<&Path>) -> Result<Config, ConfigError> {
    let mut cfg = Config::default();

    // global
    let gpath = global_config_path();
    if gpath.exists() {
        let raw = std::fs::read_to_string(&gpath)?;
        let parsed = parse_json_or_toml(&raw)?;
        merge(&mut cfg, parsed);
    }

    // workspace
    if let Some(proj) = project {
        let wpath = workspace_config_path(proj);
        if wpath.exists() {
            let raw = std::fs::read_to_string(&wpath)?;
            let parsed = parse_json_or_toml(&raw)?;
            merge(&mut cfg, parsed);
        }
    }

    // env overrides (LOCARYN_*)
    apply_env(&mut cfg);

    Ok(cfg)
}

/// Parse a file that may be JSON or TOML (we accept both forms for ergonomics;
/// strict TOML parsing lands in V1.1). JSON is used as a forgiving superset
/// here via serde_json, which accepts `{...}` documents.
fn parse_json_or_toml(raw: &str) -> Result<Config, ConfigError> {
    // Try JSON first (the MVP writes JSON despite the .toml extension).
    if let Ok(c) = serde_json::from_str::<Config>(raw) {
        return Ok(c);
    }
    // Fallback: very small TOML-ish parser for the most common flat keys.
    // This is intentionally minimal; V1.1 swaps in the `toml` crate.
    let mut cfg = Config::default();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim().trim_matches('"'));
        match k {
            "mode" => {
                cfg.connection.mode = match v {
                    "auto" => ConnectionMode::Auto,
                    "remote" => ConnectionMode::Remote,
                    "local" => ConnectionMode::Local,
                    _ => cfg.connection.mode,
                };
            }
            "local_url" => cfg.connection.local_url = v.to_string(),
            "remote_url" => cfg.connection.remote_url = Some(v.to_string()),
            "port" => {
                if let Ok(p) = v.parse() {
                    cfg.daemon.port = p;
                }
            }
            _ => {}
        }
    }
    Ok(cfg)
}

fn merge(into: &mut Config, other: Config) {
    // Last-wins merge for the fields we care about.
    if other.connection.mode != default_mode() || into.connection.mode == default_mode() {
        into.connection.mode = other.connection.mode;
    }
    if other.connection.local_url != default_local_url() {
        into.connection.local_url = other.connection.local_url;
    }
    if other.connection.remote_url.is_some() {
        into.connection.remote_url = other.connection.remote_url;
    }
    if other.daemon.port != default_daemon_port() {
        into.daemon.port = other.daemon.port;
    }
    if other.daemon.bind != default_daemon_bind() {
        into.daemon.bind = other.daemon.bind;
    }
    if other.daemon.data_dir.is_some() {
        into.daemon.data_dir = other.daemon.data_dir;
    }
    if other.remote.is_some() {
        into.remote = other.remote;
    }
    if other.logging.level != default_log_level() {
        into.logging.level = other.logging.level;
    }
    into.logging.json |= other.logging.json;
}

fn apply_env(cfg: &mut Config) {
    if let Ok(v) = std::env::var("LOCARYN_MODE") {
        cfg.connection.mode = match v.as_str() {
            "auto" => ConnectionMode::Auto,
            "remote" => ConnectionMode::Remote,
            "local" => ConnectionMode::Local,
            _ => cfg.connection.mode,
        };
    }
    if let Ok(v) = std::env::var("LOCARYN_LOCAL_URL") {
        cfg.connection.local_url = v;
    }
    if let Ok(v) = std::env::var("LOCARYN_SERVER_URL") {
        cfg.connection.remote_url = Some(v);
    }
    if let Ok(v) = std::env::var("LOCARYN_TOKEN") {
        cfg.remote.get_or_insert_with(RemoteConfig::default).token = Some(v);
    }
    if let Ok(v) = std::env::var("LOCARYN_DAEMON_PORT") {
        if let Ok(p) = v.parse() {
            cfg.daemon.port = p;
        }
    }
    // The listening address decides whether authentication is required, and a
    // container or service unit configures it through the environment — every
    // other daemon setting had an override except this one.
    if let Ok(v) = std::env::var("LOCARYN_DAEMON_BIND") {
        let v = v.trim();
        if !v.is_empty() {
            // "bind" is a host, but the name reads like an address and people
            // write "127.0.0.1:7499". Accepting that costs three lines; the
            // alternative is the daemon refusing to start on "host:port:port"
            // with a message about socket syntax that names neither.
            match v.rsplit_once(':') {
                Some((host, port)) if port.parse::<u16>().is_ok() && !host.contains(':') => {
                    cfg.daemon.bind = host.to_string();
                    cfg.daemon.port = port.parse().unwrap();
                }
                _ => cfg.daemon.bind = v.to_string(),
            }
        }
    }
    if let Ok(v) = std::env::var("LOCARYN_TRAVEL") {
        let v = v.trim();
        // An empty value turns it off, so a service unit can unset it without
        // having to rewrite the configuration file.
        cfg.daemon.travel = (!v.is_empty() && v != "0" && v != "off").then(|| v.to_string());
    }
    if let Ok(v) = std::env::var("LOCARYN_TLS_CERT") {
        if !v.trim().is_empty() {
            cfg.daemon.tls_cert = Some(v.trim().to_string());
        }
    }
    if let Ok(v) = std::env::var("LOCARYN_TLS_KEY") {
        if !v.trim().is_empty() {
            cfg.daemon.tls_key = Some(v.trim().to_string());
        }
    }
    if let Ok(v) = std::env::var("LOCARYN_REQUIRE_CLIENT_CERT") {
        let v = v.trim().to_ascii_lowercase();
        cfg.daemon.require_client_cert = matches!(v.as_str(), "1" | "true" | "yes" | "on");
    }
    if let Ok(v) = std::env::var("LOCARYN_OPEN_ROUTER_PORT") {
        let v = v.trim().to_ascii_lowercase();
        cfg.daemon.open_router_port = matches!(v.as_str(), "1" | "true" | "yes" | "on");
    }
    if let Ok(v) = std::env::var("LOCARYN_DATA_DIR") {
        if !v.trim().is_empty() {
            cfg.daemon.data_dir = Some(PathBuf::from(v.trim()));
        }
    }
    if let Ok(v) = std::env::var("LOCARYN_LOG") {
        cfg.logging.level = v;
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    /// The env override is the one knob testable in-process: `global_dir()`
    /// resolves the real home directory, so the pointer-file path cannot be
    /// redirected without touching the user's actual configuration.
    ///
    /// Runs as one test because env vars are process-global; splitting it
    /// would let cargo's parallel runner interleave the mutations.
    #[test]
    fn storage_root_drives_every_bulky_path() {
        let fake = if cfg!(windows) {
            r"X:\locaryn-test"
        } else {
            "/tmp/locaryn-test"
        };

        // SAFETY: single-threaded within this test; no other test reads these.
        std::env::set_var("LOCARYN_STORAGE_ROOT", fake);
        std::env::remove_var("LOCARYN_MODELS_DIR");

        assert_eq!(storage_root(), PathBuf::from(fake));
        assert_eq!(models_dir(), PathBuf::from(fake).join("models"));
        assert_eq!(bin_dir(), PathBuf::from(fake).join("bin"));
        assert_eq!(temp_dir(), PathBuf::from(fake).join("locaryn_tmp"));
        assert_eq!(hf_cache_dir(), PathBuf::from(fake).join("hf_cache"));
        assert_eq!(free_chats_dir(), PathBuf::from(fake).join("free_chats"));

        // The database must NOT follow: it is small and held open at runtime.
        assert_eq!(default_data_dir(), global_dir().join("data"));
        assert!(!default_data_dir().starts_with(fake));

        // An explicit models override still wins over the root.
        let models_override = if cfg!(windows) {
            r"Y:\weights"
        } else {
            "/mnt/weights"
        };
        std::env::set_var("LOCARYN_MODELS_DIR", models_override);
        assert_eq!(models_dir(), PathBuf::from(models_override));

        std::env::remove_var("LOCARYN_MODELS_DIR");

        // An empty override must fall through rather than yield "".
        std::env::set_var("LOCARYN_STORAGE_ROOT", "");
        assert!(
            !storage_root().as_os_str().is_empty(),
            "empty override must fall through to the next source"
        );

        std::env::remove_var("LOCARYN_STORAGE_ROOT");

        // Back to the built-in default once the override is gone.
        assert_eq!(default_data_dir(), global_dir().join("data"));
    }
}

/// Find the executable a bare command name refers to.
///
/// Windows only, and not a nicety: nearly every MCP server is published as an
/// npm package and started with `npx`, which on Windows is `npx.cmd`.
/// `CreateProcess` does not apply `PATHEXT`, so spawning "npx" fails with
/// "program not found" on a machine where npx plainly works in a terminal.
pub fn resolve_program(command: &str) -> std::ffi::OsString {
    #[cfg(not(windows))]
    {
        std::ffi::OsString::from(command)
    }
    #[cfg(windows)]
    {
        use std::path::Path;
        // An explicit path is the caller's business; leave it alone.
        if command.contains('/') || command.contains('\\') {
            return std::ffi::OsString::from(command);
        }
        let exts: Vec<String> = std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect();
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                // An extension already present wins, as in a shell.
                let direct = dir.join(command);
                if direct.is_file() && Path::new(command).extension().is_some() {
                    return direct.into_os_string();
                }
                for ext in &exts {
                    let candidate = dir.join(format!("{command}{ext}"));
                    if candidate.is_file() {
                        return candidate.into_os_string();
                    }
                }
            }
        }
        // Not found: spawn it anyway so the operating system produces the
        // error, rather than inventing our own for a case we mis-detected.
        std::ffi::OsString::from(command)
    }
}

/// Whether a bare command name resolves to something runnable.
pub fn program_exists(command: &str) -> bool {
    let resolved = resolve_program(command);
    if std::path::Path::new(&resolved).is_file() {
        return true;
    }
    // On Unix the resolver hands the name back unchanged; look it up here.
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(command).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod program_tests {
    use super::*;

    #[test]
    fn a_bare_command_resolves_to_a_real_executable() {
        // The case that matters: almost every MCP server is started with npx,
        // which on Windows is npx.cmd and is invisible to CreateProcess.
        let resolved = resolve_program("npx");
        let s = resolved.to_string_lossy().to_lowercase();
        if cfg!(windows) {
            assert!(
                s.ends_with(".cmd") || s.ends_with(".exe") || s == "npx",
                "npx non résolu : {s}"
            );
        } else {
            assert_eq!(s, "npx");
        }
    }

    #[test]
    fn an_explicit_path_is_left_untouched() {
        let p = "C:/Program Files/graphify/serve.exe";
        assert_eq!(resolve_program(p).to_string_lossy(), p);
        assert_eq!(
            resolve_program("/usr/local/bin/serve").to_string_lossy(),
            "/usr/local/bin/serve"
        );
    }

    #[test]
    fn an_unknown_command_is_returned_as_is_and_reported_missing() {
        // Better the OS error than a guess of ours about why it is missing.
        assert_eq!(
            resolve_program("commande-qui-nexiste-pas-42").to_string_lossy(),
            "commande-qui-nexiste-pas-42"
        );
        assert!(!program_exists("commande-qui-nexiste-pas-42"));
    }
}
