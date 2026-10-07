//! La permission d'une conversation, telle qu'elle est maintenant.
//!
//! Lue par la boucle d'outils avant chaque appel (voir
//! `locaryn_agent_runtime::trust_source`) : passer de « Prudent » à
//! « Autonome » — ou l'inverse — pendant que le modèle travaille s'applique au
//! prochain outil, sans interrompre la tâche.

use crate::Core;
use locaryn_agent_runtime::trust_source::TrustSource;
use locaryn_shared_types::TrustLevel;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

pub struct SessionTrustSource {
    app: AppHandle,
    session_id: Uuid,
}

impl SessionTrustSource {
    pub fn new(app: AppHandle, session_id: Uuid) -> Self {
        Self { app, session_id }
    }
}

#[async_trait::async_trait]
impl TrustSource for SessionTrustSource {
    async fn current(&self) -> Option<TrustLevel> {
        let core = self.app.state::<Core>();
        let session = core.storage.sessions.get(self.session_id).await.ok()?;
        // Même règle qu'à l'envoi : l'exception de la conversation d'abord,
        // sinon celle du projet qui la porte.
        match session.trust_override {
            Some(t) => Some(t),
            None => core
                .storage
                .projects
                .get(session.project_id)
                .await
                .ok()
                .map(|p| p.trust_level),
        }
    }
}
