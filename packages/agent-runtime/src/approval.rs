//! Soumettre un appel d'outil à l'utilisateur, et attendre sa réponse.
//!
//! Le runtime ne sait pas comment demander : il n'a ni fenêtre, ni terminal,
//! ni utilisateur. Il sait seulement qu'un appel exige un consentement. C'est
//! l'hôte — application de bureau, démon, test — qui fournit le moyen de
//! poser la question, à travers [`ApprovalGate`].
//!
//! Sans porte, la réponse est **refus**. C'est délibéré : un runtime intégré
//! dans un service sans interface ne doit pas exécuter d'opération sensible
//! sous prétexte que personne n'était là pour dire non.

use locaryn_events::Risk;
use std::sync::Arc;

/// Ce qu'on demande à l'utilisateur d'arbitrer.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub call_id: String,
    pub tool: String,
    pub args: serde_json::Value,
    pub risk: Risk,
    /// Pourquoi le runtime demande, dans les mots montrés à l'écran.
    pub reason: String,
    /// Aperçu de la modification (diff, ligne de commande). `None` pour les
    /// outils en lecture seule.
    pub diff: Option<String>,
    /// L'appel franchit la limite de la machine.
    pub is_remote: bool,
    /// Le projet dans lequel l'appel a lieu. Sans lui, une décision « ce
    /// projet » ne saurait pas de quel projet il s'agit et vaudrait
    /// silencieusement « partout » — l'écart le plus dangereux entre ce que la
    /// fenêtre promet et ce qu'elle fait.
    pub project_id: uuid::Uuid,
}

/// La réponse. `Deny` porte le motif tel qu'il sera renvoyé au modèle : ce
/// qu'il lit détermine s'il réessaie autrement ou s'il abandonne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Allow,
    Deny { reason: String },
}

impl ApprovalOutcome {
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        matches!(self, ApprovalOutcome::Allow)
    }

    /// Refus par défaut, employé partout où personne ne peut répondre.
    #[must_use]
    pub fn no_one_to_ask() -> Self {
        ApprovalOutcome::Deny {
            reason: "aucune interface n'est disponible pour demander votre accord".to_string(),
        }
    }
}

/// Le moyen de poser la question. Implémenté par l'hôte.
#[async_trait::async_trait]
pub trait ApprovalGate: Send + Sync {
    /// Demande, et attend. L'implémentation est responsable de ne pas attendre
    /// indéfiniment : une fenêtre fermée sans répondre doit finir en refus,
    /// sinon la conversation reste bloquée sans que rien ne l'explique.
    async fn request(&self, req: ApprovalRequest) -> ApprovalOutcome;
}

/// Enveloppe la porte pour qu'elle traverse une structure `Debug`.
///
/// Un objet-trait ne sait pas se décrire, et exiger `Debug` de toute
/// implémentation contaminerait les hôtes pour un besoin de journalisation.
/// La sortie dit ce qui compte : y a-t-il quelqu'un à qui demander.
#[derive(Clone)]
pub struct ApprovalHandle(pub Arc<dyn ApprovalGate>);

impl std::fmt::Debug for ApprovalHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApprovalGate(présente)")
    }
}

impl ApprovalHandle {
    pub fn new(gate: impl ApprovalGate + 'static) -> Self {
        Self(Arc::new(gate))
    }
}

/// Porte qui refuse tout, sans demander à personne.
///
/// C'est le comportement d'un hôte sans interface. Elle existe comme type
/// nommé plutôt que comme `Option::None` implicite pour que le refus soit un
/// choix visible dans le code qui la construit.
pub struct DenyAll;

#[async_trait::async_trait]
impl ApprovalGate for DenyAll {
    async fn request(&self, _req: ApprovalRequest) -> ApprovalOutcome {
        ApprovalOutcome::no_one_to_ask()
    }
}

/// Interroge la porte quand il y en a une, refuse sinon.
///
/// Publique : le pont de noyaux alternatifs (`core-bridge`) relaye les
/// approbations du noyau par le même chemin que la boucle locale.
pub async fn ask(gate: Option<&ApprovalHandle>, req: ApprovalRequest) -> ApprovalOutcome {
    match gate {
        Some(g) => g.0.request(req).await,
        None => ApprovalOutcome::no_one_to_ask(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ApprovalRequest {
        ApprovalRequest {
            call_id: "c1".into(),
            tool: "write_file".into(),
            args: serde_json::json!({}),
            risk: Risk::High,
            reason: "écrit un fichier".into(),
            diff: None,
            is_remote: false,
            project_id: uuid::Uuid::nil(),
        }
    }

    /// Sans porte, on refuse. Un service sans interface ne doit pas exécuter
    /// une opération sensible faute d'interlocuteur.
    #[tokio::test]
    async fn absence_de_porte_vaut_refus() {
        let outcome = ask(None, req()).await;
        assert!(!outcome.is_allowed());
        match outcome {
            ApprovalOutcome::Deny { reason } => assert!(!reason.is_empty(), "un refus s'explique"),
            ApprovalOutcome::Allow => unreachable!(),
        }
    }

    #[tokio::test]
    async fn deny_all_refuse_aussi() {
        let gate = ApprovalHandle::new(DenyAll);
        assert!(!ask(Some(&gate), req()).await.is_allowed());
    }

    /// Une porte qui accepte doit vraiment laisser passer : sans ce test, une
    /// erreur de branchement transformerait tout en refus silencieux, et la
    /// fonctionnalité paraîtrait « sûre » alors qu'elle serait morte.
    #[tokio::test]
    async fn une_porte_permissive_laisse_passer() {
        struct Oui;
        #[async_trait::async_trait]
        impl ApprovalGate for Oui {
            async fn request(&self, _r: ApprovalRequest) -> ApprovalOutcome {
                ApprovalOutcome::Allow
            }
        }
        let gate = ApprovalHandle::new(Oui);
        assert!(ask(Some(&gate), req()).await.is_allowed());
    }
}
