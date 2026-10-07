//! Les permissions d'une conversation, relues à chaque appel d'outil.
//!
//! La boucle d'outils recevait la permission au moment de l'envoi et la
//! gardait jusqu'à la fin de la tâche. Changer « Prudent » en « Autonome »
//! pendant que le modèle travaillait ne servait à rien : il fallait
//! l'interrompre et renvoyer un message, ou valider une à une des dizaines de
//! demandes. L'hôte fournit ici la permission courante ; la boucle la relit
//! avant chaque décision d'approbation, dans un sens comme dans l'autre.

use locaryn_shared_types::TrustLevel;
use std::sync::Arc;

/// Où lire la permission courante. Implémenté par l'application.
#[async_trait::async_trait]
pub trait TrustSource: Send + Sync {
    /// La permission en vigueur maintenant ; `None` si elle est illisible
    /// (la boucle garde alors la dernière connue).
    async fn current(&self) -> Option<TrustLevel>;
}

/// Enveloppe la source pour qu'elle traverse une structure `Debug`.
#[derive(Clone)]
pub struct TrustSourceHandle(pub Arc<dyn TrustSource>);

impl std::fmt::Debug for TrustSourceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TrustSource(présente)")
    }
}

impl TrustSourceHandle {
    pub fn new(source: impl TrustSource + 'static) -> Self {
        Self(Arc::new(source))
    }

    /// La permission à appliquer : la courante, sinon `derniere`.
    pub async fn refresh(&self, derniere: TrustLevel) -> TrustLevel {
        self.0.current().await.unwrap_or(derniere)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Reglage(Mutex<Option<TrustLevel>>);

    #[async_trait::async_trait]
    impl TrustSource for Reglage {
        async fn current(&self) -> Option<TrustLevel> {
            *self.0.lock().unwrap()
        }
    }

    #[tokio::test]
    async fn un_changement_en_cours_de_tache_s_applique_au_prochain_appel() {
        let reglage = Arc::new(Reglage(Mutex::new(Some(TrustLevel::Untrusted))));
        let source = TrustSourceHandle(reglage.clone());
        assert_eq!(
            source.refresh(TrustLevel::Sandbox).await,
            TrustLevel::Untrusted
        );
        *reglage.0.lock().unwrap() = Some(TrustLevel::Autonomous);
        assert_eq!(
            source.refresh(TrustLevel::Untrusted).await,
            TrustLevel::Autonomous
        );
        // Illisible : la dernière connue reste.
        *reglage.0.lock().unwrap() = None;
        assert_eq!(
            source.refresh(TrustLevel::Autonomous).await,
            TrustLevel::Autonomous
        );
    }
}
