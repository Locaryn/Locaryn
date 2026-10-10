//! Ce que le bureau a reçu pour ses conversations, servi aussi par le démon :
//! « Envoyer maintenant » (un message remis au modèle en pleine tâche) et
//! l'arbitre de la carte graphique pour les morphs à modèle propre.
//!
//! Sans eux, le téléphone et le web n'avaient ni l'un ni l'autre : une image
//! demandée depuis le mobile se battait pour la carte avec le modèle de
//! conversation, et une correction attendait la fin de la réponse.

use crate::DaemonState;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use axum::Json;
use locaryn_agent_runtime::gpu::GpuArbiterHandle;
use locaryn_agent_runtime::gpu_standard::{
    ConversationEngine, GpuToolMap, GpuToolSource, StandardArbiter,
};
use locaryn_agent_runtime::mailbox::{Courrier, Mailbox, MailboxHandle};
use locaryn_shared_types::{MessageRole, ProviderEngine};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use uuid::Uuid;

// ─── Boîte aux lettres ───────────────────────────────────────────────────────

/// Les messages remis et pas encore lus, par conversation.
static BOITES: LazyLock<Mutex<HashMap<Uuid, Vec<Courrier>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn verrou() -> std::sync::MutexGuard<'static, HashMap<Uuid, Vec<Courrier>>> {
    // Un verrou empoisonné ne protège qu'une liste de messages : on reprend
    // son contenu plutôt que de perdre ce que la personne a écrit.
    BOITES.lock().unwrap_or_else(|e| e.into_inner())
}

struct SessionMailbox {
    state: Arc<DaemonState>,
    session: Uuid,
}

#[async_trait::async_trait]
impl Mailbox for SessionMailbox {
    async fn relever(&self) -> Vec<Courrier> {
        let courrier = verrou().remove(&self.session).unwrap_or_default();
        // Lu : il entre dans l'historique, comme un message ordinaire. La
        // lecture est annoncée aux clients dans le flux (`mail_read`).
        for c in &courrier {
            if let Err(e) = self
                .state
                .storage
                .messages
                .append(self.session, MessageRole::User, &c.text)
                .await
            {
                tracing::warn!(error = %e, "message remis en cours de tâche non enregistré");
            }
        }
        courrier
    }
}

pub fn mailbox(state: Arc<DaemonState>, session: Uuid) -> MailboxHandle {
    MailboxHandle::new(SessionMailbox { state, session })
}

#[derive(serde::Deserialize)]
pub struct DepotBody {
    id: String,
    text: String,
}

fn conversation_inconnue() -> Response {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "conversation inconnue" })),
    )
        .into_response()
}

/// POST /v1/sessions/:id/mailbox — remettre un message au modèle qui travaille.
pub async fn deposer(Path(id): Path<String>, Json(body): Json<DepotBody>) -> Response {
    let Ok(sid) = Uuid::parse_str(&id) else {
        return conversation_inconnue();
    };
    if body.text.trim().is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "message vide" })),
        )
            .into_response();
    }
    verrou().entry(sid).or_default().push(Courrier {
        id: body.id,
        text: body.text,
    });
    Json(serde_json::json!({ "deposited": true })).into_response()
}

/// DELETE /v1/sessions/:id/mailbox/:mid — reprendre un message pas encore lu.
pub async fn reprendre(Path((id, mid)): Path<(String, String)>) -> Response {
    let Ok(sid) = Uuid::parse_str(&id) else {
        return conversation_inconnue();
    };
    let mut boites = verrou();
    let repris = match boites.get_mut(&sid) {
        Some(liste) => {
            let avant = liste.len();
            liste.retain(|c| c.id != mid);
            let repris = liste.len() < avant;
            if liste.is_empty() {
                boites.remove(&sid);
            }
            repris
        }
        None => false,
    };
    Json(serde_json::json!({ "withdrawn": repris })).into_response()
}

// ─── Arbitre de la carte ─────────────────────────────────────────────────────

struct Moteur {
    state: Arc<DaemonState>,
}

