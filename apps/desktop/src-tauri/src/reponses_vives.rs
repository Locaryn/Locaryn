//! Les réponses en cours sur ce poste, que l'écran peut rejoindre.
//!
//! Changer de conversation pendant une réponse la détache de l'écran : elle
//! continue et s'enregistre, mais rien ne permettait de la retrouver en
//! revenant — le fil montrait la question seule, sans indicateur, comme une
//! réponse coupée. Chaque réponse publie ici ce qu'elle produit ; l'écran qui
//! revient reçoit d'abord ce qui a déjà été dit, puis la suite en direct.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use locaryn_events::StreamEvent;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Événements tenus pour un abonné lent avant qu'il ne décroche.
const CAPACITE: usize = 1024;

struct Vive {
    ticket: u64,
    historique: Vec<StreamEvent>,
    diffusion: broadcast::Sender<StreamEvent>,
}

/// Une par application : la réponse lancée ici est rejointe ici.
#[derive(Clone, Default)]
pub struct ReponsesVives {
    table: Arc<Mutex<HashMap<Uuid, Vive>>>,
    compteur: Arc<AtomicU64>,
}

impl ReponsesVives {
    fn table(&self) -> MutexGuard<'_, HashMap<Uuid, Vive>> {
        // Un verrou empoisonné garde une table cohérente : chaque opération
        // la laisse entière avant de rendre la main.
        self.table.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Une réponse part pour `session` ; le ticket la désigne jusqu'à sa fin.
    pub fn commencer(&self, session: Uuid) -> u64 {
        let ticket = self.compteur.fetch_add(1, Ordering::Relaxed) + 1;
        let (diffusion, _) = broadcast::channel(CAPACITE);
        self.table().insert(
            session,
            Vive {
                ticket,
                historique: Vec::new(),
                diffusion,
            },
        );
        ticket
    }

    pub fn publier(&self, session: Uuid, ticket: u64, ev: StreamEvent) {
        let mut table = self.table();
        let Some(vive) = table.get_mut(&session).filter(|v| v.ticket == ticket) else {
            return;
        };
        ajouter_a_l_historique(&mut vive.historique, &ev);
        // Personne n'écoute : ce n'est pas une erreur, l'historique suffit.
        let _ = vive.diffusion.send(ev);
    }

    /// Fin de la réponse : ses abonnés voient le flux se fermer.
    pub fn terminer(&self, session: Uuid, ticket: u64) {
        let mut table = self.table();
        if table.get(&session).is_some_and(|v| v.ticket == ticket) {
            table.remove(&session);
        }
    }

    pub fn en_cours(&self) -> Vec<Uuid> {
        self.table().keys().copied().collect()
    }

    /// Ce qui a déjà été dit, puis la suite. Pris sous le même verrou : aucun
    /// événement ne tombe entre les deux.
    pub fn rejoindre(
        &self,
        session: Uuid,
    ) -> Option<(Vec<StreamEvent>, broadcast::Receiver<StreamEvent>)> {
        let table = self.table();
        let vive = table.get(&session)?;
        Some((vive.historique.clone(), vive.diffusion.subscribe()))
    }
}

/// L'historique reste court : les jetons se recollent, un brouillon remplace
/// le précédent, une demande d'accord tranchée s'efface — la rejouer
/// reposerait une question déjà répondue.
fn ajouter_a_l_historique(historique: &mut Vec<StreamEvent>, ev: &StreamEvent) {
    match ev {
        StreamEvent::Token { text } => {
            if let Some(StreamEvent::Token { text: t }) = historique.last_mut() {
                t.push_str(text);
                return;
            }
        }
        StreamEvent::ToolDraft { index, .. } => historique
            .retain(|e| !matches!(e, StreamEvent::ToolDraft { index: i, .. } if i == index)),
        StreamEvent::ToolResult { call_id, .. } => historique.retain(
            |e| !matches!(e, StreamEvent::ToolApproval { call_id: c, .. } if c == call_id),
        ),
        _ => {}
    }
    historique.push(ev.clone());
}

