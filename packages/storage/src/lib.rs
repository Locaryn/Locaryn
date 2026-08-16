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
pub mod memory;
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

    // Apply bundled migrations. The macro embeds them at compile time.
    // Path is relative to CARGO_MANIFEST_DIR (packages/storage), so the
    // repo-root migrations dir is ../../migrations.
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
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
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}

// Re-export the repos module's public types for convenience.
pub use repos::{
    ArtifactRepo, ExtensionRecord, ExtensionRepo, MessageRepo, NewExtension, ProjectRepo,
    ProviderRepo, SessionRepo, Storage, TaskRepo,
};
#[cfg(feature = "ssh-connector")]
pub use repos::{NewSshServer, SshServerPatch, SshServerRepo};
