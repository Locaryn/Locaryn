//! L'arbitre de la carte graphique côté bureau.
//!
//! L'arbitrage vit dans `locaryn_agent_runtime::gpu_standard`, partagé avec
//! le démon. Le bureau fournit ce que lui seul sait : les outils déclarés par
//! les manifestes des morphs actifs, et un superviseur qui arrête puis relance
//! le moteur de conversation avec le même modèle.

use crate::Core;
use locaryn_agent_runtime::gpu::GpuArbiterHandle;
use locaryn_agent_runtime::gpu_standard::{
    ConversationEngine, GpuToolMap, GpuToolSource, StandardArbiter,
};
use locaryn_shared_types::ProviderEngine;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

struct Moteur {
    app: AppHandle,
}

#[async_trait::async_trait]
impl ConversationEngine for Moteur {
    async fn local_running(&self) -> Option<String> {
        let core = self.app.state::<Core>();
        let actif = core.storage.providers.active().await.ok().flatten()?;
        let local = actif.endpoint.contains("127.0.0.1") || actif.endpoint.contains("localhost");
        (local && core.supervisor.is_healthy(&actif.engine).await).then(|| actif.engine.as_token())
    }

    async fn stop(&self, moteur: &str) {
        let Some(engine) = ProviderEngine::from_token(moteur) else {
            return;
        };
        let core = self.app.state::<Core>();
        if let Err(e) = core.supervisor.shutdown(&engine).await {
            tracing::warn!(error = %e, "arrêt du moteur de conversation refusé");
        }
    }

    async fn start(&self, moteur: &str) -> Result<(), String> {
        let engine = ProviderEngine::from_token(moteur)
            .ok_or_else(|| format!("moteur inconnu : {moteur}"))?;
        let core = self.app.state::<Core>();
        core.supervisor
            .ensure_running(&engine)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

struct Outils {
    app: AppHandle,
}

#[async_trait::async_trait]
impl GpuToolSource for Outils {
    async fn gpu_tools(&self) -> GpuToolMap {
        let core = self.app.state::<Core>();
        let rt = core.extensions.read().await;
        rt.gpu_tools
            .iter()
            .map(|(serveur, outils)| {
                (
                    serveur.clone(),
                    outils
                        .iter()
                        .map(|(nom, spec)| (nom.clone(), spec.produces.clone()))
                        .collect(),
                )
            })
            .collect()
    }
}

pub fn arbitre(app: &AppHandle) -> StandardArbiter {
    let core = app.state::<Core>();
    StandardArbiter {
        mcp: core.mcp.clone(),
        tools: Arc::new(Outils { app: app.clone() }),
        engine: Arc::new(Moteur { app: app.clone() }),
        free_vram_gb: locaryn_llmfit::hardware::free_vram_gb,
    }
}

pub fn handle(app: &AppHandle) -> GpuArbiterHandle {
    GpuArbiterHandle::new(arbitre(app))
}

/// Un appel venu du Studio d'une extension : même arbitrage, rechargement
/// aussitôt après. `None` : outil ordinaire.
pub async fn invoquer_arbitre(
    app: &AppHandle,
    tool: &str,
    args: &Value,
    appel: impl std::future::Future<Output = Result<Value, String>>,
) -> Option<Result<Value, String>> {
    arbitre(app).around(tool, args, appel).await
}
