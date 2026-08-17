//! Les figures : un rôle, ses consignes, ses conversations.
//!
//! Une figure retient comment le modèle doit se comporter — ce qu'il sait, ce
//! qu'il doit faire, ce qu'il ne doit pas faire — et ces consignes sont
//! réinjectées à chaque tour de ses conversations.
//!
//! Elles vivent ici et non dans les fichiers de l'extension qui les affiche :
//! une figure écrite sur l'ordinateur doit s'ouvrir sur le téléphone, et
//! survivre au retrait puis à la réinstallation de cette extension.

use crate::error::StorageError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Figure {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Ce que le modèle reçoit avant toute conversation. C'est le cœur.
    pub instructions: String,
    /// Le modèle qui la fait tourner. Absent : celui de l'application.
    pub model: Option<String>,
    /// Une première phrase, proposée à l'ouverture.
    pub opening: Option<String>,
    /// Vrai quand la figure lit la mémoire de l'utilisateur.
    pub uses_memory: bool,
    /// `user` pour une figure écrite à la main, sinon le dépôt d'où elle vient.
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Ce qu'il faut pour écrire une figure.
///
/// Un objet plutôt que huit paramètres : à ce compte-là, `upsert(nom, "",
/// consignes, None, None, false, "user")` ne se relit pas, et intervertir deux
/// options du même type passe la compilation sans qu'on s'en aperçoive.
#[derive(Debug, Clone, Default)]
pub struct NouvelleFigure<'a> {
    pub name: &'a str,
    pub description: &'a str,
    /// Ce que le modèle reçoit avant toute conversation. C'est le cœur.
    pub instructions: &'a str,
    pub model: Option<&'a str>,
    pub opening: Option<&'a str>,
    pub uses_memory: bool,
    /// `user` pour une figure écrite à la main, sinon le dépôt d'où elle vient.
    pub source: &'a str,
}

#[derive(sqlx::FromRow)]
struct FigureRow {
    id: String,
    name: String,
    description: String,
    instructions: String,
    model: Option<String>,
    opening: Option<String>,
    uses_memory: i64,
    source: String,
    created_at: String,
    updated_at: String,
}

