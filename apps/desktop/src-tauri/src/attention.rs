//! Ce qui demande l'attention de l'utilisateur, sans la lui prendre de force.
//!
//! Deux choses passent par ici, et elles se ressemblent plus qu'il n'y paraît :
//!
//! * une **question** du modèle, quand il doute et qu'inventer une réponse
//!   coûterait du travail à refaire ;
//! * une **alerte** de l'application, quand quelque chose ne marche plus — un
//!   modèle qui ne tourne plus, un serveur injoignable.
//!
//! Dans les deux cas il y a un projet concerné, une phrase à lire, parfois des
//! réponses à proposer, et une pastille qui doit apparaître. Les séparer en
//! deux mécanismes aurait donné deux endroits à consulter pour savoir si
//! quelque chose attend.
//!
//! # Pourquoi ce n'est pas une fenêtre modale
//!
//! Une question n'est pas urgente. La personne n'est peut-être pas devant
//! l'écran, ou lit un autre projet, ou attend qu'un travail de fond finisse.
//! Lui barrer l'application pour lui demander qui doit voir une note serait
//! disproportionné — et la pousserait à cliquer n'importe quoi pour retrouver
//! son écran, ce qui vide la question de son sens.
//!
//! Le prix de ce choix, c'est qu'une question sans réponse ne s'oublie pas
//! toute seule : elle est donc rendue visible (une pastille sur la
//! conversation et sur le projet) et abandonnable (un bouton). Sans ces deux
//! choses, l'attente sans limite serait un blocage sans explication.
//!
//! # Ce qui continue de tourner
//!
//! Le projet en question s'arrête ; l'application, non. Un travail de fond
//! lancé ailleurs poursuit sa route, parce qu'il n'attend pas cette réponse.
//! C'est la conséquence directe d'attendre sur un canal plutôt que sur un
//! verrou global.
//!
//! Rien ici ne suppose du code : la question peut porter sur un format de
//! rendu, une température de mesure, ou la personne qui doit voir une note.

use locaryn_agent_runtime::question::{
    ContextProposal, QuestionChoice, QuestionGate, QuestionOutcome, QuestionRequest, Urgency,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{oneshot, Mutex};

/// L'événement qui fait apparaître la bande au-dessus du champ de saisie.
///
/// L'interface interroge aussi [`pending_attention`] à son montage : sans
/// cela, une question posée pendant qu'un autre écran était affiché
/// disparaîtrait au retour, alors que le modèle l'attend toujours.
const EVENT: &str = "locaryn://attention";

/// Une réponse proposée, telle qu'elle sera affichée.
#[derive(Debug, Clone, Serialize)]
pub struct AttentionChoice {
    pub id: String,
    pub label: String,
    pub hint: Option<String>,
}

/// Ce qui attend une réponse, tel que l'interface le lit.
#[derive(Debug, Clone, Serialize)]
pub struct AttentionItem {
    pub id: String,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    /// `question` (orange) ou `erreur` (rouge).
    pub urgency: String,
    pub title: String,
    pub detail: Option<String>,
    pub choices: Vec<AttentionChoice>,
    /// L'invite du champ libre. `None` : pas de champ.
    pub free_text: Option<String>,
    pub asked_at: String,
    /// Vrai quand quelqu'un attend vraiment cette réponse — le cas d'une
    /// question du modèle. Faux pour une alerte, que l'on ferme sans répondre.
    pub blocking: bool,
}

/// Ce que l'utilisateur renvoie.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AttentionAnswer {
    /// L'`id` du choix retenu.
    #[serde(default)]
    pub choice: Option<String>,
    /// Ce qu'il a écrit lui-même.
    #[serde(default)]
    pub text: Option<String>,
}

struct EnAttente {
    item: AttentionItem,
    /// L'appelant qui attend. `None` pour une alerte : personne n'écoute, elle
    /// est là pour être vue.
    repondeur: Option<oneshot::Sender<QuestionOutcome>>,
}

#[derive(Default)]
struct Etat {
    /// Volontairement en mémoire. Une question survit à un changement d'écran,
    /// pas à la fermeture de l'application : le travail qui l'attendait
    /// n'existe plus au redémarrage, et rouvrir la question ferait attendre une
    /// réponse que plus personne ne lirait.
    items: HashMap<String, EnAttente>,
    /// L'ordre d'arrivée, pour que la plus ancienne question se règle d'abord.
    ordre: Vec<String>,
}

