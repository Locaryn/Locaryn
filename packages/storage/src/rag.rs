//! Les documents d'un projet, découpés et rangés pour être retrouvés.
//!
//! Le principe tient en trois gestes. On découpe un texte en morceaux, on note
//! ce que chaque morceau veut dire sous forme de vecteur, et à chaque question
//! on ne remonte que les morceaux les plus proches. Sans cela, ou bien on
//! envoie tout le document au modèle — ce que la fenêtre de contexte ne
//! supporte pas — ou bien on n'envoie rien, et le modèle répond à côté.
//!
//! Ce qui vit ici, c'est le rangement et la recherche. Le calcul des vecteurs
//! appartient au moteur d'inférence : c'est lui qui a le modèle en mémoire, et
//! ce module n'a pas à savoir comment on parle à un serveur.

use crate::error::StorageError;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// Ce que l'index contient, tel que l'écran le montre.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RagStatus {
    pub chunk_count: u32,
    /// La taille des vecteurs. Zéro quand rien n'est indexé.
    pub dim: u32,
    /// Le modèle qui a produit les vecteurs présents.
    pub embed_model: String,
    /// Les documents indexés, par leur nom.
    pub sources: Vec<String>,
}

/// Un morceau retrouvé, avec sa proximité à la question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagHit {
    pub source: String,
    pub text: String,
    /// Cosinus entre la question et le morceau : 1 = même direction.
    pub score: f32,
}

/// Un morceau prêt à être rangé : son texte et son vecteur.
#[derive(Debug, Clone)]
pub struct MorceauAIndexer {
    pub text: String,
    pub embedding: Vec<f32>,
}

#[derive(Clone)]
pub struct RagRepo {
    pool: SqlitePool,
}

