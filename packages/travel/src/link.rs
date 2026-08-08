//! The `lochor://` address a phone reads off the screen.
//!
//! Scanning a QR code changes which server the application talks to. That is
//! precisely what an attacker would like to do: print a code, get it scanned,
//! and receive somebody's password on their own server. So the link is
//! **signed by the deployment's own authority** — the same one that issues the
//! mTLS certificates — and a phone accepts it only for a server it already
//! knows.
//!
//! The user sees none of this. They scan, and the application says it is
//! connected. But a code from anywhere else is refused, and refused loudly.
//!
//! Two directions, because leaving travel mode has to be as easy as entering
//! it: `m=travel` points the phone at the tunnel, `m=home` puts it back on the
//! local address.

use ring::signature::{EcdsaKeyPair, KeyPair, UnparsedPublicKey, ECDSA_P256_SHA256_ASN1,
                      ECDSA_P256_SHA256_ASN1_SIGNING};

/// How long a code stays valid. Long enough to walk across a room, short
/// enough that a photograph of the screen is not a lasting key.
pub const DEFAULT_TTL_SECONDS: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Point the phone at the tunnel.
    Travel,
    /// Put it back on the local address.
    Home,
}

impl Mode {
    fn id(&self) -> &'static str {
        match self {
            Self::Travel => "travel",
            Self::Home => "home",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "travel" => Some(Self::Travel),
            "home" => Some(Self::Home),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingLink {
    pub mode: Mode,
    /// Address the client should use from now on.
    pub url: String,
    pub expires_at: u64,
    /// Identifies which server issued this, so a phone with several
    /// registered servers knows which one to update.
    pub key_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("Ce code ne vient pas de Lochor.")]
    NotALochorLink,
    #[error("Ce code est incomplet ou abîmé. Réessayez de le scanner.")]
    Malformed,
    #[error("Ce code ne correspond à aucun serveur enregistré sur cet appareil.")]
    UnknownServer,
    #[error("Ce code a expiré. Affichez-en un nouveau sur l'ordinateur.")]
    Expired,
    #[error("Ce code n'a pas été émis par votre serveur. Ne l'utilisez pas.")]
    BadSignature,
    #[error("Autorité illisible : {0}")]
    BadAuthority(String),
    #[error("Signature impossible : {0}")]
    Signing(String),
}

/// Base64 without padding, using the URL alphabet, so the result survives a
/// QR code and a URL bar unchanged.
fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        for i in 0..chunk.len() + 1 {
            out.push(T[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
        }
    }
    out
}

fn unb64(s: &str) -> Option<Vec<u8>> {
    lochor_config::provision::base64_decode(s)
}

/// The DER of the first certificate in a PEM bundle.
fn cert_der(pem: &str) -> Result<Vec<u8>, LinkError> {
    lochor_config::mtls::pem_blocks(pem, "CERTIFICATE")
        .into_iter()
        .next()
        .ok_or_else(|| LinkError::BadAuthority("aucun certificat".into()))
}

/// A short, stable name for an authority: the first bytes of the hash of its
/// certificate. Not a secret — it only says *which* server, so the phone can
/// pick the right one out of several.
pub fn key_id(ca_cert_pem: &str) -> Result<String, LinkError> {
    let der = cert_der(ca_cert_pem)?;
    Ok(b64(&lochor_config::provision::sha256(&der)[..8]))
}

/// Exactly the bytes that are signed and verified.
///
/// Built from the parsed fields in a fixed order on both sides, so shuffling
/// the parameters in the URL cannot change what was actually attested.
fn canonical(mode: Mode, url: &str, expires_at: u64, key_id: &str) -> String {
    format!("lochor-pair-v1|{}|{}|{}|{}", mode.id(), url, expires_at, key_id)
}

/// Produce the signed link.
///
/// `now` is passed in rather than read from the clock so the expiry can be
/// tested without waiting ten minutes.
pub fn sign(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    mode: Mode,
    url: &str,
    now: u64,
    ttl_seconds: u64,
) -> Result<String, LinkError> {
    let kid = key_id(ca_cert_pem)?;
    let expires_at = now + ttl_seconds;
    let msg = canonical(mode, url, expires_at, &kid);

    let pkcs8 = lochor_config::mtls::pem_blocks(ca_key_pem, "PRIVATE KEY")
        .into_iter()
        .next()
        .ok_or_else(|| LinkError::Signing("aucune clé privée".into()))?;
    let rng = ring::rand::SystemRandom::new();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &pkcs8, &rng)
        .map_err(|e| LinkError::Signing(e.to_string()))?;
    let sig = pair
        .sign(&rng, msg.as_bytes())
        .map_err(|e| LinkError::Signing(e.to_string()))?;

