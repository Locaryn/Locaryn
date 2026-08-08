//! Mutual TLS: proving the *client* is legitimate, not just the server.
//!
//! A password can be guessed, phished or reused. A client certificate cannot
//! be typed into a fake page, and without one the TLS handshake fails before
//! any application data is exchanged — a scanner on an open port sees a
//! connection that refuses, not a login form to attack.
//!
//! That is what makes exposing the daemon to the internet defensible, so mTLS
//! and port forwarding belong together.
//!
//! **Deliberately opt-in.** Turning it on means every existing client stops
//! working until it has been issued a certificate, which must be a decision,
//! never a surprise from an update.

use anyhow::Context;
use std::path::Path;

#[allow(unused_imports)] // Re-exported for callers, not used inside this file.
pub use lochor_config::mtls::{
    authority, client_cert_path, issue_client, pem_blocks, Authority, ClientCredential,
};

/// A rustls server configuration that refuses clients without a certificate
/// signed by this machine's authority.
///
/// The rejection happens during the handshake, before any request is parsed,
/// which is the whole point: an attacker who found the open port gets a closed
/// connection rather than a login endpoint to work against.
pub fn server_config_requiring_clients(
    cert_path: &Path,
    key_path: &Path,
    ca_pem: &str,
) -> anyhow::Result<rustls::ServerConfig> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    let cert_pem = std::fs::read_to_string(cert_path).context("lecture du certificat serveur")?;
    let key_pem = std::fs::read_to_string(key_path).context("lecture de la clé serveur")?;

    let certs: Vec<CertificateDer<'static>> = pem_blocks(&cert_pem, "CERTIFICATE")
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    anyhow::ensure!(!certs.is_empty(), "aucun certificat dans le fichier serveur");

    let key_der = pem_blocks(&key_pem, "PRIVATE KEY")
        .into_iter()
        .next()
        .context("aucune clé privée dans le fichier serveur")?;
    let key = PrivateKeyDer::try_from(key_der)
        .map_err(|e| anyhow::anyhow!("clé privée illisible : {e}"))?;

    let mut roots = rustls::RootCertStore::empty();
    for der in pem_blocks(ca_pem, "CERTIFICATE") {
        roots
            .add(CertificateDer::from(der))
            .context("autorité illisible")?;
    }

    let verifier = rustls::server::WebPkiClientVerifier::builder(roots.into())
        .build()
        .context("construction du vérificateur de clients")?;

    rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .context("configuration TLS du serveur")
}

#[cfg(test)]
mod pem_tests {
    use super::*;

    #[test]
    fn only_blocks_of_the_requested_kind_are_returned() {
        let dir = std::env::temp_dir().join(format!(
            "lochor_pem_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // A real bundle: certificate and key in one file.
        let c = issue_client(&dir, "marie", 30).unwrap();

        let certs = pem_blocks(&c.bundle_pem, "CERTIFICATE");
        let keys = pem_blocks(&c.bundle_pem, "PRIVATE KEY");
        assert_eq!(certs.len(), 1, "un certificat attendu");
        assert_eq!(keys.len(), 1, "une clé attendue");
        assert_ne!(certs[0], keys[0], "certificat et clé confondus");
        // DER always begins with a SEQUENCE tag.
        assert_eq!(certs[0][0], 0x30, "ce n'est pas du DER");

        assert!(pem_blocks(&c.bundle_pem, "PUBLIC KEY").is_empty());
        assert!(pem_blocks("pas du pem du tout", "CERTIFICATE").is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_client_certificate_verifies_against_its_own_authority() {
        let dir = std::env::temp_dir().join(format!(
            "lochor_verify_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let c = issue_client(&dir, "marie", 30).unwrap();

        // rustls needs its cipher backend chosen before any verifier is built.
        // Ignore the error: another test in this binary may have installed it.
        let _ = rustls::crypto::ring::default_provider().install_default();

        // The verifier accepting our own authority is what the whole scheme
        // rests on; if this ever stops building, mTLS silently would not work.
        let mut roots = rustls::RootCertStore::empty();
        for der in pem_blocks(&c.ca_pem, "CERTIFICATE") {
            roots.add(rustls::pki_types::CertificateDer::from(der)).unwrap();
        }
        assert!(
            rustls::server::WebPkiClientVerifier::builder(roots.into()).build().is_ok(),
            "l'autorité générée doit être utilisable comme racine"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
