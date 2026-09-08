//! Poser une question à l'utilisateur, et attendre sa réponse sans rien bloquer.
//!
//! C'est le pendant de [`crate::approval`], et la différence est entière :
//!
//! * Une **approbation** arbitre un acte. Faute d'interlocuteur, on refuse —
//!   personne pour dire non ne vaut pas oui.
//! * Une **question** lève un doute. Faute d'interlocuteur, il n'y a pas de
//!   réponse, et c'est tout : le modèle l'apprend et continue avec ce qu'il
//!   sait. Refuser de travailler parce qu'on n'a pas pu demander serait une
//!   panne, pas une précaution.
//!
//! Le corollaire tient à la durée. Une fenêtre d'approbation expire en cinq
//! minutes parce qu'un appel d'outil attend derrière. Une question, elle, n'a
//! pas de raison d'expirer : l'utilisateur n'est peut-être pas devant l'écran,
//! il regarde peut-être un autre projet. L'hôte décide donc lui-même de sa
//! patience, et peut choisir de ne pas en avoir de limite — à charge pour lui
//! de rendre l'attente visible, et abandonnable.
//!
//! Rien ici ne suppose du code. Une question porte aussi bien sur le format
//! d'un rendu, la température d'une mesure ou la personne qui doit voir une
//! note.

use std::sync::Arc;

/// Une réponse proposée à l'utilisateur.
///
/// `id` est ce que le modèle relira ; `label` est ce que la personne lit. Les
/// séparer évite l'étiquette qui change de langue et casse le raisonnement en
/// aval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionChoice {
    pub id: String,
    pub label: String,
    /// Ce que ce choix implique, en une ligne. Un intitulé seul se devine mal.
    pub hint: Option<String>,
}

impl QuestionChoice {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            hint: None,
        }
    }

    #[must_use]
    pub fn avec_precision(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Ce qui demande l'attention de l'utilisateur.
///
/// Deux niveaux, et un seul les distingue : peut-on continuer sans réponse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// Un doute. Le travail attend, rien n'est cassé.
    Question,
    /// Quelque chose ne marche plus — un modèle éteint, un serveur injoignable.
    /// Répondre ne suffira pas, il faut agir.
    Erreur,
}

/// La question posée.
#[derive(Debug, Clone)]
pub struct QuestionRequest {
    /// Identifiant de cette question, pour la retrouver et y répondre.
    pub id: String,
    /// Le projet concerné. C'est lui qui se met en pause, pas l'application.
    pub project_id: Option<String>,
    /// La conversation d'où vient la question, quand il y en a une : c'est là
    /// que la pastille doit apparaître.
    pub session_id: Option<String>,
    pub urgency: Urgency,
    /// La question, en une phrase.
    pub title: String,
    /// Ce qui est en jeu, si l'expliquer aide à choisir.
    pub detail: Option<String>,
    /// Les réponses proposées. Peut être vide : une alerte n'a rien à choisir.
    pub choices: Vec<QuestionChoice>,
    /// L'invite du champ libre, quand une réponse hors des propositions a un
    /// sens. `None` retire le champ — mieux vaut pas de champ qu'un champ dont
    /// la réponse serait ignorée.
    pub free_text: Option<String>,
}

/// Ce que l'utilisateur a répondu — ou pourquoi il n'a pas répondu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionOutcome {
    Answered {
        /// L'`id` du choix retenu, quand la personne a pris une proposition.
        choice: Option<String>,
        /// Ce qu'elle a écrit, quand elle a préféré ses propres mots.
        text: Option<String>,
    },
    /// Personne à qui demander, question abandonnée, ou attente écoulée.
    Unanswered { reason: String },
}

impl QuestionOutcome {
    #[must_use]
    pub fn is_answered(&self) -> bool {
        matches!(self, QuestionOutcome::Answered { .. })
    }

    /// Aucun moyen de poser la question ici.
    ///
    /// Employé par les hôtes sans interface : un démon en service, un test.
    /// Le motif part au modèle tel quel, donc il doit lui dire quoi faire —
    /// pas seulement que ça a échoué.
    #[must_use]
    pub fn no_one_to_ask() -> Self {
        QuestionOutcome::Unanswered {
            reason: "aucune interface ne permet de poser la question ici : décidez avec ce que \
                     vous savez, en choisissant l'option la plus prudente, et dites dans votre \
                     réponse ce que vous avez supposé"
                .to_string(),
        }
    }

