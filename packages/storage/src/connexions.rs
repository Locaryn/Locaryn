//! Le journal des connexions d'un compte : ce que montre l'écran « Appareils
//! et sécurité ».
//!
//! Une ligne quand une session s'ouvre (mot de passe, code temporaire,
//! appairage), puis une quand un appareil déjà connecté revient depuis une
//! autre adresse. Pas une ligne par requête : l'historique doit se lire, et un
//! changement d'adresse est ce qui mérite l'attention.

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use std::path::Path;
use uuid::Uuid;

use crate::StorageError;

/// Combien de connexions garder par appareil dans la réponse.
const HISTORIQUE_PAR_APPAREIL: i64 = 30;
/// Au-delà, une connexion ne sert plus à rien et part du journal.
const CONSERVATION_JOURS: i64 = 365;

/// Le type de client, pour la petite icône de la liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    /// L'application Android / iOS.
    Mobile,
    /// L'interface web, dans un navigateur.
    Web,
    /// L'application de bureau.
    Desktop,
    /// `locaryn` dans un terminal.
    Cli,
    /// Un client qui ne s'est pas présenté.
    App,
}

impl ClientKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClientKind::Mobile => "mobile",
            ClientKind::Web => "web",
            ClientKind::Desktop => "desktop",
            ClientKind::Cli => "cli",
            ClientKind::App => "app",
        }
    }

    fn parse(s: &str) -> ClientKind {
        match s {
            "mobile" => ClientKind::Mobile,
            "web" => ClientKind::Web,
            "desktop" => ClientKind::Desktop,
            "cli" => ClientKind::Cli,
            _ => ClientKind::App,
        }
    }

    /// Le client d'après l'en-tête `User-Agent`. Nos applications se
    /// présentent (`Locaryn-Mobile/…`, `Locaryn-Desktop/…`, `Locaryn-Cli/…`) ;
    /// un navigateur s'annonce « Mozilla/… ».
    pub fn from_user_agent(user_agent: Option<&str>) -> ClientKind {
        let Some(ua) = user_agent.map(str::trim).filter(|u| !u.is_empty()) else {
            return ClientKind::App;
        };
        let bas = ua.to_ascii_lowercase();
        if bas.starts_with("locaryn-mobile") {
            ClientKind::Mobile
        } else if bas.starts_with("locaryn-desktop") {
            ClientKind::Desktop
        } else if bas.starts_with("locaryn-cli") {
            ClientKind::Cli
        } else if bas.starts_with("mozilla/") {
            ClientKind::Web
        } else {
            ClientKind::App
        }
    }
}

/// Comment la connexion s'est faite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Password,
    Code,
    Pairing,
    /// Un appareil déjà connecté, revenu depuis une autre adresse.
    NewAddress,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Password => "password",
            Method::Code => "code",
            Method::Pairing => "pairing",
            Method::NewAddress => "new_address",
        }
    }
}

/// Une connexion telle que l'écran la montre.
#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    /// RFC 3339 à la seconde, en UTC.
    pub at: String,
    pub ip: Option<String>,
    /// Ville, région, pays ; « Réseau local » ; absent sans base.
    pub location: Option<String>,
    pub client: &'static str,
    pub user_agent: Option<String>,
    pub method: String,
}

/// Un appareil : une session ouverte (ou fermée) et ses connexions.
#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub token_id: String,
    pub label: Option<String>,
    pub client: &'static str,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    /// L'appareil qui fait la demande — on ne se déconnecte pas sans le savoir.
    pub current: bool,
    pub history: Vec<Connection>,
}

/// Ce qu'on enregistre d'une connexion.
pub struct NewConnection<'a> {
    pub user_id: Uuid,
    pub token_id: Option<Uuid>,
    pub ip: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub method: Method,
}

#[derive(FromRow)]
struct TokenRow {
    id: String,
    label: Option<String>,
    created_at: Option<String>,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(FromRow)]
struct ConnectionRow {
    at: String,
    ip: Option<String>,
    client: String,
    user_agent: Option<String>,
    method: String,
}

#[derive(Clone)]
pub struct ConnectionRepo {
    pool: SqlitePool,
}

