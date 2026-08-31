//! Mémoire de l'utilisateur : ce que Locaryn retient d'une conversation à
//! l'autre.
//!
//! Elle vit dans la base du service, jamais dans un fichier de l'application :
//! le téléphone et le bureau parlent au même service et doivent voir la même
//! mémoire, et sur un serveur partagé chaque compte a la sienne.
//!
//! Une fiche par sujet, pas une phrase par souvenir. Un titre court
//! (« Bot Bastet »), un résumé d'une ligne, et des détails qui s'accumulent au
//! fil des conversations plutôt que de s'empiler en phrases redondantes.
//! Chaque fiche appartient à l'un de quatre groupes fixes — `vous`, `sujets`,
//! `zones`, `personnes` — qui organisent l'écran des réglages. Rien n'est
//! retenu en secret : l'écran montre chaque fiche dans le texte exact qui sera
//! envoyé au modèle. Une mémoire qu'on ne peut pas lire est une mémoire qu'on
//! ne peut pas corriger.

use crate::StorageError;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

/// Les quatre groupes fixes de l'écran des réglages. Un groupe hors de cette
/// liste est ramené à `Sujets` plutôt que refusé : une extraction du modèle
/// qui invente un cinquième groupe ne doit pas faire échouer tout le fait
/// qu'elle avait par ailleurs correctement compris.
pub const GROUPS: [&str; 4] = ["vous", "sujets", "zones", "personnes"];

pub fn normalize_group(group: &str) -> &'static str {
    match group.trim().to_ascii_lowercase().as_str() {
        "vous" => "vous",
        "zones" => "zones",
        "personnes" => "personnes",
        _ => "sujets",
    }
}

/// La forme brute, telle que la base la rend : `details` y est du JSON en
/// texte, parce que SQLite n'a pas de type tableau. [`MemoryEntry`] le
/// déplie pour tout le reste de l'application.
#[derive(Debug, Clone, FromRow)]
struct MemoryEntryRow {
    id: String,
    user_id: Option<String>,
    group_name: String,
    title: String,
    summary: String,
    details: String,
    source: String,
    created_at: String,
    updated_at: String,
}

