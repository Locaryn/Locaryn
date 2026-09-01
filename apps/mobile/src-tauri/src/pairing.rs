//! What happens when a code is scanned.
//!
//! The whole flow is one function: take the string the camera read, prove it
//! came from a server this phone already trusts, and switch the address. The
//! person sees "connecté" and nothing else — no address, no port, no relay.
//!
//! The proof is not optional. Scanning a code changes which server the
//! application talks to, so a code from anywhere else would be a way to
//! collect somebody's password. `locaryn_travel::verify` refuses a link signed
//! by any other authority, a link whose address was edited, and a link for a
//! server this phone has never registered.

use crate::servers;
use serde::Serialize;

/// What the interface shows after a scan. Deliberately thin.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingResult {
    /// Server name, so the user can see they connected to the right place.
    pub server_name: String,
    /// True for a travel link, false for the one that comes home.
    pub travelling: bool,
    /// One line, already phrased for a person.
    pub message: String,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Verify a scanned link and apply it.
///
/// Split from the Tauri command so the decision can be tested without a
/// running application — this is the security boundary of the whole feature.
pub fn apply(uri: &str, store: &mut servers::Store, now: u64) -> Result<PairingResult, String> {
    let known = |key_id: &str| store.get(key_id).map(|s| s.authority_pem.clone());

    let link = locaryn_travel::verify(uri, &known, now).map_err(|e| e.to_string())?;

    let server = store
        .get_mut(&link.key_id)
        // verify() already refused an unknown key, so this cannot normally
        // happen; failing loudly beats writing to the wrong server.
        .ok_or("Ce code ne correspond à aucun serveur enregistré sur cet appareil.")?;

    match link.mode {
        locaryn_travel::Mode::Travel => {
            server.current_url = link.url;
            server.travelling = true;
        }
        locaryn_travel::Mode::Home => {
            // Trust the address in the link over the stored one: the home
            // address may have changed while the phone was away.
            server.home_url = link.url.clone();
            server.current_url = link.url;
            server.travelling = false;
        }
    }

    let result = PairingResult {
        server_name: server.name.clone(),
        travelling: server.travelling,
        message: if server.travelling {
            format!("Connecté à {} depuis l'extérieur.", server.name)
        } else {
            format!("De retour sur le réseau local de {}.", server.name)
        },
    };
    // Follow the code that was scanned: it is an explicit act by the person
    // holding both devices.
    store.active = Some(link.key_id);
    Ok(result)
}

/// Scanned from the camera, or handed over by Android as a `locaryn://` link.
#[tauri::command]
pub fn apply_pairing_link(uri: String) -> Result<PairingResult, String> {
    let mut store = servers::load();
    let result = apply(&uri, &mut store, now())?;
    servers::save(&store)?;
    tracing::info!(travelling = result.travelling, "appairage appliqué");
    Ok(result)
}

/// The device session token delivered by `POST /v1/auth/pair/confirm`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PairConfirmResult {
    /// Session token for this device — stored like a sign-in token.
    pub token: String,
    /// ISO expiry of the session, as returned by the server.
    pub expires_at: Option<String>,
    /// Normalized device label the server recorded.
    pub device_label: String,
}

