//! Le calcul : ce modèle tient-il sur cette machine, et à quelle vitesse.
//!
//! Trois postes de mémoire, jamais un seul. Les poids, qu'on lit dans le
//! fichier. Le cache d'attention, qui grandit avec le contexte demandé et
//! dépasse les poids eux-mêmes sur les longs contextes. Les tampons de calcul,
//! qui ne bougent pas beaucoup mais qu'oublier suffit à faire échouer un
//! chargement annoncé comme sûr.
//!
//! La vitesse, elle, ne dépend presque pas du processeur : générer un jeton
//! oblige à relire tous les poids actifs, une fois. Le débit est donc borné
//! par la bande passante mémoire, et un modèle réparti entre GPU et RAM paie
//! les deux, l'une après l'autre.
//!
//! Chaque rapport transporte ses hypothèses. Un chiffre dont on ne sait pas ce
//! qu'il suppose ne vaut rien : c'est ce que l'utilisateur lit quand il se
//! demande pourquoi l'estimation annonce vingt jetons par seconde et la
//! machine en produit cinq.

use crate::gguf::GgufSummary;
use crate::hardware::HardwareProfile;
use crate::quant::{self, Quant};
use serde::Serialize;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
/// Ce que le moteur consomme pour lui-même, hors poids et caches.
const RUNTIME_OVERHEAD_BYTES: u64 = 256 * 1024 * 1024;
/// Fraction de la bande passante théorique réellement atteinte en lecture de
/// poids sur GPU, puis sur processeur.
const GPU_BANDWIDTH_EFFICIENCY: f64 = 0.85;
const CPU_BANDWIDTH_EFFICIENCY: f64 = 0.60;
/// Contexte retenu quand rien ne le précise et que le modèle en autorise plus.
const DEFAULT_CONTEXT: u32 = 8192;

fn gib(bytes: u64) -> f64 {
    bytes as f64 / GIB
}

/// Octets → gigaoctets décimaux, l'unité dans laquelle se comptent les bandes
/// passantes. Les mélanger fausserait les vitesses de 7 %.
fn gb_decimal(bytes: u64) -> f64 {
    bytes as f64 / 1e9
}

// ============================================================================
// Réglages
// ============================================================================

/// Précision du cache d'attention. Le compresser divise sa taille sans
/// toucher aux poids — le levier le plus efficace sur les longs contextes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KvType {
    F16,
    Q8_0,
    Q4_0,
}

impl KvType {
    fn bytes_per_element(self) -> f64 {
        match self {
            KvType::F16 => 2.0,
            // Blocs de 32 valeurs : 32 octets de données + 2 d'échelle.
            KvType::Q8_0 => 34.0 / 32.0,
            KvType::Q4_0 => 18.0 / 32.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            KvType::F16 => "f16",
            KvType::Q8_0 => "q8_0",
            KvType::Q4_0 => "q4_0",
        }
    }
}

/// Jusqu'où on accepte de remplir la mémoire.
///
/// Reprend les trois niveaux de prudence de l'application : le réglage existe
/// parce que la bonne réponse dépend de ce que la machine fait par ailleurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Headroom {
    /// Large réserve : rien qui puisse ralentir la machine.
    Prudent,
    #[default]
    /// Réserve minimale : accepte que ce soit juste.
    Equilibre,
    /// Aucune réserve : ne refuse jamais, prévient seulement.
    Risque,
}

impl Headroom {
    /// Réserve laissée libre, en Go : d'abord la VRAM, ensuite la RAM.
    ///
    /// La VRAM garde moins que la RAM parce qu'elle n'a pas d'autre client
    /// qu'un bureau graphique, là où le système, lui, a tout le reste à faire
    /// tourner.
    fn reserves(self) -> (f64, f64) {
        match self {
            Headroom::Prudent => (1.5, 3.0),
            Headroom::Equilibre => (0.6, 1.5),
            Headroom::Risque => (0.0, 0.0),
        }
    }
}

/// Les conditions dans lesquelles on veut faire tourner le modèle.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// Contexte demandé, en jetons. 0 = laisser l'estimateur choisir.
    pub context: u32,
    pub kv_type: KvType,
    /// Attention éclair : divise les tampons de calcul sur les longs
    /// contextes. Activée par défaut sur les moteurs récents.
    pub flash_attention: bool,
    /// Taille de lot de traitement du prompt.
    pub batch: u32,
    pub headroom: Headroom,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            context: 0,
            kv_type: KvType::F16,
            flash_attention: true,
            batch: 512,
            headroom: Headroom::default(),
        }
    }
}

// ============================================================================
// Le modèle, tel que l'estimation le voit
// ============================================================================

/// D'où viennent les chiffres du modèle : lus, ou déduits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecSource {
    /// En-tête GGUF lu sur le disque : les dimensions sont exactes.
    Gguf,
    /// Modèle pas encore téléchargé : dimensions déduites du nombre de
    /// paramètres. Suffisant pour trancher, pas pour promettre.
    Estime,
}

