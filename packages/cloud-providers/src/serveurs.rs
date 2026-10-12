//! Les autres PC Locaryn dont ce poste se sert comme fournisseurs de modèle.
//!
//! Un PC avec une bonne carte graphique passe en mode serveur ; un autre s'en
//! sert pour **générer le texte**, et pour rien d'autre. La différence avec
//! « Se connecter à un serveur » est là : connecté, tout part sur le serveur —
//! conversations, projets, et surtout les outils. Un MCP comme Roblox Studio,
//! qui n'existe que sur le poste où l'on travaille, aurait été appelé sur le
//! serveur, où il ne tourne pas.
//!
//! Ici la boucle d'agent reste sur ce poste : elle lit les fichiers d'ici,
//! lance les commandes d'ici, parle aux MCP d'ici. Elle envoie seulement ses
//! requêtes de modèle à `{serveur}/v1/chat/completions`, qui relaie au moteur
//! du serveur et renvoie les appels d'outils sans les exécuter.
//!
//! Un serveur ajouté se présente comme un catalogue de plus : il apparaît
//! dans le sélecteur de modèles et peut servir un sous-agent
//! (`<id>:<modèle>`), à côté du modèle local de la conversation.

use crate::{set_key, DeclaredProvider, Host};
use locaryn_extensions::manifest::CloudProviderManifest;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Un serveur ajouté.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServeurModeles {
    /// Identifiant de catalogue (`pc-bureau`) : clé de trousseau, préfixe des
    /// modèles pour les sous-agents.
    pub id: String,
    /// Nom affiché : celui de la machine.
    pub nom: String,
    /// `https://192.168.0.57:7474`, sans `/` final.
    pub url: String,
    /// Autorité du serveur (PEM), lue au premier contact. Les certificats
    /// d'un serveur Locaryn sont émis par sa propre autorité, que les
    /// autorités publiques ignorent : sans elle, aucune requête ne passe.
    #[serde(default)]
    pub autorite: Option<String>,
}

fn fichier(data_dir: &Path) -> PathBuf {
    data_dir.join("serveurs-modeles.json")
}

/// Les serveurs ajoutés. Un fichier illisible vaut une liste vide.
pub fn lister(data_dir: &Path) -> Vec<ServeurModeles> {
    std::fs::read_to_string(fichier(data_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn enregistrer(data_dir: &Path, serveurs: &[ServeurModeles]) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("dossier de données : {e}"))?;
    let texte = serde_json::to_string_pretty(serveurs).map_err(|e| e.to_string())?;
    std::fs::write(fichier(data_dir), texte).map_err(|e| format!("écriture : {e}"))
}

/// Chaque serveur, sous la forme d'un catalogue.
pub(crate) fn declares(data_dir: &Path) -> Vec<DeclaredProvider> {
    lister(data_dir)
        .into_iter()
        .map(|s| DeclaredProvider {
            manifest: CloudProviderManifest {
                id: s.id.clone(),
                label: s.nom.clone(),
                api_url: s.url.clone(),
                key_required: Some(true),
                // La liste est celle des poids installés sur le serveur : on
                // la relit à chaque ouverture, elle ne coûte rien.
                refresh_hours: 0,
                ..CloudProviderManifest::default()
            },
            id: s.id,
            extension_id: uuid::Uuid::nil(),
            extension_name: "Autre PC Locaryn".into(),
            plugin_root: data_dir.to_path_buf(),
            autorite: s.autorite,
        })
        .collect()
}

/// Un client qui fait confiance à l'autorité du serveur, en plus des
/// autorités publiques.
pub fn client_pour(
    autorite: Option<&str>,
    delai: Option<Duration>,
) -> Result<reqwest::Client, String> {
    let mut b = reqwest::Client::builder().connect_timeout(Duration::from_secs(10));
    if let Some(d) = delai {
        b = b.timeout(d);
    }
    if let Some(pem) = autorite {
        let cert = reqwest::Certificate::from_pem(pem.as_bytes())
            .map_err(|e| format!("autorité du serveur illisible : {e}"))?;
        b = b.add_root_certificate(cert);
    }
    b.build().map_err(|e| e.to_string())
}

/// `192.168.0.57:7474`, `https://…/`, `…/v1` : la même adresse.
pub fn normaliser(brut: &str) -> Result<String, String> {
    let t = brut.trim().trim_end_matches('/');
    let t = t.strip_suffix("/v1").unwrap_or(t);
    if t.is_empty() {
        return Err("Saisissez l'adresse de l'autre PC.".into());
    }
    let avec_schema = if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else {
        format!("https://{t}")
    };
    // Sans port, celui du serveur Locaryn.
    let hote = avec_schema.split("://").nth(1).unwrap_or_default();
    let a_un_port = hote
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok());
    Ok(if a_un_port {
        avec_schema
    } else {
        format!("{avec_schema}:7474")
    })
}

