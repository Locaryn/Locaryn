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
    lochor_travel::Provider::ALL
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
    let cfg = lochor_config::load(None).map_err(|e| e.to_string())?;
    let port = cfg.daemon.port;
    let client = crate::secure_client::build(None, None, None, std::time::Duration::from_secs(90))?;
    Ok(client.get(format!("https://127.0.0.1:{port}{path}")))
}

fn not_running() -> String {
    "Le partage réseau n'est pas actif. Activez-le d'abord : le mode voyage rend \
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
    let cfg = lochor_config::load(None).map_err(|e| e.to_string())?;
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
        .unwrap_or("Le mode voyage n'a pas pu démarrer.")
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
        link: body.get("link").and_then(|v| v.as_str()).map(str::to_string),
        qr_svg: body.get("qr_svg").and_then(|v| v.as_str()).map(str::to_string),
        blocker: None,
    })
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
        assert_eq!(free.len(), 1, "un seul relais doit être utilisable sans compte");
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
