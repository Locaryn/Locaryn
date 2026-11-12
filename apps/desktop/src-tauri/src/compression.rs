//! Faire tenir une longue conversation dans la fenêtre du modèle, sans rien
//! demander à la personne.
//!
//! À l'envoi, quand l'historique dépasse sa part de la fenêtre, les vieux
//! tours cèdent la place à un résumé écrit par le modèle. Le résumé est gardé
//! pour la conversation : il était refait à chaque message, un appel au
//! modèle de plus à chaque envoi une fois la fenêtre entamée.

use crate::Core;
use locaryn_agent_runtime::{compaction, ChatTurn};
use locaryn_events::StreamEvent;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, MutexGuard, OnceLock};
use tauri::ipc::Channel;
use uuid::Uuid;

/// En tête du tour qui remplace les vieux échanges.
const TITRE_RESUME: &str = "[Résumé des échanges précédents]";

/// De quoi demander un résumé au modèle actif.
pub struct Moteur {
    pub http: reqwest::Client,
    pub endpoint: String,
    pub model: String,
    /// La clé du fournisseur distant, absente pour un moteur local.
    pub cle: Option<String>,
}

impl Moteur {
    /// Le modèle actif, avec la clé et le client de son fournisseur.
    pub async fn actif(core: &Core, provider: &locaryn_shared_types::Provider) -> Self {
        Self {
            http: crate::cloud_providers::http_for_active_provider(core, provider)
                .await
                .unwrap_or_else(|| core.http.clone()),
            endpoint: provider.endpoint.clone(),
            model: provider.model.clone().unwrap_or_else(|| "default".into()),
            cle: crate::cloud_providers::key_for_active_provider(core, provider),
        }
    }

    async fn resumer(&self, transcript: &str) -> Result<String, String> {
        compaction::resumer(
            &self.http,
            &self.endpoint,
            self.cle.as_deref(),
            &self.model,
            transcript,
        )
        .await
    }
}

/// Le résumé gardé d'une conversation.
#[derive(Clone)]
struct Resume {
    /// Nombre de tours, depuis le début, que le résumé remplace.
    couverts: usize,
    /// Empreinte du dernier tour couvert : si la conversation a été reprise
    /// plus haut, elle ne correspond plus et le résumé est refait.
    empreinte: u64,
    texte: String,
}

fn resumes() -> MutexGuard<'static, HashMap<Uuid, Resume>> {
    static RESUMES: OnceLock<Mutex<HashMap<Uuid, Resume>>> = OnceLock::new();
    match RESUMES.get_or_init(Default::default).lock() {
        Ok(g) => g,
        Err(empoisonne) => empoisonne.into_inner(),
    }
}

/// La conversation a été compressée à la main, ou vidée : son résumé gardé ne
/// décrit plus rien.
pub fn oublier(session: Uuid) {
    resumes().remove(&session);
}

fn empreinte(t: &ChatTurn) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.role.hash(&mut h);
    t.content.hash(&mut h);
    h.finish()
}

/// Le résumé gardé, s'il décrit toujours le début de ces tours.
fn resume_valide(session: Uuid, tours: &[ChatTurn]) -> Option<Resume> {
    let garde = resumes().get(&session).cloned()?;
    let dernier = tours.get(garde.couverts.checked_sub(1)?)?;
    (empreinte(dernier) == garde.empreinte).then_some(garde)
}

fn tour_de_resume(texte: &str) -> ChatTurn {
    ChatTurn {
        role: "system".into(),
        content: format!("{TITRE_RESUME}\n{}", texte.trim()),
        ..Default::default()
    }
}

fn poids(tours: &[ChatTurn]) -> usize {
    tours.iter().map(crate::approx_tokens_tour).sum()
}

