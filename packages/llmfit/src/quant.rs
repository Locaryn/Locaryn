//! L'échelle des quantifications.
//!
//! Deux usages. Avant téléchargement, on ne connaît d'un modèle que son nombre
//! de paramètres : la quantification donne les octets. Et quand un modèle ne
//! tient pas, la bonne réponse n'est presque jamais « choisissez un autre
//! modèle » mais « prenez-le en Q4 plutôt qu'en Q8 » — encore faut-il savoir
//! laquelle descendre en premier.
//!
//! L'échelle va de la meilleure qualité à la plus compressée. On la descend
//! jusqu'à ce que ça tienne, et on s'arrête au premier barreau qui passe.

/// Une quantification, telle qu'on la choisit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quant {
    /// Nom tel qu'il apparaît dans les noms de fichiers (« Q4_K_M »).
    pub name: &'static str,
    /// Bits par poids, moyenne effective sur un modèle réel — tenseurs de
    /// sortie et embeddings compris, qui sont souvent quantifiés autrement.
    pub bits_per_weight: f64,
    /// Qualité conservée, sur 100. Repère relatif tiré des écarts de
    /// perplexité couramment mesurés, pas une note absolue.
    pub quality: u8,
}

/// L'échelle, de la meilleure qualité à la plus compressée.
///
/// Les variantes intermédiaires (`_S`, `_L`) sont omises quand elles
/// n'apportent rien au choix : proposer huit barreaux entre Q4 et Q5 ne rend
/// service à personne.
pub const LADDER: &[Quant] = &[
    Quant {
        name: "F16",
        bits_per_weight: 16.0,
        quality: 100,
    },
    Quant {
        name: "Q8_0",
        bits_per_weight: 8.5,
        quality: 99,
    },
    Quant {
        name: "Q6_K",
        bits_per_weight: 6.56,
        quality: 98,
    },
    Quant {
        name: "Q5_K_M",
        bits_per_weight: 5.67,
        quality: 96,
    },
    Quant {
        name: "Q4_K_M",
        bits_per_weight: 4.83,
        quality: 93,
    },
    Quant {
        name: "IQ4_XS",
        bits_per_weight: 4.25,
        quality: 90,
    },
    Quant {
        name: "Q3_K_M",
        bits_per_weight: 3.91,
        quality: 84,
    },
    Quant {
        name: "IQ3_M",
        bits_per_weight: 3.66,
        quality: 80,
    },
    Quant {
        name: "Q2_K",
        bits_per_weight: 3.35,
        quality: 68,
    },
    Quant {
        name: "IQ2_M",
        bits_per_weight: 2.7,
        quality: 58,
    },
];

/// La quantification par défaut quand rien ne la précise : le compromis que
/// tout le monde publie et que tout le monde télécharge.
pub const DEFAULT: Quant = LADDER[4];

/// Reconnaître une quantification dans un nom de fichier ou une étiquette.
///
/// Les noms réels sont sales (`model-Q4_K_M.gguf`, `q4_k_m`, `Q4_K_S`), et la
/// comparaison se fait donc sans casse et sur le barreau le plus spécifique
/// qui correspond.
pub fn from_label(label: &str) -> Option<Quant> {
    let upper = label.to_ascii_uppercase();
    // Le plus long d'abord : « Q4_K_M » contient « Q4_K », qui contient « Q4 ».
    let mut candidates: Vec<&Quant> = LADDER.iter().collect();
    candidates.sort_by_key(|q| std::cmp::Reverse(q.name.len()));
    if let Some(found) = candidates.iter().find(|q| upper.contains(q.name)) {
        return Some(**found);
    }
    // Familles sans variante nommée : on rattache au barreau le plus proche.
    for (needle, quant) in [
        ("Q8", LADDER[1]),
        ("Q6", LADDER[2]),
        ("Q5", LADDER[3]),
        ("Q4", LADDER[4]),
        ("IQ4", LADDER[5]),
        ("Q3", LADDER[6]),
        ("IQ3", LADDER[7]),
        ("Q2", LADDER[8]),
        ("IQ2", LADDER[9]),
        (
            "F32",
            Quant {
                name: "F32",
                bits_per_weight: 32.0,
                quality: 100,
            },
        ),
        ("BF16", LADDER[0]),
        ("FP16", LADDER[0]),
        ("F16", LADDER[0]),
    ] {
        if upper.contains(needle) {
            return Some(quant);
        }
    }
    None
}

/// Rattacher un type ggml lu dans un fichier au barreau correspondant.
pub fn from_ggml_name(name: &str) -> Quant {
    from_label(name).unwrap_or(DEFAULT)
}

/// Octets des poids pour ce nombre de paramètres à cette quantification.
pub fn weights_bytes(parameters: f64, quant: Quant) -> u64 {
    (parameters * quant.bits_per_weight / 8.0).max(0.0) as u64
}

/// Les barreaux strictement plus compressés que celui-ci, du meilleur au pire.
///
/// Sert à répondre « ça ne tient pas, mais Q4_K_M tiendrait » sans proposer
/// une qualité supérieure à celle que l'utilisateur a déjà refusée faute de
/// place.
pub fn lighter_than(quant: Quant) -> impl Iterator<Item = &'static Quant> {
    LADDER
        .iter()
        .filter(move |q| q.bits_per_weight < quant.bits_per_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L'échelle doit être ordonnée : tout le mécanisme de descente en dépend.
    #[test]
    fn echelle_ordonnee() {
        for pair in LADDER.windows(2) {
            assert!(
                pair[0].bits_per_weight > pair[1].bits_per_weight,
                "{} devrait peser plus que {}",
                pair[0].name,
                pair[1].name
            );
            assert!(
                pair[0].quality >= pair[1].quality,
                "{} devrait valoir mieux que {}",
                pair[0].name,
                pair[1].name
            );
        }
    }

    /// Le barreau le plus spécifique gagne, sinon « Q4_K_M » serait lu « Q4_K ».
    #[test]
    fn reconnait_le_plus_precis() {
        assert_eq!(
            from_label("Meta-Llama-3-8B-Q4_K_M.gguf").unwrap().name,
            "Q4_K_M"
        );
        assert_eq!(from_label("q8_0").unwrap().name, "Q8_0");
        assert_eq!(from_label("model-IQ4_XS.gguf").unwrap().name, "IQ4_XS");
    }

    /// Une variante non listée doit tomber sur sa famille, pas dans le vide :
    /// « Q4_K_S » existe et doit s'estimer comme du Q4.
    #[test]
    fn repli_sur_la_famille() {
        assert_eq!(from_label("Q4_K_S").unwrap().name, "Q4_K_M");
        assert_eq!(from_label("Q5_1").unwrap().name, "Q5_K_M");
        assert!(from_label("safetensors").is_none());
    }

    /// Huit milliards de paramètres en Q4_K_M pèsent environ 4,8 Go : le
    /// chiffre que tout le monde connaît pour un Llama 8B.
    #[test]
    fn taille_dun_8b() {
        let bytes = weights_bytes(8.03e9, DEFAULT) as f64 / 1e9;
        assert!((bytes - 4.85).abs() < 0.3, "obtenu {bytes:.2} Go");
    }

    #[test]
    fn descente_ne_remonte_jamais() {
        let q5 = from_label("Q5_K_M").unwrap();
        for lighter in lighter_than(q5) {
            assert!(lighter.bits_per_weight < q5.bits_per_weight);
        }
    }
}
