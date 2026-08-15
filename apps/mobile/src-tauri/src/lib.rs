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

    let authority = p
        .authority_pem
        .clone()
        .filter(|a| a.contains("BEGIN CERTIFICATE"));
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
        return Err(format!(
            "Le serveur a refusé la connexion ({}).",
            resp.status()
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = body
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or("Le serveur n'a pas renvoyé de jeton.")?
        .to_string();

    let session = Session {
        key_id: server.key_id,
        username,
        token,
    };
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
        // Media generation (image, TTS) can run for a minute or two on the
        // machine at the other end; a chat-friendly timeout would cut it.
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())
}

/// The active server plus a live session, the two things every authenticated
/// call needs. Shared by chat and by media generation so the session logic
/// lives in one place.
fn authenticated() -> Result<(reqwest::Client, servers::KnownServer, Session), String> {
    let store = servers::load();
    let server = store
        .active_server()
        .ok_or("Aucun serveur enregistré sur cet appareil.")?
        .clone();
    let raw = std::fs::read_to_string(session_path())
        .map_err(|_| "Vous n'êtes pas connecté.".to_string())?;
    let session: Session = serde_json::from_str(&raw)
        .map_err(|_| "Session illisible ; reconnectez-vous.".to_string())?;
    let client = client_for(&server)?;
    Ok((client, server, session))
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

/// The hidden project that owns free chats, mirroring the desktop's constant.
const FREE_CHAT_PROJECT_PATH: &str = "__locaryn_free_chats__";

/// Find the free-chat project, creating it if needed.
///
/// The phone never shows projects or sessions — it just needs one stable
/// conversation bucket per server, and this is the same bucket the desktop
/// uses for chats that belong to no project.
async fn ensure_free_chat_project(
    client: &reqwest::Client,
    server: &servers::KnownServer,
    token: &str,
) -> Result<String, String> {
    let base = server.current_url.trim_end_matches('/');
    let list = client
        .get(format!("{base}/v1/projects"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| unreachable(server))?;
    if list.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Votre session a expiré. Reconnectez-vous.".into());
    }
    if !list.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            list.status()
        ));
    }
    let projects: Vec<serde_json::Value> = list.json().await.map_err(|e| e.to_string())?;
    if let Some(p) = projects
        .iter()
        .find(|p| p.get("path").and_then(|x| x.as_str()) == Some(FREE_CHAT_PROJECT_PATH))
    {
        return p
            .get("id")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .ok_or("Le serveur a renvoyé un projet sans identifiant.".into());
    }
    let created = client
        .post(format!("{base}/v1/projects"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "path": FREE_CHAT_PROJECT_PATH,
            "name": "Conversations libres",
            "trust_level": "sandbox",
        }))
        .send()
        .await
        .map_err(|_| unreachable(server))?;
    if !created.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            created.status()
        ));
    }
    let body: serde_json::Value = created.json().await.map_err(|e| e.to_string())?;
    body.get("id")
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .ok_or("Le serveur a renvoyé un projet sans identifiant.".into())
}

/// Send one message and return the reply.
///
/// Non-streaming on purpose for a first version: a phone that loses signal
/// mid-stream leaves a half-written answer on screen with no way to tell it
/// from a finished one. The reply is the concatenation of the `token` events
/// of the daemon's SSE stream.
#[tauri::command]
async fn send_message(text: String) -> Result<String, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/').to_string();

    let project_id = ensure_free_chat_project(&client, &server, &session.token).await?;

    let session_resp = client
        .post(format!("{base}/v1/projects/{project_id}/sessions"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !session_resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            session_resp.status()
        ));
    }
    let session_body: serde_json::Value = session_resp.json().await.map_err(|e| e.to_string())?;
    let session_id = session_body
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or("Le serveur a renvoyé une session sans identifiant.")?
        .to_string();

    let resp = client
        .post(format!("{base}/v1/sessions/{session_id}/messages"))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({ "content": text }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Votre session a expiré. Reconnectez-vous.".into());
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }

    // The daemon answers with a server-sent-events stream; keep the `token`
    // events, which carry the assistant's words as they are produced.
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let mut reply = String::new();
    for block in text.split("\n\n") {
        let Some(data) = block
            .lines()
            .find_map(|l| l.strip_prefix("data:").map(str::trim))
        else {
            continue;
        };
        let ev: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if ev.get("type").and_then(|t| t.as_str()) == Some("token") {
            if let Some(t) = ev.get("text").and_then(|t| t.as_str()) {
                reply.push_str(t);
            }
        }
    }
    Ok(reply)
}

// ============================================================================
// Media generation — image and speech, produced on the machine at the other
// end. The phone sends the prompt and receives the finished file as base64,
// which the webview can render or play without a file server.
// ============================================================================

/// A generated file, ready to show: the payload is base64 because the phone's
/// webview has no access to the server's disk.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MediaResult {
    pub name: String,
    pub mime: String,
    pub data_base64: String,
}

async fn fetch_media(route: &str, body: serde_json::Value) -> Result<MediaResult, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .post(format!("{base}{route}"))
        .bearer_auth(&session.token)
        .json(&body)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Votre session a expiré. Reconnectez-vous.".into());
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        // The daemon phrases its failures in `{ "error": { "message": … } }`.
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("Le serveur a refusé la demande ({status})."));
        return Err(message);
    }
    resp.json::<MediaResult>().await.map_err(|e| e.to_string())
}

/// Which models the machine can generate with. `kind` is "image" or "audio".
#[tauri::command]
async fn list_media_models(kind: String) -> Result<Vec<String>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .get(format!("{base}/v1/media/models?kind={kind}"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|e| format!("transport: {e}"))?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Votre session a expiré. Reconnectez-vous.".into());
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// Generate an image on the machine at the other end.
#[tauri::command]
async fn generate_image(
    model: String,
    prompt: String,
    negative_prompt: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<MediaResult, String> {
    fetch_media(
        "/v1/media/image",
        serde_json::json!({
            "model": model,
            "prompt": prompt,
            "negative_prompt": negative_prompt,
            "width": width,
            "height": height,
        }),
    )
    .await
}

/// Generate speech on the machine at the other end.
#[tauri::command]
async fn generate_audio(
    model: String,
    text: String,
    speed: Option<f32>,
    language: Option<String>,
) -> Result<MediaResult, String> {
    fetch_media(
        "/v1/media/audio",
        serde_json::json!({
            "model": model,
            "text": text,
            "speed": speed,
            "language": language,
        }),
    )
    .await
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
            list_media_models,
            generate_image,
            generate_audio,
            pairing::apply_pairing_link,
        ])
        .setup(|app| {
            // On Android the native process has no home directory (`$HOME` is
            // `/`), so `dirs::home_dir()` would point the whole data layout at
            // `/.locaryn` — read-only, and `servers::save` fails with
            // `Read-only file system`. Tauri knows the real per-app directory
            // (`filesDir`, writable); make it the home so `locaryn-config`
            // resolves `~/.locaryn` inside it.
            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                if let Ok(dir) = app.path().app_data_dir() {
                    std::env::set_var("HOME", &dir);
                }
            }
            #[cfg(not(target_os = "android"))]
            let _ = app;
            // A `locaryn://` link opened from outside — the camera app, a
            // message — arrives as a launch argument or a new-intent event.
            // The frontend asks for it rather than having it pushed, so a link
            // that arrives before the interface exists is not lost.
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erreur au démarrage de Locaryn");
}
