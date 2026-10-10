//! L'arbitre de la carte tel que le bureau et le démon l'emploient.
//!
//! L'arbitrage est le même partout : localiser l'outil parmi ceux que les
//! manifestes déclarent, vérifier l'appel (schéma strict, puis appel à blanc
//! du morph), libérer la carte si la place manque, relancer le modèle de
//! conversation avant la requête suivante. Seuls changent la façon de lister
//! ces outils et celle d'arrêter ou relancer le moteur : l'hôte les fournit.

use crate::gpu::{lire_preflight, Annonce, GpuArbiter, Preparation, PREFLIGHT_FLAG};
use locaryn_mcp::{McpClient, McpState};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

/// Par serveur MCP : ses outils à modèle propre, et ce que chacun produit.
pub type GpuToolMap = HashMap<String, BTreeMap<String, Option<String>>>;

/// Ce que l'hôte sait du moteur de conversation.
#[async_trait::async_trait]
pub trait ConversationEngine: Send + Sync {
    /// L'identifiant du moteur de conversation s'il tourne sur cette machine
    /// (et donc partage la carte) ; `None` pour un moteur distant ou arrêté.
    async fn local_running(&self) -> Option<String>;
    async fn stop(&self, moteur: &str);
    /// Relance le moteur avec son modèle ; revient quand il répond.
    async fn start(&self, moteur: &str) -> Result<(), String>;
}

/// Où l'hôte lit la section `gpu` des manifestes actifs.
#[async_trait::async_trait]
pub trait GpuToolSource: Send + Sync {
    async fn gpu_tools(&self) -> GpuToolMap;
}

/// Le moteur arrêté pour laisser travailler un morph. Un seul par processus :
/// le Studio d'une extension et la conversation passent par le même verrou,
/// qui fait attendre un second rechargement que le premier finisse.
static DECHARGE: LazyLock<tokio::sync::Mutex<Option<String>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));

/// Le temps laissé à un morph pour vérifier un appel à blanc.
const PREFLIGHT_DELAI: Duration = Duration::from_secs(20);
/// Ce qu'on garde en plus de ce que le morph annonce : le contexte CUDA ou
/// Vulkan, les tampons du rendu.
const MARGE_GO: f64 = 0.4;

pub struct StandardArbiter {
    pub mcp: Arc<McpState>,
    pub tools: Arc<dyn GpuToolSource>,
    pub engine: Arc<dyn ConversationEngine>,
    /// La VRAM libre, en Go (bloquant : lancé hors de l'exécuteur).
    pub free_vram_gb: fn() -> Option<f64>,
}

impl StandardArbiter {
    async fn vram_libre(&self) -> Option<f64> {
        let mesure = self.free_vram_gb;
        tokio::task::spawn_blocking(mesure)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "mesure de la VRAM libre interrompue");
                None
            })
    }

    /// Le serveur qui sert `tool`, son nom chez lui, et ce qu'il produit.
    async fn localiser(&self, tool: &str) -> Option<(String, String, Option<String>)> {
        let carte = self.tools.gpu_tools().await;
        if carte.is_empty() {
            return None;
        }
        let running: Vec<String> = self.mcp.running.read().await.keys().cloned().collect();
        let (serveur, nom) = if tool.starts_with(crate::mcp_tools::MCP_PREFIX) {
            crate::mcp_tools::resolve_mcp_tool_name(tool, &running)?
        } else {
            let serveur = carte
                .iter()
                .find(|(s, outils)| outils.contains_key(tool) && running.contains(s))
                .map(|(s, _)| s.clone())?;
            (serveur, tool.to_string())
        };
        let produit = carte.get(&serveur)?.get(&nom)?.clone();
        Some((serveur, nom, produit))
    }

    async fn client(&self, serveur: &str) -> Result<Arc<dyn McpClient>, String> {
        self.mcp
            .running
            .read()
            .await
            .get(serveur)
            .cloned()
            .ok_or_else(|| format!("le morph « {serveur} » n'est pas démarré"))
    }

    /// Un appel venu de l'interface d'une extension (son Studio) : même
    /// arbitrage, rechargement aussitôt après. `None` : outil ordinaire.
    pub async fn around<F>(
        &self,
        tool: &str,
        args: &Value,
        appel: F,
    ) -> Option<Result<Value, String>>
    where
        F: std::future::Future<Output = Result<Value, String>>,
    {
        self.claims(tool).await?;
        let annonce = Annonce::muette();
        if let Err(e) = self.prepare(tool, args, Path::new(""), &annonce).await {
            return Some(Err(e));
        }
        let resultat = appel.await;
        if let Err(e) = self.restore(&annonce).await {
            tracing::warn!(error = %e, "modèle de conversation non rechargé après le Studio");
        }
        Some(resultat)
    }
}

