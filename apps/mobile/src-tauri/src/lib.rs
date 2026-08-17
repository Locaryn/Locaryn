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
mod update;

use locaryn_shared_types::base64_encode;
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

/// Ce que les extensions actives du serveur apportent.
///
/// Le téléphone n'héberge rien : c'est le serveur qui dit ce qu'il sait faire.
/// Installer la génération d'images là-bas fait apparaître le Studio ici, la
/// retirer le fait disparaître.
#[tauri::command]
async fn server_capabilities() -> Vec<String> {
    let store = servers::load();
    let Some(server) = store.active_server() else {
        return Vec::new();
    };
    // `current_url` suit le mode voyage : c'est l'adresse par laquelle le
    // téléphone joint effectivement le serveur en ce moment.
    let url = format!("{}/v1/extensions", server.current_url.trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .build()
    else {
        return Vec::new();
    };
    let Ok(resp) = client.get(url).send().await else {
        return Vec::new();
    };
    let Ok(list) = resp.json::<Vec<serde_json::Value>>().await else {
        return Vec::new();
    };
    let mut caps: Vec<String> = list
        .iter()
        .filter(|e| e["enabled"] == true)
        .filter_map(|e| e["capabilities"].as_array())
        .flatten()
        .filter_map(|c| c.as_str().map(str::to_string))
        .collect();
    caps.sort();
    caps.dedup();
    caps
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
        access_mode: p.access_mode.clone().unwrap_or_default(),
    });
    servers::save(&store)?;
    Ok(status())
}

/// Ce que l'utilisateur a tapé, ramené à une URL utilisable.
///
/// On accepte ce qu'une personne écrit vraiment : « 192.168.1.20 »,
/// « 192.168.1.20:7474 », « http://serveur.local », « locaryn.maison:7474/ ».
/// Le port par défaut est celui du service ; le schéma par défaut est `http`,
/// parce qu'une adresse tapée à la main désigne presque toujours une machine du
/// réseau local qui n'a pas de certificat.
pub fn normalise_address(raw: &str) -> Result<String, String> {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Err("Entrez l'adresse du serveur.".into());
    }
    let (scheme, rest) = match raw.split_once("://") {
        Some(("http", r)) => ("http", r),
        Some(("https", r)) => ("https", r),
        Some((other, _)) => return Err(format!("Adresse inattendue : « {other}:// ».")),
        None => ("http", raw),
    };
    let rest = rest.trim_end_matches('/');
    if rest.is_empty() || rest.contains(' ') {
        return Err("Cette adresse n'est pas valide.".into());
    }
    // Un port déjà écrit est gardé tel quel ; sinon on ajoute celui du service.
    // Le test cherche un « : » suivi de chiffres pour ne pas confondre avec le
    // séparateur d'une adresse IPv6 entre crochets.
    let has_port = rest
        .rsplit_once(':')
        .is_some_and(|(_, p)| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if has_port {
        Ok(format!("{scheme}://{rest}"))
    } else {
        Ok(format!("{scheme}://{rest}:7474"))
    }
}

/// Enregistrer un serveur à partir de son adresse, sans code à scanner.
///
/// C'est le chemin pour un téléphone sans appareil photo, ou pour un serveur
/// personnel sur le réseau local. Ce qu'on perd par rapport au code scanné est
/// réel et l'interface le dit : le code, lui, porte le certificat de
/// l'autorité, et c'est ce certificat qui permet ensuite de vérifier un lien de
/// mode voyage et de chiffrer la liaison hors du réseau local.
///
/// L'adresse n'est pas enregistrée sur parole : on demande d'abord au serveur
/// de se présenter.
#[tauri::command]
async fn register_address(address: String) -> Result<MobileStatus, String> {
    let url = normalise_address(&address)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(format!("{url}/v1/info"))
        .send()
        .await
        .map_err(|_| format!("Aucune réponse de {url}. Vérifiez l'adresse et le réseau."))?;
    if !resp.status().is_success() {
        return Err(format!("{url} a répondu {}.", resp.status()));
    }
    let info: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| format!("{url} répond, mais ce n'est pas un serveur Locaryn."))?;
    if info["name"].as_str() != Some("locaryn-daemon") {
        return Err(format!(
            "{url} répond, mais ce n'est pas un serveur Locaryn."
        ));
    }

    let mut store = servers::load();
    store.upsert(servers::KnownServer {
        // Pas d'autorité, donc pas d'empreinte d'autorité : l'adresse elle-même
        // identifie ce serveur. Le préfixe évite toute collision avec les
        // identifiants dérivés d'un certificat.
        key_id: format!("adresse:{url}"),
        name: url
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .to_string(),
        home_url: url.clone(),
        current_url: url,
        authority_pem: String::new(),
        travelling: false,
        // Une adresse tapée à la main ne dit pas par quel chemin elle passe :
        // on ne le devine pas, on laisse vide.
        access_mode: String::new(),
    });
    servers::save(&store)?;
    Ok(status())
}