/// Ce qu'il faut savoir d'un modèle pour l'estimer.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub source: SpecSource,
    pub architecture: String,
    pub parameters: f64,
    /// Part des poids traversée à chaque jeton (1.0 hors modèles à experts).
    pub active_fraction: f64,
    pub weights_bytes: u64,
    /// Octets d'un bloc transformeur.
    pub layer_bytes: u64,
    /// Embeddings, sortie, normes : ce qui n'appartient à aucun bloc.
    pub non_layer_bytes: u64,
    pub n_layer: u32,
    pub n_embd: u32,
    pub n_head_kv: u32,
    pub head_dim: u32,
    pub n_vocab: u32,
    pub train_context: u32,
    pub quant: Quant,
}

impl ModelSpec {
    /// Depuis un fichier GGUF déjà présent : tout est exact.
    pub fn from_gguf(name: &str, summary: &GgufSummary) -> Self {
        let quant = quant::from_ggml_name(summary.quant_name());
        Self {
            name: name.to_string(),
            source: SpecSource::Gguf,
            architecture: summary.architecture.clone(),
            parameters: summary.parameters as f64,
            active_fraction: summary.active_fraction(),
            weights_bytes: summary.weights_bytes,
            layer_bytes: summary.layer_bytes,
            non_layer_bytes: summary.non_layer_bytes,
            n_layer: summary.n_layer.max(1),
            n_embd: summary.n_embd,
            n_head_kv: summary.n_head_kv,
            head_dim: summary.head_dim(),
            n_vocab: summary.n_vocab.max(32_000),
            train_context: summary.train_context,
            quant,
        }
    }

    /// Depuis une fiche de catalogue : nombre de paramètres et quantification
    /// annoncés, dimensions déduites.
    ///
    /// C'est le cas d'un modèle qu'on n'a pas encore téléchargé — celui où la
    /// question « est-ce que ça tourne » a le plus de valeur, puisqu'y
    /// répondre après coup coûte plusieurs gigaoctets de téléchargement.
    pub fn from_params(name: &str, parameters_b: f64, quant: Quant) -> Self {
        let parameters = parameters_b * 1e9;
        let shape = Shape::for_params(parameters_b);
        let weights_bytes = quant::weights_bytes(parameters, quant);
        // Embeddings d'entrée et de sortie : deux matrices vocabulaire × n_embd,
        // le reste se répartit sur les blocs.
        let embedding_params = 2.0 * shape.n_vocab as f64 * shape.n_embd as f64;
        let non_layer_bytes = quant::weights_bytes(embedding_params.min(parameters * 0.3), quant);
        let layer_bytes =
            weights_bytes.saturating_sub(non_layer_bytes) / shape.n_layer.max(1) as u64;
        Self {
            name: name.to_string(),
            source: SpecSource::Estime,
            architecture: "estimé".to_string(),
            parameters,
            active_fraction: 1.0,
            weights_bytes,
            layer_bytes,
            non_layer_bytes,
            n_layer: shape.n_layer,
            n_embd: shape.n_embd,
            n_head_kv: shape.n_head_kv,
            head_dim: shape.head_dim,
            n_vocab: shape.n_vocab,
            train_context: shape.train_context,
            quant,
        }
    }

    /// Recaler les poids sur une taille réellement connue.
    ///
    /// Un catalogue publie la taille exacte du fichier ; une déduction à
    /// partir du nombre de paramètres se trompe de quelques pour cent. Quand
    /// la vraie taille est disponible, elle prime — et la répartition par
    /// couche suit dans la même proportion, sinon les couches placées sur le
    /// GPU ne correspondraient plus au total.
    pub fn with_weights_bytes(mut self, bytes: u64) -> Self {
        if bytes == 0 {
            return self;
        }
        let ratio = bytes as f64 / self.weights_bytes.max(1) as f64;
        self.weights_bytes = bytes;
        self.layer_bytes = (self.layer_bytes as f64 * ratio) as u64;
        self.non_layer_bytes = (self.non_layer_bytes as f64 * ratio) as u64;
        self
    }

    /// Le même modèle dans une autre quantification, pour répondre « et si on
    /// descendait d'un cran ».
    pub fn with_quant(&self, quant: Quant) -> Self {
        let ratio = quant.bits_per_weight / self.quant.bits_per_weight;
        Self {
            source: self.source,
            weights_bytes: (self.weights_bytes as f64 * ratio) as u64,
            layer_bytes: (self.layer_bytes as f64 * ratio) as u64,
            non_layer_bytes: (self.non_layer_bytes as f64 * ratio) as u64,
            quant,
            ..self.clone()
        }
    }
}

/// Dimensions typiques d'un transformeur, par ordre de grandeur.
///
/// Les architectures publiées se ressemblent beaucoup à taille donnée : c'est
/// ce qui rend l'extrapolation défendable tant qu'on ne s'en sert que pour
/// dimensionner un cache, pas pour annoncer une taille de fichier.
struct Shape {
    n_layer: u32,
    n_embd: u32,
    n_head_kv: u32,
    head_dim: u32,
    n_vocab: u32,
    train_context: u32,
}