/// Circuit B, step 2: the person types the 6-digit code shown under the QR
/// on the host. The server validates it (2 min TTL, single use, 5 attempts)
/// and answers with a device session token — stored exactly like a sign-in
/// token, so every later request uses the standard Bearer circuit.
#[tauri::command]
pub async fn confirm_pairing(
    pairing_code: String,
    device_label: Option<String>,
) -> Result<PairingResult, String> {
    let mut store = servers::load();
    let active = store
        .active
        .clone()
        .ok_or("Aucun serveur enregistré. Scannez d'abord le QR de l'hôte.")?;
    let Some(server) = store.get(&active).cloned() else {
        return Err("Aucun serveur enregistré sur cet appareil.".into());
    };

    let client = crate::client_for(&server)?;
    let endpoint = format!(
        "{}/v1/auth/pair/confirm",
        server.current_url.trim_end_matches('/')
    );
    let resp = client
        .post(&endpoint)
        .json(&serde_json::json!({
            "pairing_code": pairing_code.trim(),
            "device_label": device_label.unwrap_or_else(|| "téléphone".into()),
        }))
        .send()
        .await
        .map_err(|_| format!("Serveur injoignable. Vérifiez que l'hôte est allumé."))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Code incorrect ou expiré. Affichez un nouveau QR sur l'hôte.".into());
    }
    if resp.status() == reqwest::StatusCode::CONFLICT {
        return Err("Aucun compte administrateur sur ce serveur : l'appairage par code exige un compte à appairer.".into());
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé l'appairage ({}).",
            resp.status()
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = body
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or("Le serveur n'a pas renvoyé de jeton.")?
        .to_string();
    let device_label = body
        .get("device_label")
        .and_then(|d| d.as_str())
        .unwrap_or("téléphone")
        .to_string();

    // The device session token is stored exactly like a sign-in token, so
    // every later request rides the standard Bearer circuit.
    let session = crate::Session {
        key_id: server.key_id.clone(),
        username: device_label.clone(),
        token,
    };
    std::fs::create_dir_all(locaryn_config::default_data_dir()).map_err(|e| e.to_string())?;
    std::fs::write(
        crate::session_path(),
        serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("écriture : {e}"))?;

    tracing::info!("appairage par code confirmé, session appareil enregistrée");
    Ok(PairingResult {
        server_name: server.name.clone(),
        travelling: server.travelling,
        message: format!("Appairé avec {}.", server.name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: std::path::PathBuf,
        cert: String,
        key: String,
        key_id: String,
    }

    fn fixture() -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_pair_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ca = locaryn_config::mtls::authority(&dir).unwrap();
        let key_id = locaryn_travel::link::key_id(&ca.cert_pem).unwrap();
        Fixture {
            dir,
            cert: ca.cert_pem,
            key: ca.key_pem,
            key_id,
        }
    }

    fn store_with(f: &Fixture) -> servers::Store {
        let mut s = servers::Store::default();
        s.upsert(servers::KnownServer {
            key_id: f.key_id.clone(),
            name: "Atelier Vasseur".into(),
            home_url: "https://192.168.1.10:7474".into(),
            current_url: "https://192.168.1.10:7474".into(),
            authority_pem: f.cert.clone(),
            travelling: false,
            access_mode: "local".into(),
        });
        s
    }

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn scanning_the_travel_code_switches_the_address_silently() {
        let f = fixture();
        let mut store = store_with(&f);
        let uri = locaryn_travel::sign(
            &f.cert,
            &f.key,
            locaryn_travel::Mode::Travel,
            "https://abc-123.trycloudflare.com",
            NOW,
            600,
        )
        .unwrap();

        let r = apply(&uri, &mut store, NOW + 5).unwrap();
        assert!(r.travelling);
        assert_eq!(r.server_name, "Atelier Vasseur");
        // What the user is told must not contain a machine address.
        assert!(
            !r.message.contains("http"),
            "adresse exposée : {}",
            r.message
        );
        assert!(
            !r.message.contains('.') || !r.message.contains("192."),
            "IP exposée"
        );

        let s = store.get(&f.key_id).unwrap();
        assert_eq!(s.current_url, "https://abc-123.trycloudflare.com");
        // The way home is remembered, not overwritten.
        assert_eq!(s.home_url, "https://192.168.1.10:7474");
        std::fs::remove_dir_all(&f.dir).ok();
    }

    #[test]
    fn scanning_the_home_code_puts_it_back() {
        let f = fixture();
        let mut store = store_with(&f);
        let travel = locaryn_travel::sign(
            &f.cert,
            &f.key,
            locaryn_travel::Mode::Travel,
            "https://abc-123.trycloudflare.com",
            NOW,
            600,
        )
        .unwrap();
        apply(&travel, &mut store, NOW).unwrap();

        let home = locaryn_travel::sign(
            &f.cert,
            &f.key,
            locaryn_travel::Mode::Home,
            "https://192.168.1.10:7474",
            NOW,
            600,
        )
        .unwrap();
        let r = apply(&home, &mut store, NOW).unwrap();
        assert!(!r.travelling);
        let s = store.get(&f.key_id).unwrap();
        assert_eq!(s.current_url, "https://192.168.1.10:7474");
        assert!(!s.travelling);
        std::fs::remove_dir_all(&f.dir).ok();
    }

    #[test]
    fn a_code_from_someone_elses_server_changes_nothing() {
        // The attack this whole design exists to stop: print a code, get it
        // scanned, receive the password on your own server.
        let mine = fixture();
        let theirs = fixture();
        let mut store = store_with(&mine);
        let before = store.get(&mine.key_id).unwrap().current_url.clone();

        // Signed by them, but claiming to be for my server is impossible —
        // the key id is derived from the authority. So they use their own,
        // which my phone has never registered.
        let uri = locaryn_travel::sign(
            &theirs.cert,
            &theirs.key,
            locaryn_travel::Mode::Travel,
            "https://serveur-du-pirate.example",
            NOW,
            600,
        )
        .unwrap();

        let err = apply(&uri, &mut store, NOW).unwrap_err();
        assert!(err.contains("aucun serveur enregistré"), "message : {err}");
        assert_eq!(store.get(&mine.key_id).unwrap().current_url, before);
        std::fs::remove_dir_all(&mine.dir).ok();
        std::fs::remove_dir_all(&theirs.dir).ok();
    }

    #[test]
    fn an_edited_address_is_refused_even_with_the_right_key_id() {
        // Rewriting the destination inside a genuine link is the cheapest
        // attack available, and the one a key id alone would not catch.
        let f = fixture();
        let mut store = store_with(&f);
        let uri = locaryn_travel::sign(
            &f.cert,
            &f.key,
            locaryn_travel::Mode::Travel,
            "https://vrai.trycloudflare.com",
            NOW,
            600,
        )
        .unwrap();

        // Swap the encoded address for another one.
        let encoded = uri.split("&u=").nth(1).unwrap().split('&').next().unwrap();
        let tampered = uri.replace(encoded, "aHR0cHM6Ly9waXJhdGUuZXhhbXBsZQ");
        assert_ne!(tampered, uri);

        let err = apply(&tampered, &mut store, NOW).unwrap_err();
        assert!(err.contains("Ne l'utilisez pas"), "message : {err}");
        assert_eq!(
            store.get(&f.key_id).unwrap().current_url,
            "https://192.168.1.10:7474"
        );
        std::fs::remove_dir_all(&f.dir).ok();
    }

    #[test]
    fn an_expired_code_says_to_ask_for_a_new_one() {
        let f = fixture();
        let mut store = store_with(&f);
        let uri = locaryn_travel::sign(
            &f.cert,
            &f.key,
            locaryn_travel::Mode::Travel,
            "https://abc.trycloudflare.com",
            NOW,
            600,
        )
        .unwrap();
        let err = apply(&uri, &mut store, NOW + 601).unwrap_err();
        assert!(err.contains("expiré"), "message : {err}");
        assert!(err.contains("nouveau"), "sans issue : {err}");
        std::fs::remove_dir_all(&f.dir).ok();
    }

    #[test]
    fn something_that_is_not_a_code_at_all_is_refused_kindly() {
        // People point the scanner at parcel labels and wifi codes.
        let f = fixture();
        let mut store = store_with(&f);
        for junk in ["https://exemple.com", "WIFI:S:maison;T:WPA;P:secret;;", ""] {
            let err = apply(junk, &mut store, NOW).unwrap_err();
            assert!(
                err.contains("Locaryn"),
                "message peu clair pour {junk:?} : {err}"
            );
        }
        std::fs::remove_dir_all(&f.dir).ok();
    }
}