/// L'historique à envoyer : entier s'il tient dans `budget` jetons, sinon ses
/// vieux tours remplacés par un résumé.
pub async fn compacter(
    session: Uuid,
    moteur: &Moteur,
    tours: Vec<ChatTurn>,
    budget: usize,
    on_event: &Channel<StreamEvent>,
) -> Vec<ChatTurn> {
    if poids(&tours) <= budget || tours.len() < 3 {
        return tours;
    }
    // Le résumé déjà écrit suffit tant que ce qui le suit tient encore.
    let garde = resume_valide(session, &tours);
    if let Some(r) = &garde {
        let suite = &tours[r.couverts..];
        if crate::approx_tokens(&r.texte) + poids(suite) <= budget {
            let mut sortie = vec![tour_de_resume(&r.texte)];
            sortie.extend(suite.iter().cloned());
            return sortie;
        }
    }

    // Le résumé est écrit par le modèle : quelques secondes. Le fil le montre
    // plutôt que de laisser croire à un silence.
    let _ = on_event.send(StreamEvent::ContextCompaction {
        phase: "started".into(),
        detail: "Contexte en cours de compression…".into(),
    });
    let gardes = if tours.len() >= 6 { 4 } else { 2 };
    let coupe = tours.len() - gardes;
    let depuis = garde.as_ref().map_or(0, |r| r.couverts.min(coupe));
    let mut a_resumer = Vec::new();
    if let Some(r) = garde.as_ref().filter(|_| depuis > 0) {
        a_resumer.push(tour_de_resume(&r.texte));
    }
    a_resumer.extend(tours[depuis..coupe].iter().cloned());
    let transcript = compaction::transcrire(&a_resumer);

    let mut resume = moteur.resumer(&transcript).await;
    if let Err(e) = &resume {
        // Un modèle local en mémoire serrée peut rater sa première passe.
        tracing::warn!(erreur = %e, "résumé de conversation raté — nouvelle tentative");
        resume = moteur.resumer(&transcript).await;
    }

    let recents = &tours[coupe..];
    let mut sortie = Vec::with_capacity(gardes + 1);
    match resume {
        Ok(texte) => {
            tracing::info!(tours = coupe, budget, "historique compressé");
            let _ = on_event.send(StreamEvent::ContextCompaction {
                phase: "done".into(),
                detail: format!(
                    "Contexte compressé : {coupe} anciens échanges tiennent dans un résumé."
                ),
            });
            resumes().insert(
                session,
                Resume {
                    couverts: coupe,
                    empreinte: empreinte(&tours[coupe - 1]),
                    texte: texte.clone(),
                },
            );
            sortie.push(tour_de_resume(&texte));
        }
        // Sans résumé, les vieux tours partent quand même plutôt que de faire
        // déborder la fenêtre — mais la personne le voit : un contexte qui
        // s'amincit sans rien dire ressemble à un modèle qui oublie.
        Err(e) => {
            tracing::warn!(erreur = %e, "résumé impossible : anciens échanges retirés");
            let _ = on_event.send(StreamEvent::ContextCompaction {
                phase: "failed".into(),
                detail: format!(
                    "Le modèle n'a pas su résumer ({e}) : {coupe} anciens échanges retirés sans résumé."
                ),
            });
        }
    }
    sortie.extend(recents.iter().cloned());
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tour(role: &str, texte: &str) -> ChatTurn {
        ChatTurn {
            role: role.into(),
            content: texte.into(),
            ..Default::default()
        }
    }

    #[test]
    fn un_resume_garde_vaut_tant_que_le_debut_n_a_pas_change() {
        let session = Uuid::new_v4();
        let tours = vec![tour("user", "a"), tour("assistant", "b"), tour("user", "c")];
        resumes().insert(
            session,
            Resume {
                couverts: 2,
                empreinte: empreinte(&tours[1]),
                texte: "résumé".into(),
            },
        );
        assert!(resume_valide(session, &tours).is_some());
        // La conversation reprise plus haut : le deuxième tour n'est plus le même.
        let repris = vec![tour("user", "a"), tour("assistant", "autre")];
        assert!(resume_valide(session, &repris).is_none());
        // Plus courte que ce que le résumé couvre.
        assert!(resume_valide(session, &tours[..1]).is_none());
        oublier(session);
        assert!(resume_valide(session, &tours).is_none());
    }
}