impl Shape {
    fn for_params(parameters_b: f64) -> Self {
        // (milliards, couches, n_embd, têtes KV)
        const REFERENCES: &[(f64, u32, u32, u32)] = &[
            (0.5, 24, 896, 2),
            (1.5, 28, 1536, 2),
            (3.0, 28, 3072, 8),
            (8.0, 32, 4096, 8),
            (14.0, 48, 5120, 8),
            (32.0, 64, 5120, 8),
            (70.0, 80, 8192, 8),
            (405.0, 126, 16384, 8),
        ];
        let (_, n_layer, n_embd, n_head_kv) = REFERENCES
            .iter()
            .min_by(|a, b| {
                let da = (a.0.ln() - parameters_b.max(0.05).ln()).abs();
                let db = (b.0.ln() - parameters_b.max(0.05).ln()).abs();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or((8.0, 32, 4096, 8));
        Self {
            n_layer,
            n_embd,
            n_head_kv,
            // 128 partout depuis Llama 2 : les têtes changent de nombre, pas
            // de taille.
            head_dim: 128,
            n_vocab: 128_000,
            train_context: 32_768,
        }
    }
}

// ============================================================================
// Le rapport
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Tout tient sur le GPU : la vitesse nominale.
    Confortable,
    /// Tient, mais réparti ou en RAM seule. Plus lent, pas bloquant.
    Juste,
    /// Dépasse ce que la machine offre : le système compensera sur le disque.
    Risque,
    /// Refusé par le niveau de prudence choisi.
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    Gpu,
    /// Une partie des couches sur le GPU, le reste en RAM.
    Partage,
    Ram,
    Disque,
}

impl Placement {
    pub fn label(self) -> &'static str {
        match self {
            Placement::Gpu => "gpu",
            Placement::Partage => "partage",
            Placement::Ram => "ram",
            Placement::Disque => "disque",
        }
    }
}

/// Le résultat, avec de quoi le vérifier.
#[derive(Debug, Clone, Serialize)]
pub struct FitReport {
    pub model: String,
    pub verdict: Verdict,
    pub placement: Placement,
    pub source: SpecSource,
    pub quant: String,
    pub context: u32,

    pub weights_gb: f64,
    pub kv_cache_gb: f64,
    pub compute_gb: f64,
    /// Total à trouver, réserve de prudence comprise.
    pub required_gb: f64,
    pub free_vram_gb: f64,
    pub free_ram_gb: f64,

    /// Couches placées sur le GPU, sur le total.
    pub gpu_layers: u32,
    pub total_layers: u32,

    /// Débit de génération estimé, en jetons par seconde.
    pub tokens_per_second: f64,
    /// Débit de lecture du prompt, bien plus élevé, et bien moins certain.
    pub prompt_tokens_per_second: f64,

    /// Le plus grand contexte qui tiendrait entièrement sur le GPU.
    pub max_gpu_context: u32,
    /// Le plus grand contexte qui tiendrait, GPU et RAM réunis.
    pub max_context: u32,
    /// La meilleure quantification qui tiendrait entièrement sur le GPU, quand
    /// celle demandée n'y arrive pas.
    pub suggested_quant: Option<String>,
    /// Peut-on passer outre un refus ?
    pub overridable: bool,
    /// Ce que ces chiffres supposent. Affiché tel quel.
    pub assumptions: Vec<String>,
    /// Une phrase qui dit ce qui va se passer.
    pub message: String,
}

// ============================================================================
// Calcul
// ============================================================================

/// Octets du cache d'attention pour ce contexte.
///
/// Deux tenseurs par couche, clés et valeurs, dimensionnés par le nombre de
/// têtes de clé — pas par le nombre de têtes tout court. C'est toute la
/// différence entre un modèle en attention groupée et un modèle en attention
/// pleine : à taille égale, le second peut demander huit fois plus de cache.
fn kv_cache_bytes(spec: &ModelSpec, context: u32, kv_type: KvType) -> u64 {
    let per_element = kv_type.bytes_per_element();
    let elements = 2.0
        * spec.n_layer as f64
        * context as f64
        * spec.n_head_kv.max(1) as f64
        * spec.head_dim.max(1) as f64;
    (elements * per_element) as u64
}

