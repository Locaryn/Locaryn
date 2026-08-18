//! Turning the desktop application into a shared server.
//!
//! The app does not serve HTTP itself: it supervises `locaryn-daemon`, which
//! already carries the authentication, the TLS and the account model. Adding a
//! second HTTP implementation inside Tauri would mean two places to keep
//! correct, and the security-critical one would be the one nobody tested.
//!
//! So the checkbox starts a process, and everything the daemon guarantees —
//! authentication mandatory off loopback, TLS, refusing to start with no
//! account — applies unchanged.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Stop the daemon owned by the desktop shell. This is intentionally the only
/// owner-facing shutdown path: closing or quitting Locaryn must not leave a
/// server behind listening on the user's machine.
pub fn stop_daemon() {
    if let Ok(mut guard) = CHILD.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The supervised daemon, if we started one.
static CHILD: Mutex<Option<std::process::Child>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ServerStatus {
    pub running: bool,
    /// Address it listens on, e.g. `0.0.0.0`.
    pub bind: String,
    pub port: u16,
    /// What clients should be given. Empty while stopped.
    pub url: String,
    /// Accounts on this machine. Zero means the daemon will refuse to expose.
    pub accounts: u32,
    /// Certificate fingerprint, once one exists.
    pub fingerprint: Option<String>,
    /// Why the server cannot start right now, if it cannot.
    pub blocker: Option<String>,
}

fn daemon_binary() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) {
        "locaryn-daemon.exe"
    } else {
        "locaryn-daemon"
    };
    // Beside the app when installed; in the build output during development.
    [dir.join(name), dir.join("..").join(name)]
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// Le journal du service, partagé avec `locaryn daemon logs`.
///
/// La sortie était jetée : quand le service refusait de démarrer — port déjà
/// pris, base illisible — l'écran affichait « arrêté » sans jamais dire
/// pourquoi, et il ne restait rien à lire pour le comprendre.
fn daemon_log() -> Option<std::process::Stdio> {
    let path = locaryn_config::global_dir().join("logs").join("daemon.log");
    std::fs::create_dir_all(path.parent()?).ok()?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    Some(std::process::Stdio::from(file))
}

