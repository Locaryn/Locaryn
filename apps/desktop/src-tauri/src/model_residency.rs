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
        // Profil automatique (0) : le moteur réduit le contexte jusqu'à son
        // minimum avant de déporter une couche, c'est donc lui qui compte.
        context: match config.context_length {
            0 => locaryn_provider_supervisor::AUTO_FIT_MIN_CTX,
            n => n,
        },
        kv_type: match config.kv_cache_type.as_str() {
            "f16" => llmfit::KvType::F16,
            "q4_0" => llmfit::KvType::Q4_0,
            _ => llmfit::KvType::Q8_0,
        },
        flash_attention: config.flash_attention,
        batch: config.batch_size.max(1),
        headroom: level.headroom(),
        // Une répartition choisie à la main (couches fixées, experts MoE sur le
        // processeur) est un débordement voulu : ne pas le refuser.
        strict_vram: config.gpu_layers < 0 && config.n_cpu_moe == 0,
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
fn evaluate(
    core: &Core,
    model: &str,
    level: CautionLevel,
    hardware: &llmfit::HardwareProfile,
) -> ModelFit {
    let path = locaryn_config::models_dir().join(model);
    let options = run_options(core, level);
    ModelFit::from_report(llmfit::for_file_on(&path, &options, hardware), level)
}

