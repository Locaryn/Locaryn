//! La porte d'approbation de l'application de bureau.
//!
//! Le runtime demande, cette porte fait apparaître la question à l'écran et
//! attend la réponse. Trois exigences la façonnent :
//!
//! * **Ne jamais bloquer pour toujours.** Une fenêtre fermée sans réponse, ou
//!   une application quittée en cours de route, doit finir en refus. Une
//!   conversation figée sans explication est pire qu'un refus explicite.
//! * **Se souvenir de ce qui a été décidé.** Répondre « toujours » puis
//!   revoir la même question au message suivant vide le réglage de son sens.
//! * **Ne jamais mémoriser un refus au-delà de l'appel.** Refuser une fois ne
//!   veut pas dire refuser pour la session : l'utilisateur qui change d'avis
//!   ne doit pas avoir à redémarrer.

use locaryn_agent_runtime::approval::{ApprovalGate, ApprovalOutcome, ApprovalRequest};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};

/// Au-delà, faute de réponse, on refuse.
///
/// Assez long pour lire un diff et réfléchir, assez court pour qu'une fenêtre
/// oubliée ne laisse pas la conversation en suspens indéfiniment.
const DELAI_REPONSE: Duration = Duration::from_secs(5 * 60);

/// Portée d'une décision, telle que la fenêtre la propose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Portee {
    /// Cet appel seulement.
    UneFois,
    /// Cet outil, jusqu'à la fermeture de l'application.
    Session,
    /// Cet outil, pour ce projet. Persisté par l'appelant.
    Projet,
    /// Cet outil, partout.
    Toujours,
}

impl Portee {
    pub fn depuis(texte: &str) -> Self {
        match texte {
            "session" => Portee::Session,
            "project" => Portee::Projet,
            "always" => Portee::Toujours,
            _ => Portee::UneFois,
        }
    }

    /// Une décision retenue vaut-elle au-delà de l'appel en cours ?
    fn memorisable(self) -> bool {
        !matches!(self, Portee::UneFois)
    }
}

/// Ce que l'utilisateur a répondu.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub autorise: bool,
    pub portee: Portee,
}

/// L'état partagé : ce qui attend une réponse, et ce qui a déjà été accordé.
#[derive(Default)]
struct Etat {
    /// Un émetteur par appel en vol, retiré dès qu'on répond.
    en_attente: HashMap<String, oneshot::Sender<Verdict>>,
    /// Outils autorisés d'avance. Seules les autorisations sont retenues :
    /// un refus mémorisé empêcherait de revenir sur sa décision.
    accordes: HashMap<String, Portee>,
}

/// La porte que le runtime interroge.
#[derive(Clone)]
pub struct GateBureau {
    etat: Arc<Mutex<Etat>>,
    /// Réglable pour que les tests n'attendent pas cinq minutes réelles, et
    /// pour qu'un déploiement puisse resserrer le délai s'il le souhaite.
    delai: Duration,
}

impl Default for GateBureau {
    fn default() -> Self {
        Self {
            etat: Arc::new(Mutex::new(Etat::default())),
            delai: DELAI_REPONSE,
        }
    }
}

impl GateBureau {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn avec_delai(delai: Duration) -> Self {
        Self {
            delai,
            ..Self::default()
        }
    }

    /// Enregistre la réponse de l'utilisateur. Renvoie faux quand plus rien
    /// n'attend — fenêtre déjà expirée, ou double clic.
    pub async fn repondre(&self, call_id: &str, tool: &str, verdict: Verdict) -> bool {
        let emetteur = {
            let mut etat = self.etat.lock().await;
            if verdict.autorise && verdict.portee.memorisable() {
                etat.accordes.insert(tool.to_string(), verdict.portee);
            }
            etat.en_attente.remove(call_id)
        };
        match emetteur {
            // `send` échoue si le runtime a déjà abandonné (expiration) : ce
            // n'est pas une erreur de l'utilisateur, seulement une réponse
            // arrivée trop tard.
            Some(tx) => tx.send(verdict).is_ok(),
            None => false,
        }
    }

    /// Oublie ce qui a été accordé pour la session.
    ///
    /// Pas encore appelé : la question de savoir *quand* une portée « session »
    /// expire — changement de projet, de conversation, de modèle — n'est pas
    /// tranchée. La méthode existe pour que la réponse soit un appel, pas une
    /// réécriture de la porte.
    #[allow(dead_code)]
    pub async fn oublier_session(&self) {
        let mut etat = self.etat.lock().await;
        etat.accordes.retain(|_, p| *p != Portee::Session);
    }
}