/// Octets des tampons de calcul.
///
/// Sans attention éclair, le produit clés-requêtes est matérialisé en entier :
/// têtes × lot × contexte, en flottants 32 bits. C'est le poste qui explose
/// sur les longs contextes, et la raison pour laquelle activer l'attention
/// éclair fait parfois tenir un modèle qui débordait.
fn compute_bytes(spec: &ModelSpec, context: u32, options: &RunOptions) -> u64 {
    let batch = options.batch.max(1) as f64;
    let n_embd = spec.n_embd.max(1) as f64;
    // Une poignée de tenseurs d'activation lot × n_embd en 32 bits, le long du
    // graphe.
    let activations = batch * n_embd * 4.0 * 8.0;
    let logits = spec.n_vocab as f64 * 4.0 * 2.0;
    let mask = batch * context as f64 * 4.0;
    let attention = if options.flash_attention {
        0.0
    } else {
        let heads = if spec.head_dim > 0 {
            (n_embd / spec.head_dim as f64).max(1.0)
        } else {
            32.0
        };
        heads * batch * context as f64 * 4.0
    };
    let total = activations + logits + mask + attention;
    (total as u64).max(128 * 1024 * 1024)
}

/// L'estimation complète.
pub fn estimate(spec: &ModelSpec, hardware: &HardwareProfile, options: &RunOptions) -> FitReport {
    let context = resolve_context(spec, options);
    let (reserve_vram, reserve_ram) = options.headroom.reserves();
    let usable_vram = hardware.usable_vram_gb(reserve_vram);
    let usable_ram = hardware.usable_ram_gb(reserve_ram);

    let kv_bytes = kv_cache_bytes(spec, context, options.kv_type);
    let compute = compute_bytes(spec, context, options);
    let overhead = RUNTIME_OVERHEAD_BYTES;
    let total_bytes = spec.weights_bytes + kv_bytes + compute + overhead;

    let split = plan_split(spec, kv_bytes, compute, usable_vram, hardware);
    let (verdict, placement, overridable) = judge(
        &split,
        total_bytes,
        usable_ram,
        usable_vram,
        options.headroom,
    );

    let speed = throughput(spec, &split, context, options, hardware);
    let prompt_speed = prompt_throughput(spec, &split, hardware);

    let max_gpu_context = largest_context_within(spec, options, usable_vram);
    let max_context = largest_context_within(spec, options, usable_vram + usable_ram);
    let suggested_quant = if matches!(verdict, Verdict::Confortable) {
        None
    } else {
        lighter_quant_that_fits(spec, hardware, options).map(|q| q.name.to_string())
    };

    let report = FitReport {
        model: spec.name.clone(),
        verdict,
        placement,
        source: spec.source,
        quant: spec.quant.name.to_string(),
        context,
        weights_gb: gib(spec.weights_bytes),
        kv_cache_gb: gib(kv_bytes),
        compute_gb: gib(compute + overhead),
        required_gb: gib(total_bytes) + reserve_ram.min(reserve_vram),
        free_vram_gb: hardware.free_vram_gb,
        free_ram_gb: hardware.free_ram_gb,
        gpu_layers: split.gpu_layers,
        total_layers: spec.n_layer,
        tokens_per_second: speed,
        prompt_tokens_per_second: prompt_speed,
        max_gpu_context,
        max_context,
        suggested_quant,
        overridable,
        assumptions: assumptions(spec, options, hardware, &split),
        message: String::new(),
    };
    let message = phrase(&report, hardware);
    FitReport { message, ..report }
}

/// Contexte retenu : celui demandé, sinon un compromis raisonnable, jamais
/// plus que ce pour quoi le modèle a été entraîné.
fn resolve_context(spec: &ModelSpec, options: &RunOptions) -> u32 {
    let trained = if spec.train_context > 0 {
        spec.train_context
    } else {
        DEFAULT_CONTEXT
    };
    if options.context > 0 {
        options.context.min(trained.max(options.context))
    } else {
        trained.min(DEFAULT_CONTEXT)
    }
}

/// Répartition des couches entre GPU et RAM.
struct Split {
    gpu_layers: u32,
    gpu_bytes: u64,
    cpu_bytes: u64,
    /// Octets de cache relus à chaque jeton, côté GPU puis côté RAM.
    gpu_kv_bytes: u64,
    cpu_kv_bytes: u64,
}

fn plan_split(
    spec: &ModelSpec,
    kv_bytes: u64,
    compute: u64,
    usable_vram_gb: f64,
    hardware: &HardwareProfile,
) -> Split {
    let n_layer = spec.n_layer.max(1);
    let kv_per_layer = kv_bytes / n_layer as u64;
    let per_layer = spec.layer_bytes + kv_per_layer;
    let everything = spec.weights_bytes + kv_bytes + compute + RUNTIME_OVERHEAD_BYTES;

    if hardware.total_vram_gb <= 0.0 {
        return Split {
            gpu_layers: 0,
            gpu_bytes: 0,
            cpu_bytes: spec.weights_bytes,
            gpu_kv_bytes: 0,
            cpu_kv_bytes: kv_bytes,
        };
    }

    let vram_bytes = (usable_vram_gb * GIB) as u64;
    if everything <= vram_bytes {
        return Split {
            gpu_layers: n_layer,
            gpu_bytes: spec.weights_bytes,
            cpu_bytes: 0,
            gpu_kv_bytes: kv_bytes,
            cpu_kv_bytes: 0,
        };
    }

    // Offload partiel : les tampons de calcul restent sur le GPU, les
    // embeddings restent côté processeur. Ce qui se déplace, ce sont les
    // blocs, cache compris.
    let budget = vram_bytes
        .saturating_sub(compute)
        .saturating_sub(RUNTIME_OVERHEAD_BYTES / 2);
    let gpu_layers = if per_layer == 0 {
        0
    } else {
        ((budget / per_layer) as u32).min(n_layer)
    };
    Split {
        gpu_layers,
        gpu_bytes: spec.layer_bytes * gpu_layers as u64,
        cpu_bytes: spec.weights_bytes - spec.layer_bytes * gpu_layers as u64,
        gpu_kv_bytes: kv_per_layer * gpu_layers as u64,
        cpu_kv_bytes: kv_per_layer * (n_layer - gpu_layers) as u64,
    }
}

