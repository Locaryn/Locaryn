//! Mutual TLS: proving the *client* is legitimate, not just the server.
//!
//! A password can be guessed, phished or reused. A client certificate cannot be
//! typed into a fake page, and without one the TLS handshake fails before any
//! application data is exchanged — a scanner on an open port meets a connection
//! that refuses, not a login form to attack. That is what makes forwarding a
//! port to the internet defensible.
//!
//! Lives in the shared crate because three programs need it: the daemon
//! verifies certificates, the CLI issues them on a headless server, and the
//! desktop offers them for installation.
//!
//! **Deliberately opt-in.** Enabling it stops every existing client until each
//! has a certificate — a decision, never a surprise from an update.

use anyhow::Context;
use std::path::{Path, PathBuf};

/// Files making up the local certificate authority.
pub struct Authority {
    pub cert_pem: String,
    pub key_pem: String,
}

fn ca_paths(data_dir: &Path) -> (PathBuf, PathBuf) {
    let dir = data_dir.join("tls");
    (dir.join("ca-cert.pem"), dir.join("ca-key.pem"))
}

/// Where a client certificate for `name` is written.
pub fn client_cert_path(data_dir: &Path, name: &str) -> PathBuf {
    // Keep the file name predictable but safe: a user name reaches the
    // filesystem here.
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Plain PEM, not PKCS#12: naming it .p12 would send people to an import
    // dialog that rejects it.
    data_dir
        .join("tls")
        .join("clients")
        .join(format!("{safe}.pem"))
}

/// Load the authority, creating it on first use.
///
/// The CA is what lets the server recognise the certificates it issued. It is
/// generated once and must then persist: regenerating it would invalidate every
/// certificate already handed out.
pub fn authority(data_dir: &Path) -> anyhow::Result<Authority> {
    let (cert_path, key_path) = ca_paths(data_dir);
    if cert_path.is_file() && key_path.is_file() {
        return Ok(Authority {
            cert_pem: std::fs::read_to_string(&cert_path)
                .context("lecture du certificat d'autorité")?,
            key_pem: std::fs::read_to_string(&key_path).context("lecture de la clé d'autorité")?,
        });
    }

    std::fs::create_dir_all(cert_path.parent().unwrap()).context("création du dossier tls")?;

    let mut params =
        rcgen::CertificateParams::new(Vec::<String>::new()).context("paramètres de l'autorité")?;
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Constrained(0));
    params.distinguished_name = {
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Locaryn local CA");
        dn.push(rcgen::DnType::OrganizationName, "Locaryn");
        dn
    };
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
        rcgen::KeyUsagePurpose::DigitalSignature,
    ];

    let key = rcgen::KeyPair::generate().context("clé de l'autorité")?;
    let cert = params
        .self_signed(&key)
        .context("auto-signature de l'autorité")?;

    let (cert_pem, key_pem) = (cert.pem(), key.serialize_pem());
    std::fs::write(&cert_path, &cert_pem).context("écriture du certificat d'autorité")?;
    std::fs::write(&key_path, &key_pem).context("écriture de la clé d'autorité")?;
    restrict(&key_path);
    tracing::info!(path = %cert_path.display(), "autorité de certification créée");

    Ok(Authority { cert_pem, key_pem })
}

/// A certificate for one person or one device.
pub struct ClientCredential {
    pub path: PathBuf,
    /// Certificate and key together, which is what clients ask for.
    pub bundle_pem: String,
    /// The authority to trust, so the client can also verify the server.
    pub ca_pem: String,
}