/// Un identifiant de catalogue lisible et sans collision.
fn identifiant(nom: &str, existants: &[ServeurModeles]) -> String {
    let base: String = nom
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = format!("pc-{}", if base.is_empty() { "serveur" } else { &base });
    let mut id = base.clone();
    let mut n = 2;
    while existants.iter().any(|s| s.id == id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// Le message d'une erreur réseau, causes comprises : celle de reqwest seule
/// (« error sending request ») ne dit pas ce qui s'est passé.
fn chaine(e: &reqwest::Error) -> String {
    let mut s = e.to_string();
    let mut cause = std::error::Error::source(e);
    while let Some(c) = cause {
        s.push_str(" : ");
        s.push_str(&c.to_string());
        cause = c.source();
    }
    s
}

/// Ajouter un serveur avec un compte qui y existe.
///
/// Le mot de passe sert une fois : il ouvre une session, qui crée une clé
/// d'accès propre à ce poste. Seule la clé est gardée, dans le trousseau, et
/// elle se révoque depuis le serveur sans toucher au mot de passe.
pub async fn ajouter(
    host: &Host<'_>,
    adresse: &str,
    identifiant_compte: &str,
    mot_de_passe: &str,
    ce_poste: &str,
) -> Result<ServeurModeles, String> {
    let url = normaliser(adresse)?;
    let mut serveurs = lister(host.data_dir);
    if serveurs.iter().any(|s| s.url == url) {
        return Err("Ce PC est déjà dans la liste.".into());
    }

    // L'autorité, au premier contact — elle est publique. C'est elle qui sera
    // vérifiée à chaque requête ensuite : un autre appareil qui prendrait
    // l'adresse plus tard ne serait pas accepté.
    let autorite = if url.starts_with("https://") {
        let premier = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let r = premier
            .get(format!("{url}/v1/pairing/ca"))
            .send()
            .await
            .map_err(|e| {
                format!(
                    "L'autre PC ne répond pas à {url} ({}). Vérifiez que son mode serveur est \
                     activé et que les deux PC sont sur le même réseau.",
                    chaine(&e)
                )
            })?;
        if !r.status().is_success() {
            return Err(format!(
                "{url} répond, mais ce n'est pas un serveur Locaryn récent (HTTP {}).",
                r.status()
            ));
        }
        let pem = r.text().await.map_err(|e| e.to_string())?;
        if !pem.contains("BEGIN CERTIFICATE") {
            return Err(format!("{url} n'a pas fourni son certificat."));
        }
        Some(pem)
    } else {
        None
    };
    let client = client_pour(autorite.as_deref(), Some(Duration::from_secs(20)))?;

    let info: serde_json::Value = client
        .get(format!("{url}/v1/info"))
        .send()
        .await
        .map_err(|e| format!("Connexion sécurisée impossible avec {url} : {}", chaine(&e)))?
        .json()
        .await
        .unwrap_or_default();
    let nom = info
        .get("machine")
        .and_then(|m| m.as_str())
        .filter(|m| !m.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            url.split("://")
                .nth(1)
                .and_then(|h| h.split(':').next())
                .unwrap_or("Autre PC")
                .to_string()
        });

    let session = client
        .post(format!("{url}/v1/auth/login"))
        .json(&serde_json::json!({ "username": identifiant_compte, "password": mot_de_passe }))
        .send()
        .await
        .map_err(|e| format!("Connexion impossible : {}", chaine(&e)))?;
    if session.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Identifiant ou mot de passe incorrect pour ce serveur.".into());
    }
    if !session.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la connexion (HTTP {}).",
            session.status()
        ));
    }
    let jeton_session = session
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(str::to_string))
        .ok_or("Le serveur n'a pas renvoyé de jeton.")?;

    // Une clé à ce poste, sans expiration ; le jeton de session, lui, expire
    // au bout de trente jours. Un serveur plus ancien sans clés : la session.
    let cle = match client
        .post(format!("{url}/v1/auth/tokens"))
        .bearer_auth(&jeton_session)
        .json(&serde_json::json!({ "label": format!("Modèles pour {ce_poste}") }))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(str::to_string))
            .unwrap_or(jeton_session),
        _ => jeton_session,
    };

    let serveur = ServeurModeles {
        id: identifiant(&nom, &serveurs),
        nom,
        url,
        autorite,
    };
    set_key(host, &serveur.id, &cle)?;
    serveurs.push(serveur.clone());
    enregistrer(host.data_dir, &serveurs)?;
    tracing::info!(serveur = %serveur.url, id = %serveur.id, "serveur de modèles ajouté");
    Ok(serveur)
}