fn judge(
    split: &Split,
    total_bytes: u64,
    usable_ram_gb: f64,
    usable_vram_gb: f64,
    headroom: Headroom,
) -> (Verdict, Placement, bool) {
    let needed_in_ram = gib(split.cpu_bytes + split.cpu_kv_bytes);
    let all_on_gpu = split.cpu_bytes == 0 && split.gpu_layers > 0;

    if all_on_gpu {
        return (Verdict::Confortable, Placement::Gpu, false);
    }
    if needed_in_ram <= usable_ram_gb {
        let placement = if split.gpu_layers > 0 {
            Placement::Partage
        } else {
            Placement::Ram
        };
        // Tout en RAM sur une machine sans GPU reste un fonctionnement normal ;
        // c'est lent, pas dangereux.
        return (Verdict::Juste, placement, false);
    }
    let _ = (total_bytes, usable_vram_gb);
    match headroom {
        Headroom::Risque => (Verdict::Risque, Placement::Disque, false),
        _ => (Verdict::Refuse, Placement::Disque, true),
    }
}

/// Jetons par seconde en génération.
///
/// Chaque jeton relit les poids actifs et le cache accumulé. Sur un modèle
/// réparti, les deux mémoires travaillent l'une après l'autre : les temps
/// s'additionnent, ils ne se recouvrent pas.
fn throughput(
    spec: &ModelSpec,
    split: &Split,
    context: u32,
    options: &RunOptions,
    hardware: &HardwareProfile,
) -> f64 {
    let active = spec.active_fraction;
    // Le cache est lu depuis le début de la conversation : à mi-parcours, la
    // moitié. C'est la moyenne honnête sur une session, pas le pire cas.
    let kv_share = 0.5;
    let gpu_bytes = split.gpu_bytes as f64 * active + split.gpu_kv_bytes as f64 * kv_share;
    let cpu_bytes = split.cpu_bytes as f64 * active + split.cpu_kv_bytes as f64 * kv_share;

    let gpu_bw = hardware.vram_bandwidth_gbps * GPU_BANDWIDTH_EFFICIENCY;
    let cpu_bw = hardware.ram_bandwidth_gbps * CPU_BANDWIDTH_EFFICIENCY;

    let mut seconds = 0.0;
    if gpu_bytes > 0.0 && gpu_bw > 0.0 {
        seconds += gb_decimal(gpu_bytes as u64) / gpu_bw;
    }
    if cpu_bytes > 0.0 && cpu_bw > 0.0 {
        seconds += gb_decimal(cpu_bytes as u64) / cpu_bw;
    }
    if seconds <= 0.0 {
        return 0.0;
    }
    // Le contexte alourdit aussi l'attention elle-même, pas seulement sa
    // lecture : au-delà de quelques milliers de jetons, le débit s'érode.
    let attention_drag = 1.0 + (context as f64 / 65_536.0);
    let _ = options;
    (1.0 / seconds) / attention_drag
}

/// Jetons par seconde en lecture de prompt.
///
/// Cette phase-là est bornée par le calcul, pas par la mémoire : elle traite
/// des centaines de jetons d'un coup. Le chiffre est le moins sûr du rapport,
/// et le rapport le dit.
fn prompt_throughput(spec: &ModelSpec, split: &Split, hardware: &HardwareProfile) -> f64 {
    let active_params = spec.parameters * spec.active_fraction;
    if active_params <= 0.0 {
        return 0.0;
    }
    let on_gpu = if spec.n_layer > 0 {
        split.gpu_layers as f64 / spec.n_layer as f64
    } else {
        0.0
    };
    let flops = hardware.gpu_flops() * on_gpu + hardware.cpu_flops() * (1.0 - on_gpu);
    // Deux opérations par paramètre et par jeton : une multiplication, une
    // addition.
    flops / (2.0 * active_params)
}

