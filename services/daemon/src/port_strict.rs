//! Le port Internet : un second port où le certificat client est exigé.
//!
//! Le port du service (7474) sert le réseau local, le tunnel, le bureau et
//! l'interface web. Exiger un certificat client sur lui couperait le tunnel
//! (le relais rappelle ce port sans certificat) et tout navigateur. Le mode
//! Internet ouvre donc à côté un port réservé aux appareils qui ont reçu leur
//! certificat : la poignée de main TLS échoue sans lui, avant qu'aucune
//! requête soit lue. C'est le seul port que Locaryn demande à la box
//! d'ouvrir.
//!
//! Il sert la même application que le port du service ; seule la porte
//! change.

use axum::Router;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::Mutex;

/// Ce qu'il faut pour ouvrir le port à la demande, posé au démarrage.
struct Lanceur {
    app: Router,
    data_dir: PathBuf,
    ip: IpAddr,
    noms: Vec<String>,
    port_service: u16,
}

static LANCEUR: OnceLock<Lanceur> = OnceLock::new();
/// Le port ouvert, une fois pour toute la durée du service.
static OUVERT: OnceLock<Mutex<Option<u16>>> = OnceLock::new();

/// Le port Internet : `LOCARYN_DAEMON_PUBLIC_PORT`, sinon celui du service + 1.
pub fn port_voulu(port_service: u16) -> u16 {
    std::env::var("LOCARYN_DAEMON_PUBLIC_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p != 0 && *p != port_service)
        .unwrap_or_else(|| port_service.wrapping_add(1).max(1))
}

/// Retenir l'application et l'adresse d'écoute, pour ouvrir le port plus tard.
pub fn preparer(app: Router, data_dir: PathBuf, ip: IpAddr, noms: Vec<String>, port_service: u16) {
    if LANCEUR
        .set(Lanceur {
            app,
            data_dir,
            ip,
            noms,
            port_service,
        })
        .is_err()
    {
        tracing::warn!("port Internet déjà préparé : la seconde préparation est ignorée");
    }
}

/// Ouvrir le port Internet s'il ne l'est pas encore ; rend son numéro.
pub async fn assurer() -> Result<u16, String> {
    let Some(l) = LANCEUR.get() else {
        return Err(
            "Le mode Internet exige le mode serveur : ce service n'écoute que sur cette machine."
                .into(),
        );
    };
    let verrou = OUVERT.get_or_init(|| Mutex::new(None));
    let mut ouvert = verrou.lock().await;
    if let Some(p) = *ouvert {
        return Ok(p);
    }
    let port = port_voulu(l.port_service);
    let adresse = lancer(
        l.app.clone(),
        &l.data_dir,
        l.noms.clone(),
        SocketAddr::new(l.ip, port),
    )
    .map_err(|e| format!("Port Internet {port} indisponible : {e}"))?;
    tracing::info!(
        "port Internet ouvert sur {adresse} — certificat client exigé à la poignée de main"
    );
    *ouvert = Some(adresse.port());
    Ok(adresse.port())
}

/// Écouter sur `adresse` avec le certificat client exigé ; rend l'adresse liée.
///
/// La liaison se fait tout de suite, pour qu'un port déjà pris se dise ici
/// plutôt que dans une tâche qui meurt en silence.
fn lancer(
    app: Router,
    data_dir: &Path,
    noms: Vec<String>,
    adresse: SocketAddr,
) -> anyhow::Result<SocketAddr> {
    let ca = crate::mtls::authority(data_dir)?;
    let (cert, cle) = locaryn_config::mtls::ensure_server_cert(data_dir, noms)?;
    let config = crate::mtls::server_config_requiring_clients(&cert, &cle, &ca.cert_pem)?;
    let ecoute = std::net::TcpListener::bind(adresse)?;
    ecoute.set_nonblocking(true)?;
    let liee = ecoute.local_addr()?;
    let tls = axum_server::tls_rustls::RustlsConfig::from_config(std::sync::Arc::new(config));
    tokio::spawn(async move {
        if let Err(e) = axum_server::from_tcp_rustls(ecoute, tls)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
        {
            tracing::error!(erreur = %e, "port Internet arrêté");
        }
    });
    Ok(liee)
}

/// Le certificat client d'un appareil, en PEM (certificat + clé).
///
/// Émis à chaque remise : un appareil qui le perd en reçoit un neuf. Le nom
/// voyage dans le certificat, pour que les journaux disent qui se connecte.
pub fn identite(data_dir: &Path, nom: &str) -> Result<String, String> {
    let nom = if nom.trim().is_empty() {
        "appareil"
    } else {
        nom
    };
    locaryn_config::mtls::issue_client(data_dir, nom, 365)
        .map(|c| c.bundle_pem)
        .map_err(|e| format!("certificat client impossible : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "locaryn_port_strict_{nom}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn client(ca: &str, identite: Option<&str>) -> reqwest::Client {
        let mut b = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(5))
            .add_root_certificate(reqwest::Certificate::from_pem(ca.as_bytes()).unwrap());
        if let Some(pem) = identite {
            b = b.identity(reqwest::Identity::from_pem(pem.as_bytes()).unwrap());
        }
        b.build().unwrap()
    }

    /// Le cœur de la variante : sans certificat, la porte reste fermée ; avec
    /// celui que ce serveur a émis, la même application répond.
    #[tokio::test]
    async fn seul_un_appareil_muni_de_son_certificat_passe() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let d = dossier("porte");
        let app = Router::new().route("/health", axum::routing::get(|| async { "ok" }));
        let noms = vec!["localhost".to_string(), "127.0.0.1".to_string()];
        let adresse = lancer(app, &d, noms, "127.0.0.1:0".parse().unwrap()).unwrap();
        let url = format!("https://127.0.0.1:{}/health", adresse.port());
        let ca = crate::mtls::authority(&d).unwrap().cert_pem;

        let sans = client(&ca, None).get(&url).send().await;
        assert!(
            sans.is_err(),
            "sans certificat, la poignée de main doit échouer"
        );

        let pem = identite(&d, "téléphone de test").unwrap();
        let avec = client(&ca, Some(&pem)).get(&url).send().await.unwrap();
        assert!(avec.status().is_success());
        assert_eq!(avec.text().await.unwrap(), "ok");

        // Un certificat d'une autre autorité ne vaut rien ici.
        let autre = dossier("autre");
        let etranger = identite(&autre, "intrus").unwrap();
        assert!(client(&ca, Some(&etranger)).get(&url).send().await.is_err());
        std::fs::remove_dir_all(&d).ok();
        std::fs::remove_dir_all(&autre).ok();
    }

    #[test]
    fn le_port_internet_suit_celui_du_service() {
        // La variable d'environnement n'est pas posée dans les tests.
        if std::env::var("LOCARYN_DAEMON_PUBLIC_PORT").is_err() {
            assert_eq!(port_voulu(7474), 7475);
        }
    }
}
