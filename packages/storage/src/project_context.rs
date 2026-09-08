//! Le contexte d'un projet : ce qu'il faut savoir pour y travailler, et qui a
//! le droit de le savoir.
//!
//! **Sans domaine, volontairement.** Une fiche peut dire « le rendu final est
//! en A2 sur papier grain torchon », « les mesures se font à 20 °C, sinon la
//! dilatation fausse tout », « le client refuse le violet », ou « les tests
//! passent par `cargo test` ». Un projet d'art, un dossier de physique, un
//! mémoire ou du code posent la même question à qui arrive dessus : qu'est-ce
//! qui a déjà été décidé, et pourquoi. Rien ici ne suppose du code.
//!
//! **Trois portées, et c'est la seule chose qui distingue une fiche d'une
//! autre.** La portée n'est pas un détail de rangement : elle dit qui verra la
//! fiche, et se tromper est coûteux dans les deux sens. Une exigence
//! personnelle rangée en partagé impose à tout le monde ce qu'une seule
//! personne voulait ; une décision de projet rangée en personnel laisse les
//! autres l'ignorer et refaire le débat.
//!
//! C'est pourquoi l'application demande quand elle doute, au lieu de choisir à
//! la place de la personne.

use crate::StorageError;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

/// Qui verra une fiche.
///
/// Les deux dernières portées exigent le mode serveur : sans lui, il n'y a ni
/// compte à suivre entre deux appareils, ni personne avec qui partager. Cette
/// vérification n'est pas ici — le stockage accepte les trois — mais chez
/// l'appelant, qui sait si un serveur répond. Refuser au niveau de la base
/// aurait empêché le serveur lui-même d'écrire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextScope {
    /// Ne quitte jamais cet ordinateur : outils installés, chemins locaux,
    /// versions. Inutile à un collègue, et faux sur un autre poste.
    Machine,
    /// Suit la personne entre ses appareils. Ses exigences à elle.
    Compte,
    /// Visible de tous ceux qui travaillent sur ce projet, sur ce serveur.
    /// Ce que le projet a décidé, pas ce qu'une personne préfère.
    Partage,
}

impl ContextScope {
    pub fn as_str(self) -> &'static str {
        match self {
            ContextScope::Machine => "machine",
            ContextScope::Compte => "compte",
            ContextScope::Partage => "partage",
        }
    }

    /// Lit une portée. Un mot inconnu devient `Machine` — la plus restreinte.
    ///
    /// Le sens du repli compte : une fiche mal étiquetée qui reste sur
    /// l'ordinateur ne fait de tort à personne, alors qu'une fiche promue en
    /// partagé par accident impose à toute une équipe ce qu'elle n'a pas
    /// décidé. On se trompe donc du côté qui ne coûte rien.
    pub fn depuis(texte: &str) -> Self {
        match texte.trim().to_ascii_lowercase().as_str() {
            "compte" | "account" => ContextScope::Compte,
            "partage" | "partagé" | "shared" => ContextScope::Partage,
            _ => ContextScope::Machine,
        }
    }

    /// Cette portée a-t-elle besoin d'un serveur pour vouloir dire quelque
    /// chose ?
    pub fn exige_serveur(self) -> bool {
        !matches!(self, ContextScope::Machine)
    }
}

/// Une fiche de contexte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub id: String,
    pub project_id: String,
    pub scope: ContextScope,
    /// Qui a posé la fiche. Renseigné en mode serveur, où plusieurs personnes
    /// écrivent : une décision partagée sans auteur ne se discute pas.
    pub author: Option<String>,
    pub title: String,
    pub summary: String,
    pub details: Vec<String>,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
}

/// La forme brute : `details` est du JSON en texte, SQLite n'ayant pas de type
/// tableau, et `scope` une chaîne.
#[derive(Debug, Clone, FromRow)]
struct ContextRow {
    id: String,
    project_id: String,
    scope: String,
    author: Option<String>,
    title: String,
    summary: String,
    details: String,
    source: String,
    created_at: String,
    updated_at: String,
}

