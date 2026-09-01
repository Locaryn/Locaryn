//! Accounts and API tokens for shared-server mode.
//!
//! A token's plaintext is shown once and never stored: only an Argon2id hash
//! is kept, so a stolen database yields no usable credentials. Verification
//! therefore cannot look a token up by value — it walks the live tokens and
//! checks each, which is why the candidate set is kept small (indexed on
//! revoked/expired, and expired rows are skipped before hashing).

use chrono::Utc;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    fn as_token(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
        }
    }
    fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("admin") {
            Role::Admin
        } else {
            Role::Member
        }
    }
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub role: Role,
    pub disabled: bool,
}

#[derive(FromRow)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
    disabled_at: Option<String>,
}

impl UserRow {
    fn to_user(&self) -> Result<User, StorageError> {
        Ok(User {
            id: Uuid::parse_str(&self.id)
                .map_err(|e| StorageError::Decode(format!("user id: {e}")))?,
            username: self.username.clone(),
            role: Role::parse(&self.role),
            disabled: self.disabled_at.is_some(),
        })
    }
}

/// A freshly issued token: the only moment its plaintext exists outside the
/// caller's memory.
#[derive(Debug, Clone)]
pub struct IssuedToken {
    pub id: Uuid,
    pub plaintext: String,
    pub expires_at: Option<String>,
}

/// The two access circuits share the `auth_tokens` table but must never be
/// confused in the UI: a developer key minted by hand is not a device session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Issued by a login or a device pairing — the default, so pre-existing
    /// rows keep their meaning.
    Session,
    /// Created on purpose from the settings screen for programmatic use.
    Api,
}

impl TokenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TokenKind::Session => "session",
            TokenKind::Api => "api",
        }
    }
    fn parse(s: &str) -> TokenKind {
        if s.eq_ignore_ascii_case("api") {
            TokenKind::Api
        } else {
            TokenKind::Session
        }
    }
}

/// What the settings screen may show. No plaintext, ever — only the hint
/// (a few middle characters) lets a user recognise one of their own tokens.
#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub id: Uuid,
    pub kind: TokenKind,
    pub label: Option<String>,
    pub hint: String,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Clone)]
pub struct UserRepo {
    pool: SqlitePool,
}

#[derive(FromRow)]
struct TokenRow {
    id: String,
    user_id: String,
    token_hash: String,
    expires_at: Option<String>,
}

#[derive(FromRow)]
struct TokenListRow {
    id: String,
    kind: String,
    label: Option<String>,
    hint: String,
    created_at: Option<String>,
    last_used_at: Option<String>,
    expires_at: Option<String>,
    revoked_at: Option<String>,
}

