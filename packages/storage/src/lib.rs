//! Locaryn persistence layer (SQLite via sqlx).
//!
//! Schema lives in `migrations/*.sql` at the repo root and is applied at
//! startup via `sqlx::migrate!`. This crate exposes typed repositories so
//! callers don't write raw SQL.
#![allow(dead_code)]

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub mod error;
pub mod figures;
pub mod figures_import;
pub mod memory;
pub mod metrics;
pub mod rag;
pub mod repos;
pub mod users;

pub use error::StorageError;

/// Open (and migrate) the SQLite database at `path`. Creates the file and
/// parent directory if missing. Enables WAL for concurrency.
pub async fn open(path: &Path) -> Result<SqlitePool, StorageError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
    }
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}?mode=rwc", path.display()))?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    migrer(&pool).await?;
    Ok(pool)
}

/// Applique les migrations, en tolérant une base écrite par une version plus
/// récente.
///
/// Par défaut, sqlx refuse de démarrer dès qu'une migration enregistrée dans la
/// base est absente du binaire, et ce refus a brisé l'application deux fois :
/// un build de développement applique la migration suivante à la base
/// partagée, puis le paquet installé — plus ancien — ne reconnaît plus son
/// historique et meurt au lancement. Vu de l'utilisateur, l'application
/// s'ouvre et se referme sans un mot.
///
/// La distinction que sqlx ne fait pas, on la fait ici :
///
/// * des migrations **plus récentes** que tout ce que ce binaire connaît, c'est
///   une base venue d'une version postérieure. Le cas d'un retour en arrière,
///   normal et réversible : on continue, en le disant dans le journal. Les
///   colonnes ajoutées depuis restent simplement inutilisées.
/// * un trou **au milieu** de l'historique, c'est autre chose : une migration
///   a été renumérotée ou supprimée, et passer outre masquerait un vrai
///   désordre. On échoue, comme avant.
async fn migrer(pool: &SqlitePool) -> Result<(), StorageError> {
    // Le chemin est relatif à CARGO_MANIFEST_DIR (packages/storage) ; le
    // dossier de migrations est à la racine du dépôt.
    let mut migrator = sqlx::migrate!("../../migrations");
    let connues: Vec<i64> = migrator.iter().map(|m| m.version).collect();

    // Sur une base neuve la table n'existe pas encore : pas d'historique, donc
    // rien à tolérer. L'échec de la requête est ici une réponse, pas une
    // erreur.
    let appliquees: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    let (venues_du_futur, trous) = migrations_absentes(&appliquees, &connues);

    if !trous.is_empty() {
        // On laisse sqlx échouer avec son message, qui nomme la migration.
        tracing::error!(
            migrations = ?trous,
            "des migrations enregistrées manquent au milieu de l'historique"
        );
    } else if !venues_du_futur.is_empty() {
        tracing::warn!(
            migrations = ?venues_du_futur,
            "la base a été écrite par une version plus récente de Locaryn — \
             ces migrations sont ignorées, leurs colonnes resteront inutilisées"
        );
        migrator.set_ignore_missing(true);
    }

    migrator.run(pool).await?;
    Ok(())
}

/// Ce que la base a enregistré et que ce binaire ne connaît pas, rangé en deux
/// tas : ce qui vient d'après lui, et ce qui manque au milieu.
///
/// Séparé du reste parce que c'est là qu'est le jugement — et qu'un jugement
/// se vérifie.
fn migrations_absentes(appliquees: &[i64], connues: &[i64]) -> (Vec<i64>, Vec<i64>) {
    let connue_max = connues.iter().copied().max().unwrap_or(0);
    appliquees
        .iter()
        .copied()
        .filter(|v| !connues.contains(v))
        .partition(|v| *v > connue_max)
}

/// In-memory pool for tests.
pub async fn open_in_memory() -> Result<SqlitePool, StorageError> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    migrer(&pool).await?;
    Ok(pool)
}

