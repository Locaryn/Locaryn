//! Turning the desktop application into a shared server.
//!
//! The app does not serve HTTP itself: it supervises `lochor-daemon`, which
//! already carries the authentication, the TLS and the account model. Adding a
//! second HTTP implementation inside Tauri would mean two places to keep
//! correct, and the security-critical one would be the one nobody tested.
//!
//! So the checkbox starts a process, and everything the daemon guarantees —
//! authentication mandatory off loopback, TLS, refusing to start with no
//! account — applies unchanged.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

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
    let name = if cfg!(windows) { "lochor-daemon.exe" } else { "lochor-daemon" };
    // Beside the app when installed; in the build output during development.
    for candidate in [dir.join(name), dir.join("..").join(name)] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
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
    let path = lochor_config::default_data_dir()
        .join("tls")
        .join("daemon-cert.pem");
    let pem = std::fs::read_to_string(path).ok()?;
    lochor_config::provision::certificate_fingerprint(&pem)
}

async fn account_count() -> u32 {
    let db = lochor_config::default_data_dir().join("lochor.db");
    let Ok(pool) = lochor_storage::open(&db).await else {
        return 0;
    };
    lochor_storage::users::UserRepo::new(pool)
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
    let port = lochor_config::load(None).map(|c| c.daemon.port).unwrap_or(7474);
    let ip = local_address();

    let blocker = if daemon_binary().is_none() {
        Some(
            "Le service Lochor est introuvable à côté de l'application. \
             Réinstallez-la, ou lancez `lochor-daemon` manuellement."
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
        url: if running { format!("https://{ip}:{port}") } else { String::new() },
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
        {
            let mut guard = CHILD.lock().map_err(|_| "état du serveur illisible")?;
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
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

    let bin = daemon_binary().ok_or("service Lochor introuvable")?;
    let port = args.port.unwrap_or(status.port);
    let child = std::process::Command::new(&bin)
        // Exposing it is what makes the daemon demand authentication and TLS.
        .env("LOCHOR_DAEMON_BIND", "0.0.0.0")
        .env("LOCHOR_DAEMON_PORT", port.to_string())
        .env(
            "LOCHOR_DATA_DIR",
            lochor_config::default_data_dir().to_string_lossy().to_string(),
        )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("démarrage du service : {e}"))?;

    {
        let mut guard = CHILD.lock().map_err(|_| "état du serveur illisible")?;
        *guard = Some(child);
    }
    // Give it a moment to bind, so the first status reflects reality.
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    server_status().await
}

/// Settings an administrator hands to their users, if this machine has some.
#[tauri::command]
pub fn provisioning() -> Result<Option<lochor_config::provision::Provisioning>, String> {
    lochor_config::provision::load()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_address_is_always_produced() {
        let ip = local_address();
        assert!(!ip.is_empty());
        // Must parse: it goes straight into a URL shown to the user.
        assert!(ip.parse::<std::net::IpAddr>().is_ok(), "adresse invalide: {ip}");
    }

    #[tokio::test]
    async fn status_reports_a_blocker_rather_than_pretending_it_can_serve() {
        let s = server_status().await.expect("status");
        assert!(!s.running, "aucun serveur ne doit tourner au repos");
        // Either it is ready, or it says precisely what is missing — never
        // silently unavailable.
        if s.accounts == 0 {
            let b = s.blocker.expect("un blocage doit être signalé");
            assert!(b.contains("compte"), "message peu clair: {b}");
        }
    }
}
