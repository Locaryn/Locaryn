//! Mémoire de l'utilisateur : ce que Locaryn retient d'une conversation à
//! l'autre.
//!
//! Elle vit dans la base du service, jamais dans un fichier de l'application :
//! le téléphone et le bureau parlent au même service et doivent voir la même
//! mémoire, et sur un serveur partagé chaque compte a la sienne.
//!
//! Rien n'est retenu en secret. Chaque entrée porte qui l'a écrite — la
//! personne ou le modèle — et l'écran des réglages les montre toutes, dans le
//! texte exact qui sera envoyé au modèle. Une mémoire qu'on ne peut pas lire
//! est une mémoire qu'on ne peut pas corriger.

use crate::StorageError;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

/// Une chose retenue.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemoryEntry {
    pub id: String,
    /// Nul sur une installation personnelle : la mémoire est celle de la
    /// machine, il n'y a personne d'autre.
    pub user_id: Option<String>,
    /// `preference`, `habitude`, `projet` ou `fait`.
    pub category: String,
    pub content: String,
    /// `utilisateur` ou `assistant`.
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct MemoryRepo {
    pool: SqlitePool,
}

impl MemoryRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Tout ce qui est retenu pour ce compte, le plus récent d'abord.
    pub async fn list(&self, user_id: Option<&str>) -> Result<Vec<MemoryEntry>, StorageError> {
        let rows = sqlx::query_as::<_, MemoryEntry>(
            "SELECT id, user_id, category, content, source, created_at, updated_at \
             FROM memory_entries WHERE COALESCE(user_id, '') = COALESCE(?, '') \
             ORDER BY updated_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Retient une chose. Réécrire la même phrase la met à jour plutôt que de
    /// l'empiler : une mémoire qui répète dilue le contexte du modèle.
    pub async fn remember(
        &self,
        user_id: Option<&str>,
        category: &str,
        content: &str,
        source: &str,
    ) -> Result<MemoryEntry, StorageError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(StorageError::Conflict(
                "une entrée de mémoire vide ne retient rien".into(),
            ));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        let row = sqlx::query_as::<_, MemoryEntry>(
            "INSERT INTO memory_entries (id, user_id, category, content, source, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (COALESCE(user_id, ''), lower(content)) DO UPDATE SET \
                 category = excluded.category, source = excluded.source, updated_at = excluded.updated_at \
             RETURNING id, user_id, category, content, source, created_at, updated_at",
        )
        .bind(&id)
        .bind(user_id)
        .bind(category)
        .bind(content)
        .bind(source)
        .bind(&now)
        .bind(&now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Corrige une entrée existante.
    pub async fn update(
        &self,
        id: &str,
        category: &str,
        content: &str,
    ) -> Result<MemoryEntry, StorageError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(StorageError::Conflict(
                "une entrée de mémoire vide ne retient rien".into(),
            ));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, MemoryEntry>(
            "UPDATE memory_entries SET category = ?, content = ?, updated_at = ? WHERE id = ? \
             RETURNING id, user_id, category, content, source, created_at, updated_at",
        )
        .bind(category)
        .bind(content)
        .bind(&now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("mémoire {id}")))?;
        Ok(row)
    }

    /// Oublie une entrée. Un oubli est définitif : garder une trace de ce que
    /// quelqu'un a demandé d'oublier serait le contraire de ce qu'il a demandé.
    pub async fn forget(&self, id: &str) -> Result<(), StorageError> {
        let done = sqlx::query("DELETE FROM memory_entries WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("mémoire {id}")));
        }
        Ok(())
    }

    /// Oublie tout, pour ce compte.
    pub async fn forget_all(&self, user_id: Option<&str>) -> Result<u64, StorageError> {
        let done =
            sqlx::query("DELETE FROM memory_entries WHERE COALESCE(user_id, '') = COALESCE(?, '')")
                .bind(user_id)
                .execute(&self.pool)
                .await?;
        Ok(done.rows_affected())
    }

    /// Le bloc versé au prompt système, ou `None` quand rien n'est retenu.
    ///
    /// Écrit en clair, dans la langue de l'interface : c'est exactement ce que
    /// l'écran des réglages affiche, pour que personne n'ait à deviner ce que
    /// le modèle sait de lui.
    pub async fn as_system_block(
        &self,
        user_id: Option<&str>,
    ) -> Result<Option<String>, StorageError> {
        let entries = self.list(user_id).await?;
        if entries.is_empty() {
            return Ok(None);
        }
        let mut out = String::from(
            "Ce que tu sais de la personne avec qui tu parles. Tiens-en compte \
             sans le répéter ni le commenter :\n",
        );
        for e in &entries {
            out.push_str(&format!("- [{}] {}\n", e.category, e.content));
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn repo() -> MemoryRepo {
        let pool = crate::open_in_memory().await.expect("base en mémoire");
        MemoryRepo::new(pool)
    }

    #[tokio::test]
    async fn ce_qui_est_retenu_se_relit() {
        let r = repo().await;
        r.remember(None, "preference", "Préfère le français", "utilisateur")
            .await
            .unwrap();
        let all = r.list(None).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].content, "Préfère le français");
    }

    #[tokio::test]
    async fn la_meme_phrase_ne_sempile_pas() {
        let r = repo().await;
        r.remember(None, "fait", "Travaille sur Locaryn", "utilisateur")
            .await
            .unwrap();
        r.remember(None, "projet", "travaille sur locaryn", "assistant")
            .await
            .unwrap();
        let all = r.list(None).await.unwrap();
        assert_eq!(all.len(), 1, "une phrase déjà connue est mise à jour");
        assert_eq!(all[0].category, "projet");
    }

    #[tokio::test]
    async fn un_oubli_est_definitif() {
        let r = repo().await;
        let e = r
            .remember(None, "fait", "À oublier", "utilisateur")
            .await
            .unwrap();
        r.forget(&e.id).await.unwrap();
        assert!(r.list(None).await.unwrap().is_empty());
        assert!(r.forget(&e.id).await.is_err(), "oublier deux fois échoue");
    }

    #[tokio::test]
    async fn sans_rien_en_memoire_aucun_bloc_nest_ajoute_au_prompt() {
        let r = repo().await;
        assert!(r.as_system_block(None).await.unwrap().is_none());
        r.remember(None, "habitude", "Code la nuit", "utilisateur")
            .await
            .unwrap();
        let block = r.as_system_block(None).await.unwrap().expect("un bloc");
        assert!(block.contains("Code la nuit"));
        assert!(block.contains("[habitude]"));
    }

    #[tokio::test]
    async fn deux_comptes_ne_partagent_pas_leur_memoire() {
        let pool = crate::open_in_memory().await.expect("base en mémoire");
        let users = crate::users::UserRepo::new(pool.clone());
        // De vrais comptes : la clé étrangère est vérifiée, et c'est
        // précisément ce qu'on veut — une mémoire ne peut pas appartenir à un
        // compte qui n'existe pas.
        let a = users
            .create("alice", "motdepasse-long", crate::users::Role::Member)
            .await
            .unwrap();
        let b = users
            .create("bob", "motdepasse-long", crate::users::Role::Member)
            .await
            .unwrap();
        let r = MemoryRepo::new(pool);
        r.remember(
            Some(&a.id.to_string()),
            "fait",
            "Chose d'Alice",
            "utilisateur",
        )
        .await
        .unwrap();
        r.remember(
            Some(&b.id.to_string()),
            "fait",
            "Chose de Bob",
            "utilisateur",
        )
        .await
        .unwrap();
        let seen = r.list(Some(&a.id.to_string())).await.unwrap();
        assert_eq!(seen.len(), 1, "chacun ne voit que la sienne");
        assert_eq!(seen[0].content, "Chose d'Alice");
    }
}
