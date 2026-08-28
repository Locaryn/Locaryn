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
//!
//! Le calcul lui-même vit dans `locaryn-llmfit` : il lit l'en-tête GGUF,
//! dimensionne le cache d'attention pour le contexte réglé, répartit les
//! couches entre GPU et RAM et en déduit un débit. Ce module n'en garde que
//! la décision — charger, prévenir, ou refuser.

use crate::Core;
use locaryn_llmfit as llmfit;
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
    /// Le même réglage, dit à l'estimateur.
    ///
    /// Les marges ne sont plus des pourcentages appliqués à la taille du
    /// fichier : l'estimateur calcule ce qu'il faut vraiment, et le niveau de
    /// prudence ne décide plus que de la réserve laissée libre à côté.
    fn headroom(self) -> llmfit::Headroom {
        match self {
            CautionLevel::Prudent => llmfit::Headroom::Prudent,
            CautionLevel::Equilibre => llmfit::Headroom::Equilibre,
            CautionLevel::Risque => llmfit::Headroom::Risque,
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
// Estimation
// ============================================================================

/// Les conditions réelles d'exécution, telles que le moteur les appliquera.
///
/// Estimer avec un contexte de 8 192 jetons pendant que l'utilisateur en a
/// réglé 32 768 donne un chiffre juste pour une machine qui n'existe pas : le
/// cache d'attention quadruple, et le modèle annoncé comme confortable
/// déborde au chargement. Les réglages d'inférence entrent donc dans le
/// calcul.
fn run_options(core: &Core, level: CautionLevel) -> llmfit::RunOptions {
    let config = crate::InferenceConfig::load(&core.data_dir);
    llmfit::RunOptions {
        context: config.context_length,
        kv_type: match config.kv_cache_type.as_str() {
            "f16" => llmfit::KvType::F16,
            "q4_0" => llmfit::KvType::Q4_0,
            _ => llmfit::KvType::Q8_0,
        },
        flash_attention: config.flash_attention,
        batch: config.batch_size.max(1),
        headroom: level.headroom(),
    }
}

// ============================================================================
// Verdict de chargement
// ============================================================================

/// Confortable, juste, risqué, refusé — les quatre issues, définies une seule
/// fois dans l'estimateur et réexportées ici pour les commandes.
pub use llmfit::Verdict as FitVerdict;

/// Ce que donnerait le chargement, avec de quoi le vérifier.
///
/// Les trois postes de mémoire sont séparés parce qu'ils ne se corrigent pas
/// de la même façon : des poids trop lourds appellent une quantification plus
/// basse, un cache trop gros appelle un contexte plus court.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ModelFit {
    pub model: String,
    pub verdict: FitVerdict,
    /// Taille des poids.
    pub size_gb: f64,
    /// Cache d'attention pour le contexte réglé.
    pub kv_cache_gb: f64,
    /// Tampons de calcul et surcoût du moteur.
    pub compute_gb: f64,
    /// Ce qu'il faut réellement, marge de prudence comprise.
    pub required_gb: f64,
    pub free_ram_gb: f64,
    pub free_vram_gb: f64,
    /// `gpu`, `partage`, `ram` ou `disque` — où les poids finiront.
    pub placement: String,
    pub level: CautionLevel,
    /// Contexte pris en compte, en jetons.
    pub context: u32,
    /// Couches placées sur le GPU, sur le total.
    pub gpu_layers: u32,
    pub total_layers: u32,
    /// Débit de génération estimé, en jetons par seconde.
    pub tokens_per_second: f64,
    /// Débit de lecture du prompt : plus élevé, et moins certain.
    pub prompt_tokens_per_second: f64,
    /// Le plus grand contexte qui tiendrait entièrement sur le GPU.
    pub max_gpu_context: u32,
    /// Le plus grand contexte qui tiendrait, GPU et RAM réunis.
    pub max_context: u32,
    /// Quantification du fichier.
    pub quant: String,
    /// Une quantification plus légère qui, elle, tiendrait sur le GPU.
    pub suggested_quant: Option<String>,
    /// Faux quand les dimensions ont été lues dans le fichier, vrai quand
    /// elles ont été déduites faute d'en-tête lisible.
    pub estimated: bool,
    /// Ce que ces chiffres supposent. Affiché tel quel, jamais résumé.
    pub assumptions: Vec<String>,
    /// Peut-on passer outre ? Faux quand rien ne le permettrait.
    pub overridable: bool,
    /// Phrase montrée telle quelle. Dit ce qui va se passer, pas un code.
    pub message: String,
}

impl ModelFit {
    fn from_report(report: llmfit::FitReport, level: CautionLevel) -> Self {
        Self {
            model: report.model,
            verdict: report.verdict,
            size_gb: report.weights_gb,
            kv_cache_gb: report.kv_cache_gb,
            compute_gb: report.compute_gb,
            required_gb: report.required_gb,
            free_ram_gb: report.free_ram_gb,
            free_vram_gb: report.free_vram_gb,
            placement: report.placement.label().to_string(),
            level,
            context: report.context,
            gpu_layers: report.gpu_layers,
            total_layers: report.total_layers,
            tokens_per_second: report.tokens_per_second,
            prompt_tokens_per_second: report.prompt_tokens_per_second,
            max_gpu_context: report.max_gpu_context,
            max_context: report.max_context,
            quant: report.quant,
            suggested_quant: report.suggested_quant,
            estimated: report.source == llmfit::SpecSource::Estime,
            assumptions: report.assumptions,
            overridable: report.overridable,
            message: report.message,
        }
    }
}

/// Ce que donnerait le chargement de ce modèle, sans rien charger.
fn evaluate(core: &Core, model: &str, level: CautionLevel) -> ModelFit {
    let path = locaryn_config::models_dir().join(model);
    let options = run_options(core, level);
    ModelFit::from_report(llmfit::for_file(&path, &options), level)
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
    let engine = active
        .as_ref()
        .map(|p| p.engine.clone())
        .unwrap_or(ProviderEngine::LlamaCpp);

    // La condition était la même dans les deux branches : un moteur est chargé
    // s'il répond et qu'un modèle est enregistré. Vrai pour le runtime
    // intégré comme pour un moteur apporté par une extension.
    let loaded = core.supervisor.is_healthy(&engine).await
        && active.as_ref().and_then(|p| p.model.as_ref()).is_some();

    let (idle_seconds, pinned) = match core.supervisor.residency(&engine).await {
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

/// Le moteur qui sait charger ce modèle, et son point d'entrée.
///
/// Le runtime intégré d'abord — c'est le cas courant, et il ne dépend d'aucune
/// extension. Puis les moteurs apportés, dans l'ordre de leur nom affiché. Un
/// modèle que personne ne sait charger renvoie une erreur qui dit quoi
/// installer, au lieu de démarrer un moteur qui refusera les poids.
async fn moteur_pour(core: &Core, model: &str) -> Result<(ProviderEngine, String), String> {
    if crate::is_text_chat_model(model) {
        let endpoint = core
            .storage
            .providers
            .active()
            .await
            .ok()
            .flatten()
            .filter(|p| p.engine == ProviderEngine::LlamaCpp)
            .map(|p| p.endpoint)
            .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
        return Ok((ProviderEngine::LlamaCpp, endpoint));
    }
    for spec in core.supervisor.extension_engines().await {
        if spec.serves_model(model) {
            return Ok((spec.engine(), spec.endpoint()));
        }
    }
    Err(format!(
        "Aucun moteur installé ne sait charger « {model} ». Le runtime intégré          charge du GGUF ; pour un autre format, installez le moteur          correspondant depuis Réglages → Extensions."
    ))
}

/// Ce que donnerait le chargement de ce modèle, sans rien charger.
#[tauri::command]
pub fn check_model_fit(core: State<'_, Core>, model: String) -> ModelFit {
    evaluate(&core, &model, CautionSetting::load(&core.data_dir).level)
}

/// Ce que la machine a, mesuré.
///
/// Sondé une fois par session, hormis la mémoire libre qui est relue à chaque
/// appel : c'est elle qui change entre le moment où l'utilisateur ouvre la
/// liste des modèles et celui où il en charge un.
#[tauri::command]
pub fn llmfit_hardware() -> llmfit::HardwareProfile {
    llmfit::profile()
}

/// Une fiche du catalogue, telle que l'interface la connaît avant tout
/// téléchargement.
#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    /// Identifiant stable côté interface, renvoyé tel quel pour l'appariement.
    pub id: String,
    /// Paramètres du modèle, en milliards.
    pub parameters_b: f64,
    /// Étiquette de quantification (« Q4_K_M »), si elle est connue.
    pub quant: Option<String>,
    /// Taille annoncée du téléchargement, en gigaoctets. Quand le catalogue la
    /// publie, elle prime sur la taille déduite des paramètres.
    pub size_gb: Option<f64>,
}

/// Estimer d'un coup toutes les fiches visibles dans la liste des modèles.
///
/// Un appel par ligne referait le tour de la machine à chaque fois, pour un
/// résultat identique : le profil matériel est lu une fois, et le reste n'est
/// que de l'arithmétique. Les fiches reviennent dans l'ordre reçu.
#[tauri::command]
pub fn llmfit_catalog(core: State<'_, Core>, entries: Vec<CatalogEntry>) -> Vec<ModelFit> {
    let level = CautionSetting::load(&core.data_dir).level;
    let options = run_options(&core, level);
    let hardware = llmfit::profile();
    entries
        .into_iter()
        .map(|entry| {
            let quant = entry
                .quant
                .as_deref()
                .and_then(locaryn_llmfit::quant::from_label)
                .unwrap_or(locaryn_llmfit::DEFAULT_QUANT);
            let mut spec = llmfit::ModelSpec::from_params(&entry.id, entry.parameters_b, quant);
            if let Some(size_gb) = entry.size_gb.filter(|g| *g > 0.0) {
                spec = spec.with_weights_bytes((size_gb * 1024.0 * 1024.0 * 1024.0) as u64);
            }
            ModelFit::from_report(llmfit::estimate(&spec, &hardware, &options), level)
        })
        .collect()
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
    let fit = evaluate(&core, &model, level);
    if fit.verdict == FitVerdict::Refuse && !force.unwrap_or(false) {
        return Err(fit.message);
    }

    // Quel moteur sait charger *ce* modèle. Choisir llama.cpp d'office
    // faisait échouer le démarrage sur un checkpoint safetensors, alors qu'un
    // moteur installé savait le servir.
    let (engine, endpoint) = moteur_pour(&core, &model).await?;

    // Enregistrer le modèle voulu avant de démarrer : le superviseur lance le
    // moteur avec le modèle actif, pas avec un argument qu'on lui passe.
    core.storage
        .providers
        .upsert_local(&engine, &endpoint, Some(model.clone()))
        .await
        .map_err(|e| e.to_string())?;

    // Un moteur déjà en route tient l'ancien modèle : il faut le relancer.
    if core.supervisor.is_healthy(&engine).await {
        let _ = core.supervisor.shutdown(&engine).await;
    }

    core.supervisor
        .ensure_running(&engine)
        .await
        .map_err(|e| e.to_string())?;
    core.supervisor.set_pinned(&engine, true).await;
    crate::refresh_mcp_runtime_env(&core).await;

    tracing::info!(%model, moteur = %engine.as_token(), "modèle de chat chargé et épinglé");
    model_residency(core).await
}

/// Décharger le modèle et rendre la mémoire.
#[tauri::command]
pub async fn eject_chat_model(core: State<'_, Core>) -> Result<ResidencyStatus, String> {
    let active = core.storage.providers.active().await.ok().flatten();
    let engine = active
        .as_ref()
        .map(|p| p.engine.clone())
        .unwrap_or(ProviderEngine::LlamaCpp);
    let endpoint = active
        .as_ref()
        .map(|p| p.endpoint.clone())
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

    // 1. Décharger le moteur supervisé (LlamaCpp, AirLLM, etc.)
    // Le moteur actif, les runtimes intégrés, et tous ceux qu'apportent des
    // extensions : « rendre la mémoire » veut dire toute la mémoire, pas
    // seulement celle du moteur inscrit comme actif.
    let mut a_liberer = vec![
        engine.clone(),
        ProviderEngine::LlamaCpp,
        ProviderEngine::AirLlm,
    ];
    for spec in core.supervisor.extension_engines().await {
        a_liberer.push(spec.engine());
    }
    a_liberer.dedup();
    for moteur in &a_liberer {
        core.supervisor.set_pinned(moteur, false).await;
        let _ = core.supervisor.shutdown(moteur).await;
    }

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
    let _ = core
        .storage
        .providers
        .upsert_local(&engine, &endpoint, None)
        .await;

    crate::refresh_mcp_runtime_env(&core).await;
    tracing::info!("modèle de chat déchargé à la demande");
    model_residency(core).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les trois niveaux doivent arriver distincts à l'estimateur : sans quoi
    /// le réglage ne changerait rien à ce qui est accepté.
    #[test]
    fn les_trois_niveaux_restent_distincts() {
        assert_eq!(CautionLevel::Prudent.headroom(), llmfit::Headroom::Prudent);
        assert_eq!(
            CautionLevel::Equilibre.headroom(),
            llmfit::Headroom::Equilibre
        );
        assert_eq!(CautionLevel::Risque.headroom(), llmfit::Headroom::Risque);
    }

    /// Un modèle introuvable ne doit jamais passer pour confortable : mesurer
    /// zéro octet ne veut pas dire que le chargement est sûr.
    #[test]
    fn modele_introuvable_nest_pas_confortable() {
        let path = locaryn_config::models_dir().join("ce-modele-nexiste-pas.gguf");
        let fit = ModelFit::from_report(
            llmfit::for_file(&path, &llmfit::RunOptions::default()),
            CautionLevel::Equilibre,
        );
        assert_ne!(fit.verdict, FitVerdict::Confortable);
        assert_eq!(fit.tokens_per_second, 0.0);
    }

    /// Le rapport doit toujours porter ses hypothèses jusqu'à l'interface :
    /// un débit annoncé sans ses conditions n'est pas vérifiable.
    #[test]
    fn le_verdict_transporte_ses_hypotheses() {
        let fit = ModelFit::from_report(
            llmfit::for_catalog("8B", 8.0, Some("Q4_K_M"), &llmfit::RunOptions::default()),
            CautionLevel::Equilibre,
        );
        assert!(!fit.assumptions.is_empty());
        assert!(fit.estimated, "une fiche de catalogue est une déduction");
        assert!(fit.total_layers > 0);
    }
}