/// La machine telle qu'un nouveau chargement la trouvera : la mémoire vidéo
/// que notre moteur occupe compte comme libre, puisqu'il la rend en changeant
/// de modèle. Sans cela, en profil automatique (qui remplit la carte de
/// contexte), plus aucun modèle ne paraissait tenir.
async fn machine_pour_charger(core: &Core) -> llmfit::HardwareProfile {
    let machine = llmfit::profile();
    let en_marche = core
        .supervisor
        .residency(&ProviderEngine::LlamaCpp)
        .await
        .is_some_and(|(_, _, tourne)| tourne);
    if !en_marche {
        return machine;
    }
    let avant = std::fs::read_to_string(
        core.data_dir
            .join(locaryn_provider_supervisor::VRAM_AVANT_MOTEUR),
    )
    .ok()
    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    .and_then(|v| v.get("free_vram_gb").and_then(serde_json::Value::as_f64));
    match avant {
        Some(libre_avant) => {
            let rendue = libre_avant - machine.free_vram_gb;
            machine.with_reclaimed_vram(rendue)
        }
        None => machine,
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
    /// Une réponse demandée par la personne est en cours : pas de compte à
    /// rebours, le modèle sert.
    pub busy: bool,
    /// Le moteur est lancé mais charge encore ses poids.
    pub loading: bool,
    /// La fenêtre de contexte que le moteur applique réellement, en jetons.
    pub context: Option<u32>,
}

#[tauri::command]
pub async fn model_residency(core: State<'_, Core>) -> Result<ResidencyStatus, String> {
    let active = core.storage.providers.active().await.ok().flatten();
    let engine = active
        .as_ref()
        .map(|p| p.engine.clone())
        .unwrap_or(ProviderEngine::LlamaCpp);

    // Un moteur qui répond n'est pas un moteur chargé. `llama-server` qui sert
    // encore les poids d'une session précédente répond parfaitement à la sonde
    // de santé : annoncer « chargé » sur cette seule base faisait proposer une
    // éjection qui ne libérait rien, puis payer le chargement complet au
    // premier message — exactement ce que l'utilisateur voyait.
    //
    // On confronte donc le modèle servi au modèle actif, avec le contrôle que
    // le superviseur fait déjà avant de démarrer. Les moteurs apportés par une
    // extension n'exposent pas `/props` : pour eux, répondre reste le seul
    // signal disponible, et on ne prétend pas mieux savoir.
    let a_un_modele = active.as_ref().and_then(|p| p.model.as_ref()).is_some();
    let loaded = if a_un_modele && core.supervisor.is_healthy(&engine).await {
        match active.as_ref() {
            Some(p) if matches!(engine, ProviderEngine::LlamaCpp) => {
                locaryn_provider_supervisor::is_llama_server_model_match(
                    &core.http,
                    &p.endpoint,
                    p.model.as_deref(),
                )
                .await
            }
            _ => true,
        }
    } else {
        false
    };

    let (idle_seconds, pinned, tourne) = match core.supervisor.residency(&engine).await {
        Some((idle, pinned, tourne)) => (idle, pinned, tourne),
        None => (0, false, false),
    };
    let busy = core.supervisor.utilisateur_occupe();
    // Pendant une réponse, une sonde qui tarde ne fait pas croire à un modèle
    // déchargé : le processus est là, il travaille.
    let loaded = loaded || (busy && tourne && a_un_modele);
    let loading = !loaded && tourne && a_un_modele;
    let context = if loaded {
        match active.as_ref() {
            Some(p) => locaryn_agent_runtime::tool_budget::server_context(&core.http, &p.endpoint)
                .await
                .map(|n| n.min(u32::MAX as usize) as u32),
            None => None,
        }
    } else {
        None
    };

    Ok(ResidencyStatus {
        busy,
        loading,
        context,
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

/// La personne se sert de l'application : le modèle reste en mémoire.
///
/// Le délai d'inactivité ne comptait que les messages envoyés : lire une
/// longue réponse, écrire la suite, relire du code, et l'écran annonçait
/// « libéré dans 27 min » à quelqu'un en plein travail. L'interface appelle
/// ceci, au plus une fois par minute, quand on tape ou clique.
#[tauri::command]
pub async fn signaler_activite(core: State<'_, Core>) -> Result<(), String> {
    if let Some(p) = core.storage.providers.active().await.ok().flatten() {
        if crate::moteur_supervise(&p.engine) {
            core.supervisor.note_activity(&p.engine).await;
        }
    }
    Ok(())
}

/// Ce que chaque niveau de prudence laisse libre sur *cette* machine.
#[derive(Debug, Clone, Serialize)]
pub struct CautionReserve {
    pub level: CautionLevel,
    /// VRAM laissée au système, en Go.
    pub vram_gb: f64,
    /// RAM laissée au système, en Go.
    pub ram_gb: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CautionReserves {
    pub total_vram_gb: f64,
    pub total_ram_gb: f64,
    pub unified_memory: bool,
    pub levels: Vec<CautionReserve>,
}

/// Les réserves des trois niveaux, en Go, pour les montrer à côté du choix.
#[tauri::command]
pub fn caution_reserves() -> CautionReserves {
    let p = llmfit::profile();
    let levels = [
        CautionLevel::Prudent,
        CautionLevel::Equilibre,
        CautionLevel::Risque,
    ]
    .into_iter()
    .map(|level| CautionReserve {
        level,
        vram_gb: level.headroom().vram_reserve_gb(p.total_vram_gb),
        ram_gb: level.headroom().ram_reserve_gb(),
    })
    .collect();
    CautionReserves {
        total_vram_gb: p.total_vram_gb,
        total_ram_gb: p.total_ram_gb,
        unified_memory: p.unified_memory,
        levels,
    }
}

/// Le chargement de ce modèle respecte-t-il la prudence choisie ? `Some` :
/// la raison du refus, à montrer telle quelle. Appelé avant qu'une
/// conversation ne démarre le moteur elle-même.
pub async fn refus_de_chargement(core: &Core, model: &str) -> Option<RefusDeChargement> {
    let level = CautionSetting::load(&core.data_dir).level;
    let machine = machine_pour_charger(core).await;
    let fit = evaluate(core, model, level, &machine);
    (fit.verdict == FitVerdict::Refuse).then(|| RefusDeChargement {
        message: fit.message.clone(),
        model: model.to_string(),
        level,
        overridable: fit.overridable,
    })
}

/// Un modèle que la conversation n'a pas chargé : la fenêtre dit pourquoi,
/// sous quelle prudence, et propose de passer outre comme le menu de
/// chargement — sans quoi l'un acceptait ce que l'autre refusait.
#[derive(Debug, Clone, Serialize)]
pub struct RefusDeChargement {
    pub message: String,
    pub model: String,
    pub level: CautionLevel,
    pub overridable: bool,
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
            .unwrap_or_else(|| {
                locaryn_provider_supervisor::default_endpoint(&ProviderEngine::LlamaCpp)
                    .unwrap_or("http://127.0.0.1:8080")
                    .to_string()
            });
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
pub async fn check_model_fit(core: State<'_, Core>, model: String) -> Result<ModelFit, String> {
    let machine = machine_pour_charger(&core).await;
    Ok(evaluate(
        &core,
        &model,
        CautionSetting::load(&core.data_dir).level,
        &machine,
    ))
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
    /// La forme de l'attention, quand le catalogue la publie.
    #[serde(default)]
    pub attention: Option<AttentionShape>,
}

/// Couches qui gardent un cache, têtes clés-valeurs, dimension d'une tête.
#[derive(Debug, Clone, Deserialize)]
pub struct AttentionShape {
    pub layers: u32,
    pub kv_heads: u32,
    pub head_dim: u32,
}

/// Estimer d'un coup toutes les fiches visibles dans la liste des modèles.
///
/// Un appel par ligne referait le tour de la machine à chaque fois, pour un
/// résultat identique : le profil matériel est lu une fois, et le reste n'est
/// que de l'arithmétique. Les fiches reviennent dans l'ordre reçu.
#[tauri::command]
pub async fn llmfit_catalog(
    core: State<'_, Core>,
    entries: Vec<CatalogEntry>,
) -> Result<Vec<ModelFit>, String> {
    let level = CautionSetting::load(&core.data_dir).level;
    let mut options = run_options(&core, level);
    // Le catalogue répond « ce PC peut-il le faire tourner », au contexte
    // minimal utile : le moteur agrandit ensuite le contexte à la mémoire qui
    // reste. Un contexte réglé à 128k rendait presque tout « trop lourd ».
    options.context = locaryn_provider_supervisor::AUTO_FIT_MIN_CTX;
    // Un modèle qui déborde un peu sur la RAM reste faisable : la pastille le
    // montre (couches GPU) ; le chargement, lui, exige la VRAM libre.
    options.strict_vram = false;
    if level == CautionLevel::Prudent {
        options.headroom = llmfit::Headroom::Equilibre;
    }
    // La capacité de la machine, pas la mémoire libre à l'instant : la
    // question est « ce PC peut-il le faire tourner ». Le chargement, lui,
    // contrôle la mémoire réellement libre.
    let hardware = machine_pour_charger(&core).await.capacite();
    Ok(entries
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
            if let Some(a) = &entry.attention {
                spec = spec.with_attention(a.layers, a.kv_heads, a.head_dim);
            }
            ModelFit::from_report(llmfit::estimate(&spec, &hardware, &options), level)
        })
        .collect())
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
    let machine = machine_pour_charger(&core).await;
    let fit = evaluate(&core, &model, level, &machine);
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

    // Un moteur prêt garde son modèle s'il est déjà le bon : `ensure_running`
    // ne relance que s'il en sert un autre. Un moteur encore en chargement,
    // lui, charge l'ancien : on l'arrête, et ce chargement est abandonné au
    // lieu de tenir le verrou de lancement jusqu'à son délai.
    if !core.supervisor.is_healthy(&engine).await {
        let _ = core.supervisor.shutdown(&engine).await;
    }

    core.supervisor
        .ensure_running(&engine)
        .await
        .map_err(|e| e.to_string())?;
    // L'épinglage permanent bloquait l'éjection automatique après 30 min
    // d'inactivité : le moteur n'était jamais libéré même après 3 h. On
    // s'assure ici que le moteur n'est PAS épinglé (ensure_running ne le
    // touche pas, mais un rechargement précédent aurait pu le laisser épinglé).
    core.supervisor.set_pinned(&engine, false).await;
    crate::refresh_mcp_runtime_env(&core).await;

    tracing::info!(%model, moteur = %engine.as_token(), "modèle de chat chargé");
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
        .unwrap_or_else(|| {
            locaryn_provider_supervisor::default_endpoint(&ProviderEngine::LlamaCpp)
                .unwrap_or("http://127.0.0.1:8080")
                .to_string()
        });

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
        if matches!(moteur, ProviderEngine::LlamaCpp) {
            locaryn_provider_supervisor::kill_stray_llama_servers().await;
            let _ = std::fs::remove_file(core.data_dir.join("active_llama_settings.json"));
        }
        if core.supervisor.is_healthy(moteur).await {
            tracing::warn!(
                moteur = %moteur.as_token(),
                "toujours en marche après la demande d'arrêt — tentative d'arrêt forcé"
            );
            if matches!(moteur, ProviderEngine::LlamaCpp) {
                locaryn_provider_supervisor::kill_stray_llama_servers().await;
            }
        }
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

    // Le modèle choisi n'est PAS réinitialisé ici : `model_residency` lit déjà
    // « chargé » depuis la santé du superviseur (`is_healthy` devient faux dès
    // l'arrêt ci-dessus), pas depuis ce champ — l'éjection s'y reflète sans y
    // toucher. Le mettre à `None` avait un effet de bord distinct et cassant :
    // `ensure_running` n'a alors plus de modèle actif à transmettre, et
    // `spawn_llama_server` retombe sur le nom littéral `model.gguf`, absent du
    // disque. Toute conversation qui redémarrait le moteur après une éjection
    // (manuelle, ou automatique après le délai d'inactivité) échouait donc
    // aussitôt sur « fichier de poids introuvable — …/model.gguf » — une
    // nouvelle conversation le fait plus vite, faute d'avoir déjà résolu son
    // propre modèle par un envoi précédent. Garder le champ laisse un envoi
    // suivant recharger le même modèle sans qu'on ait à le rechoisir.
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