// Re-export the repos module's public types for convenience.
pub use repos::{
    ArtifactRepo, ExtensionRecord, ExtensionRepo, MessageRepo, NewExtension, ProjectRepo,
    ProviderRepo, SessionRepo, Storage, TaskRepo,
};
#[cfg(feature = "ssh-connector")]
pub use repos::{NewSshServer, SshServerPatch, SshServerRepo};

#[cfg(test)]
mod tests {
    use super::*;

    /// Le cas qui a tué l'application deux fois : un build de développement
    /// applique la migration suivante à la base partagée, puis le paquet
    /// installé — plus ancien — la retrouve et refuse de démarrer.
    #[test]
    fn une_base_venue_du_futur_est_toleree() {
        let (futur, trous) = migrations_absentes(&[1, 2, 3, 12, 13], &[1, 2, 3]);
        assert_eq!(futur, vec![12, 13], "12 et 13 viennent d'après ce binaire");
        assert!(trous.is_empty(), "rien ne manque au milieu");
    }

    /// Un trou au milieu n'est pas un retour en arrière : une migration a été
    /// renumérotée ou supprimée, et passer outre masquerait le désordre.
    #[test]
    fn un_trou_au_milieu_reste_une_faute() {
        let (futur, trous) = migrations_absentes(&[1, 2, 7, 9], &[1, 2, 9]);
        assert!(futur.is_empty(), "7 ne vient pas d'après : 9 est connue");
        assert_eq!(trous, vec![7]);
    }

    /// Les deux à la fois : le trou l'emporte, puisqu'il faut échouer.
    #[test]
    fn un_trou_et_du_futur_se_distinguent() {
        let (futur, trous) = migrations_absentes(&[1, 5, 9, 42], &[1, 9]);
        assert_eq!(futur, vec![42]);
        assert_eq!(trous, vec![5]);
    }

    /// Le cas courant — rien à signaler — ne doit rien déclencher.
    #[test]
    fn une_base_a_jour_ne_declenche_rien() {
        let (futur, trous) = migrations_absentes(&[1, 2, 3], &[1, 2, 3]);
        assert!(futur.is_empty() && trous.is_empty());
    }

    /// Une base neuve n'a aucun historique : surtout ne rien tolérer par
    /// accident, sinon la première migration passerait pour un trou.
    #[test]
    fn une_base_neuve_na_rien_a_tolerer() {
        let (futur, trous) = migrations_absentes(&[], &[1, 2, 3]);
        assert!(futur.is_empty() && trous.is_empty());
    }

    /// Bout en bout : une base réelle portant une migration inconnue s'ouvre.
    /// Sans ce test, la fonction pure pourrait être juste et le branchement
    /// faux — c'est exactement ce qui s'est produit en production.
    #[tokio::test]
    async fn ouvrir_une_base_marquee_du_futur_fonctionne() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_migr_{}_{}",
            std::process::id(),
            {
                use std::sync::atomic::{AtomicU64, Ordering};
                static N: AtomicU64 = AtomicU64::new(0);
                N.fetch_add(1, Ordering::Relaxed)
            }
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("locaryn.db");

        // Une base normale, migrée jusqu'à ce que ce binaire connaît.
        let pool = open(&base).await.expect("première ouverture");

        // On y inscrit une migration qu'aucun binaire actuel ne porte : c'est
        // ce que ferait une version postérieure.
        sqlx::query(
            "INSERT INTO _sqlx_migrations
             (version, description, installed_on, success, checksum, execution_time)
             VALUES (9999, 'venue du futur', CURRENT_TIMESTAMP, 1, X'00', 0)",
        )
        .execute(&pool)
        .await
        .expect("inscription de la migration future");
        pool.close().await;

        // La rouvrir doit fonctionner — c'est tout l'objet du correctif.
        let pool = open(&base)
            .await
            .expect("une base écrite par une version plus récente doit s'ouvrir");
        pool.close().await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