    /// Rend la réponse dans les mots que le modèle relira.
    #[must_use]
    pub fn pour_le_modele(&self) -> String {
        match self {
            QuestionOutcome::Answered { choice, text } => match (choice, text) {
                (Some(c), Some(t)) if !t.is_empty() => {
                    format!("L'utilisateur a répondu « {c} » et a précisé : {t}")
                }
                (Some(c), _) => format!("L'utilisateur a répondu « {c} »."),
                (None, Some(t)) if !t.is_empty() => {
                    format!("L'utilisateur a répondu de lui-même : {t}")
                }
                // Ni choix ni texte : la porte a rendu une réponse vide. Le
                // dire vaut mieux que de fabriquer un accord.
                (None, _) => "L'utilisateur a fermé la question sans répondre.".to_string(),
            },
            QuestionOutcome::Unanswered { reason } => {
                format!("Pas de réponse : {reason}")
            }
        }
    }
}

/// Le moyen de poser la question. Implémenté par l'hôte.
#[async_trait::async_trait]
pub trait QuestionGate: Send + Sync {
    /// Pose, et attend.
    ///
    /// L'implémentation ne doit **pas** bloquer l'interface : la personne doit
    /// pouvoir continuer à naviguer, ouvrir un autre projet, lire une autre
    /// conversation. Elle peut en revanche attendre aussi longtemps qu'il
    /// faut — à condition que l'attente se voie, et puisse être abandonnée.
    async fn ask(&self, req: QuestionRequest) -> QuestionOutcome;
}

/// Enveloppe la porte pour qu'elle traverse une structure `Debug`.
#[derive(Clone)]
pub struct QuestionHandle(pub Arc<dyn QuestionGate>);

impl std::fmt::Debug for QuestionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QuestionGate(présente)")
    }
}

impl QuestionHandle {
    pub fn new(gate: impl QuestionGate + 'static) -> Self {
        Self(Arc::new(gate))
    }
}

/// Interroge la porte quand il y en a une, et le dit sinon.
pub async fn ask(gate: Option<&QuestionHandle>, req: QuestionRequest) -> QuestionOutcome {
    match gate {
        Some(g) => g.0.ask(req).await,
        None => QuestionOutcome::no_one_to_ask(),
    }
}