/// Le registre de ce qui attend.
#[derive(Clone)]
pub struct Attention {
    etat: Arc<Mutex<Etat>>,
    app: Arc<Mutex<Option<AppHandle>>>,
    /// Où ranger une fiche que la personne a acceptée.
    ///
    /// Posé après la construction du noyau, qui porte les deux. `None` dans
    /// les tests du registre seul : la question se pose alors sans que rien
    /// ne s'écrive, ce que la porte dit franchement.
    contexte: Arc<Mutex<Option<locaryn_storage::project_context::ProjectContextRepo>>>,
}

impl Default for Attention {
    fn default() -> Self {
        Self::new()
    }
}

impl Attention {
    #[must_use]
    pub fn new() -> Self {
        Self {
            etat: Arc::new(Mutex::new(Etat::default())),
            app: Arc::new(Mutex::new(None)),
            contexte: Arc::new(Mutex::new(None)),
        }
    }

    /// Donne au registre l'endroit où ranger une fiche acceptée.
    pub async fn brancher_contexte(
        &self,
        repo: locaryn_storage::project_context::ProjectContextRepo,
    ) {
        *self.contexte.lock().await = Some(repo);
    }

    /// Donne au registre le moyen de prévenir l'interface.
    ///
    /// Appelé au démarrage. Avant cela, une question s'enregistre quand même :
    /// elle sera vue au prochain montage de l'interface, qui interroge la
    /// liste. Perdre la question parce que la fenêtre n'existait pas encore
    /// serait pire qu'une pastille qui arrive une seconde plus tard.
    pub async fn brancher(&self, app: AppHandle) {
        *self.app.lock().await = Some(app);
    }

    async fn prevenir(&self) {
        if let Some(app) = self.app.lock().await.as_ref() {
            let _ = app.emit(EVENT, ());
        }
    }

    /// Ce qui attend, du plus ancien au plus récent.
    pub async fn liste(&self) -> Vec<AttentionItem> {
        let etat = self.etat.lock().await;
        etat.ordre
            .iter()
            .filter_map(|id| etat.items.get(id).map(|e| e.item.clone()))
            .collect()
    }

