//! Travel mode, from the application's side.
//!
//! The tunnel itself belongs to the daemon — it has to keep running after the
//! window is closed, which is the whole situation this addresses. What lives
//! here is the switch, and the code to point a camera at.
//!
//! Nothing in what the interface receives is a network setting. No address, no
//! port, no relay hostname. The user turns something on and photographs a
//! square; that is the entire interaction, and it is deliberate — every IP
//! shown on screen is one the person has to understand, remember, or mistype.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TravelStatus {
    pub active: bool,
    pub provider: Option<String>,
    /// The signed link, kept so the interface can offer to copy it when a
    /// camera is not an option.
    pub link: Option<String>,
    /// The same link, drawn — inlined SVG, ready for `dangerouslySetInnerHTML`.
    pub qr_svg: Option<String>,
    pub blocker: Option<String>,
}

/// One relay, as offered in the interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RelayChoice {
    pub id: String,
    pub label: String,
    /// Whether the tool is on this machine already.
    pub installed: bool,
    /// Whether it needs an account or a prior sign-in.
    pub needs_account: bool,
    /// What to do about it when it is missing.
    pub install_hint: String,
}

/// The relays, with the state of each on this machine.
///
/// Presented before the user chooses rather than after: discovering that the
/// one you picked needs an account, at the moment you are trying to leave, is
/// the worst possible time to find out.
#[tauri::command]
pub fn travel_relays() -> Vec<RelayChoice> {
    locaryn_travel::Provider::ALL
        .into_iter()
        .map(|p| RelayChoice {
            id: p.id().to_string(),
            label: p.label().to_string(),
            installed: p.is_available(),
            needs_account: p.needs_account(),
            install_hint: p.install_hint().to_string(),
        })
        .collect()
}

/// Ask the running daemon.
async fn daemon(path: &str) -> Result<reqwest::RequestBuilder, String> {
    let cfg = locaryn_config::load(None).map_err(|e| e.to_string())?;
    let port = cfg.daemon.port;
    let client = crate::secure_client::build(None, None, None, std::time::Duration::from_secs(90))?;
    Ok(client.get(format!("https://127.0.0.1:{port}{path}")))
}

fn not_running() -> String {
    "Le partage réseau n'est pas actif. Activez-le d'abord : le mode Remote rend \
     accessible ce que cette machine partage."
        .to_string()
}

