//! The servers this phone knows, and the address currently in use.
//!
//! A phone can hold several — a work server and a home one — so a scanned code
//! has to say *which* it applies to. That is what the key identifier in the
//! link is for; without it, a code would land on whichever server the
//! application happened to look at first.
//!
//! Nothing here is ever shown. The address, the port and the relay are
//! implementation detail: the person holding the phone chose a server by name
//! once, and after that the application follows the codes they scan.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KnownServer {
    /// Identifier derived from the server's own authority, as it appears in a
    /// pairing link.
    pub key_id: String,
    /// What the user calls it — the organisation from the deployment file.
    pub name: String,
    /// Address on the local network. Restored when coming home.
    pub home_url: String,
    /// Address currently in use. Differs from `home_url` while travelling.
    pub current_url: String,
    /// The deployment authority, in PEM. This is what makes a scanned code
    /// verifiable, so it is the one thing that must never be guessed.
    pub authority_pem: String,
    /// True while a travel link is in force.
    pub travelling: bool,
    /// Par quel chemin ce serveur a été appairé : `local`, `public` ou
    /// `tunnel`. Vide pour un code produit avant que le champ existe.
    #[serde(default)]
    pub access_mode: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub servers: Vec<KnownServer>,
    /// Which one the interface is talking to.
    #[serde(default)]
    pub active: Option<String>,
}

pub fn store_path() -> PathBuf {
    locaryn_config::default_data_dir().join("mobile-servers.json")
}

pub fn load() -> Store {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(store: &Store) -> Result<(), String> {
    let path = store_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("dossier de données : {e}"))?;
    }
    let raw = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("écriture : {e}"))
}

impl Store {
    pub fn get(&self, key_id: &str) -> Option<&KnownServer> {
        self.servers.iter().find(|s| s.key_id == key_id)
    }

    pub fn get_mut(&mut self, key_id: &str) -> Option<&mut KnownServer> {
        self.servers.iter_mut().find(|s| s.key_id == key_id)
    }

    /// Add a server, or refresh what is known about one already registered.
    ///
    /// Re-registering must not silently drop the fact that the phone is
    /// currently travelling: someone who reinstalls their deployment file
    /// while away would otherwise be pointed back at an address they cannot
    /// reach.
    pub fn upsert(&mut self, server: KnownServer) {
        match self.get_mut(&server.key_id) {
            Some(existing) => {
                existing.name = server.name;
                existing.home_url = server.home_url;
                existing.authority_pem = server.authority_pem;
                if !existing.travelling {
                    existing.current_url = existing.home_url.clone();
                }
            }
            None => {
                self.active.get_or_insert_with(|| server.key_id.clone());
                self.servers.push(server);
            }
        }
    }

    pub fn active_server(&self) -> Option<&KnownServer> {
        self.active.as_ref().and_then(|k| self.get(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(key: &str) -> KnownServer {
        KnownServer {
            key_id: key.into(),
            name: "Atelier".into(),
            home_url: "https://192.168.1.10:7474".into(),
            current_url: "https://192.168.1.10:7474".into(),
            authority_pem: "-----BEGIN CERTIFICATE-----\nAA\n-----END CERTIFICATE-----".into(),
            travelling: false,
            access_mode: "local".into(),
        }
    }

    #[test]
    fn the_first_server_becomes_the_active_one() {
        let mut s = Store::default();
        s.upsert(server("k1"));
        assert_eq!(s.active.as_deref(), Some("k1"));
        // A second one must not steal the selection out from under the user.
        s.upsert(server("k2"));
        assert_eq!(s.active.as_deref(), Some("k1"));
        assert_eq!(s.servers.len(), 2);
    }

    #[test]
    fn re_registering_while_away_does_not_drag_the_phone_home() {
        // Reinstalling the deployment file from a hotel would otherwise point
        // the application at a LAN address it cannot reach.
        let mut s = Store::default();
        s.upsert(server("k1"));
        s.get_mut("k1").unwrap().travelling = true;
        s.get_mut("k1").unwrap().current_url = "https://abc.trycloudflare.com".into();

        s.upsert(server("k1"));
        let after = s.get("k1").unwrap();
        assert!(after.travelling);
        assert_eq!(after.current_url, "https://abc.trycloudflare.com");
    }

    #[test]
    fn re_registering_at_home_refreshes_the_address() {
        let mut s = Store::default();
        s.upsert(server("k1"));
        let mut updated = server("k1");
        updated.home_url = "https://192.168.1.42:7474".into();
        s.upsert(updated);
        let after = s.get("k1").unwrap();
        assert_eq!(after.current_url, "https://192.168.1.42:7474");
    }
}
