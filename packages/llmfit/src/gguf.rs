//! Lecture native de l'en-tête GGUF.
//!
//! Estimer « est-ce que ça tourne » à partir de la taille du fichier donne un
//! chiffre faux dès qu'on demande un grand contexte : le cache KV d'un modèle
//! à 8 têtes de clé ne pèse pas le même poids que celui d'un modèle à 64, et
//! la différence se compte en gigaoctets. Les vrais nombres sont écrits dans
//! l'en-tête du fichier ; ce module les lit.
//!
//! Rien n'est chargé en mémoire au-delà de l'en-tête : les tenseurs sont
//! décrits par leurs dimensions, jamais parcourus. Un fichier de 40 Go se lit
//! en quelques millisecondes.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// `GGUF` en petit-boutien.
const MAGIC: u32 = 0x4655_4747;

/// Au-delà, on refuse de dérouler : le fichier est corrompu ou n'est pas du
/// GGUF, et allouer sur sa foi ferait tomber l'application.
const MAX_REASONABLE_COUNT: u64 = 1 << 24;

#[derive(Debug)]
pub enum GgufError {
    Io(std::io::Error),
    NotGguf,
    Unsupported(String),
    Corrupt(&'static str),
}

impl std::fmt::Display for GgufError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GgufError::Io(e) => write!(f, "lecture impossible : {e}"),
            GgufError::NotGguf => write!(f, "ce fichier n'est pas au format GGUF"),
            GgufError::Unsupported(v) => write!(f, "version GGUF non gérée : {v}"),
            GgufError::Corrupt(what) => write!(f, "en-tête GGUF incohérent ({what})"),
        }
    }
}

impl std::error::Error for GgufError {}

impl From<std::io::Error> for GgufError {
    fn from(e: std::io::Error) -> Self {
        GgufError::Io(e)
    }
}

// ============================================================================
// Types ggml
// ============================================================================

/// Taille d'un bloc quantifié : nombre de poids par bloc, et octets occupés.
///
/// C'est la table qui transforme des dimensions en octets. Elle vient des
/// définitions ggml et ne bouge qu'avec elles.
fn block_shape(ggml_type: u32) -> Option<(u64, u64)> {
    Some(match ggml_type {
        0 => (1, 4),      // F32
        1 => (1, 2),      // F16
        2 => (32, 18),    // Q4_0
        3 => (32, 20),    // Q4_1
        6 => (32, 22),    // Q5_0
        7 => (32, 24),    // Q5_1
        8 => (32, 34),    // Q8_0
        9 => (32, 36),    // Q8_1
        10 => (256, 84),  // Q2_K
        11 => (256, 110), // Q3_K
        12 => (256, 144), // Q4_K
        13 => (256, 176), // Q5_K
        14 => (256, 210), // Q6_K
        15 => (256, 292), // Q8_K
        16 => (256, 66),  // IQ2_XXS
        17 => (256, 74),  // IQ2_XS
        18 => (256, 98),  // IQ3_XXS
        19 => (256, 50),  // IQ1_S
        20 => (32, 18),   // IQ4_NL
        21 => (256, 110), // IQ3_S
        22 => (256, 82),  // IQ2_S
        23 => (256, 136), // IQ4_XS
        24 => (1, 1),     // I8
        25 => (1, 2),     // I16
        26 => (1, 4),     // I32
        27 => (1, 8),     // I64
        28 => (1, 8),     // F64
        29 => (256, 56),  // IQ1_M
        30 => (1, 2),     // BF16
        _ => return None,
    })
}

/// Nom lisible d'un type ggml, pour dire à l'utilisateur en quoi son modèle
/// est quantifié sans qu'il ait à connaître les numéros.
pub fn ggml_type_name(ggml_type: u32) -> &'static str {
    match ggml_type {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        6 => "Q5_0",
        7 => "Q5_1",
        8 => "Q8_0",
        9 => "Q8_1",
        10 => "Q2_K",
        11 => "Q3_K",
        12 => "Q4_K",
        13 => "Q5_K",
        14 => "Q6_K",
        15 => "Q8_K",
        16 => "IQ2_XXS",
        17 => "IQ2_XS",
        18 => "IQ3_XXS",
        19 => "IQ1_S",
        20 => "IQ4_NL",
        21 => "IQ3_S",
        22 => "IQ2_S",
        23 => "IQ4_XS",
        29 => "IQ1_M",
        30 => "BF16",
        _ => "inconnu",
    }
}

