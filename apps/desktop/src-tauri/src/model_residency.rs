//! Garder un modèle de chat en mémoire, et refuser de le charger quand la
//! machine n'a pas de quoi le tenir.
//!
//! Le moteur `llama-server` garde déjà les poids résidents tant qu'il tourne :
//! parler au modèle ne le recharge pas. Ce qui manquait, c'est la main de
//! l'utilisateur dessus. Sans elle, le superviseur décharge après trente
//! minutes d'inactivité, et la personne qui revient de déjeuner repaie le
//! chargement complet sans avoir rien demandé.
//!
//! Ce module ajoute donc trois choses : savoir ce qui est chargé, l'épingler
//! pour qu'aucun minuteur n'y touche, et vérifier la mémoire disponible avant
//! de charger quoi que ce soit.
//!
//! Le contrôle mémoire lit la mémoire **libre**, pas la mémoire totale. Un
//! poste de 32 Go dont 26 sont déjà pris ne peut pas accueillir un modèle de
//! 20 Go, et c'est précisément le cas où l'utilisateur a besoin qu'on l'arrête
//! avant que la machine ne se mette à ramer.

use crate::Core;
use locaryn_shared_types::ProviderEngine;
use serde::{Deserialize, Serialize};
use tauri::State;

// ============================================================================
// Niveau de prudence
// ============================================================================

/// Jusqu'où l'application accepte de remplir la mémoire.
///
/// Le réglage existe parce que la bonne réponse dépend de la machine et de ce
/// qu'on en fait. Quelqu'un qui code avec vingt onglets ouverts n'a pas la même
/// tolérance que quelqu'un dont le poste ne sert qu'à ça.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CautionLevel {
    /// Ne charge que si la machine a largement de quoi. Refuse tout ce qui
    /// pourrait la faire ralentir, même un peu.
    Prudent,
    /// Charge avec précautions : accepte que ce soit un peu juste, refuse ce
    /// qui déborderait vraiment.
    #[default]
    Equilibre,
    /// Ne refuse jamais. Prévient quand ça va déborder sur le disque, et
    /// laisse décider — au prix d'un ralentissement sévère, voire d'un plantage.
    Risque,
}