impl From<ContextRow> for ContextEntry {
    fn from(r: ContextRow) -> Self {
        ContextEntry {
            id: r.id,
            project_id: r.project_id,
            scope: ContextScope::depuis(&r.scope),
            author: r.author,
            title: r.title,
            summary: r.summary,
            details: serde_json::from_str(&r.details).unwrap_or_default(),
            source: r.source,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const COLS: &str =
    "id, project_id, scope, author, title, summary, details, source, created_at, updated_at";

#[derive(Clone)]
pub struct ProjectContextRepo {
    pool: SqlitePool,
}

impl ProjectContextRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Ajoute ce qu'on vient d'apprendre, ou le range dans la fiche existante.
    ///
    /// Une fiche par (projet, portée, titre) : un deuxième détail sur le même
    /// sujet s'y ajoute plutôt que de créer une fiche qui dirait la même chose
    /// autrement. Le résumé, lui, ne change pas — il a été écrit une fois pour
    /// dire de quoi parle la fiche, et le réécrire à chaque détail le ferait
    /// dériver.
    ///
    /// La même décision peut exister en `partage` et en `compte` sans se
    /// marcher dessus : le projet dit une chose, et quelqu'un peut en vouloir
    /// une autre pour lui.
    pub async fn remember(
        &self,
        project_id: &str,
        scope: ContextScope,
        author: Option<&str>,
        title: &str,
        detail: &str,
        source: &str,
    ) -> Result<ContextEntry, StorageError> {
        let title = title.trim();
        let detail = detail.trim();
        if title.is_empty() {
            return Err(StorageError::Conflict(
                "une fiche sans titre ne retient rien".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let existing = sqlx::query_as::<_, ContextRow>(&format!(
            "SELECT {COLS} FROM project_context \
             WHERE project_id = ? AND scope = ? AND lower(title) = lower(?)"
        ))
        .bind(project_id)
        .bind(scope.as_str())
        .bind(title)
        .fetch_optional(&mut *tx)
        .await?;
        let now = chrono::Utc::now().to_rfc3339();

        let row = match existing {
            Some(row) => {
                let mut details: Vec<String> =
                    serde_json::from_str(&row.details).unwrap_or_default();
                // Dédoublonné sans tenir compte de la casse : le même fait
                // redit autrement n'apporte rien, et une fiche qui répète se
                // lit mal.
                let deja = !detail.is_empty()
                    && details
                        .iter()
                        .any(|d| d.trim().eq_ignore_ascii_case(detail));
                if !detail.is_empty() && !deja {
                    details.push(detail.to_string());
                }
                let json = serde_json::to_string(&details).unwrap_or_else(|_| "[]".into());
                sqlx::query_as::<_, ContextRow>(&format!(
                    "UPDATE project_context SET details = ?, source = ?, updated_at = ? \
                     WHERE id = ? RETURNING {COLS}"
                ))
                .bind(&json)
                .bind(source)
                .bind(&now)
                .bind(&row.id)
                .fetch_one(&mut *tx)
                .await?
            }
            None => {
                // Le premier détail sert aussi de résumé : une fiche neuve doit
                // être lisible dans la liste sans qu'on l'ouvre.
                let details = if detail.is_empty() {
                    Vec::new()
                } else {
                    vec![detail.to_string()]
                };
                let json = serde_json::to_string(&details).unwrap_or_else(|_| "[]".into());
                sqlx::query_as::<_, ContextRow>(&format!(
                    "INSERT INTO project_context \
                     (id, project_id, scope, author, title, summary, details, source, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING {COLS}"
                ))
                .bind(Uuid::new_v4().to_string())
                .bind(project_id)
                .bind(scope.as_str())
                .bind(author)
                .bind(title)
                .bind(detail)
                .bind(&json)
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

    /// Les fiches d'un projet, la plus récemment touchée d'abord.
    ///
    /// `scopes` filtre : passer seulement `Machine` quand aucun serveur ne
    /// répond évite d'annoncer un contexte partagé qu'on ne peut pas lire.
    pub async fn list(
        &self,
        project_id: &str,
        scopes: &[ContextScope],
    ) -> Result<Vec<ContextEntry>, StorageError> {
        if scopes.is_empty() {
            return Ok(Vec::new());
        }
        // Les portées viennent d'un type fermé, jamais d'une saisie : les
        // interpoler ne peut rien injecter.
        let liste = scopes
            .iter()
            .map(|s| format!("'{}'", s.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let rows = sqlx::query_as::<_, ContextRow>(&format!(
            "SELECT {COLS} FROM project_context \
             WHERE project_id = ? AND scope IN ({liste}) \
             ORDER BY updated_at DESC"
        ))
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Une fiche par son identifiant.
    pub async fn find(&self, id: &str) -> Result<ContextEntry, StorageError> {
        let row = sqlx::query_as::<_, ContextRow>(&format!(
            "SELECT {COLS} FROM project_context WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound("fiche de contexte".into()))?;
        Ok(row.into())
    }

    /// Change la portée d'une fiche.
    ///
    /// C'est l'opération que la question à l'écran déclenche : on a écrit une
    /// fiche en personnel, on se rend compte qu'elle concerne tout le projet.
    /// Elle échoue si une fiche du même titre existe déjà dans la portée
    /// visée — les fusionner en silence perdrait des détails.
    pub async fn set_scope(
        &self,
        id: &str,
        scope: ContextScope,
    ) -> Result<ContextEntry, StorageError> {
        let mut tx = self.pool.begin().await?;
        let actuelle = sqlx::query_as::<_, ContextRow>(&format!(
            "SELECT {COLS} FROM project_context WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| StorageError::NotFound("fiche de contexte".into()))?;

        let collision: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM project_context \
             WHERE project_id = ? AND scope = ? AND lower(title) = lower(?) AND id <> ?",
        )
        .bind(&actuelle.project_id)
        .bind(scope.as_str())
        .bind(&actuelle.title)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if collision.is_some() {
            return Err(StorageError::Conflict(format!(
                "une fiche « {} » existe déjà dans cette portée : réunissez-les à la main plutôt que d'en perdre une",
                actuelle.title
            )));
        }

        let row = sqlx::query_as::<_, ContextRow>(&format!(
            "UPDATE project_context SET scope = ?, updated_at = ? WHERE id = ? RETURNING {COLS}"
        ))
        .bind(scope.as_str())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into())
    }

    /// Réécrit le résumé d'une fiche.
    pub async fn set_summary(&self, id: &str, summary: &str) -> Result<(), StorageError> {
        sqlx::query("UPDATE project_context SET summary = ?, updated_at = ? WHERE id = ?")
            .bind(summary.trim())
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Remplace les détails d'une fiche — le retrait d'un détail passe par là.
    pub async fn set_details(
        &self,
        id: &str,
        details: &[String],
    ) -> Result<ContextEntry, StorageError> {
        let json = serde_json::to_string(details).unwrap_or_else(|_| "[]".into());
        let row = sqlx::query_as::<_, ContextRow>(&format!(
            "UPDATE project_context SET details = ?, updated_at = ? WHERE id = ? RETURNING {COLS}"
        ))
        .bind(&json)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.into())
    }

    pub async fn forget(&self, id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM project_context WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn repo() -> ProjectContextRepo {
        let pool = crate::open_in_memory().await.expect("base en mémoire");
        ProjectContextRepo::new(pool)
    }

    /// Le sens du repli est une decision de securite, pas un detail.
    ///
    /// Une fiche mal etiquetee qui reste sur l'ordinateur ne fait de tort a
    /// personne. Une fiche promue en partage par accident impose a toute une
    /// equipe ce qu'elle n'a pas decide. On se trompe donc vers `Machine`.
    #[test]
    fn une_portee_inconnue_reste_la_plus_restreinte() {
        assert_eq!(ContextScope::depuis("machine"), ContextScope::Machine);
        assert_eq!(ContextScope::depuis("compte"), ContextScope::Compte);
        assert_eq!(ContextScope::depuis("partage"), ContextScope::Partage);
        assert_eq!(ContextScope::depuis("partagé"), ContextScope::Partage);
        for inconnu in ["", "equipe", "global", "tout le monde", "PARTAGER"] {
            assert_eq!(
                ContextScope::depuis(inconnu),
                ContextScope::Machine,
                "{inconnu} doit retomber sur la portee la plus restreinte"
            );
        }
    }

    #[test]
    fn seule_la_machine_se_passe_de_serveur() {
        assert!(!ContextScope::Machine.exige_serveur());
        assert!(ContextScope::Compte.exige_serveur());
        assert!(ContextScope::Partage.exige_serveur());
    }

    /// Le contexte ne suppose aucun domaine : un projet d'art et un dossier de
    /// physique s'y rangent comme du code.
    #[tokio::test]
    async fn une_fiche_se_relit_quel_que_soit_le_domaine() {
        let r = repo().await;
        r.remember(
            "atelier",
            ContextScope::Partage,
            Some("Camille"),
            "Format de rendu",
            "Le rendu final est en A2 sur papier grain torchon.",
            "utilisateur",
        )
        .await
        .unwrap();
        r.remember(
            "atelier",
            ContextScope::Partage,
            Some("Camille"),
            "Mesures",
            "Les mesures se font a 20 °C, sinon la dilatation fausse tout.",
            "utilisateur",
        )
        .await
        .unwrap();

        let fiches = r.list("atelier", &[ContextScope::Partage]).await.unwrap();
        assert_eq!(fiches.len(), 2);
        assert!(fiches.iter().any(|f| f.title == "Format de rendu"));
        assert_eq!(fiches[0].author.as_deref(), Some("Camille"));
    }

    /// Un deuxieme detail rejoint la fiche : le resume, ecrit une fois pour
    /// dire de quoi elle parle, ne derive pas.
    #[tokio::test]
    async fn un_detail_de_plus_rejoint_la_fiche_sans_changer_son_resume() {
        let r = repo().await;
        let a = r
            .remember(
                "memoire",
                ContextScope::Compte,
                None,
                "Relecture",
                "Je veux qu'on me relise avant d'envoyer.",
                "utilisateur",
            )
            .await
            .unwrap();
        let b = r
            .remember(
                "memoire",
                ContextScope::Compte,
                None,
                "relecture",
                "Surtout les chapitres theoriques.",
                "assistant",
            )
            .await
            .unwrap();

        assert_eq!(a.id, b.id, "le meme titre doit retrouver la meme fiche");
        assert_eq!(b.details.len(), 2);
        assert_eq!(b.summary, "Je veux qu'on me relise avant d'envoyer.");
    }

    #[tokio::test]
    async fn un_detail_deja_connu_ne_se_redit_pas() {
        let r = repo().await;
        r.remember(
            "p",
            ContextScope::Machine,
            None,
            "T",
            "un fait",
            "utilisateur",
        )
        .await
        .unwrap();
        let f = r
            .remember(
                "p",
                ContextScope::Machine,
                None,
                "T",
                "UN FAIT",
                "utilisateur",
            )
            .await
            .unwrap();
        assert_eq!(f.details.len(), 1, "la casse ne fait pas un fait nouveau");
    }

    /// Les portees ne se voient pas entre elles : c'est tout leur objet.
    #[tokio::test]
    async fn une_portee_ne_voit_pas_les_autres() {
        let r = repo().await;
        r.remember(
            "p",
            ContextScope::Machine,
            None,
            "Outils",
            "python 3.12 ici",
            "utilisateur",
        )
        .await
        .unwrap();
        r.remember(
            "p",
            ContextScope::Partage,
            None,
            "Cadre",
            "rendu en juin",
            "utilisateur",
        )
        .await
        .unwrap();

        let locales = r.list("p", &[ContextScope::Machine]).await.unwrap();
        assert_eq!(locales.len(), 1);
        assert_eq!(locales[0].title, "Outils");

        let sans_serveur = r.list("p", &[ContextScope::Machine]).await.unwrap();
        assert!(
            !sans_serveur.iter().any(|f| f.title == "Cadre"),
            "sans serveur, le partage ne doit pas apparaitre"
        );

        let tout = r
            .list(
                "p",
                &[
                    ContextScope::Machine,
                    ContextScope::Compte,
                    ContextScope::Partage,
                ],
            )
            .await
            .unwrap();
        assert_eq!(tout.len(), 2);
    }

    /// Le meme titre peut exister dans deux portees : le projet dit une chose,
    /// quelqu'un peut en vouloir une autre pour lui.
    #[tokio::test]
    async fn le_meme_titre_coexiste_dans_deux_portees() {
        let r = repo().await;
        let partage = r
            .remember(
                "p",
                ContextScope::Partage,
                None,
                "Langue",
                "on ecrit en francais",
                "utilisateur",
            )
            .await
            .unwrap();
        let perso = r
            .remember(
                "p",
                ContextScope::Compte,
                None,
                "Langue",
                "moi je travaille en anglais",
                "utilisateur",
            )
            .await
            .unwrap();
        assert_ne!(partage.id, perso.id);
    }

    /// Promouvoir une fiche est l'operation que la question a l'ecran
    /// declenche. Elle refuse d'ecraser une fiche du meme titre plutot que de
    /// fusionner en silence et de perdre des details.
    #[tokio::test]
    async fn promouvoir_une_fiche_refuse_d_ecraser() {
        let r = repo().await;
        let perso = r
            .remember(
                "p",
                ContextScope::Compte,
                None,
                "Langue",
                "en anglais",
                "utilisateur",
            )
            .await
            .unwrap();
        r.remember(
            "p",
            ContextScope::Partage,
            None,
            "Langue",
            "en francais",
            "utilisateur",
        )
        .await
        .unwrap();

        let err = r
            .set_scope(&perso.id, ContextScope::Partage)
            .await
            .expect_err("la collision doit etre refusee");
        assert!(format!("{err}").contains("existe déjà"), "{err}");

        // Sans collision, la promotion passe.
        let seule = r
            .remember("q", ContextScope::Compte, None, "Cadre", "x", "utilisateur")
            .await
            .unwrap();
        let promue = r.set_scope(&seule.id, ContextScope::Partage).await.unwrap();
        assert_eq!(promue.scope, ContextScope::Partage);
    }

    #[tokio::test]
    async fn une_fiche_sans_titre_est_refusee() {
        let r = repo().await;
        assert!(r
            .remember(
                "p",
                ContextScope::Machine,
                None,
                "   ",
                "un fait",
                "utilisateur"
            )
            .await
            .is_err());
    }
}