/// Octets réellement occupés par un tenseur de ces dimensions dans ce type.
fn tensor_bytes(ggml_type: u32, dims: &[u64]) -> Option<u64> {
    let (block_elems, block_bytes) = block_shape(ggml_type)?;
    let elems: u64 = dims
        .iter()
        .copied()
        .try_fold(1u64, |a, d| a.checked_mul(d))?;
    // Les tenseurs quantifiés alignent toujours leur première dimension sur la
    // taille de bloc ; la division entière suffit donc, mais on arrondit au
    // bloc supérieur par prudence sur les modèles exotiques.
    let blocks = elems.div_ceil(block_elems);
    blocks.checked_mul(block_bytes)
}

// ============================================================================
// Valeurs de métadonnées
// ============================================================================

/// Une valeur de métadonnée, réduite à ce dont l'estimation a besoin.
///
/// Les tableaux ne sont pas conservés : le vocabulaire d'un modèle pèse
/// plusieurs mégaoctets de chaînes dont seule la longueur nous intéresse.
#[derive(Debug, Clone)]
pub enum MetaValue {
    UInt(u64),
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// Longueur du tableau, contenu ignoré.
    ArrayLen(u64),
}

impl MetaValue {
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            MetaValue::UInt(v) => u32::try_from(*v).ok(),
            MetaValue::Int(v) => u32::try_from(*v).ok(),
            MetaValue::Float(v) if *v >= 0.0 => Some(*v as u32),
            MetaValue::ArrayLen(v) => u32::try_from(*v).ok(),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MetaValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            MetaValue::Float(v) => Some(*v),
            MetaValue::UInt(v) => Some(*v as f64),
            MetaValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }
}

// ============================================================================
// Résumé exploitable
// ============================================================================

/// Ce que l'en-tête dit du modèle, une fois trié.
///
/// Les champs valent 0 quand le fichier ne les déclare pas : c'est à
/// l'estimateur de décider quoi faire d'une valeur manquante, pas au lecteur
/// d'inventer.
#[derive(Debug, Clone, Default)]
pub struct GgufSummary {
    pub architecture: String,
    pub name: String,
    /// Étiquette de taille déclarée (« 8B », « 30B-A3B »…), si présente.
    pub size_label: String,
    pub n_layer: u32,
    pub n_embd: u32,
    pub n_head: u32,
    pub n_head_kv: u32,
    /// Dimension d'une tête de clé. Déduite d'`n_embd / n_head` si absente.
    pub key_length: u32,
    pub value_length: u32,
    pub n_vocab: u32,
    /// Contexte pour lequel le modèle a été entraîné.
    pub train_context: u32,
    pub expert_count: u32,
    pub expert_used_count: u32,
    /// Nombre de paramètres, calculé depuis les dimensions des tenseurs.
    pub parameters: u64,
    /// Octets des poids, calculés depuis les tenseurs (≠ taille du fichier,
    /// qui inclut l'en-tête et l'alignement).
    pub weights_bytes: u64,
    /// Octets d'un bloc transformeur, moyenne sur les blocs présents.
    pub layer_bytes: u64,
    /// Tout ce qui n'appartient à aucun bloc : embeddings, sortie, normes.
    pub non_layer_bytes: u64,
    /// Le gabarit de conversation declare par le modele, s'il en porte un.
    ///
    /// C'est lui qui dit ce que le modele sait recevoir : un gabarit qui ne
    /// mentionne jamais d'outils n'en placera aucun dans l'invite, quoi que le
    /// client envoie.
    pub chat_template: String,
    /// Type ggml majoritaire en volume — la quantification effective.
    pub dominant_type: u32,
    pub tensor_count: u64,
    pub file_bytes: u64,
}