/// Retirer un serveur, et oublier sa clé.
pub fn retirer(host: &Host<'_>, id: &str) -> Result<(), String> {
    let mut serveurs = lister(host.data_dir);
    let avant = serveurs.len();
    serveurs.retain(|s| s.id != id);
    if serveurs.len() == avant {
        return Err("Ce serveur n'est plus dans la liste.".into());
    }
    let _ = crate::clear_key(host, id);
    enregistrer(host.data_dir, &serveurs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_adresse_se_normalise() {
        assert_eq!(
            normaliser("192.168.0.57:7474").unwrap(),
            "https://192.168.0.57:7474"
        );
        assert_eq!(
            normaliser("https://192.168.0.57:7474/").unwrap(),
            "https://192.168.0.57:7474"
        );
        assert_eq!(
            normaliser("192.168.0.57").unwrap(),
            "https://192.168.0.57:7474"
        );
        assert_eq!(
            normaliser("https://pc.local:9000/v1").unwrap(),
            "https://pc.local:9000"
        );
        assert_eq!(
            normaliser("http://127.0.0.1:7474").unwrap(),
            "http://127.0.0.1:7474"
        );
        assert!(normaliser("  ").is_err());
    }

    #[test]
    fn l_identifiant_est_lisible_et_unique() {
        let existants = vec![ServeurModeles {
            id: "pc-desktop-rx570".into(),
            nom: "x".into(),
            url: "u".into(),
            autorite: None,
        }];
        assert_eq!(identifiant("DESKTOP-RX570", &[]), "pc-desktop-rx570");
        assert_eq!(
            identifiant("DESKTOP RX570", &existants),
            "pc-desktop-rx570-2"
        );
        assert_eq!(identifiant("éé", &[]), "pc-serveur");
    }

    #[test]
    fn un_serveur_devient_un_catalogue() {
        let dir = std::env::temp_dir().join(format!("locaryn-serveurs-{}", uuid::Uuid::new_v4()));
        let s = ServeurModeles {
            id: "pc-fixe".into(),
            nom: "PC fixe".into(),
            url: "https://192.168.0.57:7474".into(),
            autorite: Some("PEM".into()),
        };
        enregistrer(&dir, std::slice::from_ref(&s)).unwrap();
        let d = declares(&dir);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].id, "pc-fixe");
        assert_eq!(d[0].label(), "PC fixe");
        assert_eq!(
            d[0].manifest.effective_models_url(),
            "https://192.168.0.57:7474/v1/models"
        );
        assert_eq!(d[0].autorite.as_deref(), Some("PEM"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
