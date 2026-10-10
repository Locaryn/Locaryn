//! Les messages que la personne remet au modèle pendant qu'il travaille.
//!
//! Sans elle, une correction tapée en cours de tâche attendait la fin : le
//! modèle finissait sur une mauvaise piste, puis il fallait lui faire défaire
//! ce qu'il venait de faire. L'hôte dépose ici ce qui doit partir tout de
//! suite ; la boucle d'outils le relève entre deux étapes, après les résultats
//! d'outils, et le donne au modèle comme un message de la personne.

use std::sync::Arc;

/// Un message remis en cours de tâche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Courrier {
    /// Identifiant choisi par l'interface, renvoyé quand le message est lu.
    pub id: String,
    pub text: String,
}

/// Où relever les messages en attente. Implémenté par l'application.
#[async_trait::async_trait]
pub trait Mailbox: Send + Sync {
    /// Les messages déposés depuis le dernier relevé, dans l'ordre. Les rendre
    /// les retire de la boîte : l'hôte les considère comme lus.
    async fn relever(&self) -> Vec<Courrier>;
}

/// Enveloppe la boîte pour qu'elle traverse une structure `Debug`.
#[derive(Clone)]
pub struct MailboxHandle(pub Arc<dyn Mailbox>);

impl std::fmt::Debug for MailboxHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Mailbox(présente)")
    }
}

impl MailboxHandle {
    pub fn new(boite: impl Mailbox + 'static) -> Self {
        Self(Arc::new(boite))
    }

    /// Relève la boîte et ajoute chaque message à la conversation envoyée au
    /// modèle. Rend les identifiants des messages ajoutés.
    pub async fn verser(&self, messages: &mut serde_json::Value) -> Vec<String> {
        let courrier = self.0.relever().await;
        let Some(liste) = messages.as_array_mut() else {
            return Vec::new();
        };
        for c in &courrier {
            liste.push(serde_json::json!({ "role": "user", "content": c.text }));
        }
        courrier.into_iter().map(|c| c.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Boite(Mutex<Vec<Courrier>>);

    #[async_trait::async_trait]
    impl Mailbox for Boite {
        async fn relever(&self) -> Vec<Courrier> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    #[tokio::test]
    async fn le_courrier_rejoint_la_conversation_une_seule_fois() {
        let boite = MailboxHandle::new(Boite(Mutex::new(vec![
            Courrier {
                id: "a".into(),
                text: "plutôt en bleu".into(),
            },
            Courrier {
                id: "b".into(),
                text: "et sans bordure".into(),
            },
        ])));
        let mut messages = serde_json::json!([{ "role": "tool", "content": "ok" }]);
        assert_eq!(boite.verser(&mut messages).await, vec!["a", "b"]);
        assert_eq!(messages[1]["content"], "plutôt en bleu");
        assert_eq!(messages[2]["role"], "user");
        // Déjà lu : rien ne repart au relevé suivant.
        assert!(boite.verser(&mut messages).await.is_empty());
        assert_eq!(messages.as_array().unwrap().len(), 3);
    }
}