#[async_trait::async_trait]
impl ApprovalGate for GateBureau {
    async fn request(&self, req: ApprovalRequest) -> ApprovalOutcome {
        // Déjà accordé plus tôt ? On ne redemande pas.
        {
            let etat = self.etat.lock().await;
            if let Some(portee) = etat.accordes.get(&req.tool) {
                tracing::debug!(tool = %req.tool, ?portee, "accord déjà donné, pas de question");
                return ApprovalOutcome::Allow;
            }
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut etat = self.etat.lock().await;
            etat.en_attente.insert(req.call_id.clone(), tx);
        }

        let issue = tokio::time::timeout(self.delai, rx).await;

        // Quelle que soit l'issue, l'entrée ne doit pas survivre : une carte
        // qui grossit à chaque expiration est une fuite silencieuse.
        {
            let mut etat = self.etat.lock().await;
            etat.en_attente.remove(&req.call_id);
        }

        match issue {
            Ok(Ok(v)) if v.autorise => ApprovalOutcome::Allow,
            Ok(Ok(_)) => ApprovalOutcome::Deny {
                reason: "vous avez refusé cet appel".to_string(),
            },
            // L'émetteur a disparu sans répondre : fenêtre fermée, ou
            // application en cours d'arrêt.
            Ok(Err(_)) => ApprovalOutcome::Deny {
                reason: "la demande a été abandonnée avant votre réponse".to_string(),
            },
            Err(_) => {
                tracing::info!(tool = %req.tool, call_id = %req.call_id, "approbation expirée");
                ApprovalOutcome::Deny {
                    reason: "aucune réponse dans le délai imparti".to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demande(call_id: &str, tool: &str) -> ApprovalRequest {
        ApprovalRequest {
            call_id: call_id.into(),
            tool: tool.into(),
            args: serde_json::json!({}),
            risk: locaryn_events::Risk::High,
            reason: "test".into(),
            diff: None,
            is_remote: false,
        }
    }

    /// Le cas nominal : la réponse débloque l'appel qui attendait.
    #[tokio::test]
    async fn une_autorisation_debloque_l_appel() {
        let gate = GateBureau::new();
        let g = gate.clone();
        let attente = tokio::spawn(async move { g.request(demande("c1", "write_file")).await });

        // Laisser le temps à la demande de s'enregistrer.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            gate.repondre(
                "c1",
                "write_file",
                Verdict {
                    autorise: true,
                    portee: Portee::UneFois
                }
            )
            .await
        );
        assert!(attente.await.unwrap().is_allowed());
    }

    #[tokio::test]
    async fn un_refus_est_transmis_avec_son_motif() {
        let gate = GateBureau::new();
        let g = gate.clone();
        let attente = tokio::spawn(async move { g.request(demande("c2", "run_command")).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        gate.repondre(
            "c2",
            "run_command",
            Verdict {
                autorise: false,
                portee: Portee::UneFois,
            },
        )
        .await;
        match attente.await.unwrap() {
            ApprovalOutcome::Deny { reason } => assert!(!reason.is_empty()),
            ApprovalOutcome::Allow => panic!("un refus ne doit pas laisser passer"),
        }
    }

    /// « Toujours » doit tenir : redemander au message suivant viderait le
    /// réglage de son sens.
    #[tokio::test]
    async fn un_accord_memorise_evite_la_seconde_question() {
        let gate = GateBureau::new();
        let g = gate.clone();
        let premier = tokio::spawn(async move { g.request(demande("c3", "read_file")).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        gate.repondre(
            "c3",
            "read_file",
            Verdict {
                autorise: true,
                portee: Portee::Session,
            },
        )
        .await;
        assert!(premier.await.unwrap().is_allowed());

        // Le second appel ne doit rien enregistrer en attente : il répond seul.
        let second = gate.request(demande("c4", "read_file")).await;
        assert!(second.is_allowed());
        assert!(
            gate.etat.lock().await.en_attente.is_empty(),
            "un accord mémorisé ne doit pas créer d'attente"
        );
    }

    /// Un refus ne se mémorise pas : changer d'avis ne doit pas exiger un
    /// redémarrage.
    #[tokio::test]
    async fn un_refus_ne_se_memorise_jamais() {
        let gate = GateBureau::new();
        let g = gate.clone();
        let premier = tokio::spawn(async move { g.request(demande("c5", "delete_file")).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        gate.repondre(
            "c5",
            "delete_file",
            Verdict {
                autorise: false,
                portee: Portee::Toujours,
            },
        )
        .await;
        assert!(!premier.await.unwrap().is_allowed());
        assert!(
            gate.etat.lock().await.accordes.is_empty(),
            "aucun refus ne doit être retenu"
        );
    }

    /// Répondre à un appel qui n'attend plus ne doit ni paniquer ni mentir.
    #[tokio::test]
    async fn repondre_a_un_appel_inconnu_renvoie_faux() {
        let gate = GateBureau::new();
        assert!(
            !gate
                .repondre(
                    "jamais-vu",
                    "write_file",
                    Verdict {
                        autorise: true,
                        portee: Portee::UneFois
                    }
                )
                .await
        );
    }

    /// Sans réponse, on refuse — et l'entrée ne reste pas en mémoire.
    #[tokio::test]
    async fn l_absence_de_reponse_finit_en_refus() {
        let gate = GateBureau::avec_delai(Duration::from_millis(80));
        let g = gate.clone();
        let attente = tokio::spawn(async move { g.request(demande("c6", "run_command")).await });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!attente.await.unwrap().is_allowed());
        assert!(
            gate.etat.lock().await.en_attente.is_empty(),
            "une expiration ne doit pas laisser de trace"
        );
    }
}
