//! L'arbitre de la carte graphique côté bureau.
//!
//! Voir `locaryn_agent_runtime::gpu` pour le pourquoi. Ici, le comment : les
//! outils concernés sont ceux que les manifestes déclarent dans `gpu.tools` ;
//! le modèle de conversation est un moteur local que le superviseur sait
//! arrêter puis relancer avec le même modèle.

use crate::Core;
use locaryn_agent_runtime::gpu::{
    lire_preflight, Annonce, GpuArbiter, Preparation, PREFLIGHT_FLAG,
};
use locaryn_mcp::McpClient;
use locaryn_shared_types::ProviderEngine;
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Le moteur arrêté pour laisser travailler un morph, à relancer. Partagé :
/// le Studio d'une extension et la conversation passent par le même arbitre,
/// et le verrou fait attendre un second rechargement que le premier finisse.
static DECHARGE: LazyLock<tokio::sync::Mutex<Option<ProviderEngine>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(None));

/// Le temps laissé à un morph pour vérifier un appel à blanc.
const PREFLIGHT_DELAI: Duration = Duration::from_secs(20);
/// Ce qu'on garde en plus de ce que le morph annonce : le contexte CUDA ou
/// Vulkan, les tampons du rendu.
const MARGE_GO: f64 = 0.4;

pub struct DesktopGpuArbiter {
    app: AppHandle,
}

impl DesktopGpuArbiter {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// Le serveur MCP qui sert `tool`, le nom de l'outil chez lui, et ce qu'il
/// produit — s'il est déclaré gourmand en carte.
async fn localiser(core: &Core, tool: &str) -> Option<(String, String, Option<String>)> {
    let rt = core.extensions.read().await;
    if rt.gpu_tools.is_empty() {
        return None;
    }
    let running: Vec<String> = core.mcp.running.read().await.keys().cloned().collect();
    let (serveur, nom) = if tool.starts_with(locaryn_agent_runtime::mcp_tools::MCP_PREFIX) {
        locaryn_agent_runtime::mcp_tools::resolve_mcp_tool_name(tool, &running)?
    } else {
        let serveur = rt
            .gpu_tools
            .iter()
            .find(|(s, outils)| outils.contains_key(tool) && running.contains(s))
            .map(|(s, _)| s.clone())?;
        (serveur, tool.to_string())
    };
    let spec = rt.gpu_tools.get(&serveur)?.get(&nom)?;
    Some((serveur, nom, spec.produces.clone()))
}

/// Le moteur de conversation, s'il tourne sur cette machine.
async fn moteur_local(core: &Core) -> Option<ProviderEngine> {
    let actif = core.storage.providers.active().await.ok().flatten()?;
    let local = actif.endpoint.contains("127.0.0.1") || actif.endpoint.contains("localhost");
    (local && core.supervisor.is_healthy(&actif.engine).await).then_some(actif.engine)
}

async fn vram_libre() -> Option<f64> {
    tokio::task::spawn_blocking(locaryn_llmfit::hardware::free_vram_gb)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "mesure de la VRAM libre interrompue");
            None
        })
}

/// Un `save_to` relatif vise le projet ouvert : le morph, lui, ne sait pas
/// où il est.
fn resoudre_save_to(args: &mut Value, projet: &Path) -> Result<(), String> {
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

async fn client(core: &Core, serveur: &str) -> Result<Arc<dyn McpClient>, String> {
    core.mcp
        .running
        .read()
        .await
        .get(serveur)
        .cloned()
        .ok_or_else(|| format!("le morph « {serveur} » n'est pas démarré"))
}

#[async_trait::async_trait]
impl GpuArbiter for DesktopGpuArbiter {
    async fn claims(&self, tool: &str) -> Option<String> {
        let core = self.app.state::<Core>();
        localiser(&core, tool)
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
        let core = self.app.state::<Core>();
        let (serveur, nom, _) = localiser(&core, tool)
            .await
            .ok_or_else(|| format!("outil « {tool} » introuvable"))?;
        let client = client(&core, &serveur).await?;

        // 1. Le schéma, strictement : une clé mal orthographiée serait
        //    ignorée par le morph, et l'image rendue ne serait pas la bonne.
        let caps = client
            .discover()
            .await
            .map_err(|e| format!("le morph « {serveur} » ne répond pas : {e}"))?;
        if let Some(desc) = caps.tools.iter().find(|t| t.name == nom) {
            locaryn_agent_runtime::arg_schema::verifier(&desc.input_schema, args, true)?;
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
            if let Some(moteur) = moteur_local(&core).await {
                let besoin = plan.vram_gb + MARGE_GO;
                let libre = vram_libre().await;
                if libre.is_none_or(|l| l < besoin) {
                    annonce
                        .dire(&format!(
                            "Libération de la mémoire vidéo : le modèle de conversation se met de côté ({:.1} Go demandés{}).",
                            plan.vram_gb,
                            libre.map(|l| format!(", {l:.1} Go libres")).unwrap_or_default()
                        ))
                        .await;
                    let mut decharge = DECHARGE.lock().await;
                    if let Err(e) = core.supervisor.shutdown(&moteur).await {
                        tracing::warn!(error = %e, "arrêt du moteur de conversation refusé");
                    }
                    *decharge = Some(moteur);
                    drop(decharge);
                    // Le pilote rend la mémoire un peu après la fin du processus.
                    for _ in 0..20 {
                        if vram_libre().await.is_some_and(|l| l >= besoin) {
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
        let core = self.app.state::<Core>();
        let debut = Instant::now();
        if let Err(e) = core.supervisor.ensure_running(&moteur).await {
            // On le rendra au prochain essai plutôt que de l'oublier arrêté.
            *decharge = Some(moteur);
            return Err(e.to_string());
        }
        tracing::info!(
            secondes = debut.elapsed().as_secs(),
            "modèle de conversation rechargé"
        );
        Ok(())
    }
}

/// Un appel venu de l'interface d'une extension (son Studio) : même arbitrage,
/// rechargement aussitôt après.
pub async fn invoquer_arbitre(
    app: &AppHandle,
    tool: &str,
    args: &Value,
    appel: impl std::future::Future<Output = Result<Value, String>>,
) -> Option<Result<Value, String>> {
    let arbitre = DesktopGpuArbiter::new(app.clone());
    arbitre.claims(tool).await?;
    let annonce = Annonce::muette();
    if let Err(e) = arbitre.prepare(tool, args, Path::new(""), &annonce).await {
        return Some(Err(e));
    }
    let resultat = appel.await;
    if let Err(e) = arbitre.restore(&annonce).await {
        tracing::warn!(error = %e, "modèle de conversation non rechargé après le Studio");
    }
    Some(resultat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn save_to_relatif_rejoint_le_projet() {
        let mut a = json!({ "prompt": "x", "save_to": "assets/icon.png" });
        resoudre_save_to(&mut a, Path::new("D:/projet")).unwrap();
        assert!(a["save_to"].as_str().unwrap().ends_with("icon.png"));
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