/// Reprendre le serveur déjà connu à une nouvelle adresse, après un échec.
///
/// Ce n'est pas un nouvel appairage : c'est le **même** serveur, qui a changé
/// d'adresse — une box qui a redémarré, un tunnel qui a expiré. `register_address`
/// créerait une seconde entrée, sans autorité et sans session, et la
/// personne devrait tout resaisir. Ici, seule `current_url` (et `home_url` si le
/// téléphone n'est pas en voyage) change ; l'autorité et la session restent —
/// c'est ce qui garde le compte connecté et l'historique en place.
#[tauri::command]
async fn reconnect_active_server(address: String) -> Result<MobileStatus, String> {
    let url = normalise_address(&address)?;

    let mut store = servers::load();
    let actif = store
        .active
        .clone()
        .ok_or("Aucun serveur enregistré sur cet appareil.")?;
    let Some(serveur) = store.get_mut(&actif) else {
        return Err("Aucun serveur enregistré sur cet appareil.".into());
    };

    // Vérifié à la nouvelle adresse, avec l'autorité déjà connue de ce
    // serveur — c'est elle qui distingue ce serveur d'un autre qui
    // répondrait par hasard sur la même adresse.
    let mut sonde = serveur.clone();
    sonde.current_url = url.clone();
    let client = client_for(&sonde)?;
    let resp = client
        .get(format!("{url}/v1/info"))
        .send()
        .await
        .map_err(|_| format!("Aucune réponse de {url}. Vérifiez l'adresse et le réseau."))?;
    if !resp.status().is_success() {
        return Err(format!("{url} a répondu {}.", resp.status()));
    }
    let info: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| format!("{url} répond, mais ce n'est pas un serveur Locaryn."))?;
    if info["name"].as_str() != Some("locaryn-daemon") {
        return Err(format!(
            "{url} répond, mais ce n'est pas un serveur Locaryn."
        ));
    }

    serveur.current_url = url.clone();
    if !serveur.travelling {
        serveur.home_url = url;
    }
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

    let client = client_for_with(&server, std::time::Duration::from_secs(20))?;

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
    // Media generation (image, TTS) can run for a minute or two on the machine
    // at the other end; a chat-friendly timeout would cut it.
    client_for_with(server, std::time::Duration::from_secs(180))
}