/// Issue a client certificate valid for `days`.
pub fn issue_client(data_dir: &Path, name: &str, days: u32) -> anyhow::Result<ClientCredential> {
    anyhow::ensure!(!name.trim().is_empty(), "nom vide");
    let ca = authority(data_dir)?;

    let ca_key = rcgen::KeyPair::from_pem(&ca.key_pem).context("clé d'autorité illisible")?;
    let ca_params = rcgen::CertificateParams::from_ca_cert_pem(&ca.cert_pem)
        .context("certificat d'autorité illisible")?;
    let ca_cert = ca_params
        .self_signed(&ca_key)
        .context("reconstruction de l'autorité")?;

    let mut params = rcgen::CertificateParams::new(Vec::<String>::new())
        .context("paramètres du certificat client")?;
    params.distinguished_name = {
        let mut dn = rcgen::DistinguishedName::new();
        // The name travels inside the certificate, so the server knows who
        // connected before a single request is read.
        dn.push(rcgen::DnType::CommonName, name.trim());
        dn.push(rcgen::DnType::OrganizationName, "Locaryn");
        dn
    };
    params.use_authority_key_identifier_extension = true;
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ClientAuth];
    let now = std::time::SystemTime::now();
    params.not_before = now.into();
    params.not_after = (now + std::time::Duration::from_secs(u64::from(days) * 86_400)).into();

    let key = rcgen::KeyPair::generate().context("clé du client")?;
    let cert = params
        .signed_by(&key, &ca_cert, &ca_key)
        .context("signature du certificat client")?;

    let bundle = format!("{}{}", cert.pem(), key.serialize_pem());
    let path = client_cert_path(data_dir, name);
    std::fs::create_dir_all(path.parent().unwrap()).context("création du dossier clients")?;
    std::fs::write(&path, &bundle).context("écriture du certificat client")?;
    restrict(&path);

    Ok(ClientCredential {
        path,
        bundle_pem: bundle,
        ca_pem: ca.cert_pem,
    })
}

fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        if let Ok(md) = std::fs::metadata(path) {
            let mut p = md.permissions();
            p.set_readonly(true);
            let _ = std::fs::set_permissions(path, p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "locaryn_mtls_{tag}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_authority_is_created_once_and_then_reused() {
        let dir = scratch("ca");
        let a = authority(&dir).expect("création");
        assert!(a.cert_pem.starts_with("-----BEGIN CERTIFICATE-----"));

        // Regenerating would invalidate every certificate already handed out.
        let b = authority(&dir).expect("réutilisation");
        assert_eq!(
            a.cert_pem, b.cert_pem,
            "l'autorité ne doit pas être régénérée"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_client_credential_carries_its_key_and_the_authority() {
        let dir = scratch("client");
        let c = issue_client(&dir, "marie", 365).expect("émission");

        assert!(c.path.is_file());
        assert!(c.bundle_pem.contains("BEGIN CERTIFICATE"));
        assert!(
            c.bundle_pem.contains("PRIVATE KEY"),
            "sans la clé le certificat est inutilisable"
        );
        assert!(c.ca_pem.contains("BEGIN CERTIFICATE"));
        // The bundle must not be the authority itself.
        assert_ne!(c.bundle_pem, c.ca_pem);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn two_people_get_different_certificates_in_different_files() {
        let dir = scratch("two");
        let a = issue_client(&dir, "marie", 365).unwrap();
        let b = issue_client(&dir, "paul", 365).unwrap();
        assert_ne!(a.path, b.path);
        assert_ne!(a.bundle_pem, b.bundle_pem);
        // Same authority, so the server recognises both.
        assert_eq!(a.ca_pem, b.ca_pem);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_name_cannot_escape_the_certificates_directory() {
        let dir = scratch("path");
        // A user name reaches the filesystem here.
        let p = client_cert_path(&dir, "../../etc/passwd");
        assert!(
            p.starts_with(dir.join("tls").join("clients")),
            "échappement: {p:?}"
        );
        assert!(!p.to_string_lossy().contains(".."));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_empty_name_is_refused() {
        let dir = scratch("empty");
        assert!(issue_client(&dir, "   ", 365).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// Issue the *server* certificate from the same authority.
///
/// With mutual TLS the client already trusts this CA in order to be verified
/// by it. Leaving the server on a separate self-signed certificate would force
/// every client to trust two unrelated things, and a self-signed server
/// certificate is exactly what a client validating against the CA rejects.
/// One authority, both directions.
pub fn issue_server(data_dir: &Path, names: Vec<String>) -> anyhow::Result<(String, String)> {
    let ca = authority(data_dir)?;
    let ca_key = rcgen::KeyPair::from_pem(&ca.key_pem).context("clé d'autorité illisible")?;
    let ca_params = rcgen::CertificateParams::from_ca_cert_pem(&ca.cert_pem)
        .context("certificat d'autorité illisible")?;
    let ca_cert = ca_params
        .self_signed(&ca_key)
        .context("reconstruction de l'autorité")?;

    let mut params =
        rcgen::CertificateParams::new(names).context("paramètres du certificat serveur")?;
    params.distinguished_name = {
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Locaryn server");
        dn.push(rcgen::DnType::OrganizationName, "Locaryn");
        dn
    };
    params.use_authority_key_identifier_extension = true;
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];

    let key = rcgen::KeyPair::generate().context("clé du serveur")?;
    let cert = params
        .signed_by(&key, &ca_cert, &ca_key)
        .context("signature du certificat serveur")?;
    Ok((cert.pem(), key.serialize_pem()))
}

/// Paths of the CA-issued server certificate.
pub fn server_cert_paths(data_dir: &Path) -> (PathBuf, PathBuf) {
    let dir = data_dir.join("tls");
    (dir.join("server-cert.pem"), dir.join("server-key.pem"))
}

/// Write — or reuse — a server certificate signed by the authority.
pub fn ensure_server_cert(
    data_dir: &Path,
    names: Vec<String>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let (cert_path, key_path) = server_cert_paths(data_dir);
    if cert_path.is_file() && key_path.is_file() {
        return Ok((cert_path, key_path));
    }
    let (cert_pem, key_pem) = issue_server(data_dir, names)?;
    std::fs::create_dir_all(cert_path.parent().unwrap()).context("création du dossier tls")?;
    std::fs::write(&cert_path, cert_pem).context("écriture du certificat serveur")?;
    std::fs::write(&key_path, key_pem).context("écriture de la clé serveur")?;
    restrict(&key_path);
    tracing::info!("certificat serveur émis par l'autorité locale");
    Ok((cert_path, key_path))
}

/// Decode every PEM block of a given kind into DER.
///
/// Written here rather than pulled from a crate because the format is two
/// markers around base64, and because both the server and the client need the
/// same answer: a bundle that one side reads as "certificate + key" and the
/// other as "certificate only" fails at handshake time with no useful message.
pub fn pem_blocks(pem: &str, kind: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut body: Option<String> = None;
    for line in pem.lines() {
        let l = line.trim();
        if l.starts_with("-----BEGIN") {
            // Accept "RSA PRIVATE KEY" and "PRIVATE KEY" alike.
            body = l.contains(kind).then(String::new);
        } else if l.starts_with("-----END") {
            if let Some(b) = body.take() {
                if let Some(der) = crate::provision::base64_decode(&b) {
                    out.push(der);
                }
            }
        } else if let Some(b) = body.as_mut() {
            b.push_str(l);
        }
    }
    out
}

#[cfg(test)]
mod server_cert_tests {
    use super::*;

    #[test]
    fn the_server_certificate_shares_the_client_authority() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_srvcert_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let (cert, key) = ensure_server_cert(&dir, vec!["localhost".into(), "127.0.0.1".into()])
            .expect("émission");
        assert!(cert.is_file() && key.is_file());

        let client = issue_client(&dir, "marie", 30).unwrap();
        let ca_now = authority(&dir).unwrap();
        // Both sides must chain to the same root, or a client trusting the CA
        // still rejects the server it is talking to.
        assert_eq!(client.ca_pem, ca_now.cert_pem);

        // Reused, not reissued: a changing server certificate would break
        // pinned fingerprints on every restart.
        let (c2, _) = ensure_server_cert(&dir, vec!["localhost".into()]).unwrap();
        assert_eq!(
            std::fs::read_to_string(&cert).unwrap(),
            std::fs::read_to_string(&c2).unwrap()
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
