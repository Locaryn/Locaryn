//! Asking the router to forward a port, so the daemon is reachable from
//! outside the local network.
//!
//! This is for the person whose machine does the computing and who is rarely
//! next to it — travelling, at the office, away for the weekend. Most home
//! routers (Freebox, Livebox and the rest) accept UPnP requests, so no one has
//! to log into a web interface and copy port numbers.
//!
//! **It is refused unless mutual TLS is on.** Publishing a service to the
//! whole internet behind nothing but a password is not defensible, and doing it
//! automatically would be worse: the user would not even know it happened. With
//! client certificates required, a scanner that finds the port meets a
//! handshake that fails, not a login form.
//!
//! The mapping is also *leased*, not permanent. A router that loses power or is
//! reset forgets it, which is the desired behaviour — a forgotten permanent
//! hole is exactly what makes UPnP a liability.

use igd_next::{aio::tokio as igd_tokio, PortMappingProtocol, SearchOptions};
use std::net::{IpAddr, SocketAddr, SocketAddrV4};

/// How long the router keeps the mapping without renewal.
///
/// Short enough that an abandoned daemon stops being reachable within the
/// hour; long enough that a brief outage does not close the door.
const LEASE_SECONDS: u32 = 3600;

#[derive(Debug, Clone)]
pub struct Mapping {
    /// Address the outside world uses.
    pub external_ip: IpAddr,
    pub external_port: u16,
}

/// Why forwarding was refused or failed, phrased for the person reading it.
#[derive(Debug)]
pub enum ForwardError {
    /// mTLS is off. Deliberately not recoverable by retrying.
    ClientCertificatesRequired,
    NoRouter(String),
    Rejected(String),
    NoLocalAddress,
}

impl std::fmt::Display for ForwardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClientCertificatesRequired => write!(
                f,
                "Ouverture refusée : exposer ce serveur à Internet sans certificat client \
                 le laisserait protégé par un simple mot de passe. Activez d'abord \
                 l'exigence de certificat (require_client_cert), puis réessayez."
            ),
            Self::NoRouter(e) => write!(
                f,
                "Aucune box compatible trouvée sur le réseau ({e}). Certaines box ont l'UPnP \
                 désactivé par défaut — activez-le, ou ajoutez la redirection à la main."
            ),
            Self::Rejected(e) => write!(f, "La box a refusé la redirection : {e}"),
            Self::NoLocalAddress => write!(f, "Adresse locale de cette machine introuvable."),
        }
    }
}

impl std::error::Error for ForwardError {}

/// The address of this machine on the local network, which is what the router
/// must forward to.
fn local_ipv4() -> Option<std::net::Ipv4Addr> {
    // Connecting a UDP socket sends nothing; it only asks the OS which
    // interface it would route from.
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(v4) => Some(v4),
        IpAddr::V6(_) => None,
    }
}

/// Request a forwarding for `port`.
///
/// `client_certs_required` mirrors the daemon's own setting; passing it in
/// rather than reading configuration here keeps the refusal testable.
pub async fn open(port: u16, client_certs_required: bool) -> Result<Mapping, ForwardError> {
    if !client_certs_required {
        return Err(ForwardError::ClientCertificatesRequired);
    }

    let local = local_ipv4().ok_or(ForwardError::NoLocalAddress)?;
    let gateway = igd_tokio::search_gateway(SearchOptions::default())
        .await
        .map_err(|e| ForwardError::NoRouter(e.to_string()))?;

    let external_ip = gateway
        .get_external_ip()
        .await
        .map_err(|e| ForwardError::Rejected(e.to_string()))?;

    gateway
        .add_port(
            PortMappingProtocol::TCP,
            port,
            SocketAddr::V4(SocketAddrV4::new(local, port)),
            LEASE_SECONDS,
            "Lochor",
        )
        .await
        .map_err(|e| ForwardError::Rejected(e.to_string()))?;

    tracing::warn!(
        "port {port} ouvert vers Internet sur {external_ip} — bail de {} minutes, \
         renouvelé tant que le serveur tourne. Certificat client exigé.",
        LEASE_SECONDS / 60
    );

    Ok(Mapping {
        external_ip,
        external_port: port,
    })
}

/// Withdraw the forwarding. Best effort: a router that already forgot it is
/// the outcome we wanted anyway.
pub async fn close(port: u16) {
    let Ok(gateway) = igd_tokio::search_gateway(SearchOptions::default()).await else {
        return;
    };
    match gateway.remove_port(PortMappingProtocol::TCP, port).await {
        Ok(()) => tracing::info!("redirection du port {port} retirée"),
        Err(e) => tracing::debug!(error = %e, "retrait de la redirection sans effet"),
    }
}

/// Keep the lease alive while the server runs.
///
/// Renewal matters as much as the opening: without it the mapping expires
/// mid-session and the remote client drops with no explanation.
pub fn spawn_renewal(port: u16) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Renew well before expiry, so one missed attempt is not fatal.
        let every = std::time::Duration::from_secs(u64::from(LEASE_SECONDS) / 3);
        loop {
            tokio::time::sleep(every).await;
            let Some(local) = local_ipv4() else { continue };
            let Ok(gateway) = igd_tokio::search_gateway(SearchOptions::default()).await else {
                continue;
            };
            if let Err(e) = gateway
                .add_port(
                    PortMappingProtocol::TCP,
                    port,
                    SocketAddr::V4(SocketAddrV4::new(local, port)),
                    LEASE_SECONDS,
                    "Lochor",
                )
                .await
            {
                tracing::warn!(error = %e, "renouvellement de la redirection échoué");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn forwarding_without_client_certificates_is_refused_outright() {
        // The guard must fire before any network activity: no router lookup,
        // no partial state, and the same answer every time.
        let err = open(7474, false).await.unwrap_err();
        assert!(
            matches!(err, ForwardError::ClientCertificatesRequired),
            "attendu un refus, obtenu {err:?}"
        );
        let msg = err.to_string();
        assert!(msg.contains("certificat client"), "message peu clair : {msg}");
        // It must say what to do, not merely that it failed.
        assert!(msg.contains("require_client_cert"), "message sans issue : {msg}");
    }

    #[test]
    fn the_lease_is_short_enough_to_expire_on_its_own() {
        // A permanent mapping left behind by an abandoned daemon is the whole
        // reason UPnP has a bad reputation.
        assert!(LEASE_SECONDS > 0, "un bail nul est permanent chez certaines box");
        assert!(
            LEASE_SECONDS <= 7200,
            "bail trop long : une redirection oubliée reste ouverte des heures"
        );
    }

    #[test]
    fn every_failure_tells_the_user_what_to_do() {
        for e in [
            ForwardError::NoRouter("timeout".into()),
            ForwardError::Rejected("718".into()),
            ForwardError::NoLocalAddress,
            ForwardError::ClientCertificatesRequired,
        ] {
            let m = e.to_string();
            assert!(m.len() > 30, "message trop court : {m}");
            assert!(!m.contains("Error"), "jargon technique brut : {m}");
        }
    }
}