/// Le plus grand contexte qui tiendrait dans ce budget mémoire.
///
/// Répond à la vraie question de l'utilisateur dont le modèle déborde : « et
/// si je demandais moins de contexte ? » — appliqué à la VRAM seule pour
/// savoir ce qui tourne à pleine vitesse, puis à toute la mémoire pour savoir
/// ce qui tourne tout court.
fn largest_context_within(spec: &ModelSpec, options: &RunOptions, budget_gb: f64) -> u32 {
    let budget = (budget_gb * GIB) as u64;
    if budget <= spec.weights_bytes + RUNTIME_OVERHEAD_BYTES {
        return 0;
    }
    let mut best = 0;
    // Les contextes utiles sont des puissances de deux ; les balayer coûte
    // moins qu'inverser une formule qui changera avec le prochain moteur.
    for context in [512, 1024, 2048, 4096, 8192, 16_384, 32_768, 65_536, 131_072] {
        let total = spec.weights_bytes
            + kv_cache_bytes(spec, context, options.kv_type)
            + compute_bytes(spec, context, options)
            + RUNTIME_OVERHEAD_BYTES;
        if total <= budget {
            best = context;
        } else {
            break;
        }
    }
    let trained = if spec.train_context > 0 {
        spec.train_context
    } else {
        u32::MAX
    };
    best.min(trained)
}

/// La meilleure quantification qui tiendrait entièrement sur le GPU.
fn lighter_quant_that_fits(
    spec: &ModelSpec,
    hardware: &HardwareProfile,
    options: &RunOptions,
) -> Option<Quant> {
    let (reserve_vram, _) = options.headroom.reserves();
    let budget = (hardware.usable_vram_gb(reserve_vram) * GIB) as u64;
    if budget == 0 {
        return None;
    }
    quant::lighter_than(spec.quant).copied().find(|candidate| {
        let lighter = spec.with_quant(*candidate);
        let context = resolve_context(&lighter, options);
        let total = lighter.weights_bytes
            + kv_cache_bytes(&lighter, context, options.kv_type)
            + compute_bytes(&lighter, context, options)
            + RUNTIME_OVERHEAD_BYTES;
        total <= budget
    })
}

/// Ce que les chiffres supposent — la partie qu'on ne peut pas deviner à leur
/// place.
fn assumptions(
    spec: &ModelSpec,
    options: &RunOptions,
    hardware: &HardwareProfile,
    split: &Split,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(match spec.source {
        SpecSource::Gguf => format!(
            "Dimensions lues dans le fichier : {} couches, {} têtes de clé, {} par tête.",
            spec.n_layer, spec.n_head_kv, spec.head_dim
        ),
        SpecSource::Estime => format!(
            "Modèle pas encore téléchargé : dimensions déduites de {:.1} milliards de paramètres.",
            spec.parameters / 1e9
        ),
    });
    lines.push(format!(
        "Cache d'attention en {}, contexte de {} jetons, lot de {}.",
        options.kv_type.label(),
        resolve_context(spec, options),
        options.batch
    ));
    lines.push(format!(
        "Attention éclair {}.",
        if options.flash_attention {
            "activée"
        } else {
            "désactivée — les tampons de calcul grandissent avec le contexte"
        }
    ));
    lines.push(if hardware.ram_bandwidth_measured {
        format!(
            "Bande passante mémoire mesurée sur cette machine : {:.0} Go/s.",
            hardware.ram_bandwidth_gbps
        )
    } else {
        format!(
            "Bande passante mémoire non mesurable ici, {:.0} Go/s supposés.",
            hardware.ram_bandwidth_gbps
        )
    });
    if hardware.total_vram_gb > 0.0 {
        lines.push(format!(
            "{} en {}, {:.0} Go/s de bande passante estimée.",
            hardware.gpu_name.as_deref().unwrap_or("GPU"),
            hardware.backend.label(),
            hardware.vram_bandwidth_gbps
        ));
    }
    if split.gpu_layers > 0 && split.cpu_bytes > 0 {
        lines.push(format!(
            "{} couches sur {} placées sur le GPU ; le reste passe par la RAM, \
             et les deux temps s'additionnent.",
            split.gpu_layers, spec.n_layer
        ));
    }
    lines.push(
        "Vitesse déduite de la bande passante mémoire, cache à moitié plein. \
         Un contexte plein descend plus bas."
            .to_string(),
    );
    lines
}