fn client_for_with(
    server: &servers::KnownServer,
    timeout: std::time::Duration,
) -> Result<reqwest::Client, String> {
    let builder = reqwest::Client::builder().timeout(timeout);

    // Une adresse tapée à la main n'apporte aucune autorité : c'est un serveur
    // joint en clair sur le réseau local, ou en HTTPS avec un certificat que le
    // téléphone sait déjà vérifier. Rien à ancrer, donc rien à ajouter — et
    // surtout pas une exception de vérification qu'on n'aurait pas demandée.
    if server.authority_pem.trim().is_empty() {
        return builder.build().map_err(|e| e.to_string());
    }

    builder
        // The certificate is issued by the deployment's authority, which no
        // phone trusts by default. That authority is the anchor.
        .add_root_certificate(
            reqwest::Certificate::from_pem(server.authority_pem.as_bytes())
                .map_err(|e| format!("autorité illisible : {e}"))?,
        )
        // Reached through a relay, the server answers on a hostname its own
        // certificate does not name. The authority still vouches for it.
        .danger_accept_invalid_hostnames(true)
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
async fn send_message(
    text: String,
    conversation_id: Option<String>,
    ephemeral: Option<bool>,
) -> Result<ChatReply, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/').to_string();

    // Une conversation existante est reprise ; sinon on en ouvre une.
    //
    // Le téléphone en créait une neuve à *chaque* message : le modèle
    // repartait de zéro à chaque phrase, et rien de ce qui était dit ici
    // n'apparaissait sur l'ordinateur. C'est la même conversation, sur le même
    // serveur, quel que soit l'appareil qui la continue.
    let session_id = match conversation_id {
        Some(id) if !id.trim().is_empty() => id,
        _ => {
            let project_id = ensure_free_chat_project(&client, &server, &session.token).await?;
            let session_resp = client
                .post(format!("{base}/v1/projects/{project_id}/sessions"))
                .bearer_auth(&session.token)
                // Éphémère : le serveur ne lui donnera pas de titre et ne la
                // fera pas apparaître dans les listes.
                .json(&serde_json::json!({ "ephemeral": ephemeral.unwrap_or(false) }))
                .send()
                .await
                .map_err(|_| unreachable(&server))?;
            if !session_resp.status().is_success() {
                return Err(format!(
                    "Le serveur a refusé la demande ({}).",
                    session_resp.status()
                ));
            }
            let body: serde_json::Value = session_resp.json().await.map_err(|e| e.to_string())?;
            body.get("id")
                .and_then(|x| x.as_str())
                .ok_or("Le serveur a renvoyé une session sans identifiant.")?
                .to_string()
        }
    };

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
    // events, which carry the assistant's words as they are produced — et les
    // `artifact`, qui disent qu'un outil a produit un fichier. Sans eux, une
    // image demandée dans le chat n'arrivait jamais jusqu'au téléphone : le
    // modèle répondait « voilà l'image » et il n'y avait rien à voir.
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let mut reply = String::new();
    let mut artifact_ids: Vec<String> = Vec::new();
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
        match ev.get("type").and_then(|t| t.as_str()) {
            Some("token") => {
                if let Some(t) = ev.get("text").and_then(|t| t.as_str()) {
                    reply.push_str(t);
                }
            }
            Some("artifact") => {
                let kind = ev.get("kind").and_then(|k| k.as_str()).unwrap_or_default();
                // `image_png` aujourd'hui ; le préfixe couvre les formats que
                // le service pourra produire plus tard sans casser ici.
                if kind.starts_with("image") {
                    if let Some(id) = ev.get("artifact_id").and_then(|i| i.as_str()) {
                        artifact_ids.push(id.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    // Les octets ne voyagent pas dans le flux : on va les chercher un par un.
    // Un artefact qu'on n'arrive pas à récupérer n'est pas une raison de
    // perdre la réponse écrite, donc on le passe.
    let mut images = Vec::new();
    for id in artifact_ids {
        if let Ok(media) = fetch_artifact(&client, &base, &session.token, &id).await {
            images.push(media);
        }
    }

    Ok(ChatReply {
        text: reply,
        images,
        conversation_id: session_id,
    })
}

/// Une conversation du serveur, telle que le téléphone la liste.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Conversation {
    pub id: String,
    /// Le titre donné par le serveur, ou les premiers mots échangés.
    pub title: String,
    pub last_message_at: Option<String>,
}

/// Les conversations libres du serveur, la plus récente d'abord.
///
/// Elles viennent du serveur et pas d'une copie locale : c'est ce qui permet
/// de commencer une conversation sur l'ordinateur et de la continuer sur le
/// téléphone, ou l'inverse.
#[tauri::command]
async fn list_conversations() -> Result<Vec<Conversation>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/').to_string();
    let project_id = ensure_free_chat_project(&client, &server, &session.token).await?;

    let resp = client
        .get(format!("{base}/v1/projects/{project_id}/sessions"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    let brut: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;

    Ok(conversations_depuis(&brut))
}

/// Des sessions du serveur aux conversations que le téléphone affiche.
///
/// La plus récente en tête : c'est celle qu'on reprend neuf fois sur dix.
fn conversations_depuis(brut: &[serde_json::Value]) -> Vec<Conversation> {
    let mut out: Vec<Conversation> = brut
        .iter()
        .filter_map(|v| {
            let id = v.get("id")?.as_str()?.to_string();
            let title = v
                .get("title")
                .and_then(|t| t.as_str())
                .filter(|t| !t.trim().is_empty())
                .unwrap_or("Conversation")
                .to_string();
            Some(Conversation {
                id,
                title,
                last_message_at: v
                    .get("last_message_at")
                    .and_then(|t| t.as_str())
                    .map(str::to_string),
            })
        })
        .collect();
    out.sort_by(|a, b| b.last_message_at.cmp(&a.last_message_at));
    out
}

/// Un projet du serveur, tel que le téléphone le liste.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PhoneProject {
    pub id: String,
    pub name: String,
}

/// Les projets ouverts sur le serveur.
///
/// Le conteneur des conversations libres n'en est pas un : il n'existe que
/// pour héberger ce qui n'appartient à aucun projet, et l'afficher comme un
/// projet serait mentir sur l'organisation de quelqu'un.
#[tauri::command]
async fn list_projects() -> Result<Vec<PhoneProject>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/').to_string();
    let resp = client
        .get(format!("{base}/v1/projects"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    let brut: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(brut
        .iter()
        .filter(|p| p.get("path").and_then(|x| x.as_str()) != Some(FREE_CHAT_PROJECT_PATH))
        .filter_map(|p| {
            Some(PhoneProject {
                id: p.get("id")?.as_str()?.to_string(),
                name: p.get("name")?.as_str()?.to_string(),
            })
        })
        .collect())
}

/// Les conversations d'un projet donné.
#[tauri::command]
async fn list_project_conversations(project_id: String) -> Result<Vec<Conversation>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/').to_string();
    let resp = client
        .get(format!("{base}/v1/projects/{project_id}/sessions"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    let brut: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(conversations_depuis(&brut))
}

/// Le contenu d'une conversation, pour la reprendre là où elle en était.
#[tauri::command]
async fn load_conversation(id: String) -> Result<Vec<ChatTurn>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/').to_string();
    let resp = client
        .get(format!("{base}/v1/sessions/{id}/messages"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    let brut: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;
    Ok(brut
        .iter()
        .filter_map(|m| {
            let role = m.get("role")?.as_str()?;
            // Les tours d'outil ne se lisent pas : ils font partie du travail,
            // pas de la conversation.
            if role != "user" && role != "assistant" {
                return None;
            }
            let content = m.get("content")?.as_str()?.trim().to_string();
            if content.is_empty() {
                return None;
            }
            Some(ChatTurn {
                id: m.get("id")?.as_str()?.to_string(),
                role: role.to_string(),
                content,
            })
        })
        .collect())
}

/// Un tour déjà écrit, relu depuis le serveur.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatTurn {
    pub id: String,
    pub role: String,
    pub content: String,
}

/// Ce qu'un tour de conversation rapporte : les mots, et ce que les outils ont
/// produit en chemin.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatReply {
    pub text: String,
    pub images: Vec<MediaResult>,
    /// La conversation dans laquelle ce tour a eu lieu. Le téléphone la garde
    /// pour continuer au lieu d'en ouvrir une autre au message suivant.
    pub conversation_id: String,
}

/// Télécharger un artefact et le rendre affichable par la vue web.
async fn fetch_artifact(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    id: &str,
) -> Result<MediaResult, String> {
    let resp = client
        .get(format!("{base}/v1/artifacts/{id}/raw"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("artefact {id} : {}", resp.status()));
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(MediaResult {
        name: format!("{id}.png"),
        mime,
        data_base64: base64_encode(&bytes),
    })
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

// ============================================================================
// Extensions — installées sur le serveur, pilotées d'ici. Ce qu'une extension
// apporte apparaît ensuite dans le Studio, sur le téléphone comme ailleurs.
// ============================================================================

/// Une extension du serveur, réduite à ce que le téléphone en montre.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhoneExtension {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Ce que l'extension ajoute à l'interface. Le téléphone en affiche deux
    /// choses : les boutons près du champ de saisie et les sections de
    /// réglages. Une extension de dictée doit poser son micro ici aussi —
    /// c'est même le téléphone qui en a le plus besoin.
    #[serde(default)]
    pub ui: ExtensionUi,
}

/// Une figure du serveur, réduite à ce que le téléphone en montre.
///
/// Une figure est un rôle et un agencement : ce que le modèle reçoit avant
/// toute conversation (les consignes), avec quel modèle, quelle phrase
/// d'ouverture, et si elle lit la mémoire de l'utilisateur.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhoneFigure {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Ce que le modèle reçoit avant toute conversation. C'est le cœur.
    pub instructions: String,
    /// Le modèle qui la fait tourner. Absent : celui de l'application.
    #[serde(default)]
    pub model: Option<String>,
    /// Une première phrase, proposée à l'ouverture.
    #[serde(default)]
    pub opening: Option<String>,
    /// Vrai quand la figure lit la mémoire de l'utilisateur.
    #[serde(default)]
    pub uses_memory: bool,
    /// Les outils qu'elle a le droit d'appeler. Absents : tout ce que
    /// l'application propose.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

/// Ce qu'une extension ajoute à l'interface du téléphone.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ExtensionUi {
    #[serde(default)]
    pub composer_actions: Vec<ComposerAction>,
    #[serde(default)]
    pub settings_sections: Vec<SettingsSection>,
}

/// Un bouton posé à côté du champ de saisie.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComposerAction {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// `insert` écrit un texte dans le champ ; `tool` appelle un outil.
    pub action: String,
    pub value: String,
    #[serde(default)]
    pub hint: Option<String>,
}

/// Une section de réglages apportée par une extension.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SettingsSection {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<SettingsField>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SettingsField {
    pub key: String,
    pub label: String,
    pub kind: String,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
}

// Figures — un rôle, ses consignes, ses conversations. L'écran n'existe que
// si le serveur a une extension qui apporte la capacité `figures` ; ce qui
// suit est le pilotage. Les figures vivent sur le serveur : le téléphone ne
// fait que les lire et les confier à des conversations.

/// Toutes les figures du serveur.
#[tauri::command]
async fn list_figures() -> Result<Vec<PhoneFigure>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .get(format!("{base}/v1/figures"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    resp.json::<Vec<PhoneFigure>>()
        .await
        .map_err(|e| e.to_string())
}

/// Créer une figure, ou remplacer celle du même nom.
#[tauri::command]
async fn save_figure(
    name: String,
    description: Option<String>,
    instructions: String,
    model: Option<String>,
    opening: Option<String>,
    uses_memory: Option<bool>,
    tools: Option<Vec<String>>,
) -> Result<PhoneFigure, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .post(format!("{base}/v1/figures"))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({
            "name": name,
            "description": description.unwrap_or_default(),
            "instructions": instructions,
            "model": model,
            "opening": opening,
            "uses_memory": uses_memory.unwrap_or(false),
            "tools": tools,
        }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    resp.json::<PhoneFigure>().await.map_err(|e| e.to_string())
}

/// Retirer une figure. Ses conversations restent, détachées.
#[tauri::command]
async fn delete_figure(id: String) -> Result<(), String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .delete(format!("{base}/v1/figures/{id}"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    Ok(())
}

/// Ouvrir une conversation tenue par une figure : une session neuve, confiée
/// d'emblée. Ses consignes seront versées au prompt de chacun de ses tours.
#[tauri::command]
async fn start_figure_chat(figure_id: String) -> Result<String, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/').to_string();
    let project_id = ensure_free_chat_project(&client, &server, &session.token).await?;
    let session_resp = client
        .post(format!("{base}/v1/projects/{project_id}/sessions"))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({ "ephemeral": false }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !session_resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            session_resp.status()
        ));
    }
    let body: serde_json::Value = session_resp.json().await.map_err(|e| e.to_string())?;
    let session_id = body
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or("Le serveur a renvoyé une session sans identifiant.")?
        .to_string();

    let attach = client
        .post(format!("{base}/v1/sessions/{session_id}/figure"))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({ "figure_id": figure_id }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !attach.status().is_success() {
        return Err(format!(
            "Le serveur a refusé d'attacher la figure ({}).",
            attach.status()
        ));
    }
    Ok(session_id)
}

/// Une conversation d'une figure, telle que l'écran la liste.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhoneFigureSession {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub last_message_at: Option<String>,
}

/// Les conversations d'une figure, la plus récente d'abord.
#[tauri::command]
async fn figure_sessions(figure_id: String) -> Result<Vec<PhoneFigureSession>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .get(format!("{base}/v1/figures/{figure_id}/sessions"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    resp.json::<Vec<PhoneFigureSession>>()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_extensions() -> Result<Vec<PhoneExtension>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .get(format!("{base}/v1/extensions"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    resp.json::<Vec<PhoneExtension>>()
        .await
        .map_err(|e| e.to_string())
}

/// Appeler l'outil qu'un bouton d'extension désigne.
///
/// Le bouton nomme un outil, pas un serveur : c'est le serveur qui cherche
/// lequel de ses serveurs d'extensions le porte. Le téléphone ne fait que
/// transmettre ce que contient le champ de saisie, et rendre le texte obtenu.
#[tauri::command]
async fn run_composer_tool(tool: String, text: String) -> Result<String, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .post(format!("{base}/v1/tools/{tool}"))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    let statut = resp.status();
    let corps: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !statut.is_success() {
        // Le serveur explique pourquoi — outil inconnu, extension éteinte.
        // Répéter son message vaut mieux qu'un code d'erreur.
        return Err(corps
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .unwrap_or("L'outil a échoué.")
            .to_string());
    }
    Ok(corps
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string())
}

/// Les réglages déclarés par les extensions, tels qu'ils sont sur le serveur.
///
/// Les clés sont `extension.champ`. Le téléphone n'en garde pas de copie :
/// c'est le serveur qui exécutera, c'est lui qui fait foi.
#[tauri::command]
async fn extension_config() -> Result<std::collections::HashMap<String, String>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .get(format!("{base}/v1/extensions/config"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Ok(std::collections::HashMap::new());
    }
    Ok(resp.json().await.unwrap_or_default())
}

/// Écrire un réglage d'extension. Une valeur vide l'efface.
#[tauri::command]
async fn set_extension_config(extension: String, key: String, value: String) -> Result<(), String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .post(format!("{base}/v1/extensions/{extension}/config"))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({ "key": key, "value": value }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if resp.status().is_success() {
        return Ok(());
    }
    Err(format!(
        "Le serveur a refusé la demande ({}).",
        resp.status()
    ))
}

/// Les modèles installés sur le serveur, pour les réglages qui en demandent un.
#[tauri::command]
async fn list_models() -> Result<Vec<String>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    // La même liste que celle du modèle des micro-tâches : ce sont les modèles
    // de conversation installés, et il n'y a pas de raison d'en tenir deux.
    let resp = client
        .get(format!("{base}/v1/assistance/micro-model"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    let corps: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(corps
        .get("available")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// Installer une extension du catalogue. `source` est un `propriétaire/dépôt`.
#[tauri::command]
async fn install_extension(source: String) -> Result<PhoneExtension, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .post(format!("{base}/v1/extensions/install"))
        .bearer_auth(&session.token)
        // Le téléchargement se fait sur le serveur ; il peut prendre du temps.
        .timeout(std::time::Duration::from_secs(180))
        .json(&serde_json::json!({ "source": source, "scope": "user" }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let corps = resp.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&corps)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("Le serveur a refusé l'installation ({status})."));
        return Err(message);
    }
    resp.json::<PhoneExtension>()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_extension_enabled(name: String, enabled: bool) -> Result<(), String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let verbe = if enabled { "enable" } else { "disable" };
    let resp = client
        .post(format!("{base}/v1/extensions/{name}/{verbe}"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!("Le serveur a refusé ({}).", resp.status()));
    }
    Ok(())
}

#[tauri::command]
async fn remove_extension(name: String) -> Result<(), String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .delete(format!("{base}/v1/extensions/{name}"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!("Le serveur a refusé ({}).", resp.status()));
    }
    Ok(())
}

// ============================================================================
// Mémoire de l'utilisateur — elle vit sur le serveur, parce que c'est là que
// le modèle la lit. Le téléphone ne fait que la montrer et la corriger.
// ============================================================================

/// Une chose retenue, telle que le serveur la renvoie.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub category: String,
    pub content: String,
    #[serde(default)]
    pub source: String,
}

#[tauri::command]
async fn list_memory() -> Result<Vec<MemoryEntry>, String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .get(format!("{base}/v1/memory"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Ce serveur est trop ancien pour tenir une mémoire.".into());
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la demande ({}).",
            resp.status()
        ));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    // Le service répond `{ "entries": [...] }` ; un tableau nu est accepté
    // aussi, pour ne pas casser sur une variante de forme.
    let brut = body.get("entries").cloned().unwrap_or(body);
    serde_json::from_value(brut).map_err(|e| e.to_string())
}

#[tauri::command]
async fn remember(category: String, content: String) -> Result<(), String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .post(format!("{base}/v1/memory"))
        .bearer_auth(&session.token)
        .json(&serde_json::json!({ "category": category, "content": content }))
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!("Le serveur a refusé ({}).", resp.status()));
    }
    Ok(())
}