/// Addresses this machine can be reached on, for the UI to display.
pub fn local_address() -> String {
    // No packet is sent: connecting a UDP socket only asks the OS which
    // interface it would route from.
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

fn read_fingerprint() -> Option<String> {
    let path = locaryn_config::default_data_dir()
        .join("tls")
        .join("daemon-cert.pem");
    let pem = std::fs::read_to_string(path).ok()?;
    locaryn_config::provision::certificate_fingerprint(&pem)
}

async fn account_count() -> u32 {
    let db = locaryn_config::default_data_dir().join("locaryn.db");
    let Ok(pool) = locaryn_storage::open(&db).await else {
        return 0;
    };
    locaryn_storage::users::UserRepo::new(pool)
        .count()
        .await
        .unwrap_or(0)
        .max(0) as u32
}

#[tauri::command]
pub async fn server_status() -> Result<ServerStatus, String> {
    let running = {
        let mut guard = CHILD.lock().map_err(|_| "état du serveur illisible")?;
        let r = match guard.as_mut() {
            // `try_wait` reaps the process if it exited on its own, so the UI
            // never claims to be serving after a crash.
            Some(child) => match child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    *guard = None;
                    false
                }
                Ok(None) => true,
            },
            None => false,
        };
        r
    };

    let accounts = account_count().await;
    let port = locaryn_config::load(None)
        .map(|c| c.daemon.port)
        .unwrap_or(7474);
    let ip = local_address();

    let blocker = if daemon_binary().is_none() {
        Some(
            "Le service Locaryn est introuvable à côté de l'application. \
             Réinstallez-la, ou lancez `locaryn-daemon` manuellement."
                .to_string(),
        )
    } else if accounts == 0 {
        Some(
            "Aucun compte n'existe. Un serveur accessible sans compte serait ouvert \
             à tous : créez d'abord un administrateur."
                .to_string(),
        )
    } else {
        None
    };

    Ok(ServerStatus {
        running,
        bind: "0.0.0.0".to_string(),
        port,
        url: if running {
            format!("https://{ip}:{port}")
        } else {
            String::new()
        },
        accounts,
        fingerprint: read_fingerprint(),
        blocker,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetServerArgs {
    pub enabled: bool,
    #[serde(default)]
    pub port: Option<u16>,
}

#[tauri::command]
pub async fn set_server_mode(args: SetServerArgs) -> Result<ServerStatus, String> {
    if !args.enabled {
        // The guard is dropped before the await: a std MutexGuard held across
        // one makes the whole future non-Send, which Tauri commands must be.
        stop_daemon();
        return server_status().await;
    }

    // Re-check rather than trust the UI: the account could have been removed
    // since the screen was drawn.
    let status = server_status().await?;
    if let Some(blocker) = status.blocker {
        return Err(blocker);
    }
    if status.running {
        return Ok(status);
    }

    let bin = daemon_binary().ok_or("service Locaryn introuvable")?;
    let port = args.port.unwrap_or(status.port);
    let mut command = std::process::Command::new(&bin);
    command
        // Exposing it is what makes the daemon demand authentication and TLS.
        .env("LOCARYN_DAEMON_BIND", "0.0.0.0")
        .env("LOCARYN_DAEMON_PORT", port.to_string())
        .env(
            "LOCARYN_DATA_DIR",
            locaryn_config::default_data_dir()
                .to_string_lossy()
                .to_string(),
        )
        .stdin(std::process::Stdio::null())
        .stdout(daemon_log().unwrap_or_else(std::process::Stdio::null))
        .stderr(daemon_log().unwrap_or_else(std::process::Stdio::null));

    // `locaryn-daemon` is a console binary. When the desktop starts it, the
    // child must stay a private implementation detail rather than opening a
    // second foreground CMD window on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let child = command
        .spawn()
        .map_err(|e| format!("démarrage du service : {e}"))?;

    #[cfg(windows)]
    win_job::attach_child_to_job(&child);

    {
        let mut guard = CHILD.lock().map_err(|_| "état du serveur illisible")?;
        *guard = Some(child);
    }
    // Give it a moment to bind, so the first status reflects reality.
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    server_status().await
}

#[cfg(windows)]
mod win_job {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct IO_COUNTERS {
        ReadOperationCount: u64,
        WriteOperationCount: u64,
        OtherOperationCount: u64,
        ReadTransferCount: u64,
        WriteTransferCount: u64,
        OtherTransferCount: u64,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        PerProcessUserTimeLimit: i64,
        PerJobUserTimeLimit: i64,
        LimitFlags: u32,
        MinimumWorkingSetSize: usize,
        MaximumWorkingSetSize: usize,
        ActiveProcessLimit: u32,
        Affinity: usize,
        PriorityClass: u32,
        SchedulingClass: u32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        IoInfo: IO_COUNTERS,
        ProcessMemoryLimit: usize,
        JobMemoryLimit: usize,
        PeakProcessMemoryLimit: usize,
        PeakJobMemoryLimit: usize,
    }

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;

    type HANDLE = *mut std::ffi::c_void;
    type BOOL = i32;

    extern "system" {
        fn CreateJobObjectW(
            lpJobAttributes: *const std::ffi::c_void,
            lpName: *const u16,
        ) -> HANDLE;
        fn SetInformationJobObject(
            hJob: HANDLE,
            JobObjectInformationClass: u32,
            lpJobObjectInformation: *const std::ffi::c_void,
            cbJobObjectInformationLength: u32,
        ) -> BOOL;
        fn AssignProcessToJobObject(hJob: HANDLE, hProcess: HANDLE) -> BOOL;
        fn CloseHandle(hObject: HANDLE) -> BOOL;
    }

    struct SafeJobHandle(HANDLE);
    unsafe impl Send for SafeJobHandle {}
    unsafe impl Sync for SafeJobHandle {}

    impl Drop for SafeJobHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    static JOB: OnceLock<SafeJobHandle> = OnceLock::new();

    fn get_job_object() -> Option<HANDLE> {
        let job = JOB.get_or_init(|| {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    tracing::warn!("impossible de créer le Windows Job Object pour le daemon");
                    return SafeJobHandle(std::ptr::null_mut());
                }

                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                let res = SetInformationJobObject(
                    handle,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );

                if res == 0 {
                    tracing::warn!("impossible de configurer JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE");
                    CloseHandle(handle);
                    return SafeJobHandle(std::ptr::null_mut());
                }

                SafeJobHandle(handle)
            }
        });

        if job.0.is_null() {
            None
        } else {
            Some(job.0)
        }
    }

    /// Associe le processus enfant au Windows Job Object.
    ///
    /// Grâce au flag `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, le noyau Windows tue
    /// automatiquement tous les processus rattachés dès que le processus parent
    /// se termine ou plante brutalement.
    pub fn attach_child_to_job(child: &std::process::Child) {
        if let Some(job) = get_job_object() {
            let process_handle = child.as_raw_handle() as HANDLE;
            unsafe {
                let res = AssignProcessToJobObject(job, process_handle);
                if res == 0 {
                    tracing::warn!("échec de l'assignation du daemon au Windows Job Object");
                }
            }
        }
    }
}

/// Restarts the daemon server. If stopped, starts it up.
#[tauri::command]
pub async fn restart_server() -> Result<ServerStatus, String> {
    let status = server_status().await?;
    let port = status.port;
    stop_daemon();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    set_server_mode(SetServerArgs {
        enabled: true,
        port: Some(port),
    })
    .await
}

/// Settings an administrator hands to their users, if this machine has some.
#[tauri::command]
pub fn provisioning() -> Result<Option<locaryn_config::provision::Provisioning>, String> {
    locaryn_config::provision::load()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_address_is_always_produced() {
        let ip = local_address();
        assert!(!ip.is_empty());
        // Must parse: it goes straight into a URL shown to the user.
        assert!(
            ip.parse::<std::net::IpAddr>().is_ok(),
            "adresse invalide: {ip}"
        );
    }

    #[tokio::test]
    async fn status_reports_a_blocker_rather_than_pretending_it_can_serve() {
        let s = server_status().await.expect("status");
        assert!(!s.running, "aucun serveur ne doit tourner au repos");
        // Either it is ready, or it says precisely what is missing — never
        // silently unavailable.
        //
        // Deux empêchements coexistent, et lequel se présente dépend de la
        // machine : pas de compte administrateur, ou pas de binaire de service
        // à côté de l'application. Exiger le premier faisait échouer le test
        // partout où le second arrive d'abord — un poste de développement dont
        // le démon n'est pas encore construit, par exemple. Ce qui compte est
        // l'engagement réel : jamais indisponible sans le dire.
        if s.accounts == 0 {
            let b = s.blocker.expect("un blocage doit être signalé");
            assert!(
                b.contains("compte") || b.contains("service"),
                "le blocage doit nommer ce qui manque, or: {b}"
            );
            assert!(
                b.len() > 20,
                "message trop court pour être actionnable: {b}"
            );
        }
    }
}
