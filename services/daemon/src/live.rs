//! La réponse en cours d'une conversation, suivie depuis un autre écran.
//!
//! Une question posée sur le téléphone ne s'affichait sur l'ordinateur qu'une
//! fois la réponse finie et enregistrée — et encore, après avoir quitté puis
//! rouvert la conversation. Chaque réponse servie ici garde donc ce qu'elle a
//! déjà produit et le diffuse : qui ouvre la conversation en cours de route
//! reçoit le début, puis la suite au fil de l'eau.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use locaryn_events::StreamEvent;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Ce qu'un retardataire peut manquer entre deux lectures avant de décrocher.
const CAPACITE: usize = 1024;

struct Direct {
    /// Tout ce que la réponse a produit jusqu'ici, jetons regroupés.
    tampon: Vec<StreamEvent>,
    tx: broadcast::Sender<StreamEvent>,
}

static DIRECTS: LazyLock<Mutex<HashMap<Uuid, Direct>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// La dernière mesure de la fenêtre de chaque conversation (`ContextUsage`) :
/// la jauge du téléphone la relit entre deux réponses.
static MESURES: LazyLock<Mutex<HashMap<Uuid, StreamEvent>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// La dernière mesure de la fenêtre pour cette conversation, si une réponse
/// en a produit une depuis le lancement du service.
pub fn derniere_mesure(session: Uuid) -> Option<StreamEvent> {
    MESURES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&session)
        .cloned()
}

/// La conversation a rétréci (compressée, message retiré) : la mesure ne vaut
/// plus jusqu'à la prochaine réponse.
pub fn oublier_mesure(session: Uuid) {
    MESURES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&session);
}

fn verrou() -> std::sync::MutexGuard<'static, HashMap<Uuid, Direct>> {
    // Un verrou empoisonné ne garde qu'un tampon d'affichage : on le reprend.
    DIRECTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Une réponse commence pour cette conversation.
pub fn ouvrir(session: Uuid) {
    let (tx, _) = broadcast::channel(CAPACITE);
    verrou().insert(
        session,
        Direct {
            tampon: Vec::new(),
            tx,
        },
    );
}

/// Un événement de la réponse : retenu pour les retardataires, diffusé aux
/// écrans qui suivent déjà.
pub fn publier(session: Uuid, ev: &StreamEvent) {
    if matches!(ev, StreamEvent::ContextUsage { .. }) {
        MESURES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(session, ev.clone());
    }
    let mut directs = verrou();
    let Some(direct) = directs.get_mut(&session) else {
        return;
    };
    match (direct.tampon.last_mut(), ev) {
        // Des milliers de jetons d'un mot : on les recolle, le retardataire
        // reçoit le texte d'un bloc.
        (Some(StreamEvent::Token { text: deja }), StreamEvent::Token { text }) => {
            deja.push_str(text)
        }
        _ => direct.tampon.push(ev.clone()),
    }
    // Personne ne suit : l'envoi échoue, c'est attendu.
    let _ = direct.tx.send(ev.clone());
}

/// La réponse est finie et enregistrée : les écrans qui la suivaient relisent
/// la conversation.
pub fn fermer(session: Uuid) {
    verrou().remove(&session);
}

/// GET /v1/sessions/:id/live — la réponse en cours, depuis son début.
/// 204 quand aucune réponse ne tourne pour cette conversation.
pub async fn suivre(Path(id): Path<String>) -> Response {
    let Ok(session) = Uuid::parse_str(&id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let (deja, rx) = {
        let directs = verrou();
        let Some(direct) = directs.get(&session) else {
            return StatusCode::NO_CONTENT.into_response();
        };
        (direct.tampon.clone(), direct.tx.subscribe())
    };
    let suite = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(ev) => return Some((ev, rx)),
                // Un écran trop lent a perdu des jetons : il continue avec la
                // suite, la conversation enregistrée rattrapera le texte.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    let flux = futures::stream::iter(deja).chain(suite).map(|ev| {
        let json = serde_json::to_string(&ev).unwrap_or_else(|_| "{}".into());
        Ok::<Event, std::convert::Infallible>(Event::default().data(json))
    });
    Sse::new(flux)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn un_retardataire_recoit_le_debut_regroupe_puis_la_suite() {
        let s = Uuid::new_v4();
        ouvrir(s);
        publier(s, &StreamEvent::Token { text: "Bon".into() });
        publier(
            s,
            &StreamEvent::Token {
                text: "jour".into(),
            },
        );
        let (deja, mut rx) = {
            let d = verrou();
            let d = d.get(&s).unwrap();
            (d.tampon.clone(), d.tx.subscribe())
        };
        assert_eq!(deja.len(), 1);
        assert!(matches!(&deja[0], StreamEvent::Token { text } if text == "Bonjour"));
        publier(s, &StreamEvent::Token { text: " !".into() });
        assert!(matches!(rx.recv().await.unwrap(), StreamEvent::Token { text } if text == " !"));
        fermer(s);
        assert!(
            rx.recv().await.is_err(),
            "fermer clôt le flux des écrans qui suivent"
        );
    }
}
