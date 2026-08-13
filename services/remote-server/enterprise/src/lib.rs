// Business Source License 1.1
// Change Date: 2030-07-18 (4 years after the 0.1.0 release)
// For commercial use terms see LICENSES.md.

//! Locaryn enterprise module — the commercial layer of the remote server.
//!
//! Covers:
//! - **Team context sharing**: pre-indexed project context shared across
//!   collaborators, optimized for large codebases.
//! - **DGX Spark orchestration**: schedule inference jobs on an NVIDIA DGX
//!   Spark cluster.
//! - **Concurrent-client gate**: the free remote-server build caps
//!   concurrent authenticated sessions; this module enforces the limit.
//!
//! This module is compiled in only when the `enterprise` feature of
//! `locaryn-remote-server` is enabled. Without it, the remote-server runs
//! fully under Apache-2.0 as the free tier.

pub mod licence;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub const VERSION: &str = "0.1.0";

pub fn version_string() -> String {
    format!("locaryn-enterprise {VERSION} (BSL 1.1, change date 2030-07-18)")
}

/// Free-tier concurrent-client cap. Enterprise builds lift this via config.
pub const FREE_TIER_MAX_CONCURRENT_CLIENTS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    pub max_concurrent_clients: usize,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            max_concurrent_clients: FREE_TIER_MAX_CONCURRENT_CLIENTS,
        }
    }
}

/// Enforces the concurrent-client gate.
pub struct ClientGate {
    config: GateConfig,
    current: AtomicUsize,
}

impl ClientGate {
    pub fn new(config: GateConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            current: AtomicUsize::new(0),
        })
    }

    /// Try to admit a new client. Returns Err with a 429-style message if
    /// the cap is reached.
    pub fn admit(&self) -> Result<ClientSlot, GateError> {
        let prev = self.current.fetch_add(1, Ordering::SeqCst);
        if prev >= self.config.max_concurrent_clients {
            self.current.fetch_sub(1, Ordering::SeqCst);
            return Err(GateError::CapacityExceeded {
                max: self.config.max_concurrent_clients,
            });
        }
        Ok(ClientSlot {
            current: &self.current as *const _ as usize, // sentinel; real impl holds an Arc
        })
    }

    pub fn current(&self) -> usize {
        self.current.load(Ordering::SeqCst)
    }
}

/// RAII handle that decrements the gate on drop. (Skeleton: the real impl
/// holds an `Arc<ClientGate>`; this stub uses a sentinel to avoid lifetime
/// complexity in the bootstrap.)
pub struct ClientSlot {
    current: usize,
}

impl Drop for ClientSlot {
    fn drop(&mut self) {
        // Real impl: self.current.fetch_sub(1, SeqCst)
        let _ = self.current;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GateError {
    #[error("concurrent client cap reached ({max}); upgrade to enterprise")]
    CapacityExceeded { max: usize },
}

// ============================================================================
// Team context sharing (skeleton)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedContext {
    pub project_id: String,
    pub index_version: u64,
    pub chunks: usize,
    pub updated_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("not indexed: {0}")]
    NotIndexed(String),
    #[error("index failed: {0}")]
    IndexFailed(String),
}

/// Trigger a re-index of a project for cross-team context sharing.
/// V1 wires a vector index + chunk store.
pub async fn reindex(_project_id: &str) -> Result<SharedContext, ContextError> {
    Err(ContextError::NotIndexed("reindex skeleton".into()))
}

// ============================================================================
// DGX Spark orchestration (skeleton)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DgxSparkStatus {
    pub nodes: usize,
    pub gpus_available: usize,
    pub queue_depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DgxJob {
    pub id: String,
    pub model: String,
    pub status: String,
}

pub async fn dgx_status() -> DgxSparkStatus {
    DgxSparkStatus {
        nodes: 0,
        gpus_available: 0,
        queue_depth: 0,
    }
}

pub async fn schedule_dgx_job(_model: &str) -> Result<DgxJob, GateError> {
    Ok(DgxJob {
        id: uuid::Uuid::new_v4().to_string(),
        model: _model.into(),
        status: "queued".into(),
    })
}