#[tauri::command]
async fn forget(id: String) -> Result<(), String> {
    let (client, server, session) = authenticated()?;
    let base = server.current_url.trim_end_matches('/');
    let resp = client
        .delete(format!("{base}/v1/memory/{id}"))
        .bearer_auth(&session.token)
        .send()
        .await
        .map_err(|_| unreachable(&server))?;
    if !resp.status().is_success() {
        return Err(format!("Le serveur a refusé ({}).", resp.status()));
    }
    Ok(())
}

/// Où déposer une image que l'utilisateur veut garder.
///
/// Sur Android, le dossier public « Pictures » n'est pas accessible en
/// écriture sans une permission de stockage que l'application ne demande pas.
/// Le dossier externe propre à l'application, lui, l'est toujours, et un
/// gestionnaire de fichiers ou un branchement USB y accède. On rend le chemin
/// pour pouvoir le dire à l'utilisateur : une image « enregistrée » quelque
/// part d'introuvable ne serait pas enregistrée.
fn pictures_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    use tauri::Manager as _;
    // Le dossier de cache est celui que le fournisseur de fichiers déclare :
    // c'est de là qu'Android accepte de passer l'image à une autre
    // application. Le stockage public, lui, est fermé depuis Android 10 sans
    // passer par la médiathèque du système ; un fichier écrit ailleurs serait
    // enregistré nulle part d'utile.
    if let Ok(cache) = app.path().app_cache_dir() {
        let dir = cache.join("images");
        if std::fs::create_dir_all(&dir).is_ok() {
            return dir;
        }
    }
    let repli = locaryn_config::default_data_dir().join("images");
    let _ = std::fs::create_dir_all(&repli);
    repli
}