impl GgufSummary {
    /// Dimension d'une tête de clé, avec le repli habituel quand le fichier ne
    /// la déclare pas.
    pub fn head_dim(&self) -> u32 {
        if self.key_length > 0 {
            self.key_length
        } else if self.n_head > 0 {
            self.n_embd / self.n_head
        } else {
            0
        }
    }

    /// Le modele sait-il recevoir une liste d'outils.
    ///
    /// La reponse est dans son gabarit : celui qui gere les outils parcourt la
    /// liste `tools` pour la decrire au modele. Un gabarit qui ne la nomme
    /// jamais ignore les outils envoyes — le modele repond en prose au lieu
    /// d'appeler quoi que ce soit, et rien dans l'echange ne dit pourquoi.
    ///
    /// Un en-tete sans gabarit ne permet pas de conclure : on renvoie alors
    /// `None` plutot que de trancher a sa place.
    pub fn supports_tools(&self) -> Option<bool> {
        if self.chat_template.is_empty() {
            return None;
        }
        Some(self.chat_template.contains("tools"))
    }

    /// Nom court de la quantification effective (« Q4_K », « Q8_0 »…).
    pub fn quant_name(&self) -> &'static str {
        ggml_type_name(self.dominant_type)
    }

    /// Part des paramètres réellement traversée à chaque jeton.
    ///
    /// Un modèle dense les traverse tous. Un modèle à experts n'en active
    /// qu'une poignée, ce qui change tout au calcul de vitesse — mais rien à
    /// la mémoire, puisque tous les experts doivent être résidents.
    pub fn active_fraction(&self) -> f64 {
        if self.expert_count > 1 && self.expert_used_count > 0 {
            let used = self.expert_used_count as f64 / self.expert_count as f64;
            // Attention et embeddings restent denses : seul le bloc d'experts
            // est creux. Il pèse gros, sans être la totalité du modèle.
            0.15 + 0.85 * used
        } else {
            1.0
        }
    }
}

// ============================================================================
// Lecture
// ============================================================================

struct Reader<R: Read + Seek> {
    inner: R,
}