impl ConnectionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Inscrire une connexion. L'heure est celle de l'écriture, à la seconde.
    pub async fn record(&self, c: NewConnection<'_>) -> Result<(), StorageError> {
        let maintenant = Utc::now();
        let ua = c
            .user_agent
            .map(|u| u.chars().take(300).collect::<String>());
        sqlx::query(
            "INSERT INTO auth_connections (id, user_id, token_id, at, ip, client, user_agent, method) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(c.user_id.to_string())
        .bind(c.token_id.map(|t| t.to_string()))
        .bind(maintenant.to_rfc3339_opts(SecondsFormat::Secs, true))
        .bind(c.ip)
        .bind(ClientKind::from_user_agent(c.user_agent).as_str())
        .bind(ua)
        .bind(c.method.as_str())
        .execute(&self.pool)
        .await?;
        let limite = (maintenant - chrono::Duration::days(CONSERVATION_JOURS))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        sqlx::query("DELETE FROM auth_connections WHERE at < ?")
            .bind(limite)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// La dernière adresse connue d'un appareil.
    pub async fn last_ip(&self, token_id: Uuid) -> Result<Option<String>, StorageError> {
        let ip: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT ip FROM auth_connections WHERE token_id = ? ORDER BY at DESC, rowid DESC LIMIT 1",
        )
        .bind(token_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(ip.and_then(|(ip,)| ip))
    }

    /// Les appareils d'un compte (sessions, jamais les clés d'API) et leurs
    /// connexions, les plus récentes d'abord. `geo` : la base de
    /// géolocalisation locale, si elle a été installée.
    pub async fn devices(
        &self,
        user_id: Uuid,
        current: Option<Uuid>,
        geo: Option<&Path>,
    ) -> Result<Vec<Device>, StorageError> {
        let jetons = sqlx::query_as::<_, TokenRow>(
            "SELECT id, label, created_at, last_used_at, expires_at, revoked_at \
             FROM auth_tokens WHERE user_id = ? AND kind = 'session' \
             ORDER BY (revoked_at IS NULL) DESC, COALESCE(last_used_at, created_at) DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(jetons.len());
        for j in jetons {
            let lignes = sqlx::query_as::<_, ConnectionRow>(
                "SELECT at, ip, client, user_agent, method FROM auth_connections \
                 WHERE token_id = ? ORDER BY at DESC, rowid DESC LIMIT ?",
            )
            .bind(&j.id)
            .bind(HISTORIQUE_PAR_APPAREIL)
            .fetch_all(&self.pool)
            .await?;
            let history: Vec<Connection> = lignes
                .into_iter()
                .map(|l| Connection {
                    location: l
                        .ip
                        .as_deref()
                        .and_then(|ip| crate::geoip::locate(ip, geo.unwrap_or(Path::new("")))),
                    at: l.at,
                    ip: l.ip,
                    client: ClientKind::parse(&l.client).as_str(),
                    user_agent: l.user_agent,
                    method: l.method,
                })
                .collect();
            let client = history.first().map_or("app", |c| c.client);
            out.push(Device {
                current: current.is_some_and(|c| c.to_string() == j.id),
                token_id: j.id,
                label: j.label,
                client,
                created_at: j.created_at,
                last_used_at: j.last_used_at,
                expires_at: j.expires_at,
                revoked_at: j.revoked_at,
                history,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{Role, UserRepo};

    #[test]
    fn le_client_se_lit_dans_le_user_agent() {
        let k = |ua: &str| ClientKind::from_user_agent(Some(ua));
        assert_eq!(k("Locaryn-Mobile/0.4.72 (android)"), ClientKind::Mobile);
        assert_eq!(k("Locaryn-Desktop/0.4.72 (windows)"), ClientKind::Desktop);
        assert_eq!(k("Locaryn-Cli/0.4.72"), ClientKind::Cli);
        assert_eq!(
            k("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/130"),
            ClientKind::Web
        );
        assert_eq!(k("curl/8.0"), ClientKind::App);
        assert_eq!(ClientKind::from_user_agent(None), ClientKind::App);
    }

    #[tokio::test]
    async fn un_appareil_porte_son_historique_et_seulement_le_sien() {
        let pool = crate::open_in_memory().await.unwrap();
        let users = UserRepo::new(pool.clone());
        let journal = ConnectionRepo::new(pool);
        let a = users
            .create("alice", "mot-de-passe-long", Role::Admin)
            .await
            .unwrap();
        let b = users
            .create("bruno", "mot-de-passe-long", Role::Member)
            .await
            .unwrap();
        let tel = users
            .issue_token(a.id, Some("téléphone"), 30)
            .await
            .unwrap();
        let cle = users
            .issue_api_token(a.id, Some("éditeur"), None)
            .await
            .unwrap();
        let autre = users.issue_token(b.id, Some("portable"), 30).await.unwrap();
        for (tok, ip, ua, m) in [
            (tel.id, "192.168.1.5", "Locaryn-Mobile/1", Method::Code),
            (tel.id, "8.8.8.8", "Locaryn-Mobile/1", Method::NewAddress),
            (autre.id, "10.0.0.2", "Mozilla/5.0", Method::Password),
        ] {
            let user_id = if tok == autre.id { b.id } else { a.id };
            journal
                .record(NewConnection {
                    user_id,
                    token_id: Some(tok),
                    ip: Some(ip),
                    user_agent: Some(ua),
                    method: m,
                })
                .await
                .unwrap();
        }
        let appareils = journal.devices(a.id, Some(tel.id), None).await.unwrap();
        assert_eq!(appareils.len(), 1, "la clé d'API n'est pas un appareil");
        assert_ne!(appareils[0].token_id, cle.id.to_string());
        assert!(appareils[0].current);
        assert_eq!(appareils[0].client, "mobile");
        assert_eq!(appareils[0].history.len(), 2);
        assert!(appareils[0].history.iter().all(|c| c.at.ends_with('Z')));
        assert_eq!(
            journal.last_ip(tel.id).await.unwrap().as_deref(),
            Some(appareils[0].history[0].ip.as_deref().unwrap_or_default())
        );
        assert!(
            !users.revoke_own_token(b.id, tel.id).await.unwrap(),
            "un autre compte ne déconnecte pas cet appareil"
        );
        assert!(users.revoke_own_token(a.id, tel.id).await.unwrap());
        assert!(users.token_owner(&tel.plaintext).await.unwrap().is_none());
    }
}