impl From<MemoryEntryRow> for MemoryEntry {
    fn from(r: MemoryEntryRow) -> Self {
        let details = serde_json::from_str(&r.details).unwrap_or_default();
        MemoryEntry {
            id: r.id,
            user_id: r.user_id,
            group: r.group_name,
            title: r.title,
            summary: r.summary,
            details,
            source: r.source,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

/// Une fiche de mémoire, dans la forme que le reste de l'application
/// manipule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    /// Nul sur une installation personnelle : la mémoire est celle de la
    /// machine, il n'y a personne d'autre.
    pub user_id: Option<String>,
    /// `vous`, `sujets`, `zones` ou `personnes`.
    pub group: String,
    pub title: String,
    pub summary: String,
    pub details: Vec<String>,
    /// `utilisateur` ou `assistant` : qui a écrit le dernier détail.
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct MemoryRepo {
    pool: SqlitePool,
}

const ROW_COLUMNS: &str =
    "id, user_id, group_name, title, summary, details, source, created_at, updated_at";

impl MemoryRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Toutes les fiches de ce compte, groupées comme l'écran des réglages
    /// les montre — `vous` d'abord, puis `sujets`, `zones`, `personnes` — et
    /// les plus récemment mises à jour en tête de chaque groupe.
    pub async fn list(&self, user_id: Option<&str>) -> Result<Vec<MemoryEntry>, StorageError> {
        let rows = sqlx::query_as::<_, MemoryEntryRow>(&format!(
            "SELECT {ROW_COLUMNS} FROM memory_entries \
             WHERE COALESCE(user_id, '') = COALESCE(?, '') \
             ORDER BY \
                CASE group_name \
                    WHEN 'vous' THEN 0 WHEN 'sujets' THEN 1 \
                    WHEN 'zones' THEN 2 WHEN 'personnes' THEN 3 ELSE 4 \
                END, \
                updated_at DESC"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(MemoryEntry::from).collect())
    }

    /// Une fiche par son identifiant.
    pub async fn find(&self, id: &str) -> Result<MemoryEntry, StorageError> {
        let row = sqlx::query_as::<_, MemoryEntryRow>(&format!(
            "SELECT {ROW_COLUMNS} FROM memory_entries WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("mémoire {id}")))?;
        Ok(row.into())
    }

    /// Retient un détail sur un sujet. Une fiche du même groupe et du même
    /// titre existe déjà : le détail s'y ajoute — sauf s'il y est déjà, pour
    /// qu'entendre deux fois la même chose ne double pas la ligne. Sinon,
    /// une fiche naît, avec ce détail pour résumé initial.
    pub async fn remember(
        &self,
        user_id: Option<&str>,
        group: &str,
        title: &str,
        detail: &str,
        source: &str,
    ) -> Result<MemoryEntry, StorageError> {
        let title = title.trim();
        let detail = detail.trim();
        if title.is_empty() {
            return Err(StorageError::Conflict(
                "une fiche sans titre ne retient rien".into(),
            ));
        }
        let group = normalize_group(group);
        let mut tx = self.pool.begin().await?;
        let existing = sqlx::query_as::<_, MemoryEntryRow>(&format!(
            "SELECT {ROW_COLUMNS} FROM memory_entries \
             WHERE COALESCE(user_id, '') = COALESCE(?, '') AND group_name = ? AND lower(title) = lower(?)"
        ))
        .bind(user_id)
        .bind(group)
        .bind(title)
        .fetch_optional(&mut *tx)
        .await?;
        let now = chrono::Utc::now().to_rfc3339();

        let row = match existing {
            Some(row) => {
                let mut details: Vec<String> =
                    serde_json::from_str(&row.details).unwrap_or_default();
                let deja_connu = !detail.is_empty()
                    && details
                        .iter()
                        .any(|d| d.trim().eq_ignore_ascii_case(detail));
                if !detail.is_empty() && !deja_connu {
                    details.push(detail.to_string());
                }
                let details_json = serde_json::to_string(&details).unwrap_or_else(|_| "[]".into());
                sqlx::query_as::<_, MemoryEntryRow>(&format!(
                    "UPDATE memory_entries SET details = ?, source = ?, updated_at = ? \
                     WHERE id = ? RETURNING {ROW_COLUMNS}"
                ))
                .bind(&details_json)
                .bind(source)
                .bind(&now)
                .bind(&row.id)
                .fetch_one(&mut *tx)
                .await?
            }
            None => {
                let id = Uuid::new_v4().to_string();
                let details = if detail.is_empty() {
                    Vec::new()
                } else {
                    vec![detail.to_string()]
                };
                let details_json = serde_json::to_string(&details).unwrap_or_else(|_| "[]".into());
                sqlx::query_as::<_, MemoryEntryRow>(&format!(
                    "INSERT INTO memory_entries \
                     (id, user_id, group_name, title, summary, details, source, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
                     RETURNING {ROW_COLUMNS}"
                ))
                .bind(&id)
                .bind(user_id)
                .bind(group)
                .bind(title)
                .bind(detail)
                .bind(&details_json)
                .bind(source)
                .bind(&now)
                .bind(&now)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        tx.commit().await?;
        Ok(row.into())
    }

    /// Corrige le résumé d'une fiche — ce qui se montre sans l'ouvrir.
    pub async fn set_summary(&self, id: &str, summary: &str) -> Result<MemoryEntry, StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, MemoryEntryRow>(&format!(
            "UPDATE memory_entries SET summary = ?, updated_at = ? WHERE id = ? \
             RETURNING {ROW_COLUMNS}"
        ))
        .bind(summary.trim())
        .bind(&now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("mémoire {id}")))?;
        Ok(row.into())
    }

    /// Renomme une fiche.
    pub async fn rename(&self, id: &str, title: &str) -> Result<MemoryEntry, StorageError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(StorageError::Conflict(
                "un titre vide n'est pas un titre".into(),
            ));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, MemoryEntryRow>(&format!(
            "UPDATE memory_entries SET title = ?, updated_at = ? WHERE id = ? \
             RETURNING {ROW_COLUMNS}"
        ))
        .bind(title)
        .bind(&now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("mémoire {id}")))?;
        Ok(row.into())
    }

    /// Déplace une fiche dans un autre groupe.
    pub async fn set_group(&self, id: &str, group: &str) -> Result<MemoryEntry, StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query_as::<_, MemoryEntryRow>(&format!(
            "UPDATE memory_entries SET group_name = ?, updated_at = ? WHERE id = ? \
             RETURNING {ROW_COLUMNS}"
        ))
        .bind(normalize_group(group))
        .bind(&now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("mémoire {id}")))?;
        Ok(row.into())
    }

    /// Remplace intégralement les détails d'une fiche — utilisé quand on
    /// retire un détail précis plutôt que d'en ajouter un.
    pub async fn set_details(
        &self,
        id: &str,
        details: &[String],
    ) -> Result<MemoryEntry, StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let details_json = serde_json::to_string(details).unwrap_or_else(|_| "[]".into());
        let row = sqlx::query_as::<_, MemoryEntryRow>(&format!(
            "UPDATE memory_entries SET details = ?, updated_at = ? WHERE id = ? \
             RETURNING {ROW_COLUMNS}"
        ))
        .bind(&details_json)
        .bind(&now)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("mémoire {id}")))?;
        Ok(row.into())
    }

    /// Oublie une fiche entière. Un oubli est définitif : garder une trace de
    /// ce que quelqu'un a demandé d'oublier serait le contraire de ce qu'il a
    /// demandé.
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
            if e.details.is_empty() {
                out.push_str(&format!("- [{}] {} : {}\n", e.group, e.title, e.summary));
            } else {
                out.push_str(&format!(
                    "- [{}] {} : {}\n",
                    e.group,
                    e.title,
                    e.details.join(" ; ")
                ));
            }
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
        r.remember(
            None,
            "vous",
            "Préférences",
            "Préfère le français",
            "utilisateur",
        )
        .await
        .unwrap();
        let all = r.list(None).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "Préférences");
        assert_eq!(all[0].details, vec!["Préfère le français".to_string()]);
    }

    /// Le premier détail devient le résumé : c'est ce que l'écran montre sans
    /// ouvrir la fiche.
    #[tokio::test]
    async fn le_premier_detail_devient_le_resume() {
        let r = repo().await;
        let e = r
            .remember(
                None,
                "zones",
                "Bot Bastet",
                "Robot compagnon de campus",
                "assistant",
            )
            .await
            .unwrap();
        assert_eq!(e.summary, "Robot compagnon de campus");
    }

    /// Un deuxième détail sur le même sujet s'ajoute à la fiche existante —
    /// il ne crée pas une seconde fiche « Bot Bastet ».
    #[tokio::test]
    async fn un_second_detail_s_ajoute_a_la_fiche_existante() {
        let r = repo().await;
        r.remember(
            None,
            "zones",
            "Bot Bastet",
            "Robot compagnon de campus",
            "assistant",
        )
        .await
        .unwrap();
        let e = r
            .remember(
                None,
                "zones",
                "bot bastet",
                "Pile ROS2, YOLOv8",
                "assistant",
            )
            .await
            .unwrap();
        let all = r.list(None).await.unwrap();
        assert_eq!(all.len(), 1, "même titre, même groupe : une seule fiche");
        assert_eq!(e.details.len(), 2);
        assert_eq!(
            e.summary, "Robot compagnon de campus",
            "le résumé initial ne bouge pas"
        );
    }

    /// Le même détail entendu deux fois ne s'empile pas.
    #[tokio::test]
    async fn le_meme_detail_ne_s_empile_pas() {
        let r = repo().await;
        r.remember(
            None,
            "sujets",
            "Fpv Drones",
            "Construit ses propres drones",
            "assistant",
        )
        .await
        .unwrap();
        let e = r
            .remember(
                None,
                "sujets",
                "Fpv Drones",
                "construit ses propres drones",
                "assistant",
            )
            .await
            .unwrap();
        assert_eq!(e.details.len(), 1);
    }

    #[tokio::test]
    async fn un_groupe_inconnu_retombe_sur_sujets() {
        let r = repo().await;
        let e = r
            .remember(None, "n-importe-quoi", "Test", "Un détail", "assistant")
            .await
            .unwrap();
        assert_eq!(e.group, "sujets");
    }

    #[tokio::test]
    async fn un_oubli_est_definitif() {
        let r = repo().await;
        let e = r
            .remember(None, "sujets", "À oublier", "Un détail", "utilisateur")
            .await
            .unwrap();
        r.forget(&e.id).await.unwrap();
        assert!(r.list(None).await.unwrap().is_empty());
        assert!(r.forget(&e.id).await.is_err(), "oublier deux fois échoue");
    }

    #[tokio::test]
    async fn retirer_un_detail_precis_laisse_les_autres() {
        let r = repo().await;
        r.remember(
            None,
            "personnes",
            "Paul",
            "Coéquipier sur Bot Bastet",
            "assistant",
        )
        .await
        .unwrap();
        let e = r
            .remember(None, "personnes", "Paul", "Aime le café", "assistant")
            .await
            .unwrap();
        let restants: Vec<String> = e
            .details
            .iter()
            .filter(|d| *d != "Aime le café")
            .cloned()
            .collect();
        let e = r.set_details(&e.id, &restants).await.unwrap();
        assert_eq!(e.details, vec!["Coéquipier sur Bot Bastet".to_string()]);
    }

    #[tokio::test]
    async fn sans_rien_en_memoire_aucun_bloc_nest_ajoute_au_prompt() {
        let r = repo().await;
        assert!(r.as_system_block(None).await.unwrap().is_none());
        r.remember(
            None,
            "sujets",
            "Code la nuit",
            "Travaille surtout en soirée",
            "utilisateur",
        )
        .await
        .unwrap();
        let block = r.as_system_block(None).await.unwrap().expect("un bloc");
        assert!(block.contains("Code la nuit"));
        assert!(block.contains("[sujets]"));
    }

    /// L'ordre des groupes dans la liste suit celui de l'écran : vous,
    /// sujets, zones, personnes — jamais l'ordre alphabétique des groupes.
    #[tokio::test]
    async fn les_groupes_sortent_dans_l_ordre_de_l_ecran() {
        let r = repo().await;
        r.remember(None, "personnes", "Simon", "Coéquipier", "assistant")
            .await
            .unwrap();
        r.remember(None, "vous", "Profil", "Étudiant", "assistant")
            .await
            .unwrap();
        r.remember(None, "zones", "Reapc Infra", "Déploiements", "assistant")
            .await
            .unwrap();
        let all = r.list(None).await.unwrap();
        let groupes: Vec<&str> = all.iter().map(|e| e.group.as_str()).collect();
        assert_eq!(groupes, vec!["vous", "zones", "personnes"]);
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
            "sujets",
            "Chose d'Alice",
            "détail d'Alice",
            "utilisateur",
        )
        .await
        .unwrap();
        r.remember(
            Some(&b.id.to_string()),
            "sujets",
            "Chose de Bob",
            "détail de Bob",
            "utilisateur",
        )
        .await
        .unwrap();
        let seen = r.list(Some(&a.id.to_string())).await.unwrap();
        assert_eq!(seen.len(), 1, "chacun ne voit que la sienne");
        assert_eq!(seen[0].title, "Chose d'Alice");
    }
}
