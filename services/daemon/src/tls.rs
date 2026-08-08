//! Transport encryption for the daemon.
//!
//! Once the daemon is reachable from the network, every request carries a
//! bearer token — and over plain HTTP that token is readable by anyone on the
//! path. Encryption is therefore not optional for an exposed daemon.
//!
//! Two ways to get a certificate:
//!
//! * **Supplied** — point `tls_cert` / `tls_key` at real files. This is what a
//!   company with its own certificate authority will use.
//! * **Generated** — for a machine on a local network with no certificate at
//!   all, one is created on first exposure and reused afterwards. Clients see
//!   an untrusted-issuer warning, which is expected: the fingerprint is logged
//!   so it can be compared once and accepted deliberately.
//!
//! A self-signed certificate stops passive eavesdropping. It does not by
//! itself prove *which* machine answered, so the fingerprint check matters.

use anyhow::Context;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct TlsFiles {
    pub cert: PathBuf,
    pub key: PathBuf,
    /// True when this pair was generated rather than supplied.
    pub self_signed: bool,
}

/// Resolve the certificate to serve with, generating one if needed.
pub fn resolve(
    data_dir: &Path,
    cert: Option<&str>,
    key: Option<&str>,
    host: &str,
) -> anyhow::Result<TlsFiles> {
    match (cert, key) {
        (Some(c), Some(k)) => {
            let (c, k) = (PathBuf::from(c), PathBuf::from(k));
            // Fail loudly. Falling back to plaintext when a certificate was
            // requested is how someone ends up believing they are encrypted.
            anyhow::ensure!(c.is_file(), "certificat TLS introuvable : {}", c.display());
            anyhow::ensure!(k.is_file(), "clé TLS introuvable : {}", k.display());
            Ok(TlsFiles { cert: c, key: k, self_signed: false })
        }
        (None, None) => generate(data_dir, host),
        _ => anyhow::bail!(
            "TLS incomplet : indiquez le certificat *et* la clé, ou aucun des deux \
             (un certificat sera alors généré)."
        ),
    }
}

/// Create — or reuse — a self-signed certificate under `<data_dir>/tls`.
fn generate(data_dir: &Path, host: &str) -> anyhow::Result<TlsFiles> {
    let dir = data_dir.join("tls");
    std::fs::create_dir_all(&dir).context("création du dossier tls")?;
    let cert_path = dir.join("daemon-cert.pem");
    let key_path = dir.join("daemon-key.pem");

    if cert_path.is_file() && key_path.is_file() {
        tracing::info!(cert = %cert_path.display(), "certificat auto-signé réutilisé");
        return Ok(TlsFiles { cert: cert_path, key: key_path, self_signed: true });
    }

    // Names clients might use to reach this machine. A certificate valid only
    // for "localhost" would be rejected the moment someone connects by IP.
    let mut names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    if !host.is_empty() && host != "0.0.0.0" && !names.contains(&host.to_string()) {
        names.push(host.to_string());
    }
    if let Ok(name) = hostname() {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    for ip in local_ips() {
        if !names.contains(&ip) {
            names.push(ip);
        }
    }

    let generated = rcgen::generate_simple_self_signed(names.clone())
        .context("génération du certificat auto-signé")?;
    std::fs::write(&cert_path, generated.cert.pem()).context("écriture du certificat")?;
    std::fs::write(&key_path, generated.key_pair.serialize_pem()).context("écriture de la clé")?;
    restrict(&key_path);

    tracing::warn!(
        "certificat auto-signé généré pour {} — les clients afficheront un \
         avertissement d'émetteur inconnu au premier contact, c'est attendu. \
         Empreinte à comparer : {}",
        names.join(", "),
        fingerprint(generated.cert.der())
    );
    Ok(TlsFiles { cert: cert_path, key: key_path, self_signed: true })
}

/// SHA-256 of the DER certificate, in the colon-separated form browsers show.
fn fingerprint(der: &[u8]) -> String {
    // Shared with the provisioning command and the client, so all three print
    // the same value for the same certificate.
    locaryn_config::provision::sha256(der)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn hostname() -> anyhow::Result<String> {
    let out = std::process::Command::new(if cfg!(windows) { "hostname" } else { "uname" })
        .args(if cfg!(windows) { vec![] } else { vec!["-n"] })
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Addresses this machine answers on, so a client can connect by IP.
fn local_ips() -> Vec<String> {
    // Deliberately dependency-free: bind a UDP socket to discover the address
    // the OS would route from. No packet is sent.
    let mut out = Vec::new();
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                out.push(addr.ip().to_string());
            }
        }
    }
    out
}

/// Keep the private key readable only by its owner.
fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        // NTFS inherits the data directory's ACL, which is already user-scoped;
        // flag it read-only so it is not casually overwritten.
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

    #[test]
    fn sha256_matches_known_vectors() {
        // Without this the fingerprint we print could be quietly wrong, and a
        // fingerprint nobody can verify is worse than none.
        use locaryn_config::provision::sha256;
        let hex = |d: [u8; 32]| d.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(
            hex(sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Longer than one block, to exercise the multi-block path.
        assert_eq!(
            hex(sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn a_half_configured_tls_pair_is_refused() {
        let dir = std::env::temp_dir();
        // Silently serving plaintext when a certificate was named is the
        // failure this prevents.
        assert!(resolve(&dir, Some("cert.pem"), None, "127.0.0.1").is_err());
        assert!(resolve(&dir, None, Some("key.pem"), "127.0.0.1").is_err());
    }

    #[test]
    fn a_missing_certificate_file_is_an_error_not_a_downgrade() {
        let dir = std::env::temp_dir();
        let err = resolve(&dir, Some("D:/nope/cert.pem"), Some("D:/nope/key.pem"), "127.0.0.1")
            .unwrap_err()
            .to_string();
        assert!(err.contains("introuvable"), "got: {err}");
    }

    #[test]
    fn a_generated_pair_is_reused_on_the_next_start() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_tls_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let first = resolve(&dir, None, None, "192.168.1.10").expect("génération");
        assert!(first.self_signed);
        assert!(first.cert.is_file() && first.key.is_file());
        let pem = std::fs::read_to_string(&first.cert).unwrap();
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"));

        // Regenerating on every start would change the fingerprint each time,
        // making the one-off trust decision meaningless.
        let second = resolve(&dir, None, None, "192.168.1.10").expect("réutilisation");
        assert_eq!(first.cert, second.cert);
        assert_eq!(pem, std::fs::read_to_string(&second.cert).unwrap());

        let _ = std::fs::remove_file(&first.key);
        std::fs::remove_dir_all(&dir).ok();
    }
}