#[tauri::command]
pub async fn travel_status() -> Result<TravelStatus, String> {
    let req = daemon("/v1/travel").await?;
    match req.send().await {
        Ok(r) if r.status().is_success() => r.json().await.map_err(|e| e.to_string()),
        // A daemon that is not there is not an error to shout about: the
        // switch is simply off, and the reason is one the user can act on.
        Ok(_) | Err(_) => Ok(TravelStatus {
            blocker: Some(not_running()),
            ..Default::default()
        }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTravel {
    /// Relay identifier, or `None` to switch travel mode off.
    pub provider: Option<String>,
}

#[tauri::command]
pub async fn set_travel_mode(args: SetTravel) -> Result<TravelStatus, String> {
    let cfg = locaryn_config::load(None).map_err(|e| e.to_string())?;
    let port = cfg.daemon.port;
    let client = crate::secure_client::build(None, None, None, std::time::Duration::from_secs(90))?;
    let resp = client
        .post(format!("https://127.0.0.1:{port}/v1/travel"))
        .json(&serde_json::json!({ "provider": args.provider }))
        .send()
        .await
        .map_err(|_| not_running())?;

    if resp.status().is_success() {
        return resp.json().await.map_err(|e| e.to_string());
    }
    // The daemon's refusals are already phrased for a person; pass them
    // through rather than replacing them with a status code.
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    Err(body
        .pointer("/error/message")
        .and_then(|m| m.as_str())
        .unwrap_or("Le mode Remote n'a pas pu démarrer.")
        .to_string())
}

/// The code that puts a phone back on the local network.
#[tauri::command]
pub async fn travel_home_code() -> Result<TravelStatus, String> {
    let req = daemon("/v1/travel/home").await?;
    let resp = req.send().await.map_err(|_| not_running())?;
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(TravelStatus {
        active: false,
        provider: None,
        link: body
            .get("link")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        qr_svg: body
            .get("qr_svg")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        blocker: None,
    })
}

/// Le modèle des micro-tâches, et ce qu'on peut choisir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroModel {
    /// `None` : aucun. Les micro-tâches ne tournent pas.
    pub model: Option<String>,
    pub available: Vec<String>,
}

/// Quel modèle nomme les conversations, et lesquels sont installés.
#[tauri::command]
pub async fn micro_model() -> Result<MicroModel, String> {
    let cfg = locaryn_config::load(None).ok();
    let current_model = cfg.and_then(|c| c.assistance.micro_model);

    // Scanner directement les modèles locaux installés
    let mut available = Vec::new();
    let models_dir = locaryn_config::models_dir();
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if crate::is_text_chat_model(name) {
                        available.push(name.to_string());
                    }
                }
            } else if path.is_dir() {
                let dir_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    for sub in sub_entries.flatten() {
                        let sub_path = sub.path();
                        if sub_path.is_file() {
                            if let Some(sub_name) = sub_path.file_name().and_then(|n| n.to_str()) {
                                let full_name = format!("{dir_name}/{sub_name}");
                                if crate::is_text_chat_model(&full_name) {
                                    available.push(full_name);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    available.sort();
    available.dedup();

    Ok(MicroModel {
        model: current_model,
        available,
    })
}

/// Choisir le modèle des micro-tâches, ou n'en choisir aucun.
#[tauri::command]
pub async fn set_micro_model(model: Option<String>) -> Result<MicroModel, String> {
    let choix = model.filter(|m| !m.trim().is_empty());
    locaryn_config::set_global("assistance", serde_json::json!({ "micro_model": choix }))
        .map_err(|e| e.to_string())?;

    // Si un démon tourne, le notifier sans bloquer en cas d'erreur
    if let Ok(cfg) = locaryn_config::load(None) {
        let port = cfg.daemon.port;
        if let Ok(client) =
            crate::secure_client::build(None, None, None, std::time::Duration::from_millis(500))
        {
            let _ = client
                .post(format!(
                    "https://127.0.0.1:{port}/v1/assistance/micro-model"
                ))
                .json(&serde_json::json!({ "model": choix }))
                .send()
                .await;
        }
    }

    micro_model().await
}

/// Un code d'appairage, avec l'adresse qu'il porte.
///
/// L'adresse apparaît ici, contrairement au mode Remote : il s'agit d'un
/// premier appairage, et la personne doit pouvoir vérifier qu'elle donne bien
/// l'adresse qu'elle croit — surtout celle d'un port ouvert sur l'extérieur.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingCode {
    pub mode: String,
    pub url: String,
    pub qr_svg: String,
}

/// Le code à photographier pour un premier appairage.
///
/// `mode` vaut `local`, `public` ou `tunnel` ; `url` n'est lu que pour
/// `public`, où c'est l'utilisateur qui sait par quelle adresse on le joint.
#[tauri::command]
pub async fn pairing_code(mode: String, url: Option<String>) -> Result<PairingCode, String> {
    let mut route = format!("/v1/pairing?mode={mode}");
    if let Some(u) = url.as_deref().filter(|u| !u.trim().is_empty()) {
        route.push_str("&url=");
        route.push_str(&urlencode(u.trim()));
    }
    let req = daemon(&route).await?;
    let resp = req.send().await.map_err(|_| not_running())?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(body
            .pointer("/error/message")
            .and_then(|m| m.as_str())
            .unwrap_or("Le service a refusé de produire un code.")
            .to_string());
    }
    Ok(PairingCode {
        mode: body["mode"].as_str().unwrap_or(&mode).to_string(),
        url: body["url"].as_str().unwrap_or_default().to_string(),
        qr_svg: body["qr_svg"].as_str().unwrap_or_default().to_string(),
    })
}

/// Échapper ce qui doit l'être dans un paramètre d'URL.
///
/// Une adresse contient `:` et `/` ; les laisser passer tels quels casserait
/// la requête. Pas de dépendance pour six caractères.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod pairing_tests {
    use super::urlencode;

    #[test]
    fn une_adresse_traverse_un_parametre_sans_se_casser() {
        assert_eq!(
            urlencode("https://maison.exemple:7474"),
            "https%3A%2F%2Fmaison.exemple%3A7474"
        );
        assert_eq!(urlencode("192.168.1.20"), "192.168.1.20");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_relay_says_whether_it_needs_an_account() {
        let relays = travel_relays();
        assert_eq!(relays.len(), 3);
        // Exactly one requires nothing — that is the one to default to.
        let free: Vec<_> = relays.iter().filter(|r| !r.needs_account).collect();
        assert_eq!(
            free.len(),
            1,
            "un seul relais doit être utilisable sans compte"
        );
        assert_eq!(free[0].id, "cloudflare");
        for r in &relays {
            assert!(!r.label.is_empty());
            assert!(r.install_hint.len() > 30, "consigne trop vague : {}", r.id);
        }
    }

    #[tokio::test]
    async fn without_a_daemon_the_answer_explains_rather_than_fails() {
        // Someone who never switched on sharing will land here first; a raw
        // connection error would tell them nothing about what to do.
        let st = travel_status().await.unwrap();
        assert!(!st.active);
        if let Some(b) = st.blocker {
            assert!(b.contains("partage"), "message peu clair : {b}");
        }
    }
}

/// Ce que le petit modèle propose comme rangement.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuggestionDeProjet {
    pub project_id: Option<String>,
    pub project_name: Option<String>,
}

/// Demander où cette conversation aurait sa place.
///
/// Répond presque toujours « nulle part », et c'est voulu : la question se
/// pose après chaque échange, et une proposition à côté de la plaque coûte
/// plus cher qu'un silence. Rien n'est déplacé — le déplacement reste un
/// geste de la personne.
#[tauri::command]
pub async fn suggest_project(session_id: String) -> Result<SuggestionDeProjet, String> {
    let req = daemon(&format!("/v1/sessions/{session_id}/suggest-project")).await?;
    match req.send().await {
        Ok(r) if r.status().is_success() => r.json().await.map_err(|e| e.to_string()),
        // Pas de service, pas de proposition. Ce n'est pas une panne à
        // annoncer : c'est une aide qui ne s'affiche pas.
        _ => Ok(SuggestionDeProjet::default()),
    }
}

/// Réunir deux conversations en une.
///
/// Le petit modèle relit les deux fils et en écrit un seul récit, versé dans
/// la conversation d'accueil. Celle qui a été déposée part aux archives : une
/// fusion ratée doit pouvoir se défaire.
#[tauri::command]
pub async fn merge_sessions(session_id: String, source_id: String) -> Result<(), String> {
    let cfg = locaryn_config::load(None).map_err(|e| e.to_string())?;
    let port = cfg.daemon.port;
    let client =
        crate::secure_client::build(None, None, None, std::time::Duration::from_secs(180))?;
    let resp = client
        .post(format!(
            "https://127.0.0.1:{port}/v1/sessions/{session_id}/merge"
        ))
        .json(&serde_json::json!({ "source_id": source_id }))
        .send()
        .await
        .map_err(|_| not_running())?;
    if resp.status().is_success() {
        return Ok(());
    }
    // Le service explique pourquoi — modèle de micro-tâches absent, moteur
    // éteint. Répéter son message vaut mieux qu'un code d'erreur.
    let texte = resp.text().await.unwrap_or_default();
    Err(if texte.trim().is_empty() {
        "La fusion a échoué. Rien n'a été modifié.".to_string()
    } else {
        texte
    })
}

/// Appeler l'outil qu'un bouton d'extension désigne.
///
/// Le bouton nomme un outil, pas un serveur : c'est le service qui cherche
/// lequel de ses serveurs d'extensions le porte.
#[tauri::command]
pub async fn run_composer_tool(tool: String, text: String) -> Result<String, String> {
    let cfg = locaryn_config::load(None).map_err(|e| e.to_string())?;
    let port = cfg.daemon.port;
    let client =
        crate::secure_client::build(None, None, None, std::time::Duration::from_secs(180))?;
    let resp = client
        .post(format!("https://127.0.0.1:{port}/v1/tools/{tool}"))
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
        .map_err(|_| not_running())?;
    let statut = resp.status();
    let corps: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !statut.is_success() {
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