impl From<FigureRow> for Figure {
    fn from(r: FigureRow) -> Self {
        Figure {
            id: r.id,
            name: r.name,
            description: r.description,
            instructions: r.instructions,
            model: r.model,
            opening: r.opening,
            uses_memory: r.uses_memory != 0,
            source: r.source,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const COLONNES: &str = "id, name, description, instructions, model, opening, \
                        uses_memory, source, created_at, updated_at";

#[derive(Clone)]
pub struct FigureRepo {
    pool: SqlitePool,
}

impl FigureRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<Figure>, StorageError> {
        let rows = sqlx::query_as::<_, FigureRow>(&format!(
            "SELECT {COLONNES} FROM figures ORDER BY lower(name)"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Figure::from).collect())
    }

    pub async fn get(&self, id: &str) -> Result<Figure, StorageError> {
        let row =
            sqlx::query_as::<_, FigureRow>(&format!("SELECT {COLONNES} FROM figures WHERE id = ?"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Figure::from)
            .ok_or_else(|| StorageError::NotFound(format!("figure {id}")))
    }

    /// Créer une figure, ou remplacer celle du même nom.
    ///
    /// Le remplacement par le nom sert la réinstallation d'une extension :
    /// remettre ses figures ne doit pas en créer des doubles. Une figure
    /// écrite à la main n'est jamais écrasée par un dépôt — c'est le travail
    /// de quelqu'un.
    pub async fn upsert(&self, neuve: NouvelleFigure<'_>) -> Result<Figure, StorageError> {
        let NouvelleFigure {
            description,
            model,
            opening,
            uses_memory,
            source,
            ..
        } = neuve;
        let name = neuve.name.trim();
        let instructions = neuve.instructions.trim();
        if name.is_empty() {
            return Err(StorageError::NotFound("une figure sans nom".into()));
        }
        if instructions.is_empty() {
            return Err(StorageError::NotFound(
                "une figure sans consignes ne dit rien au modèle".into(),
            ));
        }

        let existante = sqlx::query_as::<_, FigureRow>(&format!(
            "SELECT {COLONNES} FROM figures WHERE lower(name) = lower(?)"
        ))
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        let maintenant = chrono::Utc::now().to_rfc3339();
        if let Some(ex) = existante {
            if ex.source == "user" && source != "user" {
                // Une extension ne réécrit pas ce qu'une personne a écrit.
                return Ok(ex.into());
            }
            sqlx::query(
                "UPDATE figures SET description = ?, instructions = ?, model = ?, \
                 opening = ?, uses_memory = ?, source = ?, updated_at = ? WHERE id = ?",
            )
            .bind(description)
            .bind(instructions)
            .bind(model)
            .bind(opening)
            .bind(i64::from(uses_memory))
            .bind(source)
            .bind(&maintenant)
            .bind(&ex.id)
            .execute(&self.pool)
            .await?;
            return self.get(&ex.id).await;
        }

        let id = Uuid::new_v4().to_string();
        sqlx::query(&format!(
            "INSERT INTO figures ({COLONNES}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(instructions)
        .bind(model)
        .bind(opening)
        .bind(i64::from(uses_memory))
        .bind(source)
        .bind(&maintenant)
        .bind(&maintenant)
        .execute(&self.pool)
        .await?;
        self.get(&id).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let res = sqlx::query("DELETE FROM figures WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if res.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("figure {id}")));
        }
        Ok(())
    }

    /// Rattacher une conversation à une figure, ou l'en détacher.
    pub async fn attach_session(
        &self,
        session_id: Uuid,
        figure_id: Option<&str>,
    ) -> Result<(), StorageError> {
        sqlx::query("UPDATE sessions SET figure_id = ? WHERE id = ?")
            .bind(figure_id)
            .bind(session_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// La figure qui tient une conversation, s'il y en a une.
    pub async fn for_session(&self, session_id: Uuid) -> Result<Option<Figure>, StorageError> {
        let row = sqlx::query_as::<_, FigureRow>(
            "SELECT f.id, f.name, f.description, f.instructions, f.model, f.opening, \
             f.uses_memory, f.source, f.created_at, f.updated_at \
             FROM figures f JOIN sessions s ON s.figure_id = f.id WHERE s.id = ?",
        )
        .bind(session_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Figure::from))
    }

    /// Les conversations d'une figure, la plus récente d'abord.
    pub async fn session_ids(&self, figure_id: &str) -> Result<Vec<String>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM sessions WHERE figure_id = ? AND archived_at IS NULL \
             ORDER BY COALESCE(last_message_at, created_at) DESC",
        )
        .bind(figure_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn depot() -> FigureRepo {
        let pool = crate::open(std::path::Path::new(":memory:")).await.unwrap();
        FigureRepo::new(pool)
    }

    #[tokio::test]
    async fn une_figure_sans_consignes_est_refusee() {
        // Elle ne dirait rien au modèle : autant ne pas la créer.
        let d = depot().await;
        assert!(d
            .upsert(NouvelleFigure {
                name: "vide",
                description: "",
                instructions: "   ",
                model: None,
                opening: None,
                uses_memory: false,
                source: "user",
            })
            .await
            .is_err());
        assert!(d
            .upsert(NouvelleFigure {
                name: "  ",
                description: "",
                instructions: "des consignes",
                model: None,
                opening: None,
                uses_memory: false,
                source: "user",
            })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn reinstaller_une_extension_ne_double_pas_ses_figures() {
        let d = depot().await;
        let a = d
            .upsert(NouvelleFigure {
                name: "Relecteur",
                description: "",
                instructions: "v1",
                model: None,
                opening: None,
                uses_memory: false,
                source: "Locaryn/plugin-figures",
            })
            .await
            .unwrap();
        let b = d
            .upsert(NouvelleFigure {
                name: "relecteur",
                description: "",
                instructions: "v2",
                model: None,
                opening: None,
                uses_memory: false,
                source: "Locaryn/plugin-figures",
            })
            .await
            .unwrap();
        assert_eq!(a.id, b.id, "le nom identifie la figure");
        assert_eq!(b.instructions, "v2");
        assert_eq!(d.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn une_extension_ne_reecrit_pas_ce_qu_une_personne_a_ecrit() {
        let d = depot().await;
        d.upsert(NouvelleFigure {
            name: "Relecteur",
            description: "",
            instructions: "les miennes",
            model: None,
            opening: None,
            uses_memory: false,
            source: "user",
        })
        .await
        .unwrap();
        let apres = d
            .upsert(NouvelleFigure {
                name: "Relecteur",
                description: "",
                instructions: "celles du dépôt",
                model: None,
                opening: None,
                uses_memory: false,
                source: "Locaryn/plugin-figures",
            })
            .await
            .unwrap();
        assert_eq!(
            apres.instructions, "les miennes",
            "le travail de quelqu'un ne se fait pas écraser par une mise à jour"
        );
    }
}
