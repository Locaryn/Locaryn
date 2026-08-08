//! Locaryn on a phone.
//!
//! A thin client: the models, the accounts and the encryption all live on the
//! machine at the other end. What is here is a conversation view, a sign-in,
//! and the pairing flow that lets the address change without the user ever
//! seeing one.
//!
//! It shares the Rust that matters — `locaryn-travel` verifies scanned codes
//! here exactly as it signs them on the server, so the two cannot drift apart.

mod pairing;
mod servers;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MobileStatus {
    /// Name of the server in use, if any is registered.
    pub server_name: Option<String>,
    pub travelling: bool,
    /// Whether someone is signed in on this phone.
    pub signed_in: bool,
    /// How many servers this phone knows.
    pub servers: usize,
}

fn session_path() -> std::path::PathBuf {
    locaryn_config::default_data_dir().join("mobile-session.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    key_id: String,
    username: String,
    token: String,
}

#[tauri::command]
fn status() -> MobileStatus {
    let store = servers::load();
    let active = store.active_server();
    MobileStatus {
        server_name: active.map(|s| s.name.clone()),
        travelling: active.is_some_and(|s| s.travelling),
        signed_in: session_path().is_file(),
        servers: store.servers.len(),
    }
}

/// Register a server from the deployment file an administrator produced.
///
/// The phone gets the same file the desktop does, usually by scanning it or
/// receiving it; what matters is that it carries the authority, because that
/// is what makes every later scanned code verifiable.
#[tauri::command]
fn register_server(provisioning_json: String) -> Result<MobileStatus, String> {
    let p: locaryn_config::provision::Provisioning = serde_json::from_str(&provisioning_json)
        .map_err(|_| {
            "Ce fichier n'est pas une configuration Locaryn. Demandez-la à votre \
             administrateur."
                .to_string()
        })?;

    let authority = p.authority_pem.clone().filter(|a| a.contains("BEGIN CERTIFICATE"));
    let Some(authority) = authority else {
        return Err(
            "Cette configuration ne contient pas l'autorité du serveur, sans laquelle \
             les codes scannés ne peuvent pas être vérifiés. Regénérez-la avec une \
             version récente de Locaryn."
                .into(),
        );
    };
    let key_id = locaryn_travel::link::key_id(&authority).map_err(|e| e.to_string())?;

    let mut store = servers::load();
    store.upsert(servers::KnownServer {
        key_id,
        name: if p.organisation.trim().is_empty() {
            "Serveur".to_string()
        } else {
            p.organisation.clone()
        },
        home_url: p.server_url.clone(),
        current_url: p.server_url,
        authority_pem: authority,
        travelling: false,
    });
    servers::save(&store)?;
    Ok(status())
}

/// Sign in against whichever address is currently in force.
#[tauri::command]
async fn sign_in(username: String, password: String) -> Result<MobileStatus, String> {
    let store = servers::load();
    let server = store
        .active_server()
        .ok_or("Aucun serveur enregistré sur cet appareil.")?
        .clone();

    let client = reqwest::Client::builder()
        // The deployment's certificate is issued by its own authority, which
        // no phone trusts by default. The authority we stored is the anchor.
        .add_root_certificate(
            reqwest::Certificate::from_pem(server.authority_pem.as_bytes())
                .map_err(|e| format!("autorité illisible : {e}"))?,
        )
        // Servers reached through a relay answer on a hostname their own
        // certificate does not name; the authority still vouches for them.
        .danger_accept_invalid_hostnames(true)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post(format!("{}/v1/auth/login", server.current_url.trim_end_matches('/')))
        .json(&serde_json::json!({ "username": username, "password": password, "label": "téléphone" }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Identifiant ou mot de passe incorrect.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("Le serveur a refusé la connexion ({}).", resp.status()));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = body
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or("Le serveur n'a pas renvoyé de jeton.")?
        .to_string();

    let session = Session { key_id: server.key_id, username, token };
    std::fs::create_dir_all(locaryn_config::default_data_dir()).map_err(|e| e.to_string())?;
    std::fs::write(
        session_path(),
        serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("écriture : {e}"))?;
    Ok(status())
}

#[tauri::command]
fn sign_out() -> MobileStatus {
    let _ = std::fs::remove_file(session_path());
    status()
}

/// An HTTP client anchored on the deployment's own authority.
///
/// Shared by sign-in and by messaging so both trust exactly the same thing —
/// two anchors would eventually disagree, and the lenient one would win.
fn client_for(server: &servers::KnownServer) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        // The certificate is issued by the deployment's authority, which no
        // phone trusts by default. That authority is the anchor.
        .add_root_certificate(
            reqwest::Certificate::from_pem(server.authority_pem.as_bytes())
                .map_err(|e| format!("autorité illisible : {e}"))?,
        )
        // Reached through a relay, the server answers on a hostname its own
        // certificate does not name. The authority still vouches for it.
        .danger_accept_invalid_hostnames(true)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

fn unreachable(server: &servers::KnownServer) -> String {
    if server.travelling {
        "Le serveur ne répond pas. Le mode voyage a peut-être été coupé sur \
         l'ordinateur : scannez le code de retour."
            .to_string()
    } else {
        "Le serveur ne répond pas. Êtes-vous sur le même réseau que lui ?".to_string()
    }
}

/// Send one message and return the reply.
///
/// Non-streaming on purpose for a first version: a phone that loses signal
/// mid-stream leaves a half-written answer on screen with no way to tell it
/// from a finished one.
#[tauri::command]
async fn send_message(text: String) -> Result<String, String> {
    let store = servers::load();
    let server = store
        .active_server()
        .ok_or("Aucun serveur enregistré sur cet appareil.")?
        .clone();
    let raw = std::fs::read_to_string(session_path())
        .map_err(|_| "Vous n'êtes pas connecté.".to_string())?;
    let session: Session =
        serde_json::from_str(&raw).map_err(|_| "Session illisible ; reconnectez-vous.".to_string())?;

    let resp = client_for(&server)?
        .post(format!("{}/v1/chat", server.current_url.trim_end_matches('/')))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({ "message": text }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Votre session a expiré. Reconnectez-vous.".into());
    }
    if !resp.status().is_success() {
        return Err(format!("Le serveur a refusé la demande ({}).", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body
        .get("reply")
        .and_then(|r| r.as_str())
        .unwrap_or_default()
        .to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_deep_link::init());

    // The camera plugin only exists on a phone. Registering it unconditionally
    // would stop the desktop build, which is what the layout is developed on.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());

    builder
        .invoke_handler(tauri::generate_handler![
            status,
            register_server,
            sign_in,
            sign_out,
            send_message,
            pairing::apply_pairing_link,
        ])
        .setup(|_app| {
            // A `locaryn://` link opened from outside — the camera app, a
            // message — arrives as a launch argument or a new-intent event.
            // The frontend asks for it rather than having it pushed, so a link
            // that arrives before the interface exists is not lost.
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au démarrage de Locaryn");
}
