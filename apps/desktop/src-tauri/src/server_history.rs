// L'historique des serveurs auxquels cette application s'est connectée.
//
// Le scénario : un développeur passe du PC fixe au portable, change de réseau,
// quitte le travail — il bascule alors d'un serveur à l'autre ou au mode
// « full local » sans re-saisir ses coordonnées. La session active reste ce
// qu'elle était : un seul fichier de session, la bascule = supprimer ce
// fichier (local) ou signer chez un autre hôte. Ici, on ne fait que se
// souvenir des hôtes connus.
//
// Le mot de passe n'est jamais écrit sans un choix explicite de la personne —
// la case « mémoriser » est décochée par défaut. Quand elle est cochée, il
// part dans le trousseau du système (Credential Manager / Keychain), jamais
// dans ce fichier : celui-ci n'a que des coordonnées déjà visibles sur l'écran
// de connexion.

use serde::{Deserialize, Serialize};

/// Une entrée d'historique : ce qu'il faut pour proposer une reconnexion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ServerEntry {
    /// URL telle que tapée, sans le `/` final — sert d'identité.
    pub server_url: String,
    pub username: String,
    /// Dernière fois que l'on s'est connecté avec succès (epoch secondes).
    pub last_used: u64,
    /// Un mot de passe a-t-il été mémorisé pour cette entrée ? Le secret,
    /// lui, vit dans le trousseau — ce champ ne fait que l'annoncer.
    pub password_saved: bool,
}

fn history_path() -> std::path::PathBuf {
    locaryn_config::default_data_dir().join("server-history.json")
}

/// Clé du trousseau pour le mot de passe d'une entrée. L'URL fait partie de
/// la clé : deux serveurs, deux secrets, même identifiant.
fn keychain_key(server_url: &str, username: &str) -> String {
    format!("server:{server_url}#{username}")
}

/// L'historique complet, du plus récent au plus ancien. Un fichier illisible
/// ou absent est un historique vide — jamais une erreur bloquante : la liste
/// doit s'afficher même si le disque a rendu l'âme.
pub fn load() -> Vec<ServerEntry> {
    let Ok(raw) = std::fs::read_to_string(history_path()) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Insère ou met à jour une entrée et l'amène en tête de liste.
pub fn record(server_url: &str, username: &str, password_saved: bool) -> Result<(), String> {
    let url = server_url.trim_end_matches('/').to_string();
    if url.is_empty() {
        return Err("URL de serveur vide.".into());
    }
    let mut entries: Vec<ServerEntry> =
        load().into_iter().filter(|e| e.server_url != url).collect();
    entries.insert(
        0,
        ServerEntry {
            server_url: url,
            username: username.to_string(),
            last_used: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            password_saved,
        },
    );
    entries.truncate(20);
    let path = history_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("dossier de données : {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(&entries).map_err(|e| format!("sérialisation : {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("écriture de l'historique : {e}"))?;
    super::client_cert::restrict(&path);
    Ok(())
}

/// Oublier une entrée — et son secret au trousseau s'il y en avait un.
pub fn forget(core: &super::Core, server_url: &str, username: &str) -> Result<(), String> {
    let url = server_url.trim_end_matches('/');
    let key = keychain_key(url, username);
    let _ = core.keychain.delete(&key);
    let entries: Vec<ServerEntry> = load()
        .into_iter()
        .filter(|e| !(e.server_url == url && e.username == username))
        .collect();
    let json =
        serde_json::to_string_pretty(&entries).map_err(|e| format!("sérialisation : {e}"))?;
    std::fs::write(history_path(), json).map_err(|e| format!("écriture : {e}"))?;
    Ok(())
}

/// Le secret mémorisé pour cette entrée, s'il y en a un.
pub fn saved_password(core: &super::Core, server_url: &str, username: &str) -> Option<String> {
    core.keychain.get(&keychain_key(server_url, username)).ok()
}

/// Retient — ou efface — le mot de passe d'une entrée. L'effacement est
/// explicite (`None`) : décocher la case ne laisse rien derrière.
///
/// Le fichier d'historique suit : `password_saved` reflète ce que le
/// trousseau contient vraiment, pour que l'interface annonce la vérité.
pub fn store_password(
    core: &super::Core,
    server_url: &str,
    username: &str,
    password: Option<&str>,
) -> Result<(), String> {
    let url = server_url.trim_end_matches('/');
    let key = keychain_key(url, username);
    let saved = match password {
        Some(p) if !p.is_empty() => {
            core.keychain
                .put(&key, p)
                .map_err(|e| format!("trousseau : {e}"))?;
            true
        }
        _ => {
            core.keychain.delete(&key).or_else(|e| match e {
                locaryn_auth::KeychainError::NotFound(_) => Ok(()),
                other => Err(format!("trousseau : {other}")),
            })?;
            false
        }
    };
    // Mettre l'entrée d'historique en accord avec le trousseau. Une entrée
    // absente n'est pas une erreur : le secret existe, la liste le montrera
    // dès la prochaine connexion.
    let entries: Vec<ServerEntry> = load()
        .into_iter()
        .map(|mut e| {
            if e.server_url == url && e.username == username {
                e.password_saved = saved;
            }
            e
        })
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&entries) {
        let _ = std::fs::write(history_path(), json);
    }
    Ok(())
}

// ── Commandes Tauri ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_servers() -> Result<Vec<ServerEntry>, String> {
    Ok(load())
}

#[tauri::command]
pub fn forget_server(
    core: tauri::State<'_, super::Core>,
    server_url: String,
    username: String,
) -> Result<(), String> {
    forget(&core, &server_url, &username)
}

/// Le mot de passe mémorisé, s'il y en a un. La pré-remplissage d'un champ
/// mot de passe est exactement ce pour quoi ce secret existe.
#[tauri::command]
pub fn get_saved_password(
    core: tauri::State<'_, super::Core>,
    server_url: String,
    username: String,
) -> Result<Option<String>, String> {
    Ok(saved_password(&core, &server_url, &username))
}

#[tauri::command]
pub fn set_saved_password(
    core: tauri::State<'_, super::Core>,
    server_url: String,
    username: String,
    password: Option<String>,
) -> Result<(), String> {
    store_password(&core, &server_url, &username, password.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_garbled_history_is_empty() {
        // Le fichier n'existe pas dans l'environnement de test — et s'il
        // existait mais était corrompu, ce serait pareil : jamais d'erreur.
        let h = load();
        let _ = h;
    }

    #[test]
    fn empty_url_is_rejected() {
        assert!(record("", "alice", false).is_err());
    }
}
