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

impl UserRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
        let hash = lochor_auth::hash_token(password).hash;
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
            let _ = lochor_auth::verify_token(
                password,
                &lochor_auth::TokenHash {
                    hash: DUMMY_HASH.to_string(),
                },
            );
            return Ok(None);
        };

        let ok = lochor_auth::verify_token(
            password,
            &lochor_auth::TokenHash {
                hash: row.password_hash.clone(),
            },
        );
        if !ok || row.disabled_at.is_some() {
            return Ok(None);
        }
        Ok(Some(row.to_user()?))
    }

    /// Issue an API token for a user. The plaintext is returned once.
    pub async fn issue_token(
        &self,
        user_id: Uuid,
        label: Option<&str>,
        valid_days: i64,
    ) -> Result<IssuedToken, StorageError> {
        let plaintext = lochor_auth::generate_token();
        let hash = lochor_auth::hash_token(&plaintext).hash;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let expires = if valid_days > 0 {
            Some((now + chrono::Duration::days(valid_days)).to_rfc3339())
        } else {
            None
        };
        // Enough to recognise a token in a list, far too little to reconstruct.
        let hint: String = plaintext.chars().skip(7).take(6).collect();

        sqlx::query(
            "INSERT INTO auth_tokens (id, user_id, token_hash, hint, label, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(&hash)
        .bind(&hint)
        .bind(label)
        .bind(now.to_rfc3339())
        .bind(&expires)
        .execute(&self.pool)
        .await?;

        Ok(IssuedToken {
            id,
            plaintext,
            expires_at: expires,
        })
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
            if !lochor_auth::verify_token(
                plaintext,
                &lochor_auth::TokenHash {
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
    async fn an_account_authenticates_only_with_its_own_password() {
        let (repo, pool) = repo().await;

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
        assert!(repo.authenticate("Marie", "mauvais").await.unwrap().is_none());
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
        assert!(!hash.contains("correct-horse-battery"), "mot de passe en clair !");
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
        assert!(dup.is_err(), "un doublon insensible à la casse doit être refusé");

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
        assert!(issued.plaintext.starts_with("lochor_"));

        let who = repo.user_for_token(&issued.plaintext).await.unwrap();
        assert_eq!(who.expect("le jeton doit identifier son porteur").id, u.id);

        // The plaintext must not be recoverable from storage.
        let (stored,): (String,) = sqlx::query_as("SELECT token_hash FROM auth_tokens LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!stored.contains(&issued.plaintext), "jeton stocké en clair !");

        assert!(repo.user_for_token("lochor_inventé").await.unwrap().is_none());
        assert!(repo.user_for_token("").await.unwrap().is_none());

        repo.revoke_token(issued.id).await.unwrap();
        assert!(
            repo.user_for_token(&issued.plaintext).await.unwrap().is_none(),
            "un jeton révoqué doit cesser de fonctionner immédiatement"
        );

    }

    #[tokio::test]
    async fn disabling_an_account_kills_its_tokens_at_once() {
        let (repo, _pool) = repo().await;
        let u = repo.create("eve", "mot-de-passe-valide", Role::Member).await.unwrap();
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