impl RagRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Ranger les morceaux d'un document.
    ///
    /// Réindexer le même document le remplace au lieu de le doubler : c'est ce
    /// qu'on veut quand un fichier a changé, et personne ne pense à effacer
    /// l'ancienne version d'abord.
    pub async fn index(
        &self,
        project_id: Uuid,
        source: &str,
        embed_model: &str,
        morceaux: &[MorceauAIndexer],
    ) -> Result<RagStatus, StorageError> {
        if morceaux.is_empty() {
            return self.status(project_id).await;
        }
        let dim = morceaux[0].embedding.len();
        if dim == 0 {
            return Err(StorageError::Decode(
                "le moteur a rendu un vecteur vide".into(),
            ));
        }
        // Des vecteurs de tailles différentes ne se comparent pas. Mieux vaut
        // refuser que ranger quelque chose qui faussera toutes les recherches.
        if morceaux.iter().any(|m| m.embedding.len() != dim) {
            return Err(StorageError::Decode(
                "le moteur a rendu des vecteurs de tailles différentes".into(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM rag_chunks WHERE project_id = ? AND source = ?")
            .bind(project_id.to_string())
            .bind(source)
            .execute(&mut *tx)
            .await?;

        let maintenant = chrono::Utc::now().to_rfc3339();
        for (rang, m) in morceaux.iter().enumerate() {
            sqlx::query(
                "INSERT INTO rag_chunks \
                 (id, project_id, source, ordinal, text, embedding, dim, embed_model, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(project_id.to_string())
            .bind(source)
            .bind(rang as i64)
            .bind(&m.text)
            .bind(en_octets(&m.embedding))
            .bind(dim as i64)
            .bind(embed_model)
            .bind(&maintenant)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.status(project_id).await
    }

    /// Ce que l'index du projet contient.
    pub async fn status(&self, project_id: Uuid) -> Result<RagStatus, StorageError> {
        let rows = sqlx::query(
            "SELECT source, dim, embed_model FROM rag_chunks WHERE project_id = ? \
             ORDER BY source, ordinal",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut sources: Vec<String> = Vec::new();
        let mut dim = 0i64;
        let mut modele = String::new();
        for r in &rows {
            let s: String = r.try_get("source")?;
            if !sources.contains(&s) {
                sources.push(s);
            }
            dim = r.try_get("dim")?;
            modele = r.try_get("embed_model")?;
        }
        Ok(RagStatus {
            chunk_count: rows.len() as u32,
            dim: dim as u32,
            embed_model: modele,
            sources,
        })
    }

    /// Combien de morceaux porte un document.
    pub async fn count_for_source(
        &self,
        project_id: Uuid,
        source: &str,
    ) -> Result<u32, StorageError> {
        let row =
            sqlx::query("SELECT COUNT(*) AS n FROM rag_chunks WHERE project_id = ? AND source = ?")
                .bind(project_id.to_string())
                .bind(source)
                .fetch_one(&self.pool)
                .await?;
        let n: i64 = row.try_get("n")?;
        Ok(n as u32)
    }

    /// Effacer l'index d'un projet, ou d'un seul document.
    pub async fn clear(&self, project_id: Uuid, source: Option<&str>) -> Result<(), StorageError> {
        match source {
            Some(s) => {
                sqlx::query("DELETE FROM rag_chunks WHERE project_id = ? AND source = ?")
                    .bind(project_id.to_string())
                    .bind(s)
                    .execute(&self.pool)
                    .await?;
            }
            None => {
                sqlx::query("DELETE FROM rag_chunks WHERE project_id = ?")
                    .bind(project_id.to_string())
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    /// Les `k` morceaux les plus proches de la question.
    ///
    /// La comparaison se fait ici, en mémoire, sur les morceaux du seul projet
    /// concerné. C'est linéaire, et c'est très bien à cette échelle : quelques
    /// milliers de morceaux se parcourent en quelques millisecondes, et un
    /// index vectoriel dédié serait une dépendance de plus pour un gain que
    /// personne ne remarquerait.
    pub async fn search(
        &self,
        project_id: Uuid,
        question: &[f32],
        k: usize,
    ) -> Result<Vec<RagHit>, StorageError> {
        if question.is_empty() {
            return Ok(Vec::new());
        }
        let rows =
            sqlx::query("SELECT source, text, embedding, dim FROM rag_chunks WHERE project_id = ?")
                .bind(project_id.to_string())
                .fetch_all(&self.pool)
                .await?;

        let mut hits: Vec<RagHit> = Vec::new();
        for r in rows {
            let dim: i64 = r.try_get("dim")?;
            // Un morceau vectorisé par un autre modèle ne se compare pas à la
            // question : on l'ignore plutôt que de rendre un score qui n'a
            // aucun sens.
            if dim as usize != question.len() {
                continue;
            }
            let brut: Vec<u8> = r.try_get("embedding")?;
            let v = depuis_octets(&brut);
            hits.push(RagHit {
                source: r.try_get("source")?,
                text: r.try_get("text")?,
                score: cosinus(question, &v),
            });
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(k.max(1));
        Ok(hits)
    }
}

/// Le vecteur en octets : des `f32` en petit-boutien, bout à bout.
fn en_octets(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn depuis_octets(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Le cosinus de l'angle entre deux vecteurs.
///
/// Pas la distance euclidienne : deux textes qui disent la même chose avec plus
/// ou moins de mots donnent des vecteurs de longueurs différentes mais de même
/// direction, et c'est la direction qui porte le sens.
fn cosinus(a: &[f32], b: &[f32]) -> f32 {
    let mut produit = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len().min(b.len()) {
        produit += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    produit / (na.sqrt() * nb.sqrt())
}

/// Lesquels de ces morceaux valent d'être donnés au modèle.
///
/// Le score absolu ne dit rien : son échelle dépend entièrement du modèle de
/// plongement, et un seuil écrit en dur conviendrait à l'un en trahissant
/// l'autre. Ce qui parle, c'est le **détachement** du premier par rapport aux
/// autres — mesuré ici avec nomic-embed-text sur trois documents sans rapport :
///
/// | question | 1er | moyenne des autres | détachement |
/// | --- | --- | --- | --- |
/// | « détartrer la cafetière » | 0,667 | 0,577 | 0,135 |
/// | « combien de temps le pain pousse » | 0,662 | 0,577 | 0,128 |
/// | « prix du bitcoin » | 0,636 | 0,615 | **0,033** |
///
/// Quand un document répond vraiment, il se détache. Quand aucun ne répond,
/// tous les scores se tassent — c'est précisément à quoi ressemble « rien ici
/// ne parle de ça », et c'est ce qu'on refuse d'envoyer au modèle.
///
/// Rend les indices à garder, dans l'ordre reçu.
pub fn retenir(scores: &[f32], maximum: usize) -> Vec<usize> {
    if scores.is_empty() {
        return Vec::new();
    }
    let meilleur = scores[0];
    // Un score nul ou négatif ne ressemble à rien, quelle que soit l'échelle.
    if meilleur <= 0.0 {
        return Vec::new();
    }
    // Un seul morceau : aucune comparaison possible, donc aucune raison de le
    // refuser. Le modèle dira lui-même s'il ne répond pas.
    if scores.len() == 1 {
        return vec![0];
    }

    let moyenne_des_autres: f32 = scores[1..].iter().sum::<f32>() / (scores.len() - 1) as f32;
    let detachement = (meilleur - moyenne_des_autres) / meilleur;
    if detachement < 0.05 {
        return Vec::new();
    }

    // On garde le premier et ceux qui se détachent avec lui : au-dessus de la
    // moitié du chemin entre la moyenne et le meilleur.
    let seuil = moyenne_des_autres + (meilleur - moyenne_des_autres) * 0.5;
    scores
        .iter()
        .enumerate()
        .filter(|(_, s)| **s >= seuil)
        .map(|(i, _)| i)
        .take(maximum.max(1))
        .collect()
}

/// Découper un texte en morceaux qui tiennent dans une fenêtre de contexte.
///
/// La coupe suit les paragraphes tant qu'elle peut : couper au milieu d'une
/// phrase produit des morceaux dont ni l'un ni l'autre ne veut dire grand-chose.
/// Les morceaux se chevauchent un peu, parce qu'une réponse tombe souvent à
/// cheval sur une coupure, et qu'un chevauchement coûte moins cher qu'une
/// réponse manquée.
pub fn decouper(texte: &str, taille: usize, chevauchement: usize) -> Vec<String> {
    let taille = taille.max(200);
    let chevauchement = chevauchement.min(taille / 2);
    let mut morceaux = Vec::new();
    let mut courant = String::new();

    for paragraphe in texte.split("\n\n") {
        let p = paragraphe.trim();
        if p.is_empty() {
            continue;
        }
        // Un paragraphe plus long que la fenêtre est coupé net : il n'y a pas
        // de meilleure frontière à trouver dedans.
        if p.chars().count() > taille {
            if !courant.trim().is_empty() {
                morceaux.push(courant.trim().to_string());
                courant.clear();
            }
            let lettres: Vec<char> = p.chars().collect();
            let mut i = 0;
            while i < lettres.len() {
                let fin = (i + taille).min(lettres.len());
                morceaux.push(
                    lettres[i..fin]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string(),
                );
                if fin == lettres.len() {
                    break;
                }
                i = fin.saturating_sub(chevauchement);
            }
            continue;
        }
        if courant.chars().count() + p.chars().count() > taille && !courant.trim().is_empty() {
            morceaux.push(courant.trim().to_string());
            // Le chevauchement reprend la fin du morceau précédent.
            let queue: String = courant
                .chars()
                .rev()
                .take(chevauchement)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            courant = queue;
        }
        courant.push_str(p);
        courant.push_str("\n\n");
    }
    if !courant.trim().is_empty() {
        morceaux.push(courant.trim().to_string());
    }
    morceaux.retain(|m| !m.is_empty());
    morceaux
}

#[cfg(test)]
mod tests {
    use super::{cosinus, decouper, depuis_octets, en_octets};

    #[test]
    fn un_vecteur_survit_a_l_aller_retour() {
        let v = vec![0.5f32, -1.25, 3.0, 0.0];
        assert_eq!(depuis_octets(&en_octets(&v)), v);
    }

    #[test]
    fn le_cosinus_mesure_la_direction() {
        let a = [1.0f32, 0.0];
        assert!(
            (cosinus(&a, &[2.0, 0.0]) - 1.0).abs() < 1e-6,
            "même direction"
        );
        assert!(cosinus(&a, &[0.0, 1.0]).abs() < 1e-6, "perpendiculaires");
        assert!((cosinus(&a, &[-1.0, 0.0]) + 1.0).abs() < 1e-6, "opposés");
    }

    #[test]
    fn un_vecteur_nul_ne_ressemble_a_rien() {
        assert_eq!(cosinus(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn un_document_qui_repond_se_detache() {
        // Mesures réelles, nomic-embed-text, « détartrer la cafetière ».
        let gardes = super::retenir(&[0.667, 0.587, 0.567], 4);
        assert_eq!(gardes, vec![0], "seul celui qui se détache");
    }

    #[test]
    fn quand_rien_ne_repond_on_n_envoie_rien() {
        // « quel est le prix du bitcoin ? » sur un corpus qui n'en parle pas :
        // les scores se tassent, et c'est à ça qu'on le reconnaît.
        assert!(
            super::retenir(&[0.636, 0.622, 0.608], 4).is_empty(),
            "des scores tassés ne désignent rien"
        );
    }

    #[test]
    fn deux_documents_proches_passent_ensemble() {
        let gardes = super::retenir(&[0.90, 0.88, 0.40, 0.38], 4);
        assert_eq!(gardes, vec![0, 1], "les deux se détachent du reste");
    }

    #[test]
    fn un_seul_morceau_ne_se_compare_a_rien() {
        assert_eq!(super::retenir(&[0.5], 4), vec![0]);
        assert!(super::retenir(&[], 4).is_empty());
        assert!(super::retenir(&[0.0, 0.0], 4).is_empty());
    }

    #[test]
    fn le_maximum_est_respecte() {
        assert_eq!(super::retenir(&[0.9, 0.89, 0.88, 0.1], 2).len(), 2);
    }

    #[test]
    fn la_coupe_suit_les_paragraphes() {
        let texte = "Premier paragraphe.\n\nDeuxième paragraphe.\n\nTroisième.";
        let m = decouper(texte, 200, 20);
        assert_eq!(m.len(), 1, "court : un seul morceau");
        assert!(m[0].contains("Premier") && m[0].contains("Troisième"));
    }

    #[test]
    fn un_texte_long_est_coupe() {
        let paragraphe = "phrase. ".repeat(60); // ~480 caractères
        let texte = format!("{paragraphe}\n\n{paragraphe}\n\n{paragraphe}");
        let m = decouper(&texte, 500, 50);
        assert!(
            m.len() >= 3,
            "trois paragraphes de 480 dans des fenêtres de 500"
        );
        assert!(m.iter().all(|x| !x.is_empty()));
    }

    #[test]
    fn un_paragraphe_plus_long_que_la_fenetre_est_coupe_net() {
        let texte = "a".repeat(1200);
        let m = decouper(&texte, 400, 40);
        assert!(m.len() >= 3);
        assert!(m.iter().all(|x| x.chars().count() <= 400));
    }

    #[test]
    fn le_vide_ne_produit_rien() {
        assert!(decouper("", 500, 50).is_empty());
        assert!(decouper("   \n\n  \n\n ", 500, 50).is_empty());
    }
}