    /// Enregistre une alerte que personne n'attend.
    ///
    /// Pour ce qui est cassé : un modèle éteint, un serveur qui ne répond plus.
    /// Elle n'a pas de réponse à donner, seulement à être vue puis fermée.
    pub async fn alerter(
        &self,
        project_id: Option<String>,
        session_id: Option<String>,
        title: impl Into<String>,
        detail: Option<String>,
    ) -> String {
        let titre = title.into();
        // Deux fois la même panne ne fait pas deux pastilles : le modèle
        // toujours éteint au message suivant produirait sinon une pile
        // d'alertes identiques que personne ne lirait.
        {
            let etat = self.etat.lock().await;
            if let Some(existant) = etat.ordre.iter().find(|id| {
                etat.items.get(*id).is_some_and(|e| {
                    e.item.title == titre
                        && e.item.project_id == project_id
                        // La conversation compte dans la clé : la pastille est
                        // portée par elle, et deux chats libres — tous deux
                        // sans projet — partageraient sinon une seule alerte,
                        // donc une seule pastille sur le premier des deux.
                        && e.item.session_id == session_id
                        && e.item.urgency == "erreur"
                })
            }) {
                return existant.clone();
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let item = AttentionItem {
            id: id.clone(),
            project_id,
            session_id,
            urgency: "erreur".into(),
            title: titre,
            detail,
            choices: Vec::new(),
            free_text: None,
            asked_at: horodatage(),
            blocking: false,
        };
        {
            let mut etat = self.etat.lock().await;
            etat.ordre.push(id.clone());
            etat.items.insert(
                id.clone(),
                EnAttente {
                    item,
                    repondeur: None,
                },
            );
        }
        self.prevenir().await;
        id
    }

    /// Transmet la réponse de l'utilisateur. Faux quand plus rien n'attend
    /// sous cet identifiant — double clic, ou question déjà abandonnée.
    pub async fn repondre(&self, id: &str, reponse: AttentionAnswer) -> bool {
        let entree = {
            let mut etat = self.etat.lock().await;
            etat.ordre.retain(|x| x != id);
            etat.items.remove(id)
        };
        self.prevenir().await;
        match entree {
            Some(EnAttente {
                repondeur: Some(tx),
                ..
            }) => {
                // `send` échoue si l'appelant a disparu entre-temps : ce n'est
                // pas une erreur de l'utilisateur, seulement une réponse
                // arrivée trop tard.
                tx.send(QuestionOutcome::Answered {
                    choice: reponse.choice,
                    text: reponse.text,
                })
                .is_ok()
            }
            // Une alerte : rien à transmettre, mais la fermer a bien eu lieu.
            Some(_) => true,
            None => false,
        }
    }

    /// Retire une question sans y répondre.
    ///
    /// L'appelant l'apprend et continue avec ce qu'il sait. C'est ce qui rend
    /// l'attente sans limite acceptable : elle est toujours interrompable.
    pub async fn abandonner(&self, id: &str) -> bool {
        let entree = {
            let mut etat = self.etat.lock().await;
            etat.ordre.retain(|x| x != id);
            etat.items.remove(id)
        };
        self.prevenir().await;
        match entree {
            Some(EnAttente {
                repondeur: Some(tx),
                ..
            }) => {
                let _ = tx.send(QuestionOutcome::Unanswered {
                    reason: "l'utilisateur a écarté la question sans répondre : continuez avec \
                             l'option la plus prudente, et dites ce que vous avez supposé"
                        .into(),
                });
                true
            }
            Some(_) => true,
            None => false,
        }
    }
}

#[async_trait::async_trait]
impl QuestionGate for Attention {
    /// Soumet une fiche à la personne, et n'écrit que ce qu'elle a choisi.
    ///
    /// Le modèle ne décide pas de la portée. Il ne peut pas : il ne sait pas
    /// si ce qu'il vient d'apprendre est une décision du projet, une exigence
    /// de cette personne, ou un détail de cette machine — et se tromper coûte
    /// dans les deux sens. La question porte donc les portées comme réponses,
    /// et la réponse est ce qui écrit.
    ///
    /// « Ne pas retenir » est une réponse à part entière, et la plus facile à
    /// cliquer par mégarde : elle n'écrit rien, ce qui est le seul choix sans
    /// conséquence.
    async fn propose_context(&self, p: ContextProposal) -> QuestionOutcome {
        let Some(projet) = p.project_id.clone() else {
            return QuestionOutcome::Unanswered {
                reason: "aucun projet ouvert : il n'y a pas de contexte de projet où ranger \
                         cette fiche, gardez l'information dans votre réponse"
                    .into(),
            };
        };
        let repo = self.contexte.lock().await.clone();
        let Some(repo) = repo else {
            return QuestionOutcome::Unanswered {
                reason: "le contexte de projet n'est pas disponible ici".into(),
            };
        };

        // Les portées que cette installation ne peut pas honorer ne sont pas
        // proposées : offrir « tout le projet » sans serveur ferait écrire une
        // fiche que personne ne verrait jamais, sous une étiquette qui
        // mentirait.
        let serveur = self
            .app
            .lock()
            .await
            .is_some()
            .then_some(())
            .and(crate::server_mode::server_status().await.ok())
            .map(|s| s.running)
            .unwrap_or(false);
        let mut choix = Vec::new();
        if serveur {
            choix.push(
                QuestionChoice::new("partage", "Tout le projet")
                    .avec_precision("Une décision du projet, visible de tous."),
            );
            choix.push(
                QuestionChoice::new("compte", "Moi seulement")
                    .avec_precision("Me suit sur mes appareils, invisible aux autres."),
            );
        }
        choix.push(
            QuestionChoice::new("machine", "Cet ordinateur")
                .avec_precision("Ne part jamais d'ici."),
        );
        choix
            .push(QuestionChoice::new("non", "Ne pas retenir").avec_precision("Rien n'est écrit."));

        let reponse = self
            .ask(QuestionRequest {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: p.project_id.clone(),
                session_id: p.session_id.clone(),
                urgency: Urgency::Question,
                title: format!("Retenir « {} » pour ce projet ?", p.title),
                detail: Some(p.detail.clone()),
                choices: choix,
                // Pas de champ libre : les réponses possibles sont exactement
                // les portées, plus le refus. Un texte libre n'aurait pas de
                // portée où être rangé.
                free_text: None,
            })
            .await;

        let QuestionOutcome::Answered { choice, .. } = &reponse else {
            return reponse;
        };
        let Some(portee) = choice.as_deref() else {
            return QuestionOutcome::Unanswered {
                reason: "la question a été fermée sans choisir : rien n'a été retenu".into(),
            };
        };
        if portee == "non" {
            return QuestionOutcome::Answered {
                choice: Some("non".into()),
                text: Some("l'utilisateur a refusé de retenir cette fiche".into()),
            };
        }

        let scope = locaryn_storage::project_context::ContextScope::depuis(portee);
        let auteur = crate::client_cert::current_session()
            .ok()
            .flatten()
            .map(|s| s.username);
        match repo
            .remember(
                &projet,
                scope,
                auteur.as_deref(),
                &p.title,
                &p.detail,
                "assistant",
            )
            .await
        {
            Ok(_) => QuestionOutcome::Answered {
                choice: Some(portee.to_string()),
                text: Some(format!("« {} » est retenu ({portee})", p.title)),
            },
            // Une écriture qui échoue ne doit pas se lire comme un succès :
            // le modèle annoncerait à l'utilisateur une fiche qui n'existe pas.
            Err(e) => QuestionOutcome::Unanswered {
                reason: format!("la fiche n'a pas pu être écrite ({e})"),
            },
        }
    }

    async fn ask(&self, req: QuestionRequest) -> QuestionOutcome {
        let (tx, rx) = oneshot::channel();
        let item = AttentionItem {
            id: req.id.clone(),
            project_id: req.project_id,
            session_id: req.session_id,
            urgency: match req.urgency {
                Urgency::Question => "question".into(),
                Urgency::Erreur => "erreur".into(),
            },
            title: req.title,
            detail: req.detail,
            choices: req
                .choices
                .into_iter()
                .map(|c| AttentionChoice {
                    id: c.id,
                    label: c.label,
                    hint: c.hint,
                })
                .collect(),
            free_text: req.free_text,
            asked_at: horodatage(),
            blocking: true,
        };
        {
            let mut etat = self.etat.lock().await;
            etat.ordre.push(req.id.clone());
            etat.items.insert(
                req.id.clone(),
                EnAttente {
                    item,
                    repondeur: Some(tx),
                },
            );
        }
        self.prevenir().await;

        // Pas de délai. Une question n'expire pas : la personne répond quand
        // elle revient. L'attente est bornée par le bouton « écarter » et par
        // la fermeture de l'application, pas par une minuterie qui répondrait
        // à sa place.
        match rx.await {
            Ok(outcome) => outcome,
            // L'émetteur a été détruit sans envoyer : le registre a été vidé
            // sous nos pieds. Rien de mieux à dire que la vérité.
            Err(_) => QuestionOutcome::Unanswered {
                reason: "la question a disparu avant d'avoir une réponse".into(),
            },
        }
    }
}

fn horodatage() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Sans dépendance de date : l'interface formate, le back-end horodate.
    format!("{s}")
}

#[tauri::command]
pub async fn pending_attention(
    attention: State<'_, Attention>,
) -> Result<Vec<AttentionItem>, String> {
    Ok(attention.liste().await)
}

#[tauri::command]
pub async fn answer_attention(
    attention: State<'_, Attention>,
    id: String,
    answer: AttentionAnswer,
) -> Result<bool, String> {
    Ok(attention.repondre(&id, answer).await)
}

#[tauri::command]
pub async fn dismiss_attention(
    attention: State<'_, Attention>,
    id: String,
) -> Result<bool, String> {
    Ok(attention.abandonner(&id).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(id: &str, projet: Option<&str>) -> QuestionRequest {
        QuestionRequest {
            id: id.into(),
            project_id: projet.map(str::to_string),
            session_id: None,
            urgency: Urgency::Question,
            title: "Le rendu final est-il en A2 ou en A3 ?".into(),
            detail: None,
            choices: vec![
                QuestionChoice::new("a2", "A2"),
                QuestionChoice::new("a3", "A3"),
            ],
            free_text: Some("Autre réponse".into()),
        }
    }

    /// Le cas nominal : on pose, la personne répond, l'appelant reçoit.
    #[tokio::test]
    async fn une_reponse_parvient_a_celui_qui_attend() {
        let a = Attention::new();
        let porte = a.clone();
        let attente = tokio::spawn(async move { porte.ask(question("q1", Some("p1"))).await });

        // La question doit être visible avant qu'on y réponde, sinon
        // l'interface ne saurait pas qu'il y a quelque chose à afficher.
        let mut liste = a.liste().await;
        for _ in 0..50 {
            if !liste.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            liste = a.liste().await;
        }
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].project_id.as_deref(), Some("p1"));
        assert!(liste[0].blocking, "quelqu'un attend cette réponse");

        assert!(
            a.repondre(
                "q1",
                AttentionAnswer {
                    choice: Some("a2".into()),
                    text: None,
                }
            )
            .await
        );

        let outcome = attente.await.unwrap();
        assert!(outcome.is_answered());
        assert!(outcome.pour_le_modele().contains("a2"));
        assert!(
            a.liste().await.is_empty(),
            "une question réglée ne doit plus attendre"
        );
    }

