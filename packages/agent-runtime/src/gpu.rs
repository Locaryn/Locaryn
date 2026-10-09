//! Les outils de morph qui chargent **leur propre modèle** sur la carte
//! graphique (image, voix, dictée).
//!
//! Le modèle de conversation occupe déjà la carte : sur 6 Go, un modèle de
//! diffusion ne tient pas à côté — il déborde en RAM (une demi-heure au lieu
//! de douze secondes) ou échoue. L'hôte arbitre donc, appel par appel :
//!
//! 1. vérifier l'appel en entier — schéma, puis le morph lui-même à blanc —
//!    pendant que le modèle de conversation est encore là pour se corriger ;
//! 2. décharger le modèle de conversation si la carte n'a pas la place ;
//! 3. laisser le morph travailler (il libère sa mémoire en finissant) ;
//! 4. recharger le modèle de conversation avant la requête suivante — une
//!    seule fois pour plusieurs appels d'affilée.
//!
//! L'arbitrage lui-même vit dans l'hôte, qui connaît le moteur et les
//! manifestes ; la boucle d'outils ne fait qu'appeler aux bons moments.

use locaryn_events::{LogLevel, StreamEvent};
use std::sync::Arc;

/// Ce qu'un appel va produire, pour que l'interface réserve sa place.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaPlan {
    /// `image`, `audio`, `video`.
    pub kind: String,
    pub count: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// L'appel vérifié, prêt à partir.
#[derive(Debug, Clone)]
pub struct Preparation {
    /// Les arguments à envoyer : ceux du modèle, chemins relatifs résolus.
    pub args: serde_json::Value,
    pub media: Option<MediaPlan>,
}

/// Dire où l'on en est : sur la carte de l'outil quand il y en a une, sinon
/// dans le journal de la conversation.
pub struct Annonce<'a> {
    tx: Option<&'a tokio::sync::mpsc::Sender<StreamEvent>>,
    call_id: Option<&'a str>,
}

impl<'a> Annonce<'a> {
    pub fn pour_l_outil(tx: &'a tokio::sync::mpsc::Sender<StreamEvent>, call_id: &'a str) -> Self {
        Self {
            tx: Some(tx),
            call_id: Some(call_id),
        }
    }

    pub fn dans_le_journal(tx: &'a tokio::sync::mpsc::Sender<StreamEvent>) -> Self {
        Self {
            tx: Some(tx),
            call_id: None,
        }
    }

    /// Sans conversation (le Studio d'une extension) : le journal de l'hôte.
    pub fn muette() -> Self {
        Self {
            tx: None,
            call_id: None,
        }
    }

    pub async fn dire(&self, etape: &str) {
        tracing::info!(etape, "arbitrage de la carte graphique");
        let Some(tx) = self.tx else {
            return;
        };
        let evenement = match self.call_id {
            Some(id) => StreamEvent::TaskUpdate {
                task_id: id.to_string(),
                status: etape.to_string(),
                progress: 0.0,
            },
            None => StreamEvent::Log {
                level: LogLevel::Info,
                msg: etape.to_string(),
                source: "gpu".into(),
            },
        };
        if tx.send(evenement).await.is_err() {
            tracing::debug!("plus personne n'écoute l'avancement");
        }
    }
}

/// Implémenté par l'hôte.
#[async_trait::async_trait]
pub trait GpuArbiter: Send + Sync {
    /// Ce que produit l'outil s'il charge son propre modèle (`image`,
    /// `audio`…) ; `None` pour un outil ordinaire, qui passe sans arbitrage.
    async fn claims(&self, tool: &str) -> Option<String>;

    /// Vérifie l'appel et fait de la place. `Err` : l'appel est refusé tel
    /// quel au modèle, **rien n'a été déchargé**.
    async fn prepare(
        &self,
        tool: &str,
        args: &serde_json::Value,
        project: &std::path::Path,
        annonce: &Annonce<'_>,
    ) -> Result<Preparation, String>;

    /// Recharge le modèle de conversation s'il a été déchargé. Sans effet
    /// sinon : la boucle l'appelle avant chaque requête.
    async fn restore(&self, annonce: &Annonce<'_>) -> Result<(), String>;
}

#[derive(Clone)]
pub struct GpuArbiterHandle(pub Arc<dyn GpuArbiter>);

impl std::fmt::Debug for GpuArbiterHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GpuArbiter(présent)")
    }
}

impl GpuArbiterHandle {
    pub fn new(arbitre: impl GpuArbiter + 'static) -> Self {
        Self(Arc::new(arbitre))
    }
}

/// Lire la réponse à blanc d'un morph (`__locaryn_preflight`).
///
/// `{ "ready": true, "vram_gb": 2.1, "media": {…}, "summary": "…" }` ; un
/// refus porte `ready: false` et `problem`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preflight {
    pub vram_gb: f64,
    pub media: Option<MediaPlan>,
    pub summary: String,
}

/// Le drapeau que l'hôte ajoute aux arguments pour l'appel à blanc.
pub const PREFLIGHT_FLAG: &str = "__locaryn_preflight";

pub fn lire_preflight(reponse: &serde_json::Value) -> Result<Preflight, String> {
    // Le texte rendu par un serveur MCP, ou l'objet directement.
    let objet = reponse
        .as_str()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        .unwrap_or_else(|| reponse.clone());
    if objet.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(objet
            .get("problem")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("le morph n'a pas confirmé que l'appel était prêt")
            .to_string());
    }
    let media = objet.get("media").and_then(|m| {
        Some(MediaPlan {
            kind: m.get("kind")?.as_str()?.to_string(),
            count: m
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .map_or(1, |c| c.clamp(1, 16) as u32),
            width: m
                .get("width")
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32),
            height: m
                .get("height")
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as u32),
        })
    });
    Ok(Preflight {
        vram_gb: objet
            .get("vram_gb")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        media,
        summary: objet
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn une_reponse_prete_donne_le_plan() {
        let texte = json!(
            r#"{"ready":true,"vram_gb":2.5,"summary":"SD 1.5","media":{"kind":"image","count":4,"width":512,"height":768}}"#
        );
        let p = lire_preflight(&texte).unwrap();
        assert_eq!(p.vram_gb, 2.5);
        assert_eq!(
            p.media,
            Some(MediaPlan {
                kind: "image".into(),
                count: 4,
                width: Some(512),
                height: Some(768)
            })
        );
    }

    #[test]
    fn un_refus_rend_le_probleme_du_morph() {
        let e = lire_preflight(&json!({ "ready": false, "problem": "aucun modèle installé" }))
            .unwrap_err();
        assert_eq!(e, "aucun modèle installé");
        // Un serveur qui ne connaît pas l'appel à blanc ne passe pas pour prêt.
        assert!(lire_preflight(&json!({ "paths": [] })).is_err());
    }
}