/// Décoder du base64 standard. L'inverse de ce que le serveur a encodé.
fn base64_decode(texte: &str) -> Result<Vec<u8>, String> {
    fn valeur(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let filtre: Vec<u8> = texte
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        .collect();
    let mut sortie = Vec::with_capacity(filtre.len() / 4 * 3);
    for morceau in filtre.chunks(4) {
        let mut accumulateur = 0u32;
        for (i, c) in morceau.iter().enumerate() {
            let v = valeur(*c).ok_or("données illisibles")?;
            accumulateur |= v << (18 - 6 * i as u32);
        }
        sortie.push((accumulateur >> 16) as u8);
        if morceau.len() > 2 {
            sortie.push((accumulateur >> 8) as u8);
        }
        if morceau.len() > 3 {
            sortie.push(accumulateur as u8);
        }
    }
    Ok(sortie)
}

/// Enregistrer une image reçue, et dire où elle est allée.
#[tauri::command]
fn save_image(app: tauri::AppHandle, name: String, data_base64: String) -> Result<String, String> {
    let octets = base64_decode(&data_base64)?;
    // Un nom venu du serveur ne décide pas d'un chemin : on ne garde que le
    // dernier segment, jamais un « ../ ».
    let feuille = std::path::Path::new(&name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("locaryn.png");
    let mut chemin = pictures_dir(&app).join(feuille);
    // Ne jamais écraser : on numérote.
    let mut n = 1;
    while chemin.exists() {
        let tige = std::path::Path::new(feuille)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("locaryn");
        let ext = std::path::Path::new(feuille)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png");
        chemin = pictures_dir(&app).join(format!("{tige}-{n}.{ext}"));
        n += 1;
    }
    std::fs::write(&chemin, octets).map_err(|e| format!("écriture impossible : {e}"))?;

    // Le fichier est écrit, puis confié au système : c'est là que la personne
    // choisit ce qu'elle en fait — la ranger dans la galerie, l'envoyer, la
    // garder. Un fichier déposé dans un dossier d'application et jamais
    // proposé serait un enregistrement pour personne.
    #[cfg(target_os = "android")]
    {
        use tauri_plugin_opener::OpenerExt as _;
        if let Err(e) = app
            .opener()
            .open_path(chemin.to_string_lossy(), None::<&str>)
        {
            tracing::warn!(error = %e, "image enregistrée mais non proposée au système");
        }
    }

    Ok(chemin
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| chemin.to_string_lossy().to_string()))
}

/// Un modèle proposé au téléphone, et s'il peut réellement produire.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaModel {
    pub name: String,
    pub ready: bool,
    /// Ce qui manque, dit à l'utilisateur. Vide quand `ready`.
    #[serde(default)]
    pub missing: Vec<String>,
}

