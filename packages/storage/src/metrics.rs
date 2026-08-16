//! Vitesses mesurées des modèles, sur cette machine.
//!
//! Un catalogue annonce des chiffres obtenus ailleurs, sur un autre matériel.
//! Ce qui aide vraiment à choisir, c'est ce que le modèle donne ici : jetons
//! par seconde pour une conversation, secondes par image pour une génération.
//! Ces mesures viennent des exécutions réelles.
//!
//! On conserve une moyenne courante et le nombre de mesures qui la composent.
//! Une moyenne sur trente générations et un relevé unique ne se lisent pas de
//! la même façon, et l'interface doit pouvoir le dire.

use crate::StorageError;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelMetric {
    pub model: String,
    /// `chat`, `image` ou `audio`.
    pub kind: String,
    /// Nombre d'exécutions derrière la moyenne.
    pub samples: i64,
    /// Conversation : jetons produits par seconde.
    pub avg_tokens_per_second: Option<f64>,
    /// Image et audio : durée moyenne d'une génération, en millisecondes.
    pub avg_duration_ms: Option<f64>,
    pub last_measured_at: String,
}

#[derive(Clone)]
pub struct MetricsRepo {
    pool: SqlitePool,
}

impl MetricsRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<ModelMetric>, StorageError> {
        let rows = sqlx::query_as::<_, ModelMetric>(
            "SELECT model, kind, samples, avg_tokens_per_second, avg_duration_ms, \
                    last_measured_at FROM model_metrics ORDER BY model ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get(&self, model: &str, kind: &str) -> Result<Option<ModelMetric>, StorageError> {
        let row = sqlx::query_as::<_, ModelMetric>(
            "SELECT model, kind, samples, avg_tokens_per_second, avg_duration_ms, \
                    last_measured_at FROM model_metrics WHERE model = ? AND kind = ?",
        )
        .bind(model)
        .bind(kind)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Ajoute une mesure de conversation.
    ///
    /// Une génération d'un seul jeton, ou plus courte qu'une centaine de
    /// millisecondes, est ignorée : elle mesure le temps de démarrage du
    /// moteur, pas sa vitesse, et fausserait la moyenne vers le haut.
    pub async fn record_chat(
        &self,
        model: &str,
        tokens_out: u64,
        duration_ms: u64,
    ) -> Result<(), StorageError> {
        if tokens_out < 2 || duration_ms < 100 {
            return Ok(());
        }
        let tps = tokens_out as f64 / (duration_ms as f64 / 1000.0);
        self.merge(model, "chat", Some(tps), None).await
    }

    /// Ajoute une mesure de génération (image ou audio).
    pub async fn record_generation(
        &self,
        model: &str,
        kind: &str,
        duration_ms: u64,
    ) -> Result<(), StorageError> {
        if duration_ms == 0 {
            return Ok(());
        }
        self.merge(model, kind, None, Some(duration_ms as f64))
            .await
    }

    /// Fond une mesure dans la moyenne existante.
    async fn merge(
        &self,
        model: &str,
        kind: &str,
        tps: Option<f64>,
        duration_ms: Option<f64>,
    ) -> Result<(), StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let previous = self.get(model, kind).await?;
        let (samples, avg_tps, avg_ms) = match previous {
            Some(p) => {
                let n = p.samples.max(0) as f64;
                let blend = |old: Option<f64>, new: Option<f64>| match (old, new) {
                    (Some(o), Some(v)) => Some((o * n + v) / (n + 1.0)),
                    (None, Some(v)) => Some(v),
                    (o, None) => o,
                };
                (
                    p.samples + 1,
                    blend(p.avg_tokens_per_second, tps),
                    blend(p.avg_duration_ms, duration_ms),
                )
            }
            None => (1, tps, duration_ms),
        };

        sqlx::query(
            "INSERT INTO model_metrics \
                 (model, kind, samples, avg_tokens_per_second, avg_duration_ms, last_measured_at) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT (model, kind) DO UPDATE SET \
                 samples = excluded.samples, \
                 avg_tokens_per_second = excluded.avg_tokens_per_second, \
                 avg_duration_ms = excluded.avg_duration_ms, \
                 last_measured_at = excluded.last_measured_at",
        )
        .bind(model)
        .bind(kind)
        .bind(samples)
        .bind(avg_tps)
        .bind(avg_ms)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn repo() -> MetricsRepo {
        let pool = crate::open_in_memory().await.expect("base en mémoire");
        MetricsRepo::new(pool)
    }

    #[tokio::test]
    async fn une_conversation_donne_des_jetons_par_seconde() {
        let r = repo().await;
        // 100 jetons en 2 secondes = 50 jetons/s.
        r.record_chat("qwen.gguf", 100, 2000).await.unwrap();
        let m = r.get("qwen.gguf", "chat").await.unwrap().expect("mesure");
        assert_eq!(m.samples, 1);
        assert!((m.avg_tokens_per_second.unwrap() - 50.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn les_mesures_se_moyennent() {
        let r = repo().await;
        r.record_chat("qwen.gguf", 100, 2000).await.unwrap(); // 50/s
        r.record_chat("qwen.gguf", 100, 1000).await.unwrap(); // 100/s
        let m = r.get("qwen.gguf", "chat").await.unwrap().expect("mesure");
        assert_eq!(m.samples, 2);
        assert!(
            (m.avg_tokens_per_second.unwrap() - 75.0).abs() < 0.01,
            "moyenne des deux, pas la dernière"
        );
    }

    #[tokio::test]
    async fn une_generation_trop_courte_ne_compte_pas() {
        let r = repo().await;
        // Une réponse d'un seul jeton mesure le démarrage du moteur.
        r.record_chat("qwen.gguf", 1, 5000).await.unwrap();
        assert!(r.get("qwen.gguf", "chat").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn image_et_conversation_ne_se_melangent_pas() {
        let r = repo().await;
        r.record_chat("modele.gguf", 100, 1000).await.unwrap();
        r.record_generation("modele.gguf", "image", 60_000)
            .await
            .unwrap();
        let chat = r.get("modele.gguf", "chat").await.unwrap().unwrap();
        let image = r.get("modele.gguf", "image").await.unwrap().unwrap();
        assert!(chat.avg_duration_ms.is_none());
        assert_eq!(image.avg_duration_ms.unwrap() as u64, 60_000);
        assert_eq!(r.list().await.unwrap().len(), 2);
    }
}