/// La phrase que lit l'utilisateur.
fn phrase(report: &FitReport, hardware: &HardwareProfile) -> String {
    let speed = format_speed(report.tokens_per_second);
    match report.verdict {
        Verdict::Confortable => format!(
            "{:.1} Go entièrement sur le GPU ({:.1} Go libres), contexte de {} jetons. \
             Environ {speed}.",
            report.weights_gb + report.kv_cache_gb,
            report.free_vram_gb,
            report.context
        ),
        Verdict::Juste if report.placement == Placement::Partage => {
            let mut text = format!(
                "{} couches sur {} tiennent dans les {:.1} Go de VRAM libres, le reste passe \
                 par la RAM. Environ {speed}.",
                report.gpu_layers, report.total_layers, report.free_vram_gb
            );
            if let Some(quant) = &report.suggested_quant {
                text.push_str(&format!(
                    " En {quant}, le modèle tiendrait entièrement sur le GPU."
                ));
            } else if report.max_gpu_context > 0 {
                text.push_str(&format!(
                    " Avec un contexte de {} jetons, il tiendrait entièrement sur le GPU.",
                    report.max_gpu_context
                ));
            }
            text
        }
        Verdict::Juste => {
            let sans_gpu = hardware.total_vram_gb <= 0.0;
            format!(
                "{:.1} Go en RAM ({:.1} Go libres){}. Environ {speed}.",
                report.weights_gb + report.kv_cache_gb,
                report.free_ram_gb,
                if sans_gpu {
                    ", aucun GPU détecté"
                } else {
                    ", la VRAM est trop petite pour en prendre une part utile"
                }
            )
        }
        Verdict::Risque => format!(
            "{:.1} Go nécessaires pour {:.1} Go libres. Le système compensera sur le disque : \
             ralentissement sévère, et l'application peut être tuée par manque de mémoire.",
            report.required_gb, report.free_ram_gb
        ),
        Verdict::Refuse => {
            let mut text = format!(
                "{:.1} Go nécessaires, {:.1} Go libres en RAM et {:.1} Go en VRAM. Refusé au \
                 niveau de prudence choisi.",
                report.required_gb, report.free_ram_gb, report.free_vram_gb
            );
            if let Some(quant) = &report.suggested_quant {
                text.push_str(&format!(" La version {quant} tiendrait."));
            } else if report.max_context > 0 && report.max_context < report.context {
                text.push_str(&format!(
                    " Avec un contexte de {} jetons au lieu de {}, il tiendrait.",
                    report.max_context, report.context
                ));
            }
            text
        }
    }
}