    /// Écarter n'est pas répondre. Le motif doit dire à l'appelant quoi faire,
    /// sinon il attendrait une réponse qui n'arrivera pas.
    #[tokio::test]
    async fn ecarter_une_question_la_termine_sans_reponse() {
        let a = Attention::new();
        let porte = a.clone();
        let attente = tokio::spawn(async move { porte.ask(question("q2", None)).await });
        for _ in 0..50 {
            if !a.liste().await.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(a.abandonner("q2").await);
        let outcome = attente.await.unwrap();
        assert!(!outcome.is_answered());
        assert!(
            outcome.pour_le_modele().contains("prudente"),
            "le motif doit dire quoi faire, pas seulement que ça a échoué"
        );
    }

    /// Deux fois la même panne ne fait pas deux pastilles.
    #[tokio::test]
    async fn une_alerte_identique_ne_se_duplique_pas() {
        let a = Attention::new();
        let un = a
            .alerter(
                Some("p1".into()),
                None,
                "Le modèle ne répond plus",
                Some("llama-server est arrêté".into()),
            )
            .await;
        let deux = a
            .alerter(Some("p1".into()), None, "Le modèle ne répond plus", None)
            .await;
        assert_eq!(un, deux, "la même alerte revient sur la même pastille");
        assert_eq!(a.liste().await.len(), 1);

        // Un autre projet, en revanche, mérite la sienne : la panne y est une
        // information distincte.
        a.alerter(Some("p2".into()), None, "Le modèle ne répond plus", None)
            .await;
        assert_eq!(a.liste().await.len(), 2);

        // Et deux conversations distinctes aussi, y compris sans projet : la
        // pastille est portée par la conversation, pas par la panne.
        a.alerter(None, Some("s1".into()), "Le modèle ne répond plus", None)
            .await;
        a.alerter(None, Some("s2".into()), "Le modèle ne répond plus", None)
            .await;
        assert_eq!(a.liste().await.len(), 4);
    }

    /// Une alerte se ferme, alors que personne n'attend derrière.
    #[tokio::test]
    async fn une_alerte_se_ferme_sans_appelant() {
        let a = Attention::new();
        let id = a.alerter(None, None, "Serveur injoignable", None).await;
        assert_eq!(a.liste().await[0].urgency, "erreur");
        assert!(!a.liste().await[0].blocking, "personne n'attend une alerte");
        assert!(a.repondre(&id, AttentionAnswer::default()).await);
        assert!(a.liste().await.is_empty());
    }

    /// Répondre deux fois ne doit pas paniquer : le second clic constate
    /// simplement qu'il n'y a plus rien.
    #[tokio::test]
    async fn repondre_deux_fois_est_sans_effet() {
        let a = Attention::new();
        let id = a.alerter(None, None, "Serveur injoignable", None).await;
        assert!(a.repondre(&id, AttentionAnswer::default()).await);
        assert!(!a.repondre(&id, AttentionAnswer::default()).await);
        assert!(!a.abandonner(&id).await);
    }

    /// L'ordre d'arrivée est conservé : la plus ancienne question se règle
    /// d'abord, sinon la bande changerait de contenu à chaque nouvelle.
    #[tokio::test]
    async fn l_ordre_d_arrivee_est_conserve() {
        let a = Attention::new();
        a.alerter(None, None, "Première", None).await;
        a.alerter(None, None, "Deuxième", None).await;
        a.alerter(None, None, "Troisième", None).await;
        let titres: Vec<String> = a.liste().await.into_iter().map(|i| i.title).collect();
        assert_eq!(titres, vec!["Première", "Deuxième", "Troisième"]);
    }

    /// Sans interface branchée, une question s'enregistre quand même : elle
    /// sera vue au prochain montage. La perdre serait pire.
    #[tokio::test]
    async fn sans_interface_la_question_est_quand_meme_enregistree() {
        let a = Attention::new();
        let id = a.alerter(None, None, "Rien de branché", None).await;
        assert_eq!(a.liste().await.len(), 1);
        assert!(a.repondre(&id, AttentionAnswer::default()).await);
    }
}
