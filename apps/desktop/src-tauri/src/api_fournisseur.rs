//! L'API fournisseur vue de l'application : l'allumer, donner les adresses
//! aux autres applications, gérer les clés d'API.
//!
//! Le service (`locaryn-daemon`) sert l'API ; l'application n'en tient que le
//! réglage et les clés, dans la même base. Les clés appartiennent au premier
//! administrateur — la personne devant cette machine, celle qui a ouvert le
//! mode serveur.

use locaryn_config::api_fournisseur;
use locaryn_storage::users::{TokenKind, UserRepo};
use serde::Serialize;

async fn depot() -> Result<UserRepo, String> {
    let db = locaryn_config::default_data_dir().join("locaryn.db");
    let pool = locaryn_storage::open(&db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(UserRepo::new(pool))
}

#[derive(Debug, Clone, Serialize)]
pub struct EtatApiFournisseur {
    pub enabled: bool,
    /// Le mode serveur tourne : sans lui, rien ne sert l'API.
    pub server_running: bool,
    /// Pour les applications de cette machine (HTTP simple, 127.0.0.1).
    pub local_url: String,
    /// Pour les autres appareils du réseau (HTTPS, certificat du serveur).
    pub network_url: String,
    /// Le compte administrateur qui porte les clés.
    pub admin: Option<String>,
    /// Ce qui empêche de s'en servir maintenant, dit en clair.
    pub blocker: Option<String>,
}

#[tauri::command]
pub async fn provider_api_status() -> Result<EtatApiFournisseur, String> {
    let serveur = crate::server_mode::server_status().await?;
    let repo = depot().await?;
    let admin = match repo.first_admin_id().await {
        Some(id) => repo
            .list()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|u| u.id == id)
            .map(|u| u.username),
        None => None,
    };
    let blocker = if admin.is_none() {
        Some(
            "Créez d'abord un compte administrateur (ci-dessus) : c'est lui qui porte les clés d'API."
                .to_string(),
        )
    } else if !serveur.running {
        Some("Allumez le mode serveur : c'est le service Locaryn qui sert l'API.".to_string())
    } else {
        None
    };
    Ok(EtatApiFournisseur {
        enabled: api_fournisseur::active(),
        server_running: serveur.running,
        local_url: format!(
            "http://127.0.0.1:{}/v1",
            api_fournisseur::port_local(serveur.port)
        ),
        network_url: format!(
            "https://{}:{}/v1",
            crate::server_mode::local_address(),
            serveur.port
        ),
        admin,
        blocker,
    })
}

#[tauri::command]
pub async fn set_provider_api(enabled: bool) -> Result<EtatApiFournisseur, String> {
    api_fournisseur::regler(enabled).map_err(|e| e.to_string())?;
    tracing::info!(enabled, "API fournisseur");
    provider_api_status().await
}

#[derive(Debug, Clone, Serialize)]
pub struct CleApi {
    pub id: String,
    pub label: Option<String>,
    /// Quelques caractères du milieu : de quoi reconnaître la clé, jamais la
    /// clé elle-même.
    pub hint: String,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleCreee {
    pub id: String,
    /// La clé en clair : affichée une seule fois.
    pub token: String,
    pub expires_at: Option<String>,
}

async fn admin_id(repo: &UserRepo) -> Result<uuid::Uuid, String> {
    repo.first_admin_id()
        .await
        .ok_or_else(|| "aucun compte administrateur".to_string())
}

#[tauri::command]
pub async fn list_api_keys() -> Result<Vec<CleApi>, String> {
    let repo = depot().await?;
    let Some(id) = repo.first_admin_id().await else {
        return Ok(Vec::new());
    };
    Ok(repo
        .list_tokens(id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|t| t.kind == TokenKind::Api && t.revoked_at.is_none())
        .map(|t| CleApi {
            id: t.id.to_string(),
            label: t.label,
            hint: t.hint,
            created_at: t.created_at,
            last_used_at: t.last_used_at,
            expires_at: t.expires_at,
        })
        .collect())
}

#[tauri::command]
pub async fn create_api_key(
    label: String,
    expires_in_days: Option<i64>,
) -> Result<CleCreee, String> {
    if !api_fournisseur::active() {
        return Err("Allumez d'abord l'API fournisseur (Réglages → Serveur).".into());
    }
    let repo = depot().await?;
    let id = admin_id(&repo).await?;
    let label = label.trim();
    let cle = repo
        .issue_api_token(
            id,
            (!label.is_empty()).then_some(label),
            expires_in_days.filter(|d| *d > 0),
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(CleCreee {
        id: cle.id.to_string(),
        token: cle.plaintext,
        expires_at: cle.expires_at,
    })
}

#[tauri::command]
pub async fn revoke_api_key(id: String) -> Result<(), String> {
    let repo = depot().await?;
    let admin = admin_id(&repo).await?;
    let id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    // Seulement une clé de ce compte, et seulement une clé d'API : jamais la
    // session d'un appareil appairé.
    let a_lui = repo
        .list_tokens(admin)
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .any(|t| t.id == id && t.kind == TokenKind::Api);
    if !a_lui {
        return Err("clé introuvable".into());
    }
    repo.revoke_token(id).await.map_err(|e| e.to_string())
}

/// Le suivi Git du travail de l'agent : un dépôt dans les projets qui n'en ont
/// pas, et Locaryn co-auteur de ses commits. Rangé ici avec les autres
/// réglages globaux simples.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ReglagesGit {
    pub init_auto: bool,
    pub co_auteur: bool,
}

#[tauri::command]
pub fn git_reglages() -> ReglagesGit {
    ReglagesGit {
        init_auto: locaryn_config::git::init_auto(),
        co_auteur: locaryn_config::git::co_auteur(),
    }
}

#[tauri::command]
pub fn set_git_reglages(reglages: ReglagesGit) -> Result<(), String> {
    locaryn_config::git::regler(reglages.init_auto, reglages.co_auteur).map_err(|e| e.to_string())
}
