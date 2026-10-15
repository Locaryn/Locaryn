//! Les mêmes outils, dans le même ordre, d'un tour à l'autre d'une conversation.
//!
//! Le gabarit du modèle écrit la définition des outils en tête du prompt,
//! avant la conversation. Le moteur ne relit que ce qui a changé depuis le
//! tour précédent ; un outil de plus, de moins ou déplacé, et tout ce qui
//! suit est relu. Or le choix des outils suivait le texte de chaque message
//! (pertinence, outils d'écran offerts si on parlait du navigateur) : chaque
//! tour relisait ~6 000 jetons, 90 secondes sur une RX 570 avant le premier
//! mot — et `/goal`, qui enchaîne les tours, paraissait arrêté.
//!
//! On retient donc, par conversation, les outils offerts au tour précédent :
//! ils gardent leur place, les nouveaux viennent après, l'outil de chargement
//! des outils différés reste le dernier.

use crate::tool_budget::CHARGER_OUTILS;
use crate::tools::ToolSpec;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

#[derive(Default)]
struct Memo {
    offerts: Vec<String>,
    ecran: bool,
}

static MEMO: LazyLock<Mutex<HashMap<uuid::Uuid, Memo>>> = LazyLock::new(Default::default);
/// Conversations retenues au plus : au-delà, la plus ancienne est oubliée.
const CONVERSATIONS_MAX: usize = 64;

fn memo<R>(f: impl FnOnce(&mut HashMap<uuid::Uuid, Memo>) -> R) -> R {
    let mut m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut m)
}

/// Les outils offerts au tour précédent de cette conversation.
pub fn precedents(session: uuid::Uuid) -> Vec<String> {
    memo(|m| {
        m.get(&session)
            .map(|x| x.offerts.clone())
            .unwrap_or_default()
    })
}

/// Les outils d'écran ont déjà été offerts dans cette conversation : ils le
/// restent, au lieu d'apparaître et disparaître au gré des messages.
pub fn ecran_deja_offert(session: uuid::Uuid) -> bool {
    memo(|m| m.get(&session).is_some_and(|x| x.ecran))
}

/// Range `specs` dans l'ordre du tour précédent, puis retient ce tour.
pub fn ordonner_et_retenir(session: uuid::Uuid, specs: &mut [ToolSpec], ecran: bool) {
    let avant = precedents(session);
    ordonner(&avant, specs);
    let noms = specs.iter().map(|s| s.name.clone()).collect();
    memo(|m| {
        if m.len() >= CONVERSATIONS_MAX && !m.contains_key(&session) {
            if let Some(k) = m.keys().next().copied() {
                m.remove(&k);
            }
        }
        let e = m.entry(session).or_default();
        e.offerts = noms;
        e.ecran |= ecran;
    });
}

/// L'ordre du tour précédent d'abord, les nouveaux ensuite (dans leur ordre),
/// l'outil de chargement en dernier.
pub fn ordonner(avant: &[String], specs: &mut [ToolSpec]) {
    let rang: HashMap<&str, usize> = avant
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    // Tri stable : à rang égal (les nouveaux), l'ordre courant est gardé.
    specs.sort_by_key(|s| {
        if s.name == CHARGER_OUTILS {
            (2, 0)
        } else {
            match rang.get(s.name.as_str()) {
                Some(&r) => (0, r),
                None => (1, 0),
            }
        }
    });
}

/// Pour le tri par pertinence de `tool_budget::fit` : ces outils passent
/// devant, pour que la sélection ne change pas d'un message à l'autre.
pub fn ensemble(avant: &[String]) -> HashSet<String> {
    avant.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outil(n: &str) -> ToolSpec {
        ToolSpec {
            name: n.into(),
            description: String::new(),
            input_schema: serde_json::json!({}),
            risk: locaryn_shared_types::Risk::Low,
            required_permissions: Vec::new(),
        }
    }

    fn noms(v: &[ToolSpec]) -> Vec<&str> {
        v.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn l_ordre_du_tour_precedent_est_garde() {
        let avant = vec!["b".to_string(), "a".to_string(), CHARGER_OUTILS.to_string()];
        let mut v = vec![outil(CHARGER_OUTILS), outil("c"), outil("a"), outil("b")];
        ordonner(&avant, &mut v);
        assert_eq!(noms(&v), ["b", "a", "c", CHARGER_OUTILS]);
    }

    #[test]
    fn une_conversation_retient_ses_outils() {
        let s = uuid::Uuid::new_v4();
        let mut v = vec![outil("x"), outil("y")];
        ordonner_et_retenir(s, &mut v, false);
        assert!(!ecran_deja_offert(s));
        let mut w = vec![outil("z"), outil("y"), outil("x")];
        ordonner_et_retenir(s, &mut w, true);
        assert_eq!(noms(&w), ["x", "y", "z"]);
        assert!(ecran_deja_offert(s));
        assert_eq!(precedents(s), ["x", "y", "z"]);
    }
}