    Ok(format!(
        "lochor://travel?v=1&m={}&u={}&e={}&k={}&s={}",
        mode.id(),
        b64(url.as_bytes()),
        expires_at,
        kid,
        b64(sig.as_ref())
    ))
}

/// Check a scanned link against an authority the device already trusts.
///
/// `known` maps a key id to that server's authority certificate — what the
/// provisioning file installed. A link whose key id is absent is rejected
/// without any cryptography: the device has simply never heard of that server.
pub fn verify(
    uri: &str,
    known: &dyn Fn(&str) -> Option<String>,
    now: u64,
) -> Result<PairingLink, LinkError> {
    let rest = uri
        .trim()
        .strip_prefix("lochor://travel?")
        .ok_or(LinkError::NotALochorLink)?;

    let mut v = None;
    let mut mode = None;
    let mut url_b64 = None;
    let mut exp = None;
    let mut kid = None;
    let mut sig_b64 = None;
    for pair in rest.split('&') {
        let (k, val) = pair.split_once('=').ok_or(LinkError::Malformed)?;
        match k {
            "v" => v = Some(val),
            "m" => mode = Mode::parse(val),
            "u" => url_b64 = Some(val),
            "e" => exp = val.parse::<u64>().ok(),
            "k" => kid = Some(val),
            "s" => sig_b64 = Some(val),
            // Unknown parameters are ignored on purpose: a future version may
            // add one, and an old phone should still be able to come home.
            _ => {}
        }
    }

    if v != Some("1") {
        return Err(LinkError::Malformed);
    }
    let (mode, url_b64, expires_at, kid, sig_b64) = (
        mode.ok_or(LinkError::Malformed)?,
        url_b64.ok_or(LinkError::Malformed)?,
        exp.ok_or(LinkError::Malformed)?,
        kid.ok_or(LinkError::Malformed)?,
        sig_b64.ok_or(LinkError::Malformed)?,
    );
    let url = String::from_utf8(unb64(url_b64).ok_or(LinkError::Malformed)?)
        .map_err(|_| LinkError::Malformed)?;

    let ca_pem = known(kid).ok_or(LinkError::UnknownServer)?;

    // Signature before expiry: an expired link and a forged one deserve
    // different messages, and only the signature says which this is.
    let der = cert_der(&ca_pem)?;
    let (_, cert) = x509_parser::parse_x509_certificate(&der)
        .map_err(|e| LinkError::BadAuthority(e.to_string()))?;
    let spki = cert.public_key().subject_public_key.data.as_ref();
    let sig = unb64(sig_b64).ok_or(LinkError::Malformed)?;
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, spki)
        .verify(canonical(mode, &url, expires_at, kid).as_bytes(), &sig)
        .map_err(|_| LinkError::BadSignature)?;

    if now > expires_at {
        return Err(LinkError::Expired);
    }

    Ok(PairingLink {
        mode,
        url,
        expires_at,
        key_id: kid.to_string(),
    })
}