/// Un `save_to` relatif vise le projet ouvert : le morph, lui, ne sait pas
/// où il est.
pub fn resoudre_save_to(args: &mut Value, projet: &Path) -> Result<(), String> {
    let Some(brut) = args.get("save_to").and_then(Value::as_str) else {
        return Ok(());
    };
    let chemin = Path::new(brut);
    if chemin.is_absolute() {
        return Ok(());
    }
    if projet.as_os_str().is_empty() {
        return Err(format!(
            "« save_to » vaut « {brut} », un chemin relatif, mais aucun dossier de projet n'est ouvert : donnez un chemin absolu"
        ));
    }
    if chemin
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "« save_to » ne peut pas sortir du projet (« {brut} »)"
        ));
    }
    args["save_to"] = Value::String(projet.join(chemin).to_string_lossy().to_string());
    Ok(())
}

#[async_trait::async_trait]
impl GpuArbiter for StandardArbiter {
    async fn claims(&self, tool: &str) -> Option<String> {
        self.localiser(tool)
            .await
            .map(|(_, _, produit)| produit.unwrap_or_default())
    }

    async fn prepare(
        &self,
        tool: &str,
        args: &Value,
        project: &Path,
        annonce: &Annonce<'_>,
    ) -> Result<Preparation, String> {
        let (serveur, nom, _) = self
            .localiser(tool)
            .await
            .ok_or_else(|| format!("outil « {tool} » introuvable"))?;
        let client = self.client(&serveur).await?;

        // 1. Le schéma, strictement : une clé mal orthographiée serait
        //    ignorée par le morph, et le résultat ne serait pas le bon.
        let caps = client
            .discover()
            .await
            .map_err(|e| format!("le morph « {serveur} » ne répond pas : {e}"))?;
        if let Some(desc) = caps.tools.iter().find(|t| t.name == nom) {
            crate::arg_schema::verifier(&desc.input_schema, args, true)?;
        }
        let mut args = args.clone();
        resoudre_save_to(&mut args, project)?;

        // 2. Le morph vérifie à blanc ce que lui seul sait : modèle installé,
        //    moteur présent, fichiers source lisibles, place demandée.
        let mut a_blanc = args.clone();
        if let Some(o) = a_blanc.as_object_mut() {
            o.insert(PREFLIGHT_FLAG.into(), Value::Bool(true));
        }
        let reponse = tokio::time::timeout(PREFLIGHT_DELAI, client.invoke_tool(&nom, &a_blanc))
            .await
            .map_err(|_| format!("le morph « {serveur} » n'a pas confirmé l'appel à temps"))?
            .map_err(|e| e.to_string())?;
        let plan = lire_preflight(&reponse)?;

        // 3. De la place, seulement s'il en manque.
        if plan.vram_gb > 0.05 {
            if let Some(moteur) = self.engine.local_running().await {
                let besoin = plan.vram_gb + MARGE_GO;
                let libre = self.vram_libre().await;
                if libre.is_none_or(|l| l < besoin) {
                    annonce
                        .dire(&format!(
                            "Libération de la mémoire vidéo : le modèle de conversation se met de côté ({:.1} Go demandés{}).",
                            plan.vram_gb,
                            libre.map(|l| format!(", {l:.1} Go libres")).unwrap_or_default()
                        ))
                        .await;
                    let mut decharge = DECHARGE.lock().await;
                    self.engine.stop(&moteur).await;
                    *decharge = Some(moteur);
                    drop(decharge);
                    // Le pilote rend la mémoire un peu après la fin du processus.
                    for _ in 0..20 {
                        if self.vram_libre().await.is_some_and(|l| l >= besoin) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                }
            }
        }
        let quoi = if plan.summary.is_empty() {
            String::new()
        } else {
            format!(" — {}", plan.summary)
        };
        annonce.dire(&format!("Génération en cours{quoi}…")).await;
        Ok(Preparation {
            args,
            media: plan.media,
        })
    }

    async fn restore(&self, annonce: &Annonce<'_>) -> Result<(), String> {
        let mut decharge = DECHARGE.lock().await;
        let Some(moteur) = decharge.take() else {
            return Ok(());
        };
        annonce
            .dire("Rechargement du modèle de conversation…")
            .await;
        let debut = Instant::now();
        if let Err(e) = self.engine.start(&moteur).await {
            // On le rendra au prochain essai plutôt que de l'oublier arrêté.
            *decharge = Some(moteur);
            return Err(e);
        }
        tracing::info!(
            secondes = debut.elapsed().as_secs(),
            "modèle de conversation rechargé"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn save_to_relatif_rejoint_le_projet() {
        let mut a = json!({ "prompt": "x", "save_to": "assets/icon.png" });
        resoudre_save_to(&mut a, Path::new("D:/projet")).unwrap();
        assert!(Path::new(a["save_to"].as_str().unwrap()).starts_with("D:/projet"));
    }

    #[test]
    fn save_to_ne_sort_pas_du_projet_ni_ne_se_devine_sans_lui() {
        let mut a = json!({ "save_to": "../ailleurs.png" });
        assert!(resoudre_save_to(&mut a, Path::new("D:/projet")).is_err());
        let mut b = json!({ "save_to": "icon.png" });
        assert!(resoudre_save_to(&mut b, Path::new("")).is_err());
    }
}