impl UserRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The first admin account, if any — the pairing host. Devices pair to
    /// an account, and on a single-host deployment that account is this one.
    pub async fn first_admin_id(&self) -> Option<Uuid> {
        let (id,): (String,) = sqlx::query_as(
            "SELECT id FROM users WHERE role = 'admin' AND disabled_at IS NULL              ORDER BY created_at ASC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()??;
        Uuid::parse_str(&id).ok()
    }

    pub async fn count(&self) -> Result<i64, StorageError> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(n)
    }

    pub async fn list(&self) -> Result<Vec<User>, StorageError> {
        let rows = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role, disabled_at FROM users \
             ORDER BY lower(username) ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(UserRow::to_user).collect()
    }

    /// Create an account. The password is hashed before it touches the disk.
    pub async fn create(
        &self,
        username: &str,
        password: &str,
        role: Role,
    ) -> Result<User, StorageError> {
        let username = username.trim();
        if username.is_empty() {
            return Err(StorageError::Conflict("nom d'utilisateur vide".into()));
        }
        // Short passwords are the weakest link in a system whose whole purpose
        // is to sit on a network, so refuse them here rather than trusting
        // every caller to check.
        if password.chars().count() < 8 {
            return Err(StorageError::Conflict(
                "mot de passe trop court (8 caractères minimum)".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        let hash = locaryn_auth::hash_token(password).hash;
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(username)
        .bind(&hash)
        .bind(role.as_token())
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db) if db.message().contains("UNIQUE") => {
                StorageError::Conflict(format!("le compte « {username} » existe déjà"))
            }
            other => StorageError::from(other),
        })?;

        Ok(User {
            id,
            username: username.to_string(),
            role,
            disabled: false,
        })
    }

    /// Check a username and password.
    ///
    /// Returns `None` for a wrong password, an unknown account and a disabled
    /// one alike: distinguishing them would tell an attacker which usernames
    /// exist. A hash is computed even when the user is missing, so the reply
    /// takes the same time either way.
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<User>, StorageError> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role, disabled_at FROM users \
             WHERE lower(username) = lower(?)",
        )
        .bind(username.trim())
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            // Same cost as a real check, so timing does not reveal that the
            // account is unknown.
            let _ = locaryn_auth::verify_token(
                password,
                &locaryn_auth::TokenHash {
                    hash: DUMMY_HASH.to_string(),
                },
            );
            return Ok(None);
        };

        let ok = locaryn_auth::verify_token(
            password,
            &locaryn_auth::TokenHash {
                hash: row.password_hash.clone(),
            },
        );
        if !ok || row.disabled_at.is_some() {
            return Ok(None);
        }
        Ok(Some(row.to_user()?))
    }

    /// Change an account's password, after checking the current one.
    ///
    /// `Ok(false)` means the current password is wrong — the caller must not
    /// tell a remote user anything more specific. The new password goes
    /// through the same minimum-length rule as `create`.
    pub async fn change_password(
        &self,
        user_id: Uuid,
        current: &str,
        nouveau: &str,
    ) -> Result<bool, StorageError> {
        if nouveau.chars().count() < 8 {
            return Err(StorageError::Conflict(
                "mot de passe trop court (8 caractères minimum)".into(),
            ));
        }
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role, disabled_at FROM users WHERE id = ?",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let ok = locaryn_auth::verify_token(
            current,
            &locaryn_auth::TokenHash {
                hash: row.password_hash.clone(),
            },
        );
        if !ok {
            return Ok(false);
        }
        let hash = locaryn_auth::hash_token(nouveau).hash;
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
            .bind(&hash)
            .bind(&now)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(true)
    }

    /// Issue an API token for a user. The plaintext is returned once.
    pub async fn issue_token(
        &self,
        user_id: Uuid,
        label: Option<&str>,
        valid_days: i64,
    ) -> Result<IssuedToken, StorageError> {
        let plaintext = locaryn_auth::generate_token();
        let hash = locaryn_auth::hash_token(&plaintext).hash;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let expires = if valid_days > 0 {
            Some((now + chrono::Duration::days(valid_days)).to_rfc3339())
        } else {
            None
        };
        // Enough to recognise a token in a list, far too little to reconstruct.
        let hint: String = plaintext.chars().skip(7).take(6).collect();

        self.insert_token(
            id,
            user_id,
            &hash,
            &hint,
            label,
            &now,
            expires.as_deref(),
            TokenKind::Session,
        )
        .await?;

        Ok(IssuedToken {
            id,
            plaintext,
            expires_at: expires,
        })
    }

    /// Mint a developer API key. `expires_days` of `None` means the key never
    /// expires — the default, mirroring the API keys people already know;
    /// the caller may pick 7/30/90 days for anything more cautious.
    pub async fn issue_api_token(
        &self,
        user_id: Uuid,
        label: Option<&str>,
        expires_days: Option<i64>,
    ) -> Result<IssuedToken, StorageError> {
        let plaintext = locaryn_auth::generate_token();
        let hash = locaryn_auth::hash_token(&plaintext).hash;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let expires =
            expires_days.map(|d| (now + chrono::Duration::days(d)).to_rfc3339());
        let hint: String = plaintext.chars().skip(7).take(6).collect();

        self.insert_token(
            id,
            user_id,
            &hash,
            &hint,
            label,
            &now,
            expires.as_deref(),
            TokenKind::Api,
        )
        .await?;

        Ok(IssuedToken {
            id,
            plaintext,
            expires_at: expires,
        })
    }

    /// Shared insert for both circuits. The plaintext never lands here — only
    /// its Argon2id hash and a recognition hint.
    #[allow(clippy::too_many_arguments)]
    async fn insert_token(
        &self,
        token_id: Uuid,
        user_id: Uuid,
        token_hash: &str,
        hint: &str,
        label: Option<&str>,
        created_at: &chrono::DateTime<Utc>,
        expires_at: Option<&str>,
        kind: TokenKind,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO auth_tokens (id, user_id, token_hash, hint, label, created_at, expires_at, kind)              VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(token_id.to_string())
        .bind(user_id.to_string())
        .bind(token_hash)
        .bind(hint)
        .bind(label)
        .bind(created_at.to_rfc3339())
        .bind(expires_at)
        .bind(kind.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Every token the user could see in their settings: both circuits,
    /// including revoked and expired rows — a device list you cannot audit is
    /// a device list you cannot trust. The plaintext is gone the moment it is
    /// issued; only the hint comes back.
    pub async fn list_tokens(&self, user_id: Uuid) -> Result<Vec<TokenInfo>, StorageError> {
        let rows = sqlx::query_as::<_, TokenListRow>(
            "SELECT id, kind, label, hint, created_at, last_used_at, expires_at, revoked_at              FROM auth_tokens WHERE user_id = ?              ORDER BY (revoked_at IS NULL) DESC, COALESCE(last_used_at, created_at) DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let id = Uuid::parse_str(&r.id).ok()?;
                Some(TokenInfo {
                    id,
                    kind: TokenKind::parse(&r.kind),
                    label: r.label,
                    hint: r.hint,
                    created_at: r.created_at,
                    last_used_at: r.last_used_at,
                    expires_at: r.expires_at,
                    revoked_at: r.revoked_at,
                })
            })
            .collect())
    }

    /// Resolve a bearer token to its owner, or `None` if it is not valid.
    pub async fn user_for_token(&self, plaintext: &str) -> Result<Option<User>, StorageError> {
        if plaintext.trim().is_empty() {
            return Ok(None);
        }
        let now = Utc::now().to_rfc3339();
        let rows = sqlx::query_as::<_, TokenRow>(
            "SELECT id, user_id, token_hash, expires_at FROM auth_tokens \
             WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
        )
        .bind(&now)
        .fetch_all(&self.pool)
        .await?;

        for t in rows {
            if !locaryn_auth::verify_token(
                plaintext,
                &locaryn_auth::TokenHash {
                    hash: t.token_hash.clone(),
                },
            ) {
                continue;
            }
            // Best effort: a failed touch must not deny a valid request.
            let _ = sqlx::query("UPDATE auth_tokens SET last_used_at = ? WHERE id = ?")
                .bind(&now)
                .bind(&t.id)
                .execute(&self.pool)
                .await;

            let user = sqlx::query_as::<_, UserRow>(
                "SELECT id, username, password_hash, role, disabled_at FROM users WHERE id = ?",
            )
            .bind(&t.user_id)
            .fetch_optional(&self.pool)
            .await?;
            return match user {
                // A disabled account's tokens stop working immediately.
                Some(u) if u.disabled_at.is_none() => Ok(Some(u.to_user()?)),
                _ => Ok(None),
            };
        }
        Ok(None)
    }

    pub async fn revoke_token(&self, token_id: Uuid) -> Result<(), StorageError> {
        sqlx::query("UPDATE auth_tokens SET revoked_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(token_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_disabled(&self, user_id: Uuid, disabled: bool) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE users SET disabled_at = ?, updated_at = ? WHERE id = ?")
            .bind(if disabled { Some(now.clone()) } else { None })
            .bind(&now)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete(&self, user_id: Uuid) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM auth_tokens WHERE user_id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await?;
        let res = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(user_id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(res.rows_affected() > 0)
    }
}

