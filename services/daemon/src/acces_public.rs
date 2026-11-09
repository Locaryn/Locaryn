//! L'adresse publique par défaut d'un code « port ouvert ».
//!
//! Personne ne devrait avoir à taper son IP publique : on la lit. D'abord en
//! demandant à la box d'ouvrir le port (UPnP), ce qui donne l'IP et rend le
//! port joignable d'un coup ; à défaut, la box est seulement interrogée sur son
//! IP extérieure, puis un service d'IP publique en dernier recours. Le port
//! par défaut est celui du service, et la personne peut en choisir un autre.
//!
//! Quand rien ne garantit que le port est joignable de dehors, on le dit, et on
//! oriente vers le tunnel : un QR qui mène à une porte fermée est pire qu'un
//! message.

use crate::port_forward;
use igd_next::{aio::tokio as igd_tokio, SearchOptions};
use std::net::IpAddr;
use std::sync::OnceLock;
use tokio::sync::Mutex;

/// Service d'IP publique, en dernier recours. Ne reçoit que la requête elle-même.
const SERVICE_IP: &str = "https://api.ipify.org";

/// L'adresse publique retenue, et ce qu'on sait de sa joignabilité.
#[derive(Debug, Clone)]
pub struct AdressePublique {
    pub url: String,
    /// Rien ne garantit que le port est joignable de dehors. `None` quand la
    /// box a accepté la redirection.
    pub avertissement: Option<Avertissement>,
}

/// Pourquoi le port risque d'être fermé : une ligne à afficher, une cause que
/// l'interface sait traiter, et le détail technique à part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Avertissement {
    /// `certificat` (ouverture refusée sans certificat client), `box` (pas de
    /// box UPnP, ou refus), `operateur` (NAT d'opérateur), `port` (port choisi
    /// à la main, redirection non vérifiable).
    pub cause: &'static str,
    /// Une ligne courte, lisible d'un coup d'œil.
    pub ligne: String,
    /// Ce qui s'est passé, pour qui veut savoir.
    pub detail: String,
}

/// La redirection UPnP ouverte à la demande, gardée pour la durée du service
/// (avec son renouvellement), pour ne pas la redemander à chaque QR.
static OUVERTURE: OnceLock<Mutex<Option<IpAddr>>> = OnceLock::new();

/// Lire « 8443 », « :8443 » ou rien : le port voulu, s'il y en a un.
pub fn port_saisi(brut: &str) -> Option<Result<u16, String>> {
    let b = brut.trim().trim_start_matches(':');
    if b.is_empty() {
        return None;
    }
    if !b.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(match b.parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        _ => Err(format!("Port invalide : « {b} » (1 à 65535).")),
    })
}

