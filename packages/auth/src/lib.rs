//! Locaryn auth: token management, OS keychain abstraction, server-side
//! Argon2id hashing. Used by both the daemon (local credential storage for
//! remote-server tokens) and the remote-server (token issuance + verification).
//!
//! Token generation uses the OS CSPRNG and secrets are hashed with Argon2id,
//! verified in constant time. Anything stored by the earlier placeholder
//! hasher no longer verifies — that is deliberate: those values were a 64-bit
//! non-cryptographic digest mislabelled as Argon2id, so they must fail closed
//! rather than be honoured.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Token model
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub id: Uuid,
    pub user_id: Uuid,
    /// Plaintext token — only ever held in memory or the OS keychain.
    /// Never stored in the DB.
    #[serde(skip_serializing)]
    pub plaintext: Option<String>,
    pub label: Option<String>,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHash {
    /// Argon2id hash string.
    pub hash: String,
}

// ============================================================================
// Token issuance / verification interface
// ============================================================================

pub trait TokenStore: Send + Sync {
    fn store_hash(&self, token: &Token, hash: &TokenHash) -> futures::future::BoxFuture<'_, ()>;
    fn lookup_hash(&self, token_id: Uuid) -> futures::future::BoxFuture<'_, Option<TokenHash>>;
    fn revoke(&self, token_id: Uuid) -> futures::future::BoxFuture<'_, ()>;
}

/// Generate a new token: 32 bytes from the OS CSPRNG, URL-safe.
///
/// The previous implementation concatenated a UUID with a nanosecond clock
/// reading. Timestamps are guessable and the layout was fixed, which narrows a
/// brute-force search enormously — a token must come from a cryptographic
/// generator, with no structure an attacker can exploit.
pub fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    // `OsRng` reads the operating system's CSPRNG directly.
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    // Unpadded URL-safe base64: no `+`, `/` or `=` to escape in a header.
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(50);
    out.push_str("locaryn_");
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let take = chunk.len() + 1;
        for i in 0..take {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3F) as usize] as char);
        }
    }
    out
}

/// Hash a secret for storage with Argon2id.
///
/// What this replaced was a 64-bit FNV-1a labelled `argon2id$placeholder$` —
/// not a password hash at all, reversible by brute force in moments, and
/// mislabelled in a way that would let a reader believe the database was safe.
pub fn hash_token(plaintext: &str) -> TokenHash {
    use argon2::password_hash::{rand_core::OsRng as PwOsRng, PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut PwOsRng);
    let hash = argon2::Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        // Argon2id only fails here on a malformed salt, which we just made.
        .expect("hachage Argon2id")
        .to_string();
    TokenHash { hash }
}

/// Verify a secret against a stored hash.
///
/// Argon2's own verifier compares in constant time; the previous `==` on hash
/// strings leaked how many leading characters matched.
pub fn verify_token(plaintext: &str, stored: &TokenHash) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    match PasswordHash::new(&stored.hash) {
        Ok(parsed) => argon2::Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok(),
        Err(e) => {
            // A hash we cannot parse is a hash we must not accept. The old
            // placeholder format lands here, so stored credentials from before
            // this change stop working instead of being trusted.
            tracing::warn!(error = %e, "empreinte illisible — accès refusé");
            false
        }
    }
}

/// Default token validity: 30 days.
pub fn default_expiry() -> DateTime<Utc> {
    Utc::now() + Duration::days(30)
}

// ============================================================================
// Keychain abstraction (client side)
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("keychain entry not found: {0}")]
    NotFound(String),
    #[error("keychain error: {0}")]
    Backend(String),
}

/// OS-agnostic secret store. V1.1 wires the real keychain crates
/// (`keyring` on Windows/macOS, `secret-service` on Linux). Skeleton uses
/// a file-backed store under `~/.locaryn/credentials.toml` (mode 0600).
pub trait Keychain: Send + Sync {
    fn put(&self, key: &str, value: &str) -> Result<(), KeychainError>;
    fn get(&self, key: &str) -> Result<String, KeychainError>;
    fn delete(&self, key: &str) -> Result<(), KeychainError>;
}

/// A no-op keychain for headless/CI environments.
pub struct NullKeychain;