/// A valid Argon2id encoding of an arbitrary string, used only to spend the
/// same time on an unknown username as on a real one.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$\
c29tZXNhbHRzb21lc2FsdA$K5tGRnQxHF9k5jCPZ5F1n9wLhF7lQm2vJ8xN0oYZ3aM";

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory database, migrations applied — no files to clean up.
    async fn repo() -> (UserRepo, sqlx::SqlitePool) {
        let pool = crate::open_in_memory().await.expect("base en mémoire");
        (UserRepo::new(pool.clone()), pool)
    }

    #[tokio::test]
    async fn a_password_changes_only_with_the_current_one() {
        let (repo, _pool) = repo().await;

        let u = repo
            .create("Marie", "un-mot-de-passe-solide", Role::Admin)
            .await
            .expect("création");

        // Le mauvais mot de passe actuel est refusé, sans rien changer.
        assert!(!repo
            .change_password(u.id, "mauvais", "tout-neuf-et-solide")
            .await
            .unwrap());
        assert!(repo
            .authenticate("Marie", "un-mot-de-passe-solide")
            .await
            .unwrap()
            .is_some());

        // Le bon mot de passe change, et le nouveau prend effet aussitôt.
        assert!(repo
            .change_password(u.id, "un-mot-de-passe-solide", "tout-neuf-et-solide")
            .await
            .unwrap());
        assert!(repo
            .authenticate("Marie", "tout-neuf-et-solide")
            .await
            .unwrap()
            .is_some());
        assert!(repo
            .authenticate("Marie", "un-mot-de-passe-solide")
            .await
            .unwrap()
            .is_none());

        // Un nouveau mot de passe trop court est refusé.
        assert!(repo
            .change_password(u.id, "tout-neuf-et-solide", "court")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn an_account_authenticates_only_with_its_own_password() {
        let (repo, _pool) = repo().await;

        let u = repo
            .create("Marie", "un-mot-de-passe-solide", Role::Admin)
            .await
            .expect("création");
        assert_eq!(u.role, Role::Admin);

        assert!(repo
            .authenticate("Marie", "un-mot-de-passe-solide")
            .await
            .unwrap()
            .is_some());
        // Usernames are matched case-insensitively, so a lookalike cannot
        // become a second account.
        assert!(repo
            .authenticate("marie", "un-mot-de-passe-solide")
            .await
            .unwrap()
            .is_some());
        assert!(repo
            .authenticate("Marie", "mauvais")
            .await
            .unwrap()
            .is_none());
        assert!(repo
            .authenticate("inconnu", "un-mot-de-passe-solide")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn the_password_is_never_stored_in_clear() {
        let (repo, pool) = repo().await;
        repo.create("bob", "correct-horse-battery", Role::Member)
            .await
            .unwrap();

        let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            !hash.contains("correct-horse-battery"),
            "mot de passe en clair !"
        );
        assert!(hash.starts_with("$argon2id$"), "pas un Argon2id: {hash}");
    }

    #[tokio::test]
    async fn duplicate_names_and_weak_passwords_are_refused() {
        let (repo, _pool) = repo().await;
        repo.create("alice", "assez-long-comme-ca", Role::Member)
            .await
            .unwrap();

        let dup = repo
            .create("ALICE", "un-autre-mot-de-passe", Role::Member)
            .await;
        assert!(
            dup.is_err(),
            "un doublon insensible à la casse doit être refusé"
        );

        let weak = repo.create("carl", "court", Role::Member).await;
        assert!(weak.is_err(), "un mot de passe court doit être refusé");
    }

    #[tokio::test]
    async fn a_token_identifies_its_owner_and_dies_when_revoked() {
        let (repo, pool) = repo().await;
        let u = repo
            .create("dana", "mot-de-passe-valide", Role::Member)
            .await
            .unwrap();

        let issued = repo.issue_token(u.id, Some("portable"), 30).await.unwrap();
        assert!(issued.plaintext.starts_with("locaryn_"));

        let who = repo.user_for_token(&issued.plaintext).await.unwrap();
        assert_eq!(who.expect("le jeton doit identifier son porteur").id, u.id);

        // The plaintext must not be recoverable from storage.
        let (stored,): (String,) = sqlx::query_as("SELECT token_hash FROM auth_tokens LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            !stored.contains(&issued.plaintext),
            "jeton stocké en clair !"
        );

        assert!(repo
            .user_for_token("locaryn_inventé")
            .await
            .unwrap()
            .is_none());
        assert!(repo.user_for_token("").await.unwrap().is_none());

        repo.revoke_token(issued.id).await.unwrap();
        assert!(
            repo.user_for_token(&issued.plaintext)
                .await
                .unwrap()
                .is_none(),
            "un jeton révoqué doit cesser de fonctionner immédiatement"
        );
    }

    #[tokio::test]
    async fn api_tokens_are_typed_listed_and_never_expire_by_default() {
        let (repo, _pool) = repo().await;
        let u = repo
            .create("frank", "mot-de-passe-valide", Role::Member)
            .await
            .unwrap();

        // Default: no expiry. The plaintext is returned exactly once.
        let key = repo.issue_api_token(u.id, Some("vs-code"), None).await.unwrap();
        assert!(key.plaintext.starts_with("locaryn_"));
        assert_eq!(key.expires_at, None);

        // Optional expiry days are honoured.
        let short = repo
            .issue_api_token(u.id, Some("ci"), Some(7))
            .await
            .unwrap();
        assert!(short.expires_at.is_some());

        // A login-style session token (Circuit B) — same table, other kind.
        let sess = repo.issue_token(u.id, Some("portable"), 30).await.unwrap();
        assert!(sess.expires_at.is_some());

        // Both kinds show up in the list with their metadata, never plaintext.
        let listed = repo.list_tokens(u.id).await.unwrap();
        assert_eq!(listed.len(), 2);
        let kinds: Vec<_> = listed.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::Api));
        assert!(kinds.contains(&TokenKind::Session));
        let api = listed
            .iter()
            .find(|t| t.kind == TokenKind::Api)
            .unwrap();
        assert_eq!(api.label.as_deref(), Some("vs-code"));
        assert_eq!(api.hint.len(), 6);
        assert!(api.revoked_at.is_none());

        // An API key authenticates its owner just like a session token.
        let who = repo.user_for_token(&key.plaintext).await.unwrap();
        assert_eq!(who.expect("la clé API identifie son porteur").id, u.id);
    }

    #[tokio::test]
    async fn the_first_admin_is_found_for_device_pairing() {
        let (repo, _pool) = repo().await;
        assert_eq!(repo.first_admin_id().await, None, "aucun compte: pas d'admin");

        let admin = repo
            .create("host", "mot-de-passe-valide", Role::Admin)
            .await
            .unwrap();
        repo.create("membre", "mot-de-passe-valide", Role::Member)
            .await
            .unwrap();

        assert_eq!(repo.first_admin_id().await, Some(admin.id));
    }

    #[tokio::test]
    async fn revoking_an_api_key_kills_it_immediately() {
        let (repo, _pool) = repo().await;
        let u = repo
            .create("gisel", "mot-de-passe-valide", Role::Member)
            .await
            .unwrap();
        let key = repo
            .issue_api_token(u.id, Some("script"), None)
            .await
            .unwrap();
        assert!(repo.user_for_token(&key.plaintext).await.unwrap().is_some());

        repo.revoke_token(key.id).await.unwrap();
        assert!(
            repo.user_for_token(&key.plaintext).await.unwrap().is_none(),
            "révoquer une clé API doit cesser son accès immédiatement"
        );
        // The revoked key stays visible in the listing for audit.
        let listed = repo.list_tokens(u.id).await.unwrap();
        assert!(listed[0].revoked_at.is_some());
    }

    #[tokio::test]
    async fn disabling_an_account_kills_its_tokens_at_once() {
        let (repo, _pool) = repo().await;
        let u = repo
            .create("eve", "mot-de-passe-valide", Role::Member)
            .await
            .unwrap();
        let tok = repo.issue_token(u.id, None, 30).await.unwrap();
        assert!(repo.user_for_token(&tok.plaintext).await.unwrap().is_some());

        repo.set_disabled(u.id, true).await.unwrap();
        assert!(
            repo.user_for_token(&tok.plaintext).await.unwrap().is_none(),
            "désactiver un compte doit invalider ses jetons sans les révoquer un par un"
        );
        assert!(repo
            .authenticate("eve", "mot-de-passe-valide")
            .await
            .unwrap()
            .is_none());
    }
}
