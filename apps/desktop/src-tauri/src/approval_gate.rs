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
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
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
}

/// Ce que l'utilisateur a répondu.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub autorise: bool,
    pub portee: Portee,
}

/// Les accords qui survivent à un redémarrage.
///
/// « Ce projet » et « Toujours » l'annoncent à l'utilisateur ; les garder en
/// mémoire seule rendait la promesse fausse dès la fermeture de la fenêtre.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Accords {
    /// Outils autorisés partout.
    #[serde(default)]
    toujours: HashSet<String>,
    /// Outils autorisés projet par projet.
    #[serde(default)]
    projets: HashMap<String, HashSet<String>>,
}

impl Accords {
    fn chemin(data_dir: &Path) -> PathBuf {
        data_dir.join("tool_approvals.json")
    }

    fn charger(data_dir: &Path) -> Self {
        std::fs::read_to_string(Self::chemin(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn enregistrer(&self, data_dir: &Path) {
        match serde_json::to_string_pretty(self) {
            // Un accord qu'on ne parvient pas à écrire sera redemandé au
            // prochain démarrage : gênant, mais sans danger. On le journalise
            // plutôt que d'interrompre la conversation en cours.
            Ok(txt) => {
                if let Err(e) = std::fs::write(Self::chemin(data_dir), txt) {
                    tracing::warn!(error = %e, "accords d'outils non enregistrés");
                }
            }
            Err(e) => tracing::warn!(error = %e, "accords d'outils non sérialisables"),
        }
    }

    fn autorise(&self, tool: &str, projet: &str) -> bool {
        self.toujours.contains(tool) || self.projets.get(projet).is_some_and(|s| s.contains(tool))
    }
}

/// L'état partagé : ce qui attend une réponse, et ce qui a déjà été accordé.
#[derive(Default)]
struct Etat {
    /// Un émetteur par appel en vol, retiré dès qu'on répond, avec le projet
    /// d'où venait la demande. Le retenir ici évite de le faire transiter par
    /// l'interface pour revenir identique : la porte l'a déjà vu.
    en_attente: HashMap<String, (oneshot::Sender<Verdict>, String)>,
    /// Accordé pour la durée de vie de l'application. Volontairement hors du
    /// fichier : « jusqu'à la fermeture » ne doit pas survivre à la fermeture.
    session: HashSet<String>,
    /// Accordé au-delà, et écrit sur disque.
    durables: Accords,
}

/// La porte que le runtime interroge.
#[derive(Clone)]
pub struct GateBureau {
    etat: Arc<Mutex<Etat>>,
    /// Réglable pour que les tests n'attendent pas cinq minutes réelles, et
    /// pour qu'un déploiement puisse resserrer le délai s'il le souhaite.
    delai: Duration,
    /// Où les accords durables sont écrits.
    data_dir: PathBuf,
}

impl GateBureau {
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        let durables = Accords::charger(&data_dir);
        Self {
            etat: Arc::new(Mutex::new(Etat {
                durables,
                ..Etat::default()
            })),
            delai: DELAI_REPONSE,
            data_dir,
        }
    }

    /// Recharge les accords durables depuis le même dossier, comme le ferait
    /// un redémarrage de l'application.
    #[cfg(test)]
    fn rouvrir(&self) -> Self {
        Self::new(self.data_dir.clone())
    }

    #[cfg(test)]
    fn pour_test(delai: Duration) -> Self {
        // Un dossier par instance : deux tests parallèles ne doivent pas se
        // relire l'un l'autre.
        let dir = std::env::temp_dir().join(format!("locaryn_approb_{}_{}", std::process::id(), {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            N.fetch_add(1, Ordering::Relaxed)
        }));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            etat: Arc::new(Mutex::new(Etat::default())),
            delai,
            data_dir: dir,
        }
    }

    /// Enregistre la réponse de l'utilisateur. Renvoie faux quand plus rien
    /// n'attend — fenêtre déjà expirée, ou double clic.
    pub async fn repondre(&self, call_id: &str, tool: &str, verdict: Verdict) -> bool {
        let emetteur = {
            let mut etat = self.etat.lock().await;
            let projet = etat
                .en_attente
                .get(call_id)
                .map(|(_, p)| p.clone())
                .unwrap_or_default();
            // Seules les autorisations sont retenues. Un refus mémorisé
            // empêcherait de revenir sur sa décision sans redémarrer.
            if verdict.autorise {
                let mut ecrire = false;
                match verdict.portee {
                    Portee::UneFois => {}
                    Portee::Session => {
                        etat.session.insert(tool.to_string());
                    }
                    Portee::Projet => {
                        etat.durables
                            .projets
                            .entry(projet.clone())
                            .or_default()
                            .insert(tool.to_string());
                        ecrire = true;
                    }
                    Portee::Toujours => {
                        etat.durables.toujours.insert(tool.to_string());
                        ecrire = true;
                    }
                }
                if ecrire {
                    etat.durables.enregistrer(&self.data_dir);
                }
            }
            etat.en_attente.remove(call_id).map(|(tx, _)| tx)
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
        etat.session.clear();
    }
}

#[async_trait::async_trait]
impl ApprovalGate for GateBureau {
    async fn request(&self, req: ApprovalRequest) -> ApprovalOutcome {
        // Déjà accordé plus tôt ? On ne redemande pas.
        {
            let projet = req.project_id.to_string();
            let etat = self.etat.lock().await;
            if etat.session.contains(&req.tool) || etat.durables.autorise(&req.tool, &projet) {
                tracing::debug!(tool = %req.tool, "accord déjà donné, pas de question");
                return ApprovalOutcome::Allow;
            }
        }

        let (tx, rx) = oneshot::channel();
        {
            let mut etat = self.etat.lock().await;
            etat.en_attente
                .insert(req.call_id.clone(), (tx, req.project_id.to_string()));
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
            project_id: uuid::Uuid::nil(),
        }
    }

    /// Le cas nominal : la réponse débloque l'appel qui attendait.
    #[tokio::test]
    async fn une_autorisation_debloque_l_appel() {
        let gate = GateBureau::pour_test(DELAI_REPONSE);
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
        let gate = GateBureau::pour_test(DELAI_REPONSE);
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
        let gate = GateBureau::pour_test(DELAI_REPONSE);
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
        let gate = GateBureau::pour_test(DELAI_REPONSE);
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
            {
                let e = gate.etat.lock().await;
                e.session.is_empty() && e.durables.toujours.is_empty()
            },
            "aucun refus ne doit être retenu"
        );
    }

    /// Répondre à un appel qui n'attend plus ne doit ni paniquer ni mentir.
    #[tokio::test]
    async fn repondre_a_un_appel_inconnu_renvoie_faux() {
        let gate = GateBureau::pour_test(DELAI_REPONSE);
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

    /// « Toujours » doit survivre à un redémarrage. Sans ce test, la fenêtre
    /// promettrait « partout, sans limite de durée » pour une décision effacée
    /// à la fermeture — l'écart exact entre ce qui est dit et ce qui est fait.
    #[tokio::test]
    async fn un_accord_toujours_survit_a_un_redemarrage() {
        let gate = GateBureau::pour_test(DELAI_REPONSE);
        let g = gate.clone();
        let premier = tokio::spawn(async move { g.request(demande("c7", "run_command")).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        gate.repondre(
            "c7",
            "run_command",
            Verdict {
                autorise: true,
                portee: Portee::Toujours,
            },
        )
        .await;
        assert!(premier.await.unwrap().is_allowed());

        // Une porte neuve sur le même dossier : c'est ce que fait un
        // redémarrage. Elle ne doit plus poser la question.
        let apres_redemarrage = gate.rouvrir();
        assert!(
            apres_redemarrage
                .request(demande("c8", "run_command"))
                .await
                .is_allowed(),
            "l'accord « toujours » n'a pas survécu"
        );
    }

    /// Un accord « session » ne doit PAS survivre : la fenêtre annonce
    /// « jusqu'à la fermeture de Locaryn », et cette promesse-là engage aussi.
    #[tokio::test]
    async fn un_accord_session_ne_survit_pas() {
        let gate = GateBureau::pour_test(DELAI_REPONSE);
        let g = gate.clone();
        let premier = tokio::spawn(async move { g.request(demande("c9", "write_file")).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        gate.repondre(
            "c9",
            "write_file",
            Verdict {
                autorise: true,
                portee: Portee::Session,
            },
        )
        .await;
        assert!(premier.await.unwrap().is_allowed());

        let apres = gate.rouvrir();
        let g2 = apres.clone();
        let seconde = tokio::spawn(async move { g2.request(demande("c10", "write_file")).await });
        tokio::time::sleep(Duration::from_millis(80)).await;
        let attend = !apres.etat.lock().await.en_attente.is_empty();
        seconde.abort();
        assert!(attend, "après redémarrage, la question doit être reposée");
    }

    /// Sans réponse, on refuse — et l'entrée ne reste pas en mémoire.
    #[tokio::test]
    async fn l_absence_de_reponse_finit_en_refus() {
        let gate = GateBureau::pour_test(Duration::from_millis(80));
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