/// The authority's public key, for callers that want to pin it themselves.
pub fn authority_public_key(ca_key_pem: &str) -> Result<Vec<u8>, LinkError> {
    let pkcs8 = lochor_config::mtls::pem_blocks(ca_key_pem, "PRIVATE KEY")
        .into_iter()
        .next()
        .ok_or_else(|| LinkError::Signing("aucune clé privée".into()))?;
    let rng = ring::rand::SystemRandom::new();
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &pkcs8, &rng)
        .map_err(|e| LinkError::Signing(e.to_string()))?;
    Ok(pair.public_key().as_ref().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority() -> (String, String, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "lochor_link_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let a = lochor_config::mtls::authority(&dir).unwrap();
        (a.cert_pem, a.key_pem, dir)
    }

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn a_link_from_our_own_server_is_accepted() {
        let (cert, key, dir) = authority();
        let uri = sign(&cert, &key, Mode::Travel, "https://abc.trycloudflare.com", NOW, 600).unwrap();
        let kid = key_id(&cert).unwrap();

        let link = verify(&uri, &|k| (k == kid).then(|| cert.clone()), NOW + 10).unwrap();
        assert_eq!(link.mode, Mode::Travel);
        assert_eq!(link.url, "https://abc.trycloudflare.com");
        assert_eq!(link.key_id, kid);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_link_signed_by_someone_else_is_refused() {
        // The whole point: a QR code printed by an attacker must not be able
        // to repoint the application at their server.
        let (mine_cert, _mine_key, d1) = authority();
        let (_their_cert, their_key, d2) = authority();
        let kid = key_id(&mine_cert).unwrap();

        // They forge a link claiming to be from my server.
        let forged = format!(
            "lochor://travel?v=1&m=travel&u={}&e={}&k={}&s={}",
            b64(b"https://serveur-du-pirate.example"),
            NOW + 600,
            kid,
            // Signature made with *their* key over the right message.
            {
                let msg = canonical(Mode::Travel, "https://serveur-du-pirate.example", NOW + 600, &kid);
                let pkcs8 = lochor_config::mtls::pem_blocks(&their_key, "PRIVATE KEY")
                    .into_iter()
                    .next()
                    .unwrap();
                let rng = ring::rand::SystemRandom::new();
                let p = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &pkcs8, &rng).unwrap();
                b64(p.sign(&rng, msg.as_bytes()).unwrap().as_ref())
            }
        );

        let err = verify(&forged, &|k| (k == kid).then(|| mine_cert.clone()), NOW).unwrap_err();
        assert!(matches!(err, LinkError::BadSignature), "obtenu {err:?}");
        // And the message must tell the user not to proceed.
        assert!(err.to_string().contains("Ne l'utilisez pas"));
        std::fs::remove_dir_all(&d1).ok();
        std::fs::remove_dir_all(&d2).ok();
    }

    #[test]
    fn changing_the_address_invalidates_the_link() {
        // Rewriting the destination in a genuine link is the cheapest attack
        // there is; the signature covers the address itself.
        let (cert, key, dir) = authority();
        let uri = sign(&cert, &key, Mode::Travel, "https://vrai.trycloudflare.com", NOW, 600).unwrap();
        let kid = key_id(&cert).unwrap();
        let tampered = uri.replace(
            &b64(b"https://vrai.trycloudflare.com"),
            &b64(b"https://faux.trycloudflare.com"),
        );
        assert_ne!(tampered, uri);
        let err = verify(&tampered, &|k| (k == kid).then(|| cert.clone()), NOW).unwrap_err();
        assert!(matches!(err, LinkError::BadSignature), "obtenu {err:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unknown_server_is_refused_before_any_cryptography() {
        let (cert, key, dir) = authority();
        let uri = sign(&cert, &key, Mode::Travel, "https://x.trycloudflare.com", NOW, 600).unwrap();
        let err = verify(&uri, &|_| None, NOW).unwrap_err();
        assert!(matches!(err, LinkError::UnknownServer), "obtenu {err:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_expired_link_is_refused_but_says_so_plainly() {
        // Distinguishable from a forgery: one means "ask for a new code", the
        // other means "something is wrong".
        let (cert, key, dir) = authority();
        let uri = sign(&cert, &key, Mode::Travel, "https://x.trycloudflare.com", NOW, 600).unwrap();
        let kid = key_id(&cert).unwrap();
        let err = verify(&uri, &|k| (k == kid).then(|| cert.clone()), NOW + 601).unwrap_err();
        assert!(matches!(err, LinkError::Expired), "obtenu {err:?}");
        assert!(err.to_string().contains("nouveau"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn coming_home_uses_the_same_guarantee() {
        let (cert, key, dir) = authority();
        let uri = sign(&cert, &key, Mode::Home, "https://192.168.1.10:7474", NOW, 600).unwrap();
        let kid = key_id(&cert).unwrap();
        let link = verify(&uri, &|k| (k == kid).then(|| cert.clone()), NOW).unwrap();
        assert_eq!(link.mode, Mode::Home);
        assert_eq!(link.url, "https://192.168.1.10:7474");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn anything_that_is_not_one_of_our_links_is_rejected_clearly() {
        let f = |_: &str| None;
        assert!(matches!(
            verify("https://example.com", &f, NOW).unwrap_err(),
            LinkError::NotALochorLink
        ));
        assert!(matches!(
            verify("lochor://travel?v=1&m=travel", &f, NOW).unwrap_err(),
            LinkError::Malformed
        ));
        // A future version number must not be silently treated as v1.
        assert!(matches!(
            verify("lochor://travel?v=2&m=travel&u=aa&e=1&k=x&s=aa", &f, NOW).unwrap_err(),
            LinkError::Malformed
        ));
    }

    #[test]
    fn the_encoding_survives_a_round_trip() {
        for s in ["", "a", "ab", "abc", "https://a-b_c.trycloudflare.com/x?y=1"] {
            assert_eq!(unb64(&b64(s.as_bytes())).unwrap(), s.as_bytes(), "échec sur {s:?}");
        }
        // No padding and no characters a URL would mangle.
        let e = b64(b"\xff\xfe\xfd\xfc");
        assert!(!e.contains('='), "padding présent : {e}");
        assert!(!e.contains('+') && !e.contains('/'), "alphabet non URL : {e}");
    }

    #[test]
    fn two_servers_get_different_identifiers() {
        // Otherwise a phone with two registered servers would apply a link to
        // whichever it happened to look at first.
        let (c1, _k1, d1) = authority();
        let (c2, _k2, d2) = authority();
        assert_ne!(key_id(&c1).unwrap(), key_id(&c2).unwrap());
        std::fs::remove_dir_all(&d1).ok();
        std::fs::remove_dir_all(&d2).ok();
    }
}