impl CautionLevel {
    /// Marge exigée au-dessus de la taille du modèle, et réserve laissée au
    /// système. Les deux comptent : un facteur seul laisserait passer un petit
    /// modèle sur une machine déjà saturée.
    fn margin(self) -> (f64, f64) {
        match self {
            // 35 % de marge et 3 Go réservés : de quoi garder l'OS et un
            // navigateur réactifs pendant que le modèle tourne.
            CautionLevel::Prudent => (1.35, 3.0),
            // 12 % et 1,5 Go : le cache KV et l'overhead d'inférence, sans
            // confort supplémentaire.
            CautionLevel::Equilibre => (1.12, 1.5),
            // Juste de quoi tenir les poids. Tout le reste est assumé.
            CautionLevel::Risque => (1.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CautionSetting {
    pub level: CautionLevel,
}

impl CautionSetting {
    fn path(data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join("model_caution.json")
    }
    fn load(data_dir: &std::path::Path) -> Self {
        std::fs::read_to_string(Self::path(data_dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    fn save(&self, data_dir: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

// ============================================================================
// Mesure de la mémoire libre
// ============================================================================

/// Mémoire libre en gigaoctets : RAM d'abord, VRAM ensuite (0 si pas de GPU
/// interrogeable).
///
/// Lu à la demande, jamais en boucle : chaque appel lance un processus, ce qui
/// n'a rien à faire dans un rafraîchissement de barre d'état.
fn free_memory_gb() -> (f64, f64) {
    let ram = free_ram_gb().unwrap_or(0.0);
    let vram = free_vram_gb().unwrap_or(0.0);
    (ram, vram)
}

#[cfg(target_os = "windows")]
fn free_ram_gb() -> Option<f64> {
    // `wmic` a disparu des versions récentes de Windows 11 ; CIM le remplace
    // et répond sur tout ce qui porte PowerShell.
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory",
        ])
        .output()
        .ok()?;
    let kb: f64 = String::from_utf8(out.stdout).ok()?.trim().parse().ok()?;
    Some(kb / (1024.0 * 1024.0))
}

#[cfg(target_os = "linux")]
fn free_ram_gb() -> Option<f64> {
    // MemAvailable, pas MemFree : le cache page est récupérable, l'ignorer
    // ferait refuser des chargements parfaitement possibles.
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: f64 = text
        .lines()
        .find(|l| l.starts_with("MemAvailable:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb / (1024.0 * 1024.0))
}

#[cfg(target_os = "macos")]
fn free_ram_gb() -> Option<f64> {
    let out = std::process::Command::new("vm_stat").output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let page_size = 4096.0;
    let mut free_pages = 0.0;
    for line in text.lines() {
        // Libres + inactives + purgeables : ce que le système peut rendre.
        if line.starts_with("Pages free:")
            || line.starts_with("Pages inactive:")
            || line.starts_with("Pages purgeable:")
        {
            if let Some(n) = line.split(':').nth(1) {
                if let Ok(v) = n.trim().trim_end_matches('.').parse::<f64>() {
                    free_pages += v;
                }
            }
        }
    }
    Some(free_pages * page_size / (1024.0 * 1024.0 * 1024.0))
}

fn free_vram_gb() -> Option<f64> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.free", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    let mb: f64 = String::from_utf8(out.stdout)
        .ok()?
        .lines()
        .next()?
        .trim()
        .parse()
        .ok()?;
    Some(mb / 1024.0)
}

/// Taille sur disque d'un modèle, en gigaoctets. Un modèle peut être un
/// fichier unique ou un dossier de shards.
fn model_size_gb(model: &str) -> f64 {
    let path = locaryn_config::models_dir().join(model);
    let bytes = if path.is_dir() {
        walk_size(&path)
    } else {
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    };
    bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}

fn walk_size(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            total += if p.is_dir() {
                walk_size(&p)
            } else {
                std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
            };
        }
    }
    total
}

// ============================================================================
// Verdict de chargement
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FitVerdict {
    /// Tient sur le GPU : la vitesse nominale.
    Confortable,
    /// Tient, mais sans marge, ou en débordant sur la RAM. Plus lent.
    Juste,
    /// Dépasse ce que la machine offre. Chargeable seulement en mode risqué,
    /// et au prix du disque d'échange.
    Risque,
    /// Refusé par le niveau de prudence choisi.
    Refuse,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ModelFit {
    pub model: String,
    pub verdict: FitVerdict,
    /// Taille des poids sur disque.
    pub size_gb: f64,
    /// Ce qu'il faut réellement, marge de prudence comprise.
    pub required_gb: f64,
    pub free_ram_gb: f64,
    pub free_vram_gb: f64,
    /// `gpu`, `ram` ou `disque` — où les poids finiront.
    pub placement: String,
    pub level: CautionLevel,
    /// Peut-on passer outre ? Faux quand rien ne le permettrait.
    pub overridable: bool,
    /// Phrase montrée telle quelle. Dit ce qui va se passer, pas un code.
    pub message: String,
}

fn evaluate(model: &str, level: CautionLevel) -> ModelFit {
    let size_gb = model_size_gb(model);
    let (free_ram, free_vram) = free_memory_gb();
    let (factor, reserve) = level.margin();
    let required = size_gb * factor + reserve;

    // Le GPU d'abord : c'est le seul placement qui tourne à pleine vitesse.
    let fits_vram = free_vram > 0.0 && required <= free_vram;
    let fits_ram = required <= free_ram;

    let (verdict, placement, message) = if size_gb <= 0.0 {
        (
            FitVerdict::Risque,
            "inconnu".to_string(),
            format!(
                "Impossible de mesurer « {model} » : le fichier est introuvable dans le dossier des modèles. \
                 Le chargement peut échouer."
            ),
        )
    } else if fits_vram {
        (
            FitVerdict::Confortable,
            "gpu".to_string(),
            format!("{size_gb:.1} Go sur le GPU, {free_vram:.1} Go libres. Vitesse maximale."),
        )
    } else if fits_ram {
        let on_gpu = free_vram > 1.0;
        (
            FitVerdict::Juste,
            "ram".to_string(),
            if on_gpu {
                format!(
                    "{size_gb:.1} Go à répartir : trop pour les {free_vram:.1} Go de VRAM libres, \
                     le reste ira en RAM ({free_ram:.1} Go libres). Plus lent qu'en tout-GPU."
                )
            } else {
                format!(
                    "{size_gb:.1} Go en RAM, {free_ram:.1} Go libres. Ça tient, mais la génération \
                     sera nettement plus lente que sur GPU."
                )
            },
        )
    } else if level == CautionLevel::Risque {
        (
            FitVerdict::Risque,
            "disque".to_string(),
            format!(
                "{size_gb:.1} Go demandés pour {free_ram:.1} Go libres. Le système va compenser sur \
                 le disque : ralentissement sévère, et l'application peut être tuée par manque de mémoire."
            ),
        )
    } else {
        (
            FitVerdict::Refuse,
            "disque".to_string(),
            format!(
                "{size_gb:.1} Go demandés, {required:.1} Go nécessaires avec la marge choisie, et \
                 seulement {free_ram:.1} Go libres. Fermez des applications, choisissez un modèle \
                 plus petit, ou passez le niveau de prudence sur « risqué » pour forcer."
            ),
        )
    };

    ModelFit {
        model: model.to_string(),
        verdict,
        size_gb,
        required_gb: required,
        free_ram_gb: free_ram,
        free_vram_gb: free_vram,
        placement,
        level,
        overridable: verdict == FitVerdict::Refuse,
        message,
    }
}

// ============================================================================
// État de résidence
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ResidencyStatus {
    /// Le moteur tourne et répond.
    pub loaded: bool,
    /// Le modèle que le moteur a en mémoire, tel qu'enregistré.
    pub model: Option<String>,
    /// Épinglé : aucun minuteur ne le déchargera.
    pub pinned: bool,
    /// Secondes écoulées depuis le dernier message.
    pub idle_seconds: u64,
    /// Au-delà, un modèle non épinglé est déchargé.
    pub idle_timeout_seconds: u64,
    pub endpoint: Option<String>,
}

#[tauri::command]
pub async fn model_residency(core: State<'_, Core>) -> Result<ResidencyStatus, String> {
    let active = core.storage.providers.active().await.ok().flatten();
    let engine = active.as_ref().map(|p| p.engine).unwrap_or(ProviderEngine::LlamaCpp);

    let is_managed = matches!(engine, ProviderEngine::LlamaCpp | ProviderEngine::AirLlm);
    let loaded = if is_managed {
        core.supervisor.is_healthy(engine).await
            && active.as_ref().and_then(|p| p.model.as_ref()).is_some()
    } else {
        core.supervisor.is_healthy(engine).await
            && active.as_ref().and_then(|p| p.model.as_ref()).is_some()
    };

    let (idle_seconds, pinned) = match core.supervisor.residency(engine).await {
        Some((idle, pinned, _)) => (idle, pinned),
        None => (0, false),
    };

    Ok(ResidencyStatus {
        loaded,
        model: if loaded {
            active.as_ref().and_then(|p| p.model.clone())
        } else {
            None
        },
        pinned,
        idle_seconds,
        idle_timeout_seconds: core.supervisor.idle_timeout_secs(),
        endpoint: active.map(|p| p.endpoint),
    })
}

#[tauri::command]
pub fn caution_level(core: State<'_, Core>) -> CautionLevel {
    CautionSetting::load(&core.data_dir).level
}

#[tauri::command]
pub fn set_caution_level(core: State<'_, Core>, level: CautionLevel) -> Result<(), String> {
    CautionSetting { level }
        .save(&core.data_dir)
        .map_err(|e| e.to_string())
}

/// Ce que donnerait le chargement de ce modèle, sans rien charger.
#[tauri::command]
pub fn check_model_fit(core: State<'_, Core>, model: String) -> ModelFit {
    evaluate(&model, CautionSetting::load(&core.data_dir).level)
}

/// Charger un modèle et l'épingler en mémoire.
///
/// `force` passe outre un refus, et n'a d'effet que si l'utilisateur l'a
/// explicitement demandé après avoir lu le message : un refus silencieusement
/// contournable ne protégerait personne.
#[tauri::command]
pub async fn load_chat_model(
    core: State<'_, Core>,
    model: String,
    force: Option<bool>,
) -> Result<ResidencyStatus, String> {
    let level = CautionSetting::load(&core.data_dir).level;
    let fit = evaluate(&model, level);
    if fit.verdict == FitVerdict::Refuse && !force.unwrap_or(false) {
        return Err(fit.message);
    }

    let engine = ProviderEngine::LlamaCpp;
    let endpoint = core
        .storage
        .providers
        .active()
        .await
        .ok()
        .flatten()
        .map(|p| p.endpoint)
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

    // Enregistrer le modèle voulu avant de démarrer : le superviseur lance
    // llama-server avec le modèle actif, pas avec un argument qu'on lui passe.
    core.storage
        .providers
        .upsert_local(engine, &endpoint, Some(model.clone()))
        .await
        .map_err(|e| e.to_string())?;

    // Un moteur déjà en route tient l'ancien modèle : il faut le relancer.
    if core.supervisor.is_healthy(engine).await {
        let _ = core.supervisor.shutdown(engine).await;
    }

    core.supervisor
        .ensure_running(engine)
        .await
        .map_err(|e| e.to_string())?;
    core.supervisor.set_pinned(engine, true).await;
    crate::refresh_mcp_runtime_env(&core).await;

    tracing::info!(%model, "modèle de chat chargé et épinglé");
    model_residency(core).await
}

/// Décharger le modèle et rendre la mémoire.
#[tauri::command]
pub async fn eject_chat_model(core: State<'_, Core>) -> Result<ResidencyStatus, String> {
    let active = core.storage.providers.active().await.ok().flatten();
    let engine = active.as_ref().map(|p| p.engine).unwrap_or(ProviderEngine::LlamaCpp);
    let endpoint = active
        .as_ref()
        .map(|p| p.endpoint.clone())
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

    // 1. Décharger le moteur supervisé (LlamaCpp, AirLLM, etc.)
    core.supervisor.set_pinned(engine, false).await;
    core.supervisor.set_pinned(ProviderEngine::LlamaCpp, false).await;
    core.supervisor.set_pinned(ProviderEngine::AirLlm, false).await;

    let _ = core.supervisor.shutdown(engine).await;
    let _ = core.supervisor.shutdown(ProviderEngine::LlamaCpp).await;
    let _ = core.supervisor.shutdown(ProviderEngine::AirLlm).await;

    // 2. Si Ollama ou API externe, envoyer la commande de déchargement immédiat
    if let Some(model_name) = active.as_ref().and_then(|p| p.model.as_deref()) {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap_or_default();

        let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
        let _ = http
            .post(url)
            .json(&serde_json::json!({
                "model": model_name,
                "keep_alive": 0
            }))
            .send()
            .await;
    }

    // 3. Réinitialiser le modèle actif dans le stockage local
    let _ = core.storage.providers.upsert_local(engine, &endpoint, None).await;

    crate::refresh_mcp_runtime_env(&core).await;
    tracing::info!("modèle de chat déchargé à la demande");
    model_residency(core).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le mode risqué ne refuse jamais : c'est ce qui le distingue des deux
    /// autres, et ce que l'utilisateur choisit en connaissance de cause.
    #[test]
    fn risque_ne_refuse_jamais() {
        let (factor, reserve) = CautionLevel::Risque.margin();
        assert_eq!(factor, 1.0);
        assert_eq!(reserve, 0.0);
    }

    /// Prudent doit exiger strictement plus qu'équilibré, sinon le réglage ne
    /// veut rien dire.
    #[test]
    fn prudent_exige_plus_qu_equilibre() {
        let (pf, pr) = CautionLevel::Prudent.margin();
        let (ef, er) = CautionLevel::Equilibre.margin();
        assert!(pf > ef, "le facteur prudent doit dépasser l'équilibré");
        assert!(pr > er, "la réserve prudente doit dépasser l'équilibrée");
    }

    /// Un modèle introuvable ne doit jamais passer pour confortable : mesurer
    /// zéro octet ne veut pas dire que le chargement est sûr.
    #[test]
    fn modele_introuvable_nest_pas_confortable() {
        let fit = evaluate("ce-modele-nexiste-pas.gguf", CautionLevel::Equilibre);
        assert_ne!(fit.verdict, FitVerdict::Confortable);
        assert_eq!(fit.size_gb, 0.0);
    }

    /// La réserve système s'applique même à un modèle minuscule : c'est elle
    /// qui protège une machine déjà saturée.
    #[test]
    fn la_reserve_sapplique_aux_petits_modeles() {
        let (_, reserve) = CautionLevel::Prudent.margin();
        assert!(reserve > 0.0);
    }
}