#[async_trait::async_trait]
impl ConversationEngine for Moteur {
    async fn local_running(&self) -> Option<String> {
        let actif = self.state.storage.providers.active().await.ok().flatten()?;
        let local = actif.endpoint.contains("127.0.0.1") || actif.endpoint.contains("localhost");
        (local && self.state.supervisor.is_healthy(&actif.engine).await)
            .then(|| actif.engine.as_token())
    }

    async fn stop(&self, moteur: &str) {
        if let Some(engine) = ProviderEngine::from_token(moteur) {
            if let Err(e) = self.state.supervisor.shutdown(&engine).await {
                tracing::warn!(error = %e, "arrêt du moteur de conversation refusé");
            }
        }
    }

    async fn start(&self, moteur: &str) -> Result<(), String> {
        let engine = ProviderEngine::from_token(moteur)
            .ok_or_else(|| format!("moteur inconnu : {moteur}"))?;
        self.state
            .supervisor
            .ensure_running(&engine)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

struct Outils {
    state: Arc<DaemonState>,
}

#[async_trait::async_trait]
impl GpuToolSource for Outils {
    async fn gpu_tools(&self) -> GpuToolMap {
        self.state
            .gpu_tools
            .read()
            .map(|carte| carte.clone())
            .unwrap_or_default()
    }
}

pub fn arbitre(state: Arc<DaemonState>) -> GpuArbiterHandle {
    GpuArbiterHandle::new(StandardArbiter {
        mcp: state.mcp_state.clone(),
        tools: Arc::new(Outils {
            state: state.clone(),
        }),
        engine: Arc::new(Moteur { state }),
        free_vram_gb: locaryn_llmfit::hardware::free_vram_gb,
    })
}

// ─── Fenêtre de contexte ─────────────────────────────────────────────────────

/// GET /v1/sessions/:id/context — ce que la conversation occupe de la fenêtre
/// du modèle, pour la jauge du téléphone.
pub async fn contexte(
    axum::extract::State(s): axum::extract::State<Arc<DaemonState>>,
    Path(id): Path<String>,
) -> Response {
    let Ok(sid) = Uuid::parse_str(&id) else {
        return conversation_inconnue();
    };
    let messages = s
        .storage
        .messages
        .list_for_session(sid)
        .await
        .unwrap_or_default();
    let fenetre = match s.storage.providers.active().await.ok().flatten() {
        Some(p) => locaryn_agent_runtime::tool_budget::server_context(&s.http, &p.endpoint).await,
        None => None,
    };
    Json(serde_json::json!({
        "used": locaryn_agent_runtime::compaction::jetons_estimes(&messages),
        "window": fenetre,
        "messages": messages.len(),
        "compressible": messages.len() >= locaryn_agent_runtime::compaction::MINIMUM,
    }))
    .into_response()
}

/// POST /v1/sessions/:id/compress — résumer les vieux échanges, comme l'appui
/// long sur la jauge du bureau.
pub async fn compresser(
    axum::extract::State(s): axum::extract::State<Arc<DaemonState>>,
    Path(id): Path<String>,
) -> Response {
    let Ok(sid) = Uuid::parse_str(&id) else {
        return conversation_inconnue();
    };
    match compresser_session(&s, sid).await {
        Ok(retires) => Json(serde_json::json!({ "removed": retires })).into_response(),
        Err(e) => (
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn compresser_session(s: &DaemonState, sid: Uuid) -> Result<u64, String> {
    use locaryn_agent_runtime::compaction;
    let messages = s
        .storage
        .messages
        .list_for_session(sid)
        .await
        .map_err(|e| e.to_string())?;
    let plan = compaction::planifier(&messages)?;
    let actif = s
        .storage
        .providers
        .active()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("aucun fournisseur actif")?;
    let modele = actif.model.clone().unwrap_or_else(|| "default".into());
    let resume = compaction::resumer(&s.http, &actif.endpoint, &modele, &plan.transcript).await?;
    let retires = s
        .storage
        .messages
        .delete_before(sid, plan.cutoff)
        .await
        .map_err(|e| e.to_string())?;
    s.storage
        .messages
        .append_full(
            sid,
            MessageRole::Assistant,
            &format!("{}{resume}", compaction::PREFIXE_RESUME),
            None,
            None,
            0,
            0,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(session = %sid, retires, "conversation compressée depuis un client");
    Ok(retires)
}
