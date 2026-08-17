//! Sérialisation par session et mappage session Locaryn → session noyau.
//!
//! Décisions D3 et D8 (doc 14 §9) :
//!
//! - **D3** : deux messages envoyés d'affilée sur la même session ne doivent
//!   pas produire deux runs concurrents sur la même session noyau (désordre,
//!   mémoire croisée). Chaque session Locaryn possède un verrou ; un run le
//!   tient pendant toute sa durée.
//! - **D8** : on retient le dernier message Locaryn envoyé au noyau
//!   (`last_sent_message_id`) pour la ré-hydratation des messages non
//!   accusés si la session noyau est perdue.
//!
//! La clé noyau est dérivée déterministiquement : `locaryn-{session_uuid}`.
//! Elle porte le routage (`user`, `conversation`) ou alimente le chaînage
//! (`previous_response_id`).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// État d'une session confiée à un noyau. Clone : les mutations passent par
/// les `Arc<Mutex<_>>` internes, visibles de tous les clones.
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    /// Verrou de sérialisation : un seul run à la fois sur cette session.
    pub gate: Arc<Mutex<()>>,
    /// Clé stable côté noyau (`locaryn-{uuid}`).
    pub key: String,
    /// Dernier `response_id` reçu (dialecte `responses`) — continuité de
    /// session quand le routage est `response`.
    pub last_response_id: Arc<Mutex<Option<String>>>,
    /// Dernier message Locaryn envoyé au noyau — ré-hydratation (D8).
    pub last_sent_message_id: Arc<Mutex<Option<String>>>,
}

/// Table des états de session, indexée par session Locaryn.
#[derive(Debug, Default)]
pub struct SessionStore {
    inner: Mutex<HashMap<Uuid, SessionState>>,
}

impl SessionStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// État d'une session, créé au besoin avec une clé déterministe.
    pub async fn entry(&self, session_id: Uuid) -> SessionState {
        let mut map = self.inner.lock().await;
        map.entry(session_id)
            .or_insert_with(|| SessionState {
                gate: Arc::new(Mutex::new(())),
                key: format!("locaryn-{session_id}"),
                last_response_id: Arc::new(Mutex::new(None)),
                last_sent_message_id: Arc::new(Mutex::new(None)),
            })
            .clone()
    }
}
