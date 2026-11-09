//! Ce qu'il manque à un relais avant de pouvoir l'ouvrir : l'outil, puis le
//! compte.
//!
//! Rien ne s'installe ni ne se connecte en silence. Pour l'outil, on donne la
//! page officielle de téléchargement ; pour le compte, la page officielle de
//! connexion — ou la commande de connexion de l'outil, qui ouvre elle-même le
//! navigateur. Locaryn ne crée aucun compte et ne saisit aucun identifiant :
//! le seul secret qui passe ici est le jeton que la personne colle elle-même,
//! remis tel quel à l'outil du relais.

use crate::providers::Provider;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

/// Combien de temps attendre qu'un outil dise s'il est connecté.
const STATUT_DELAI: Duration = Duration::from_secs(10);

/// Ce que l'interface affiche pour guider la connexion d'un relais.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignIn {
    /// Rien à faire : pas de compte pour ce relais.
    None,
    /// Coller un jeton lu sur la page de connexion.
    PasteToken,
    /// Lancer la commande de connexion de l'outil, qui ouvre le navigateur.
    Command,
}

impl Provider {
    /// La page officielle où obtenir l'outil.
    pub fn download_url(&self) -> Option<&'static str> {
        match self {
            Self::Cloudflare => Some(
                "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/",
            ),
            Self::Ngrok => Some("https://ngrok.com/download"),
            Self::DevTunnel => Some(
                "https://learn.microsoft.com/azure/developer/dev-tunnels/get-started",
            ),
            // Le client SSH est livré avec le système.
            Self::Ssh => None,
        }
    }

    /// La page officielle où lire le jeton à coller (connexion ou
    /// inscription d'abord, si besoin).
    pub fn login_url(&self) -> Option<&'static str> {
        match self {
            Self::Ngrok => Some("https://dashboard.ngrok.com/get-started/your-authtoken"),
            // `devtunnel user login` ouvre lui-même la page Microsoft.
            Self::DevTunnel | Self::Cloudflare | Self::Ssh => None,
        }
    }

    pub fn sign_in(&self) -> SignIn {
        match self {
            Self::Ngrok => SignIn::PasteToken,
            Self::DevTunnel => SignIn::Command,
            Self::Cloudflare | Self::Ssh => SignIn::None,
        }
    }

    /// Le compte est-il prêt ? Vrai pour un relais sans compte.
    ///
    /// Lu sur la machine, jamais supposé : la configuration de ngrok porte
    /// son jeton, devtunnel dit lui-même s'il est connecté.
    pub async fn account_ready(&self) -> bool {
        match self {
            Self::Cloudflare | Self::Ssh => true,
            Self::Ngrok => ngrok_a_un_jeton(),
            Self::DevTunnel => devtunnel_connecte().await,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SignInError {
    #[error("{0} ne se connecte pas depuis Locaryn : aucun compte n'est nécessaire.")]
    Inutile(&'static str),
    #[error("Collez le jeton affiché sur la page ngrok.")]
    JetonManquant,
    #[error("Ce jeton contient des caractères inattendus : recopiez-le depuis la page ngrok.")]
    JetonIllisible,
    #[error("{0} n'est pas installé.")]
    NonInstalle(&'static str),
    #[error("{0} : {1}")]
    Outil(&'static str, String),
}

/// Connecter le compte d'un relais.
///
/// ngrok : le jeton collé par la personne est enregistré par ngrok lui-même
/// (`ngrok config add-authtoken`). devtunnel : `devtunnel user login` est
/// lancé et ouvre la page de connexion Microsoft ; la personne s'y connecte,
/// l'interface relit ensuite l'état.
pub async fn sign_in(provider: Provider, token: Option<&str>) -> Result<(), SignInError> {
    if !provider.is_available() {
        return Err(SignInError::NonInstalle(provider.binary()));
    }
    match provider.sign_in() {
        SignIn::None => Err(SignInError::Inutile(provider.label())),
        SignIn::PasteToken => {
            let jeton = token.map(str::trim).unwrap_or_default();
            if jeton.is_empty() {
                return Err(SignInError::JetonManquant);
            }
            if !jeton_lisible(jeton) {
                return Err(SignInError::JetonIllisible);
            }
            let sortie = commande(provider)
                .args(["config", "add-authtoken", jeton])
                .stdin(Stdio::null())
                .output()
                .await
                .map_err(|e| SignInError::Outil(provider.binary(), e.to_string()))?;
            if sortie.status.success() {
                Ok(())
            } else {
                Err(SignInError::Outil(
                    provider.binary(),
                    String::from_utf8_lossy(&sortie.stderr).trim().to_string(),
                ))
            }
        }
        SignIn::Command => {
            // Détaché : la commande attend que la connexion se termine dans le
            // navigateur, l'interface relit l'état de son côté.
            let mut enfant = commande(provider)
                .args(["user", "login"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| SignInError::Outil(provider.binary(), e.to_string()))?;
            tokio::spawn(async move {
                if let Err(e) = enfant.wait().await {
                    tracing::warn!(erreur = %e, "connexion devtunnel interrompue");
                }
            });
            Ok(())
        }
    }
}

fn commande(provider: Provider) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(locaryn_config::resolve_program(provider.binary()));
    locaryn_config::hide_console(c.as_std_mut());
    c
}

/// Un jeton ngrok : lettres, chiffres et soulignés. Tout le reste serait passé
/// en argument à un programme ; on refuse plutôt que de deviner.
fn jeton_lisible(jeton: &str) -> bool {
    jeton.len() <= 256 && jeton.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Les fichiers de configuration que ngrok lit, selon le système et la
/// version (la v2 gardait `~/.ngrok2`).
fn configs_ngrok() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let maison = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(PathBuf::from(local).join("ngrok").join("ngrok.yml"));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        out.push(PathBuf::from(xdg).join("ngrok").join("ngrok.yml"));
    }
    if let Some(m) = maison {
        out.push(m.join(".config").join("ngrok").join("ngrok.yml"));
        out.push(
            m.join("Library")
                .join("Application Support")
                .join("ngrok")
                .join("ngrok.yml"),
        );
        out.push(m.join(".ngrok2").join("ngrok.yml"));
    }
    out
}

fn ngrok_a_un_jeton() -> bool {
    if std::env::var("NGROK_AUTHTOKEN").is_ok_and(|v| !v.trim().is_empty()) {
        return true;
    }
    configs_ngrok()
        .iter()
        .any(|p| std::fs::read_to_string(p).is_ok_and(|texte| config_porte_un_jeton(&texte)))
}

/// `authtoken: …` non vide, à la racine ou sous `agent:` (format v3).
fn config_porte_un_jeton(texte: &str) -> bool {
    texte.lines().any(|l| {
        l.trim()
            .strip_prefix("authtoken:")
            .is_some_and(|v| !v.trim().trim_matches(['"', '\'']).is_empty())
    })
}

async fn devtunnel_connecte() -> bool {
    let sortie = commande(Provider::DevTunnel)
        .args(["user", "show"])
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(STATUT_DELAI, sortie).await {
        Ok(Ok(s)) => {
            let texte = String::from_utf8_lossy(&s.stdout).to_lowercase();
            s.status.success() && texte.contains("logged in as")
        }
        Ok(Err(e)) => {
            tracing::warn!(erreur = %e, "état de connexion devtunnel illisible");
            false
        }
        Err(_) => {
            tracing::warn!("devtunnel n'a pas répondu à temps sur son état de connexion");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_jeton_ngrok_se_lit_dans_la_config() {
        assert!(config_porte_un_jeton(
            "version: \"3\"\nagent:\n    authtoken: 2abc_DEF\n"
        ));
        assert!(config_porte_un_jeton("authtoken: 2abc\n"));
        assert!(!config_porte_un_jeton(
            "version: \"3\"\nagent:\n    authtoken: \"\"\n"
        ));
        assert!(!config_porte_un_jeton("version: \"3\"\n"));
    }

    #[test]
    fn un_jeton_bizarre_est_refuse() {
        assert!(jeton_lisible("2abcDEF_123"));
        assert!(!jeton_lisible("abc; rm -rf /"));
        assert!(!jeton_lisible("--config=x"));
    }

    #[test]
    fn chaque_relais_a_sa_page_officielle() {
        for p in Provider::ALL {
            if p != Provider::Ssh {
                assert!(p.download_url().is_some_and(|u| u.starts_with("https://")));
            }
            if p.sign_in() == SignIn::PasteToken {
                assert!(p.login_url().is_some());
            }
        }
    }
}