/// Lit l'appel d'outil `ask_user` du modèle.
///
/// Sépare la lecture des arguments de l'attente : la première se teste sans
/// porte ni utilisateur, la seconde exige les deux.
///
/// Renvoie l'erreur telle qu'elle partira au modèle. Une question sans énoncé
/// n'est pas rattrapable : l'inventer afficherait à l'utilisateur une question
/// que personne n'a posée.
pub fn lire_appel(
    args: &serde_json::Value,
    project_id: Option<String>,
    session_id: Option<String>,
) -> Result<QuestionRequest, String> {
    let title = args
        .get("question")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if title.is_empty() {
        return Err("`question` est obligatoire : dites ce que vous demandez.".to_string());
    }

    let detail = args
        .get("detail")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Les propositions arrivent en chaînes ou en objets `{label, hint}` : les
    // modèles font les deux, et refuser l'une des formes coûterait un
    // aller-retour pour rien.
    let mut choices = Vec::new();
    if let Some(list) = args.get("options").and_then(|v| v.as_array()) {
        for (i, item) in list.iter().enumerate() {
            let (label, hint) = match item {
                serde_json::Value::String(s) => (s.trim().to_string(), None),
                serde_json::Value::Object(_) => (
                    item.get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    item.get("hint")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                ),
                _ => continue,
            };
            if label.is_empty() {
                continue;
            }
            // L'`id` du choix est son intitulé quand le modèle n'en donne pas :
            // c'est ce qu'il relira, et c'est déjà lisible.
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| label.clone());
            let _ = i;
            choices.push(QuestionChoice { id, label, hint });
        }
    }

    // Le champ libre par défaut. Sans lui, une question dont aucune proposition
    // ne convient n'aurait aucune réponse honnête.
    let libre = args
        .get("allow_free_text")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(QuestionRequest {
        id: uuid::Uuid::new_v4().to_string(),
        project_id,
        session_id,
        urgency: Urgency::Question,
        title,
        detail,
        choices,
        free_text: if libre {
            Some("Autre réponse".to_string())
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sans porte, on n'invente pas de réponse — et le motif dit au modèle quoi
    /// faire, sinon il resterait bloqué sur un échec qu'il ne sait pas traiter.
    #[tokio::test]
    async fn absence_de_porte_ne_vaut_pas_reponse() {
        let r = ask(
            None,
            lire_appel(&serde_json::json!({"question": "A2 ou A3 ?"}), None, None).unwrap(),
        )
        .await;
        assert!(!r.is_answered());
        match &r {
            QuestionOutcome::Unanswered { reason } => {
                assert!(reason.contains("prudente"), "le motif doit dire quoi faire");
            }
            QuestionOutcome::Answered { .. } => unreachable!(),
        }
    }

    #[tokio::test]
    async fn une_porte_qui_repond_transmet_le_choix() {
        struct Repond;
        #[async_trait::async_trait]
        impl QuestionGate for Repond {
            async fn ask(&self, _r: QuestionRequest) -> QuestionOutcome {
                QuestionOutcome::Answered {
                    choice: Some("partage".into()),
                    text: None,
                }
            }
        }
        let gate = QuestionHandle::new(Repond);
        let r = ask(
            Some(&gate),
            lire_appel(
                &serde_json::json!({"question": "Qui doit voir ?"}),
                None,
                None,
            )
            .unwrap(),
        )
        .await;
        assert!(r.pour_le_modele().contains("partage"));
    }

    /// Une question sans énoncé est refusée. L'inventer afficherait à l'écran
    /// une question que personne n'a posée.
    #[test]
    fn une_question_vide_est_refusee() {
        assert!(lire_appel(&serde_json::json!({}), None, None).is_err());
        assert!(lire_appel(&serde_json::json!({"question": "   "}), None, None).is_err());
    }

    /// Les deux formes de proposition sont acceptées : les modèles écrivent
    /// tantôt des chaînes, tantôt des objets.
    #[test]
    fn les_propositions_acceptent_les_deux_formes() {
        let q = lire_appel(
            &serde_json::json!({
                "question": "Qui doit voir cette note ?",
                "options": [
                    "Tout le projet",
                    {"id": "compte", "label": "Moi seulement", "hint": "Sur tous mes appareils"},
                    {"label": ""},
                    42
                ]
            }),
            Some("p1".into()),
            None,
        )
        .unwrap();
        assert_eq!(
            q.choices.len(),
            2,
            "l'intitulé vide et le nombre sont ignorés"
        );
        assert_eq!(
            q.choices[0].id, "Tout le projet",
            "l'id retombe sur l'intitulé"
        );
        assert_eq!(q.choices[1].id, "compte");
        assert_eq!(
            q.choices[1].hint.as_deref(),
            Some("Sur tous mes appareils"),
            "la précision suit le choix"
        );
        assert_eq!(q.project_id.as_deref(), Some("p1"));
    }

    /// Le champ libre est là par défaut : c'est la troisième option de toute
    /// question, celle qui existe quand aucune proposition ne convient.
    #[test]
    fn le_champ_libre_est_present_sauf_refus_explicite() {
        let q = lire_appel(&serde_json::json!({"question": "Quoi ?"}), None, None).unwrap();
        assert!(q.free_text.is_some());
        let q = lire_appel(
            &serde_json::json!({"question": "Quoi ?", "allow_free_text": false}),
            None,
            None,
        )
        .unwrap();
        assert!(q.free_text.is_none());
    }

    /// Une porte qui rend une réponse vide ne doit pas se lire comme un accord.
    #[test]
    fn une_reponse_vide_se_dit_comme_telle() {
        let r = QuestionOutcome::Answered {
            choice: None,
            text: None,
        };
        assert!(r.pour_le_modele().contains("sans répondre"));
        let r = QuestionOutcome::Answered {
            choice: None,
            text: Some(String::new()),
        };
        assert!(
            r.pour_le_modele().contains("sans répondre"),
            "un texte vide n'est pas une réponse"
        );
    }

    #[test]
    fn un_choix_et_une_precision_arrivent_ensemble_au_modele() {
        let r = QuestionOutcome::Answered {
            choice: Some("compte".into()),
            text: Some("mais pas sur le portable".into()),
        };
        let dit = r.pour_le_modele();
        assert!(dit.contains("compte"));
        assert!(dit.contains("portable"));
    }
}
