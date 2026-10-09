//! « Envoyer maintenant » : un message de la file remis au modèle en pleine
//! tâche.
//!
//! L'interface dépose le message ici ; la boucle d'outils le relève entre deux
//! étapes (voir `locaryn_agent_runtime::mailbox`). À ce moment il est
//! enregistré dans la conversation et l'interface est prévenue qu'il a été lu.
//! Un message encore dans la boîte peut être repris : il retourne alors dans la
//! file et partira à la fin de la réponse.

use crate::Core;
use locaryn_agent_runtime::mailbox::{Courrier, Mailbox};
use locaryn_shared_types::MessageRole;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

/// Les messages déposés et pas encore lus, par conversation.
static BOITES: LazyLock<Mutex<HashMap<Uuid, Vec<Courrier>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Ce que l'interface reçoit quand le modèle a relevé la boîte.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Lus {
    session_id: String,
    ids: Vec<String>,
}

pub struct SessionMailbox {
    app: AppHandle,
    session_id: Uuid,
}

impl SessionMailbox {
    pub fn new(app: AppHandle, session_id: Uuid) -> Self {
        Self { app, session_id }
    }
}

fn verrou() -> std::sync::MutexGuard<'static, HashMap<Uuid, Vec<Courrier>>> {
    // Un verrou empoisonné ne protège qu'une liste de messages : on reprend
    // son contenu plutôt que de perdre ce que la personne a écrit.
    BOITES.lock().unwrap_or_else(|e| e.into_inner())
}

#[async_trait::async_trait]
impl Mailbox for SessionMailbox {
    async fn relever(&self) -> Vec<Courrier> {
        let courrier = verrou().remove(&self.session_id).unwrap_or_default();
        if courrier.is_empty() {
            return courrier;
        }
        let core = self.app.state::<Core>();
        for c in &courrier {
            if let Err(e) = core
                .storage
                .messages
                .append(self.session_id, MessageRole::User, &c.text)
                .await
            {
                tracing::warn!(error = %e, "message remis en cours de tâche non enregistré");
            }
        }
        let lus = Lus {
            session_id: self.session_id.to_string(),
            ids: courrier.iter().map(|c| c.id.clone()).collect(),
        };
        if let Err(e) = self.app.emit("chat-mail-read", lus) {
            tracing::warn!(error = %e, "interface non prévenue de la lecture");
        }
        courrier
    }
}

fn session(id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(id).map_err(|e| format!("conversation inconnue : {e}"))
}

/// Remet un message au modèle qui travaille dans cette conversation.
#[tauri::command]
pub fn chat_mail_deposit(session_id: String, id: String, text: String) -> Result<(), String> {
    let sid = session(&session_id)?;
    if text.trim().is_empty() {
        return Err("message vide".into());
    }
    verrou().entry(sid).or_default().push(Courrier { id, text });
    Ok(())
}

/// Reprend un message pas encore lu. Rend `false` s'il l'a déjà été.
#[tauri::command]
pub fn chat_mail_withdraw(session_id: String, id: String) -> Result<bool, String> {
    let sid = session(&session_id)?;
    let mut boites = verrou();
    let Some(liste) = boites.get_mut(&sid) else {
        return Ok(false);
    };
    let avant = liste.len();
    liste.retain(|c| c.id != id);
    let repris = liste.len() < avant;
    if liste.is_empty() {
        boites.remove(&sid);
    }
    Ok(repris)
}