impl<R: Read + Seek> Reader<R> {
    fn u8(&mut self) -> Result<u8, GgufError> {
        let mut b = [0u8; 1];
        self.inner.read_exact(&mut b)?;
        Ok(b[0])
    }
    fn u16(&mut self) -> Result<u16, GgufError> {
        let mut b = [0u8; 2];
        self.inner.read_exact(&mut b)?;
        Ok(u16::from_le_bytes(b))
    }
    fn u32(&mut self) -> Result<u32, GgufError> {
        let mut b = [0u8; 4];
        self.inner.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    fn u64(&mut self) -> Result<u64, GgufError> {
        let mut b = [0u8; 8];
        self.inner.read_exact(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }
    fn f32(&mut self) -> Result<f32, GgufError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn f64(&mut self) -> Result<f64, GgufError> {
        Ok(f64::from_bits(self.u64()?))
    }
    fn skip(&mut self, n: u64) -> Result<(), GgufError> {
        self.inner.seek(SeekFrom::Current(n as i64))?;
        Ok(())
    }
    /// Chaîne préfixée par sa longueur. Les chaînes du vocabulaire peuvent se
    /// compter en centaines de milliers : `limit` permet de les sauter sans
    /// allouer.
    fn string(&mut self, limit: usize) -> Result<String, GgufError> {
        let len = self.u64()?;
        if len > 1 << 20 {
            return Err(GgufError::Corrupt("chaîne démesurée"));
        }
        if len as usize > limit {
            self.skip(len)?;
            return Ok(String::new());
        }
        let mut buf = vec![0u8; len as usize];
        self.inner.read_exact(&mut buf)?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}

/// Type de valeur GGUF → nombre d'octets fixes, quand il en a.
fn scalar_width(kind: u32) -> Option<u64> {
    Some(match kind {
        0 | 1 | 7 => 1, // u8, i8, bool
        2 | 3 => 2,     // u16, i16
        4..=6 => 4,     // u32, i32, f32
        10..=12 => 8,
        _ => return None,
    })
}

fn read_value<R: Read + Seek>(r: &mut Reader<R>, kind: u32) -> Result<MetaValue, GgufError> {
    Ok(match kind {
        0 => MetaValue::UInt(r.u8()? as u64),
        1 => MetaValue::Int(r.u8()? as i8 as i64),
        2 => MetaValue::UInt(r.u16()? as u64),
        3 => MetaValue::Int(r.u16()? as i16 as i64),
        4 => MetaValue::UInt(r.u32()? as u64),
        5 => MetaValue::Int(r.u32()? as i32 as i64),
        6 => MetaValue::Float(r.f32()? as f64),
        7 => MetaValue::Bool(r.u8()? != 0),
        // 64 Kio, et non 4 Kio : un gabarit de conversation en depasse
        // couramment 4 000 octets, et la lecture rendait alors une chaine vide
        // — le modele paraissait n'en declarer aucun. Les valeurs de chaine
        // sont une trentaine dans un en-tete, le cout reste negligeable.
        8 => MetaValue::Str(r.string(64 * 1024)?),
        9 => {
            let elem = r.u32()?;
            let count = r.u64()?;
            skip_array(r, elem, count)?;
            MetaValue::ArrayLen(count)
        }
        10 => MetaValue::UInt(r.u64()?),
        11 => MetaValue::Int(r.u64()? as i64),
        12 => MetaValue::Float(r.f64()?),
        other => return Err(GgufError::Unsupported(format!("type de valeur {other}"))),
    })
}

/// Passer par-dessus un tableau sans le matérialiser.
///
/// Les types fixes se sautent d'un seul déplacement. Les chaînes, non : leur
/// longueur est écrite devant chacune, il faut donc les parcourir. C'est le
/// seul endroit où la lecture coûte quelque chose, et c'est pour le
/// vocabulaire — quelques mégaoctets, une fois.
fn skip_array<R: Read + Seek>(r: &mut Reader<R>, elem: u32, count: u64) -> Result<(), GgufError> {
    if count > MAX_REASONABLE_COUNT * 32 {
        return Err(GgufError::Corrupt("tableau démesuré"));
    }
    if let Some(width) = scalar_width(elem) {
        r.skip(width.saturating_mul(count))?;
        return Ok(());
    }
    match elem {
        8 => {
            for _ in 0..count {
                let len = r.u64()?;
                if len > 1 << 20 {
                    return Err(GgufError::Corrupt("chaîne de tableau démesurée"));
                }
                r.skip(len)?;
            }
            Ok(())
        }
        9 => Err(GgufError::Unsupported("tableau de tableaux".into())),
        other => Err(GgufError::Unsupported(format!(
            "type d'élément de tableau {other}"
        ))),
    }
}

/// Lire l'en-tête d'un fichier GGUF et en tirer un résumé exploitable.
pub fn read_summary(path: &Path) -> Result<GgufSummary, GgufError> {
    let file = File::open(path)?;
    let file_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut r = Reader {
        inner: BufReader::with_capacity(1 << 20, file),
    };

    if r.u32()? != MAGIC {
        return Err(GgufError::NotGguf);
    }
    let version = r.u32()?;
    // v1 comptait en 32 bits ; v2 et v3 en 64. Les deux se lisent, la
    // différence tient à la largeur de deux compteurs.
    let (tensor_count, kv_count) = match version {
        1 => (r.u32()? as u64, r.u32()? as u64),
        2 | 3 => (r.u64()?, r.u64()?),
        other => return Err(GgufError::Unsupported(format!("v{other}"))),
    };
    if tensor_count > MAX_REASONABLE_COUNT || kv_count > MAX_REASONABLE_COUNT {
        return Err(GgufError::Corrupt("compteurs invraisemblables"));
    }

    let mut meta: HashMap<String, MetaValue> = HashMap::new();
    for _ in 0..kv_count {
        let key = r.string(512)?;
        let kind = r.u32()?;
        let value = read_value(&mut r, kind)?;
        if !key.is_empty() {
            meta.insert(key, value);
        }
    }

    let architecture = meta
        .get("general.architecture")
        .and_then(MetaValue::as_str)
        .unwrap_or("llama")
        .to_string();

    let mut summary = GgufSummary {
        name: meta
            .get("general.name")
            .and_then(MetaValue::as_str)
            .unwrap_or_default()
            .to_string(),
        size_label: meta
            .get("general.size_label")
            .and_then(MetaValue::as_str)
            .unwrap_or_default()
            .to_string(),
        n_layer: arch_u32(&meta, &architecture, "block_count"),
        n_embd: arch_u32(&meta, &architecture, "embedding_length"),
        n_head: arch_u32(&meta, &architecture, "attention.head_count"),
        n_head_kv: arch_u32(&meta, &architecture, "attention.head_count_kv"),
        key_length: arch_u32(&meta, &architecture, "attention.key_length"),
        value_length: arch_u32(&meta, &architecture, "attention.value_length"),
        train_context: arch_u32(&meta, &architecture, "context_length"),
        expert_count: arch_u32(&meta, &architecture, "expert_count"),
        expert_used_count: arch_u32(&meta, &architecture, "expert_used_count"),
        n_vocab: arch_u32(&meta, &architecture, "vocab_size"),
        chat_template: meta
            .get("tokenizer.chat_template")
            .and_then(MetaValue::as_str)
            .unwrap_or_default()
            .to_string(),
        architecture,
        tensor_count,
        file_bytes,
        ..Default::default()
    };
    if summary.n_vocab == 0 {
        summary.n_vocab = meta
            .get("tokenizer.ggml.tokens")
            .and_then(MetaValue::as_u32)
            .unwrap_or(0);
    }
    // Grouped-query attention non déclarée veut dire attention pleine.
    if summary.n_head_kv == 0 {
        summary.n_head_kv = summary.n_head;
    }

    accumulate_tensors(&mut r, tensor_count, &mut summary)?;
    Ok(summary)
}

/// `<arch>.<suffixe>`, la convention de nommage des métadonnées GGUF.
fn arch_u32(meta: &HashMap<String, MetaValue>, arch: &str, suffix: &str) -> u32 {
    meta.get(&format!("{arch}.{suffix}"))
        .and_then(MetaValue::as_u32)
        .unwrap_or(0)
}

/// Parcourir les descripteurs de tenseurs pour compter poids, octets et
/// répartition par bloc.
fn accumulate_tensors<R: Read + Seek>(
    r: &mut Reader<R>,
    tensor_count: u64,
    summary: &mut GgufSummary,
) -> Result<(), GgufError> {
    let mut per_type_bytes: HashMap<u32, u64> = HashMap::new();
    let mut layer_bytes: u64 = 0;
    let mut seen_layers: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut total_bytes: u64 = 0;
    let mut parameters: u64 = 0;

    for _ in 0..tensor_count {
        let name = r.string(256)?;
        let n_dims = r.u32()?;
        if n_dims > 8 {
            return Err(GgufError::Corrupt("tenseur à plus de 8 dimensions"));
        }
        let mut dims = Vec::with_capacity(n_dims as usize);
        for _ in 0..n_dims {
            dims.push(r.u64()?);
        }
        let ggml_type = r.u32()?;
        let _offset = r.u64()?;

        let elems: u64 = dims.iter().copied().fold(1u64, |a, d| a.saturating_mul(d));
        let bytes = tensor_bytes(ggml_type, &dims).unwrap_or(0);
        parameters = parameters.saturating_add(elems);
        total_bytes = total_bytes.saturating_add(bytes);
        *per_type_bytes.entry(ggml_type).or_insert(0) += bytes;

        // `blk.<n>.…` : la convention de nommage des blocs transformeurs.
        if let Some(index) = layer_index(&name) {
            seen_layers.insert(index);
            layer_bytes = layer_bytes.saturating_add(bytes);
        }
    }

    summary.parameters = parameters;
    summary.weights_bytes = total_bytes;
    summary.non_layer_bytes = total_bytes.saturating_sub(layer_bytes);
    let counted = seen_layers.len() as u64;
    summary.layer_bytes = if counted > 0 {
        layer_bytes / counted
    } else {
        0
    };
    if summary.n_layer == 0 {
        summary.n_layer = counted as u32;
    }
    summary.dominant_type = per_type_bytes
        .into_iter()
        .max_by_key(|(_, bytes)| *bytes)
        .map(|(kind, _)| kind)
        .unwrap_or(1);
    Ok(())
}

fn layer_index(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("blk.")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les tailles de bloc sont la base de tout le calcul mémoire : une erreur
    /// ici se propage partout.
    #[test]
    fn tailles_de_bloc_connues() {
        assert_eq!(block_shape(12), Some((256, 144)), "Q4_K");
        assert_eq!(block_shape(14), Some((256, 210)), "Q6_K");
        assert_eq!(block_shape(8), Some((32, 34)), "Q8_0");
        assert_eq!(block_shape(1), Some((1, 2)), "F16");
        assert_eq!(block_shape(999), None, "type inconnu");
    }

    /// Un tenseur Q4_K de 4096×4096 doit peser exactement ce que la table dit,
    /// pas une approximation en « bits par poids ».
    #[test]
    fn octets_dun_tenseur() {
        let bytes = tensor_bytes(12, &[4096, 4096]).unwrap();
        assert_eq!(bytes, (4096 * 4096 / 256) * 144);
    }

    #[test]
    fn index_de_bloc() {
        assert_eq!(layer_index("blk.17.attn_q.weight"), Some(17));
        assert_eq!(layer_index("token_embd.weight"), None);
        assert_eq!(layer_index("output_norm.weight"), None);
    }

    /// Sans clé déclarée, la dimension de tête se déduit ; sinon elle prime.
    #[test]
    fn dimension_de_tete() {
        let mut s = GgufSummary {
            n_embd: 4096,
            n_head: 32,
            ..Default::default()
        };
        assert_eq!(s.head_dim(), 128);
        s.key_length = 64;
        assert_eq!(s.head_dim(), 64);
    }

    /// Un modèle à experts ne traverse qu'une partie de ses poids par jeton :
    /// c'est ce qui le rend rapide malgré sa taille.
    #[test]
    fn fraction_active_des_experts() {
        let dense = GgufSummary::default();
        assert_eq!(dense.active_fraction(), 1.0);
        let moe = GgufSummary {
            expert_count: 128,
            expert_used_count: 8,
            ..Default::default()
        };
        assert!(moe.active_fraction() < 0.25);
        assert!(moe.active_fraction() > 0.1);
    }

    /// Un fichier qui n'est pas du GGUF doit être refusé, pas mal interprété.
    #[test]
    fn refuse_ce_qui_nest_pas_gguf() {
        let dir = std::env::temp_dir().join("locaryn-llmfit-tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("faux.gguf");
        std::fs::write(&path, b"PAS DU GGUF DU TOUT ------------").unwrap();
        assert!(matches!(read_summary(&path), Err(GgufError::NotGguf)));
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod capacites_tests {
    use super::*;

    /// La detection porte sur le gabarit, pas sur le nom du modele.
    #[test]
    fn le_gabarit_dit_si_les_outils_sont_geres() {
        let sans = GgufSummary {
            chat_template: "{% for m in messages %}{{ m.content }}{% endfor %}".into(),
            ..Default::default()
        };
        assert_eq!(sans.supports_tools(), Some(false));

        let avec = GgufSummary {
            chat_template:
                "{% if tools %}{% for t in tools %}{{ t.function.name }}{% endfor %}{% endif %}"
                    .into(),
            ..Default::default()
        };
        assert_eq!(avec.supports_tools(), Some(true));

        // Aucun gabarit : on ne conclut pas a sa place.
        assert_eq!(GgufSummary::default().supports_tools(), None);
    }
}