/// L'adresse publique par défaut : IP publique de cette machine, port du
/// service ou celui que la personne a choisi.
pub async fn par_defaut(
    http: &reqwest::Client,
    port_service: u16,
    port_voulu: Option<u16>,
    certificats_exiges: bool,
) -> Result<AdressePublique, String> {
    let port = port_voulu.unwrap_or(port_service);

    // La box ne redirige que le port du service vers lui-même : un autre port
    // choisi à la main suppose une redirection posée par la personne.
    let (ip, refus) = if port == port_service {
        match ouvrir(port_service, certificats_exiges).await {
            Ok(ip) => (Some(ip), None),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };

    let ip = match ip {
        Some(ip) => ip,
        None => ip_exterieure(http).await?,
    };

    let avertissement = if refus.is_none() && port == port_service {
        None
    } else {
        Some(avertir(port, refus.as_ref(), partagee_par_l_operateur(ip)))
    };

    Ok(AdressePublique {
        url: url_de(ip, port),
        avertissement,
    })
}

/// La ligne à afficher quand le port n'est pas confirmé ouvert.
fn avertir(
    port: u16,
    refus: Option<&port_forward::ForwardError>,
    operateur: bool,
) -> Avertissement {
    let redirection = format!(
        "Le code ne fonctionne de l'extérieur que si la box redirige le port {port} (TCP) \
         vers cette machine."
    );
    if operateur {
        return Avertissement {
            cause: "operateur",
            ligne: "Adresse partagée par l'opérateur : port impossible à ouvrir".into(),
            detail: "L'opérateur partage cette adresse publique entre plusieurs abonnés \
                     (NAT d'opérateur) : aucun port ne peut être ouvert vers cette machine."
                .into(),
        };
    }
    match refus {
        Some(port_forward::ForwardError::ClientCertificatesRequired) => Avertissement {
            cause: "certificat",
            ligne: format!("Port {port} fermé : certificat client exigé pour l'ouvrir"),
            detail: format!(
                "Locaryn n'ouvre pas de port vers Internet tant que le certificat client \
                 n'est pas exigé (require_client_cert) : sinon le serveur ne serait protégé \
                 que par un mot de passe. {redirection}"
            ),
        },
        Some(e) => Avertissement {
            cause: "box",
            ligne: format!("Port {port} fermé sur la box"),
            detail: format!("{e} {redirection}"),
        },
        None => Avertissement {
            cause: "port",
            ligne: format!("Port {port} : redirection non vérifiée"),
            detail: format!("Locaryn ne redirige que le port du service. {redirection}"),
        },
    }
}

/// Ouvrir le port du service sur la box, une fois pour toute la durée du service.
async fn ouvrir(port: u16, certificats_exiges: bool) -> Result<IpAddr, port_forward::ForwardError> {
    let verrou = OUVERTURE.get_or_init(|| Mutex::new(None));
    let mut deja = verrou.lock().await;
    if let Some(ip) = *deja {
        return Ok(ip);
    }
    let m = port_forward::open(port, certificats_exiges).await?;
    // Renouvelé tant que le service tourne ; à son arrêt, le bail expire de
    // lui-même dans l'heure.
    std::mem::drop(port_forward::spawn_renewal(port));
    *deja = Some(m.external_ip);
    Ok(m.external_ip)
}

/// L'IP extérieure sans rien ouvrir : la box d'abord, un service public ensuite.
async fn ip_exterieure(http: &reqwest::Client) -> Result<IpAddr, String> {
    match igd_tokio::search_gateway(SearchOptions::default()).await {
        Ok(gw) => match gw.get_external_ip().await {
            Ok(ip) if !ip.is_unspecified() => return Ok(ip),
            Ok(_) => tracing::debug!("la box ne connaît pas son adresse extérieure"),
            Err(e) => tracing::debug!(erreur = %e, "adresse extérieure refusée par la box"),
        },
        Err(e) => tracing::debug!(erreur = %e, "aucune box UPnP"),
    }
    let texte = http
        .get(SERVICE_IP)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Adresse publique introuvable : {e}"))?
        .text()
        .await
        .map_err(|e| format!("Adresse publique illisible : {e}"))?;
    texte
        .trim()
        .parse::<IpAddr>()
        .map_err(|_| "Adresse publique illisible.".to_string())
}

/// 100.64.0.0/10 : l'espace du NAT d'opérateur (CGNAT).
fn partagee_par_l_operateur(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (64..128).contains(&o[1])
        }
        IpAddr::V6(_) => false,
    }
}

fn url_de(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(v4) => format!("https://{v4}:{port}"),
        IpAddr::V6(v6) => format!("https://[{v6}]:{port}"),
    }
}

/// Le lien `locaryn://connect` qui mène un PC à ce serveur, avec l'autorité à
/// installer pour vérifier son certificat.
pub fn lien_pc(url: &str) -> String {
    let base = url.trim_end_matches('/');
    format!(
        "locaryn://connect?server={}&ca={}",
        encoder(base),
        encoder(&format!("{base}/v1/pairing/ca"))
    )
}

fn encoder(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_port_seul_se_lit_et_une_adresse_ne_passe_pas_pour_un_port() {
        assert_eq!(port_saisi(""), None);
        assert_eq!(port_saisi(":8443"), Some(Ok(8443)));
        assert_eq!(port_saisi("8443"), Some(Ok(8443)));
        assert!(matches!(port_saisi("99999"), Some(Err(_))));
        assert_eq!(port_saisi("maison.exemple:8443"), None);
    }

    #[test]
    fn l_avertissement_tient_en_une_ligne_et_garde_sa_cause() {
        let a = avertir(
            7474,
            Some(&port_forward::ForwardError::ClientCertificatesRequired),
            false,
        );
        assert_eq!(a.cause, "certificat");
        assert!(a.ligne.len() < 80 && !a.ligne.contains('\n'));
        assert!(a.detail.contains("require_client_cert"));
        assert_eq!(avertir(7474, None, true).cause, "operateur");
        assert_eq!(avertir(8443, None, false).cause, "port");
        let b = avertir(
            7474,
            Some(&port_forward::ForwardError::NoRouter("délai".into())),
            false,
        );
        assert_eq!(
            (b.cause, b.ligne.as_str()),
            ("box", "Port 7474 fermé sur la box")
        );
    }

    #[test]
    fn le_nat_d_operateur_est_reconnu() {
        assert!(partagee_par_l_operateur("100.72.3.4".parse().unwrap()));
        assert!(!partagee_par_l_operateur("82.64.1.2".parse().unwrap()));
    }

    #[test]
    fn le_lien_pc_porte_le_serveur_et_son_autorite() {
        let l = lien_pc("https://82.64.1.2:7474");
        assert_eq!(
            l,
            "locaryn://connect?server=https%3A%2F%2F82.64.1.2%3A7474&ca=https%3A%2F%2F82.64.1.2%3A7474%2Fv1%2Fpairing%2Fca"
        );
        assert_eq!(url_de("::1".parse().unwrap(), 7474), "https://[::1]:7474");
    }
}
