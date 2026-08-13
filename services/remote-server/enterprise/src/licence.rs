// Business Source License 1.1
// Change Date: 2030-07-18 (4 years after the 0.1.0 release)
// For commercial use terms see LICENSES.md.

//! Licence keys: issuing, verifying, and what they unlock.
//!
//! ## Why this lives here and not in the core
//!
//! The core is Apache-2.0, which grants an irrevocable right to modify and
//! redistribute. A seat cap written in Apache-2.0 code is therefore something
//! anyone may legally delete, and redistribute without it. Putting the check
//! here does not make it undeletable either — nothing can — but it changes what
//! deleting it buys you: this module is BSL, so using it productively without a
//! licence is already a breach, cap or no cap. The cap is a courtesy that lets
//! a small team evaluate; the licence is the actual agreement.
//!
//! ## Design
//!
//! **Offline verification.** An Ed25519 signature checked against a public key
//! compiled into the binary. No call home, ever: the customers most likely to
//! pay for an on-premise assistant are exactly those whose servers have no
//! outbound internet, and a licence check that needs the network would fail on
//! the best accounts.
//!
//! **Expiry degrades, never bricks.** An expired licence drops the server back
//! to the free seat count. It does not lock anyone out, delete anything, or
//! refuse to start. Software that holds a customer's own data hostage over a
//! lapsed renewal earns a reputation it deserves.

use base64::Engine as _;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

/// Seats available without any licence. Enough for a small team to run the
/// thing for real before deciding, which is the point.
pub const FREE_SEATS: u32 = 3;

/// The public half of the issuing key, compiled in.
///
/// Replaced at release time by the real key. The placeholder is all zeroes,
/// which is not a valid Ed25519 point, so `verify` fails closed on a build that
/// forgot to set it rather than accepting every licence.
const PUBLIC_KEY_HEX: &str = match option_env!("LOCARYN_LICENCE_PUBKEY") {
    Some(k) => k,
    // Build sans cle injectee : 32 octets nuls, qui ne sont pas un point
    // Ed25519 valide. La verification echoue donc en refusant tout, plutot
    // qu'en acceptant tout — un oubli de configuration ne doit pas ouvrir la
    // porte.
    None => "0000000000000000000000000000000000000000000000000000000000000000",
};

/// What a licence grants. Signed as canonical JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Licence {
    /// Unique id, so a leaked licence can be named in a revocation list.
    pub id: String,
    /// Who it was issued to. Shown in the admin UI so an operator can tell at a
    /// glance whether the running server holds the right licence.
    pub customer: String,
    /// Maximum simultaneous authenticated users.
    pub seats: u32,
    pub issued_at: DateTime<Utc>,
    /// `None` = perpetual. Otherwise the server reverts to `FREE_SEATS` after.
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error)]
pub enum LicenceError {
    #[error("format de licence invalide : {0}")]
    Malformed(String),
    #[error("signature invalide — cette clé n'a pas été émise par Locaryn")]
    BadSignature,
    #[error("clé publique de vérification absente ou invalide dans ce binaire")]
    BadPublicKey,
}

impl Licence {
    /// Has it lapsed at `now`? A perpetual licence never has.
    pub fn expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|e| now > e)
    }

    /// Seats this licence actually grants at `now` — the free count once lapsed.
    pub fn effective_seats(&self, now: DateTime<Utc>) -> u32 {
        if self.expired_at(now) {
            FREE_SEATS
        } else {
            self.seats.max(FREE_SEATS)
        }
    }
}

/// Seats the server should allow: the licence's, or the free count without one.
pub fn seat_limit(licence: Option<&Licence>, now: DateTime<Utc>) -> u32 {
    licence.map_or(FREE_SEATS, |l| l.effective_seats(now))
}

/// Canonical bytes that get signed. Field order is fixed by `serde_json`'s
/// struct order, so issuer and verifier agree without a canonicalisation spec.
fn payload_bytes(licence: &Licence) -> Result<Vec<u8>, LicenceError> {
    serde_json::to_vec(licence).map_err(|e| LicenceError::Malformed(e.to_string()))
}

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Verify a licence key and return what it grants.
///
/// The key is `<payload>.<signature>`, both base64url. Deliberately close to a
/// JWT in shape but not one: no algorithm field, so there is no `alg: none`
/// class of bug — the algorithm is fixed by this function.
pub fn verify(key: &str) -> Result<Licence, LicenceError> {
    verify_with(key, PUBLIC_KEY_HEX)
}