impl Keychain for NullKeychain {
    fn put(&self, _key: &str, _value: &str) -> Result<(), KeychainError> {
        Ok(())
    }
    fn get(&self, key: &str) -> Result<String, KeychainError> {
        Err(KeychainError::NotFound(key.to_string()))
    }
    fn delete(&self, _key: &str) -> Result<(), KeychainError> {
        Ok(())
    }
}

pub fn provider_key(provider_id: Uuid) -> String {
    format!("locaryn/provider/{provider_id}")
}

pub fn token_key(server_url: &str) -> String {
    format!("locaryn/remote/{server_url}")
}

/// Keychain entry name for an SSH server's secret (password or key passphrase).
pub fn ssh_key(server_id: Uuid) -> String {
    format!("locaryn/ssh/{server_id}")
}

// ============================================================================
// Real OS keychain (feature = "system-keychain")
// ============================================================================

/// OS-backed secret store using the `keyring` crate: Windows Credential
/// Manager, macOS Keychain, or Linux Secret Service. Enabled by the desktop.
#[cfg(feature = "system-keychain")]
pub struct SystemKeychain {
    service: String,
}

#[cfg(feature = "system-keychain")]
impl SystemKeychain {
    /// `service` is the keychain service/namespace (e.g. "locaryn").
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, KeychainError> {
        keyring::Entry::new(&self.service, key).map_err(|e| KeychainError::Backend(e.to_string()))
    }
}

#[cfg(feature = "system-keychain")]
impl Keychain for SystemKeychain {
    fn put(&self, key: &str, value: &str) -> Result<(), KeychainError> {
        self.entry(key)?
            .set_password(value)
            .map_err(|e| KeychainError::Backend(e.to_string()))
    }

    fn get(&self, key: &str) -> Result<String, KeychainError> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(v),
            Err(keyring::Error::NoEntry) => Err(KeychainError::NotFound(key.to_string())),
            Err(e) => Err(KeychainError::Backend(e.to_string())),
        }
    }

    fn delete(&self, key: &str) -> Result<(), KeychainError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(KeychainError::Backend(e.to_string())),
        }
    }
}

#[cfg(test)]
mod crypto_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn tokens_are_unpredictable_and_unique() {
        let n = 2000;
        let set: HashSet<String> = (0..n).map(|_| generate_token()).collect();
        assert_eq!(
            set.len(),
            n,
            "collision — le générateur n'est pas aléatoire"
        );

        let t = generate_token();
        assert!(t.starts_with("locaryn_"));
        // 32 bytes of entropy, base64 → at least 42 characters after the prefix.
        assert!(
            t.len() - "locaryn_".len() >= 42,
            "entropie insuffisante: {t}"
        );
        // URL-safe alphabet only: it travels in an Authorization header.
        assert!(
            t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "caractère non URL-safe dans {t}"
        );
    }

    #[test]
    fn two_tokens_share_no_long_prefix() {
        // The old generator embedded a nanosecond clock, so tokens issued close
        // together shared a long common prefix. Real randomness must not.
        let a = generate_token();
        let b = generate_token();
        let common = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
        assert!(
            common <= 10,
            "préfixe commun de {common} caractères: {a} / {b}"
        );
    }

    #[test]
    fn hashing_is_salted_and_verifiable() {
        let secret = "locaryn_un_secret_de_test";
        let h1 = hash_token(secret);
        let h2 = hash_token(secret);

        assert!(
            h1.hash.starts_with("$argon2id$"),
            "pas un vrai Argon2id: {}",
            h1.hash
        );
        assert_ne!(
            h1.hash, h2.hash,
            "hachage non salé : deux fois le même résultat"
        );
        assert!(
            !h1.hash.contains(secret),
            "le secret ne doit jamais apparaître"
        );

        assert!(verify_token(secret, &h1));
        assert!(verify_token(secret, &h2));
        assert!(!verify_token("mauvais secret", &h1));
        assert!(!verify_token("", &h1));
    }

    #[test]
    fn the_old_placeholder_format_is_rejected_not_trusted() {
        // Credentials written by the previous fake hasher must fail closed.
        let legacy = TokenHash {
            hash: "argon2id$placeholder$cbf29ce484222325".into(),
        };
        assert!(!verify_token("nimporte quoi", &legacy));
        assert!(!verify_token("", &legacy));
    }
}
