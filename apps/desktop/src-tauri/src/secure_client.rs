//! The HTTP client used to reach a Lochor server.
//!
//! Two things separate it from a default `reqwest` client, and both exist
//! because the servers people deploy are their own:
//!
//! 1. **It presents the client certificate**, when one was installed. Without
//!    this the "Install…" button in the connection screen would copy a file
//!    that is never used, and a server with mTLS on would refuse every attempt
//!    with a message about the handshake rather than about the certificate.
//!
//! 2. **It verifies the server against the deployment**, not against the
//!    public certificate authorities — which cannot vouch for
//!    `https://192.168.1.10:7474`. The provisioning file carries the
//!    fingerprint of the certificate to expect; the authority file, when the
//!    administrator sent one, allows the same check to keep working after the
//!    server rotates its certificate.
//!
//! Without either reference there is nothing to compare against and the
//! connection is accepted, which is the situation of someone talking to the
//! daemon on their own machine.

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use std::sync::Arc;

/// Decode a fingerprint as displayed to users — `AB:CD:…`, any case, with or
/// without the colons.
fn parse_fingerprint(text: &str) -> Option<[u8; 32]> {
    let hex: Vec<u8> = text
        .bytes()
        .filter(|b| b.is_ascii_hexdigit())
        .collect();
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in hex.chunks(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

#[derive(Debug)]
struct DeploymentVerifier {
    /// Certificate the administrator published, if the provisioning file
    /// carried one.
    pinned: Option<[u8; 32]>,
    /// The deployment's own authority, if its certificate was installed.
    authority: Option<Arc<rustls::client::WebPkiServerVerifier>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for DeploymentVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        if let Some(pin) = self.pinned {
            if lochor_config::provision::sha256(end_entity.as_ref()) == pin {
                return Ok(ServerCertVerified::assertion());
            }
            // The certificate is not the published one. That is expected after
            // a rotation — but only the authority can say so. With no
            // authority to ask, refuse: a mismatch is indistinguishable from
            // someone else answering on that address.
            if self.authority.is_none() {
                return Err(TlsError::General(
                    "Le certificat présenté ne correspond pas à l'empreinte fournie par \
                     votre administrateur. Ne saisissez pas votre mot de passe : \
                     signalez-le d'abord."
                        .into(),
                ));
            }
        }

        match &self.authority {
            Some(v) => v.verify_server_cert(end_entity, intermediates, server_name, ocsp, now),
            None => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Read a certificate + key bundle into what rustls wants.
fn identity(pem: &str) -> Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let certs: Vec<CertificateDer<'static>> = lochor_config::mtls::pem_blocks(pem, "CERTIFICATE")
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let key_der = lochor_config::mtls::pem_blocks(pem, "PRIVATE KEY")
        .into_iter()
        .next()?;
    let key = PrivateKeyDer::try_from(key_der).ok()?;
    (!certs.is_empty()).then_some((certs, key))
}

/// Build a client for one server.
///
/// `client_pem` is the installed identity bundle, `authority_pem` the
/// deployment authority, `fingerprint` the certificate published in the
/// provisioning file — all three optional and independent.
pub fn build(
    client_pem: Option<&str>,
    authority_pem: Option<&str>,
    fingerprint: Option<&str>,
    timeout: std::time::Duration,
) -> Result<reqwest::Client, String> {
    // Both cipher backends end up compiled in through other crates, so the
    // default provider is ambiguous and has to be named.
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let authority = match authority_pem {
        Some(pem) => {
            let mut roots = rustls::RootCertStore::empty();
            for der in lochor_config::mtls::pem_blocks(pem, "CERTIFICATE") {
                roots
                    .add(CertificateDer::from(der))
                    .map_err(|e| format!("Le certificat d'autorité installé est illisible : {e}"))?;
            }
            if roots.is_empty() {
                None
            } else {
                Some(
                    rustls::client::WebPkiServerVerifier::builder_with_provider(
                        roots.into(),
                        provider.clone(),
                    )
                    .build()
                    .map_err(|e| format!("autorité inutilisable : {e}"))?,
                )
            }
        }
        None => None,
    };

    let pinned = fingerprint.and_then(parse_fingerprint);
    if fingerprint.is_some() && pinned.is_none() {
        // Better to say so than to connect while believing a check happened.
        tracing::warn!("empreinte de déploiement illisible : le serveur ne sera pas vérifié");
    }
    if pinned.is_none() && authority.is_none() {
        tracing::warn!("aucune empreinte ni autorité : l'identité du serveur n'est pas vérifiée");
    }

    let verifier = Arc::new(DeploymentVerifier {
        pinned,
        authority,
        provider: provider.clone(),
    });

    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("configuration TLS : {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    let config = match client_pem.and_then(identity) {
        Some((certs, key)) => builder
            .with_client_auth_cert(certs, key)
            .map_err(|e| format!("Le certificat installé est inutilisable : {e}"))?,
        None => builder.with_no_client_auth(),
    };

    reqwest::Client::builder()
        .use_preconfigured_tls(config)
        .timeout(timeout)
        .build()
        .map_err(|e| format!("client HTTP : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_displayed_fingerprint_round_trips() {
        let der = b"n'importe quel certificat";
        let shown = lochor_config::provision::certificate_fingerprint(&format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----",
            base64_of(der)
        ))
        .unwrap();
        // Exactly the form the connection screen and the CLI print.
        assert!(shown.contains(':'), "empreinte sans séparateurs : {shown}");
        assert_eq!(parse_fingerprint(&shown), Some(lochor_config::provision::sha256(der)));
        // Users retype these; be forgiving about case and colons.
        assert_eq!(
            parse_fingerprint(&shown.replace(':', "").to_lowercase()),
            parse_fingerprint(&shown)
        );
    }

    #[test]
    fn a_truncated_fingerprint_is_rejected_rather_than_padded() {
        // Accepting a short value would silently pin on nothing.
        assert_eq!(parse_fingerprint("AB:CD"), None);
        assert_eq!(parse_fingerprint(""), None);
        assert_eq!(parse_fingerprint(&"AB".repeat(31)), None);
        assert!(parse_fingerprint(&"AB".repeat(32)).is_some());
    }

    #[test]
    fn the_installed_certificate_is_offered_to_the_server() {
        let dir = std::env::temp_dir().join(format!(
            "lochor_sc_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cred = lochor_config::mtls::issue_client(&dir, "marie", 30).unwrap();

        // The bundle the administrator hands over must be usable as an
        // identity; if it is not, mTLS fails for every user at once.
        let (certs, _key) = identity(&cred.bundle_pem).expect("bundle inutilisable");
        assert_eq!(certs.len(), 1);
        assert!(
            build(Some(&cred.bundle_pem), Some(&cred.ca_pem), None, std::time::Duration::from_secs(5))
                .is_ok(),
            "client refusé alors que le certificat est valide"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_certificate_without_its_key_is_not_an_identity() {
        let dir = std::env::temp_dir().join(format!(
            "lochor_sc2_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cred = lochor_config::mtls::issue_client(&dir, "paul", 30).unwrap();
        let cert_only: String = cred
            .bundle_pem
            .lines()
            .take_while(|l| !l.contains("PRIVATE KEY"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(identity(&cert_only).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The three outcomes that matter, against a server that really demands a
    /// certificate. Ignored because it needs one running:
    ///
    /// ```text
    /// LOCHOR_TEST_SERVER=https://127.0.0.1:7499 \
    /// LOCHOR_TEST_CLIENT_PEM=…/clients/tester.pem \
    /// LOCHOR_TEST_CA_PEM=…/tls/ca-cert.pem \
    /// cargo test -p lochor-desktop --lib secure_client -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs a daemon started with require_client_cert"]
    async fn a_live_server_answers_only_to_the_certificate_it_issued() {
        let url = std::env::var("LOCHOR_TEST_SERVER").expect("LOCHOR_TEST_SERVER");
        let client_pem =
            std::fs::read_to_string(std::env::var("LOCHOR_TEST_CLIENT_PEM").unwrap()).unwrap();
        let ca_pem = std::fs::read_to_string(std::env::var("LOCHOR_TEST_CA_PEM").unwrap()).unwrap();
        let health = format!("{url}/health");
        let short = std::time::Duration::from_secs(10);

        // 1. The certificate the administrator issued gets through.
        let ok = build(Some(&client_pem), Some(&ca_pem), None, short).unwrap();
        let r = ok.get(&health).send().await.expect("connexion refusée avec certificat");
        assert!(r.status().is_success(), "statut {}", r.status());
        println!("avec certificat      : {}", r.status());

        // 2. Without it, the handshake fails — before any request is served.
        let bare = build(None, Some(&ca_pem), None, short).unwrap();
        let err = bare.get(&health).send().await.expect_err("accepté sans certificat");
        println!("sans certificat      : refusé ({err})");

        // 3. A wrong fingerprint is caught even though the certificate chains
        //    correctly: the pin is what distinguishes this server from another
        //    one holding a certificate from the same authority.
        let wrong = build(Some(&client_pem), None, Some(&"AB".repeat(32)), short).unwrap();
        let err = wrong.get(&health).send().await.expect_err("empreinte fausse acceptée");
        let mut chain = String::new();
        let mut cur: Option<&dyn std::error::Error> = Some(&err);
        while let Some(c) = cur {
            chain.push_str(&c.to_string());
            cur = c.source();
        }
        assert!(chain.contains("empreinte"), "refus pour une autre raison : {chain}");
        println!("empreinte incorrecte : refusé");

        // 4. And the credentials still decide who gets in: the certificate
        //    proves the machine, never the person.
        if let Ok(pass) = std::env::var("LOCHOR_TEST_PASSWORD") {
            let login = format!("{url}/v1/auth/login");
            let user = std::env::var("LOCHOR_TEST_USER").unwrap_or_else(|_| "tester".into());

            let r = ok
                .post(&login)
                .json(&serde_json::json!({ "username": user, "password": pass }))
                .send()
                .await
                .unwrap();
            assert!(r.status().is_success(), "connexion refusée : {}", r.status());
            let body: serde_json::Value = r.json().await.unwrap();
            assert!(
                body.get("token").and_then(|t| t.as_str()).is_some_and(|t| t.len() > 20),
                "jeton absent ou trop court : {body}"
            );
            println!("identifiants valides : jeton reçu");

            let bad = ok
                .post(&login)
                .json(&serde_json::json!({ "username": user, "password": "faux" }))
                .send()
                .await
                .unwrap();
            assert_eq!(bad.status(), reqwest::StatusCode::UNAUTHORIZED);
            println!("mot de passe faux    : {}", bad.status());
        }
    }

    /// Minimal base64 encoder, for the fingerprint test only.
    fn base64_of(data: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }
}