/// `verify`, against an explicit public key. Exists so the tests can sign with
/// a throwaway key instead of needing the real one.
pub fn verify_with(key: &str, public_key_hex: &str) -> Result<Licence, LicenceError> {
    let (payload_b64, sig_b64) = key
        .trim()
        .split_once('.')
        .ok_or_else(|| LicenceError::Malformed("séparateur « . » absent".into()))?;

    let payload = B64
        .decode(payload_b64)
        .map_err(|e| LicenceError::Malformed(format!("payload : {e}")))?;
    let sig_bytes = B64
        .decode(sig_b64)
        .map_err(|e| LicenceError::Malformed(format!("signature : {e}")))?;

    let verifying_key = parse_public_key(public_key_hex)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| LicenceError::BadSignature)?;

    // Signature first, parse second: never deserialise attacker-controlled JSON
    // that has not been authenticated.
    verifying_key
        .verify(&payload, &signature)
        .map_err(|_| LicenceError::BadSignature)?;

    serde_json::from_slice(&payload).map_err(|e| LicenceError::Malformed(e.to_string()))
}

fn parse_public_key(hex: &str) -> Result<VerifyingKey, LicenceError> {
    let raw = decode_hex(hex.trim()).ok_or(LicenceError::BadPublicKey)?;
    let bytes: [u8; 32] = raw.try_into().map_err(|_| LicenceError::BadPublicKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| LicenceError::BadPublicKey)
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Sign a licence. Used by the issuing tool, which is the only place the
/// private key ever exists — never on a customer's machine.
pub fn issue(
    licence: &Licence,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<String, LicenceError> {
    use ed25519_dalek::Signer;
    let payload = payload_bytes(licence)?;
    let signature = signing_key.sign(&payload);
    Ok(format!(
        "{}.{}",
        B64.encode(&payload),
        B64.encode(signature.to_bytes())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use ed25519_dalek::SigningKey;

    fn keypair() -> (SigningKey, String) {
        // Fixed seed: the test must not depend on entropy to be reproducible.
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let public_hex = signing
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        (signing, public_hex)
    }

    fn licence(seats: u32, expires_at: Option<DateTime<Utc>>) -> Licence {
        Licence {
            id: "LIC-0001".into(),
            customer: "Fromagerie Durand".into(),
            seats,
            issued_at: Utc::now(),
            expires_at,
        }
    }

    #[test]
    fn a_licence_survives_the_round_trip() {
        let (signing, public_hex) = keypair();
        let original = licence(25, None);
        let key = issue(&original, &signing).unwrap();
        assert_eq!(verify_with(&key, &public_hex).unwrap(), original);
    }

    #[test]
    fn a_forged_licence_is_refused() {
        let (signing, public_hex) = keypair();
        let key = issue(&licence(3, None), &signing).unwrap();

        // Re-encode a payload claiming 9999 seats, keeping the real signature.
        let (_, sig) = key.split_once('.').unwrap();
        let forged_payload = B64.encode(serde_json::to_vec(&licence(9999, None)).unwrap());
        let forged = format!("{forged_payload}.{sig}");

        assert!(matches!(
            verify_with(&forged, &public_hex),
            Err(LicenceError::BadSignature)
        ));
    }

    #[test]
    fn a_licence_from_another_key_is_refused() {
        let (signing, _) = keypair();
        let key = issue(&licence(50, None), &signing).unwrap();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let other_hex = other
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();

        assert!(matches!(
            verify_with(&key, &other_hex),
            Err(LicenceError::BadSignature)
        ));
    }

    #[test]
    fn garbage_is_refused_without_panicking() {
        let (_, public_hex) = keypair();
        for junk in ["", ".", "pas-une-licence", "a.b", "....", "%%%.%%%"] {
            assert!(
                verify_with(junk, &public_hex).is_err(),
                "accepté : {junk:?}"
            );
        }
    }

    #[test]
    fn an_all_zero_public_key_refuses_everything() {
        // A build that forgot to inject the real key must fail closed, not open.
        let (signing, _) = keypair();
        let key = issue(&licence(10, None), &signing).unwrap();
        let zeros = "0".repeat(64);
        assert!(verify_with(&key, &zeros).is_err());
    }

    #[test]
    fn no_licence_means_the_free_seat_count() {
        assert_eq!(seat_limit(None, Utc::now()), FREE_SEATS);
    }

    #[test]
    fn an_expired_licence_drops_back_to_free_rather_than_locking_out() {
        let now = Utc::now();
        let lapsed = licence(50, Some(now - Duration::days(1)));
        assert!(lapsed.expired_at(now));
        assert_eq!(seat_limit(Some(&lapsed), now), FREE_SEATS);
    }

    #[test]
    fn a_valid_licence_grants_its_seats() {
        let now = Utc::now();
        let live = licence(50, Some(now + Duration::days(30)));
        assert!(!live.expired_at(now));
        assert_eq!(seat_limit(Some(&live), now), 50);
    }

    #[test]
    fn a_perpetual_licence_never_lapses() {
        let forever = licence(12, None);
        assert!(!forever.expired_at(Utc::now() + Duration::days(36_500)));
    }

    #[test]
    fn a_licence_can_never_grant_fewer_seats_than_free() {
        // A mis-issued 1-seat licence must not make things worse than no licence.
        let now = Utc::now();
        assert_eq!(seat_limit(Some(&licence(1, None)), now), FREE_SEATS);
    }
}