/// Which models the machine can generate with. `kind` is "image" or "audio".
///
/// Le serveur détaille les modèles d'image : certains sont des poids de
/// diffusion seuls et ne produiront rien sans leurs encodeurs. Un serveur plus
/// ancien ne renvoie que des noms — on les considère alors utilisables, faute
/// de mieux, plutôt que de vider la liste.
#[tauri::command]
async fn list_media_models(kind: String) -> Result<Vec<MediaModel>, String> {
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
    if let Some(details) = body.get("details") {
        if let Ok(models) = serde_json::from_value::<Vec<MediaModel>>(details.clone()) {
            return Ok(models);
        }
    }
    Ok(body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(|name| MediaModel {
                    name: name.to_string(),
                    ready: true,
                    missing: Vec::new(),
                })
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
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init());

    // The camera plugin only exists on a phone. Registering it unconditionally
    // would stop the desktop build, which is what the layout is developed on.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());

    builder
        .invoke_handler(tauri::generate_handler![
            status,
            register_server,
            register_address,
            reconnect_active_server,
            sign_in,
            sign_out,
            send_message,
            list_media_models,
            list_conversations,
            list_projects,
            list_project_conversations,
            load_conversation,
            list_memory,
            remember,
            forget,
            list_extensions,
            list_figures,
            save_figure,
            delete_figure,
            start_figure_chat,
            figure_sessions,
            run_composer_tool,
            extension_config,
            set_extension_config,
            list_models,
            install_extension,
            set_extension_enabled,
            remove_extension,
            save_image,
            generate_image,
            generate_audio,
            pairing::apply_pairing_link,
            server_capabilities,
            update::check_update,
            update::install_update,
            update::resume_install,
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

#[cfg(test)]
mod base64_aller_retour {
    use super::base64_decode;
    use locaryn_shared_types::base64_encode;

    #[test]
    fn ce_qui_est_encode_se_decode() {
        for cas in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foobar",
            &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        ] {
            assert_eq!(base64_decode(&base64_encode(cas)).unwrap(), cas);
        }
    }

    #[test]
    fn les_caracteres_invalides_sont_refuses() {
        assert!(base64_decode("!!!!").is_err());
    }
}

#[cfg(test)]
mod adresse_tests {
    use super::normalise_address;

    #[test]
    fn une_ip_seule_recoit_schema_et_port() {
        assert_eq!(
            normalise_address("192.168.1.20").unwrap(),
            "http://192.168.1.20:7474"
        );
    }

    #[test]
    fn le_port_ecrit_est_respecte() {
        assert_eq!(
            normalise_address(" 192.168.1.20:9000 ").unwrap(),
            "http://192.168.1.20:9000"
        );
    }

    #[test]
    fn https_et_barre_finale() {
        assert_eq!(
            normalise_address("https://maison.local/").unwrap(),
            "https://maison.local:7474"
        );
    }

    #[test]
    fn un_nom_dhote_sans_port_reste_un_nom_dhote() {
        // Le « : » d'un IPv6 entre crochets ne doit pas passer pour un port.
        assert_eq!(
            normalise_address("[fe80::1]").unwrap(),
            "http://[fe80::1]:7474"
        );
    }

    #[test]
    fn refus_du_vide_et_des_schemas_inconnus() {
        assert!(normalise_address("   ").is_err());
        assert!(normalise_address("ftp://serveur").is_err());
    }
}

#[cfg(test)]
mod config_tests {
    /// Sans `app.windows`, Tauri démarre sans créer la moindre fenêtre : sur
    /// Android l'activité s'ouvre, le thème peint le fond, et l'utilisateur
    /// regarde un écran vide. Aucun plantage, aucune trace — c'est le bogue le
    /// plus silencieux du projet, et il a été livré. Ce test relit le fichier
    /// de configuration réel pour qu'il ne puisse plus repartir vide.
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
        assert_eq!(fenetres[0]["url"], "index.html");
    }
}