/// Le canal donné à la boucle d'agent : chaque événement va à l'écran qui a
/// envoyé le message *et* au registre, pour qui reviendra.
pub fn dedoubler(
    origine: Channel<InvokeResponseBody>,
    vives: ReponsesVives,
    session: Uuid,
    ticket: u64,
) -> Channel<StreamEvent> {
    Channel::new(move |corps| {
        if let InvokeResponseBody::Json(json) = &corps {
            match serde_json::from_str::<StreamEvent>(json) {
                Ok(ev) => vives.publier(session, ticket, ev),
                Err(e) => tracing::warn!(error = %e, "événement de réponse illisible"),
            }
        }
        origine.send(corps)
    })
}

/// Les conversations dont une réponse tourne sur ce poste.
#[tauri::command]
pub fn reponses_locales_en_cours(core: tauri::State<'_, crate::Core>) -> Vec<String> {
    core.reponses_vives
        .en_cours()
        .iter()
        .map(Uuid::to_string)
        .collect()
}

/// Rejoint la réponse en cours de `session_id` : `true` une fois suivie
/// jusqu'au bout, `false` si aucune ne tourne.
#[tauri::command]
pub async fn suivre_reponse_locale(
    core: tauri::State<'_, crate::Core>,
    session_id: Uuid,
    on_event: Channel<StreamEvent>,
) -> Result<bool, String> {
    let Some((deja, mut suite)) = core.reponses_vives.rejoindre(session_id) else {
        return Ok(false);
    };
    for ev in deja {
        if on_event.send(ev).is_err() {
            return Ok(true);
        }
    }
    loop {
        match suite.recv().await {
            Ok(ev) => {
                if on_event.send(ev).is_err() {
                    // L'écran a quitté la conversation : on lâche.
                    return Ok(true);
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(perdus = n, "suivi de réponse en retard");
            }
            Err(broadcast::error::RecvError::Closed) => return Ok(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jeton(t: &str) -> StreamEvent {
        StreamEvent::Token { text: t.into() }
    }

    #[test]
    fn rejoindre_rend_le_deja_dit_puis_la_suite() {
        let vives = ReponsesVives::default();
        let s = Uuid::new_v4();
        assert!(vives.rejoindre(s).is_none());
        let t = vives.commencer(s);
        vives.publier(s, t, jeton("Bon"));
        vives.publier(s, t, jeton("jour"));
        let (deja, mut suite) = vives.rejoindre(s).expect("réponse en cours");
        assert_eq!(deja.len(), 1);
        assert!(matches!(&deja[0], StreamEvent::Token { text } if text == "Bonjour"));
        vives.publier(s, t, jeton(" !"));
        assert!(matches!(suite.try_recv(), Ok(StreamEvent::Token { text }) if text == " !"));
        assert_eq!(vives.en_cours(), vec![s]);
        vives.terminer(s, t);
        assert!(matches!(
            suite.try_recv(),
            Err(broadcast::error::TryRecvError::Closed)
        ));
        assert!(vives.en_cours().is_empty());
    }

    #[test]
    fn une_reponse_plus_ancienne_ne_retire_pas_la_suivante() {
        let vives = ReponsesVives::default();
        let s = Uuid::new_v4();
        let ancienne = vives.commencer(s);
        let nouvelle = vives.commencer(s);
        vives.publier(s, ancienne, jeton("perdu"));
        vives.terminer(s, ancienne);
        let (deja, _) = vives.rejoindre(s).expect("la nouvelle tourne");
        assert!(deja.is_empty());
        vives.terminer(s, nouvelle);
        assert!(vives.rejoindre(s).is_none());
    }

    #[test]
    fn une_demande_d_accord_tranchee_ne_se_rejoue_pas() {
        let mut h = Vec::new();
        let accord: StreamEvent = serde_json::from_value(serde_json::json!({
            "type": "tool_approval", "call_id": "c1", "tool": "write_file",
            "args": {}, "risk": "medium", "reason": "écrit un fichier", "is_remote": false
        }))
        .expect("événement d'accord");
        ajouter_a_l_historique(&mut h, &accord);
        ajouter_a_l_historique(
            &mut h,
            &StreamEvent::ToolResult {
                call_id: "c1".into(),
                ok: true,
                output: "fait".into(),
            },
        );
        assert_eq!(h.len(), 1);
        assert!(matches!(&h[0], StreamEvent::ToolResult { .. }));
    }
}