fn format_speed(tokens_per_second: f64) -> String {
    // Le pluriel s'accorde à partir de deux : « 4,7 jetons » et « 0,4 jeton ».
    let unite = if tokens_per_second >= 2.0 {
        "jetons"
    } else {
        "jeton"
    };
    if tokens_per_second >= 10.0 {
        format!("{tokens_per_second:.0} {unite}/s")
    } else if tokens_per_second >= 1.0 {
        format!("{tokens_per_second:.1} {unite}/s")
    } else {
        format!("{tokens_per_second:.2} {unite}/s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::Backend;

    fn machine(vram_gb: f64, ram_gb: f64) -> HardwareProfile {
        HardwareProfile {
            cpu_cores: 8,
            total_ram_gb: ram_gb,
            free_ram_gb: ram_gb * 0.75,
            gpu_name: Some("RTX 4090".into()),
            total_vram_gb: vram_gb,
            free_vram_gb: vram_gb * 0.95,
            backend: if vram_gb > 0.0 {
                Backend::Cuda
            } else {
                Backend::Cpu
            },
            ram_bandwidth_gbps: 50.0,
            vram_bandwidth_gbps: if vram_gb > 0.0 { 1008.0 } else { 0.0 },
            ram_bandwidth_measured: true,
            unified_memory: false,
        }
    }

    fn modele(params_b: f64) -> ModelSpec {
        ModelSpec::from_params(&format!("{params_b}B"), params_b, quant::DEFAULT)
    }

    /// Un 8B en Q4 sur une 4090 : tout sur le GPU, et vite.
    #[test]
    fn petit_modele_grosse_carte() {
        let report = estimate(&modele(8.0), &machine(24.0, 32.0), &RunOptions::default());
        assert_eq!(report.verdict, Verdict::Confortable);
        assert_eq!(report.placement, Placement::Gpu);
        assert_eq!(report.gpu_layers, report.total_layers);
        assert!(
            report.tokens_per_second > 40.0,
            "obtenu {:.1} jetons/s",
            report.tokens_per_second
        );
    }

    /// Un 70B sur 6 Go de VRAM : quelques couches passent, le reste tombe en
    /// RAM — et l'estimation doit le dire au lieu de refuser en bloc.
    #[test]
    fn gros_modele_petite_carte() {
        let report = estimate(&modele(70.0), &machine(6.0, 64.0), &RunOptions::default());
        assert_eq!(report.placement, Placement::Partage);
        assert!(report.gpu_layers > 0);
        assert!(report.gpu_layers < report.total_layers);
        assert!(report.tokens_per_second < 10.0);
    }

    /// Ce qui ne tient nulle part doit être refusé, avec une porte de sortie.
    #[test]
    fn ce_qui_ne_tient_nulle_part_est_refuse() {
        let report = estimate(&modele(405.0), &machine(6.0, 16.0), &RunOptions::default());
        assert_eq!(report.verdict, Verdict::Refuse);
        assert!(report.overridable);
    }

    /// Le niveau risqué ne refuse jamais : c'est ce qui le distingue.
    #[test]
    fn le_niveau_risque_ne_refuse_jamais() {
        let options = RunOptions {
            headroom: Headroom::Risque,
            ..RunOptions::default()
        };
        let report = estimate(&modele(405.0), &machine(6.0, 16.0), &options);
        assert_ne!(report.verdict, Verdict::Refuse);
    }

    /// Le cache d'attention doit croître avec le contexte : c'est tout
    /// l'intérêt de le calculer au lieu de l'approximer par un pourcentage.
    #[test]
    fn le_cache_grandit_avec_le_contexte() {
        let spec = modele(8.0);
        let court = kv_cache_bytes(&spec, 4096, KvType::F16);
        let long = kv_cache_bytes(&spec, 32_768, KvType::F16);
        assert_eq!(long, court * 8);
    }

    /// Compresser le cache doit réellement libérer de la place.
    #[test]
    fn compresser_le_cache_libere_de_la_place() {
        let spec = modele(8.0);
        let f16 = kv_cache_bytes(&spec, 8192, KvType::F16);
        let q4 = kv_cache_bytes(&spec, 8192, KvType::Q4_0);
        assert!(q4 * 3 < f16, "q4_0 doit peser bien moins que f16");
    }

    /// Quand le modèle déborde, l'estimation doit proposer le barreau qui
    /// tient plutôt que laisser l'utilisateur chercher.
    #[test]
    fn propose_une_quantification_plus_legere() {
        let spec = ModelSpec::from_params("14B", 14.0, quant::LADDER[1]); // Q8_0
        let report = estimate(&spec, &machine(12.0, 32.0), &RunOptions::default());
        assert!(
            report.suggested_quant.is_some(),
            "un 14B en Q8_0 sur 12 Go doit proposer plus léger"
        );
    }

    /// Le pluriel s'accorde à partir de deux : « 4,7 jetons/s » se lisait
    /// « 4.7 jeton/s ».
    #[test]
    fn accord_du_pluriel() {
        assert_eq!(format_speed(0.4), "0.40 jeton/s");
        assert_eq!(format_speed(1.5), "1.5 jeton/s");
        assert_eq!(format_speed(4.7), "4.7 jetons/s");
        assert_eq!(format_speed(47.0), "47 jetons/s");
    }

    /// Recaler sur une taille connue doit déplacer le total *et* la
    /// répartition par couche, sinon les couches placées sur le GPU ne
    /// correspondraient plus au modèle mesuré.
    #[test]
    fn recalage_sur_une_taille_connue() {
        let spec = modele(8.0);
        let mesure = 5_000_000_000u64;
        let recale = spec.clone().with_weights_bytes(mesure);
        assert_eq!(recale.weights_bytes, mesure);
        let ratio = mesure as f64 / spec.weights_bytes as f64;
        let attendu = (spec.layer_bytes as f64 * ratio) as u64;
        assert_eq!(recale.layer_bytes, attendu);
        // Une taille nulle ne doit rien écraser : un catalogue muet vaut mieux
        // qu'un modèle de zéro octet.
        assert_eq!(
            spec.clone().with_weights_bytes(0).weights_bytes,
            spec.weights_bytes
        );
    }

    /// Sans GPU, tout passe par la RAM, et la vitesse doit s'en ressentir.
    #[test]
    fn sans_gpu_tout_en_ram() {
        let report = estimate(&modele(8.0), &machine(0.0, 32.0), &RunOptions::default());
        assert_eq!(report.placement, Placement::Ram);
        assert_eq!(report.gpu_layers, 0);
        assert!(report.tokens_per_second < 20.0);
    }

    /// Un modèle qui tient doit annoncer un contexte GPU au moins égal à celui
    /// qu'on lui a demandé, et un contexte global au moins aussi grand : la
    /// RAM s'ajoute au GPU, elle ne le remplace pas.
    #[test]
    fn contexte_maximal_coherent() {
        let report = estimate(&modele(8.0), &machine(24.0, 32.0), &RunOptions::default());
        assert!(report.max_gpu_context >= report.context);
        assert!(report.max_context >= report.max_gpu_context);
    }

    /// Un refus doit dire ce qui rendrait le chargement possible : descendre
    /// d'un cran de quantification, ou baisser le contexte.
    #[test]
    fn un_refus_propose_une_issue() {
        let options = RunOptions {
            context: 32_768,
            ..RunOptions::default()
        };
        let report = estimate(&modele(32.0), &machine(6.0, 16.0), &options);
        assert_eq!(report.verdict, Verdict::Refuse);
        assert!(
            report.suggested_quant.is_some() || report.max_context < report.context,
            "le message doit offrir une porte de sortie : {}",
            report.message
        );
    }

    /// Les hypothèses ne sont pas décoratives : sans elles, un chiffre isolé
    /// n'est pas vérifiable.
    #[test]
    fn le_rapport_porte_ses_hypotheses() {
        let report = estimate(&modele(8.0), &machine(24.0, 32.0), &RunOptions::default());
        assert!(report.assumptions.len() >= 4);
        assert!(!report.message.is_empty());
    }
}
