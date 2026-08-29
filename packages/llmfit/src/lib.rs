//! `llmfit` — est-ce que ce modèle tourne sur cette machine, et à quelle
//! vitesse.
//!
//! Trois réponses, dans l'ordre où on se les pose :
//!
//! 1. Avant de télécharger : `for_catalog` répond depuis un nombre de
//!    paramètres et une quantification, sans rien lire sur le disque.
//! 2. Avant de charger : `for_file` lit l'en-tête GGUF et donne les vrais
//!    chiffres — couches, têtes de clé, cache d'attention pour le contexte
//!    voulu.
//! 3. Après avoir choisi : chaque rapport porte ses hypothèses, pour que le
//!    chiffre annoncé soit vérifiable au lieu d'être cru sur parole.
//!
//! Tout est natif : aucun binaire externe, aucun service à installer, aucun
//! appel réseau. Le seul coût est le sondage de la machine, fait une fois par
//! session.

pub mod estimate;
pub mod gguf;
pub mod hardware;
pub mod quant;

pub use estimate::{
    estimate, FitReport, Headroom, KvType, ModelSpec, Placement, RunOptions, SpecSource, Verdict,
};
pub use gguf::{read_summary, GgufError, GgufSummary};
pub use hardware::{profile, Backend, HardwareProfile};
pub use quant::{Quant, DEFAULT as DEFAULT_QUANT, LADDER as QUANT_LADDER};

use std::path::Path;

/// Estimer depuis un fichier de poids présent sur le disque.
///
/// Un GGUF est lu pour de vrai. Tout autre format — safetensors, checkpoints
/// PyTorch — n'expose pas ses dimensions aussi simplement : on retombe alors
/// sur la taille du fichier, en le disant.
pub fn for_file(path: &Path, options: &RunOptions) -> FitReport {
    let hardware = profile();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("modèle")
        .to_string();

    let spec = match gguf::read_summary(path) {
        Ok(summary) => ModelSpec::from_gguf(&name, &summary),
        Err(err) => {
            tracing::debug!(%name, erreur = %err, "en-tête illisible, estimation par la taille");
            match spec_from_size(&name, path) {
                Some(spec) => spec,
                None => return introuvable(&name, &hardware, options),
            }
        }
    };
    estimate(&spec, &hardware, options)
}

/// Estimer un modèle du catalogue, pas encore téléchargé.
///
/// `parameters_b` est le nombre de paramètres en milliards, `quant_label` une
/// étiquette du genre « Q4_K_M ». Les dimensions internes sont déduites, ce que
/// le rapport signale.
pub fn for_catalog(
    name: &str,
    parameters_b: f64,
    quant_label: Option<&str>,
    options: &RunOptions,
) -> FitReport {
    let quant = quant_label
        .and_then(quant::from_label)
        .unwrap_or(quant::DEFAULT);
    let spec = ModelSpec::from_params(name, parameters_b, quant);
    estimate(&spec, &profile(), options)
}

/// Estimer plusieurs fiches d'un coup.
///
/// La liste des modèles en compte des centaines : les sonder un par un
/// referait le tour de la machine à chaque ligne. Ici le profil matériel est
/// lu une fois, et le reste n'est que de l'arithmétique.
pub fn for_catalog_batch(
    entries: &[(String, f64, Option<String>)],
    options: &RunOptions,
) -> Vec<FitReport> {
    let hardware = profile();
    entries
        .iter()
        .map(|(name, parameters_b, quant_label)| {
            let quant = quant_label
                .as_deref()
                .and_then(quant::from_label)
                .unwrap_or(quant::DEFAULT);
            estimate(
                &ModelSpec::from_params(name, *parameters_b, quant),
                &hardware,
                options,
            )
        })
        .collect()
}

/// Repli pour un format dont on ne sait pas lire l'en-tête : la taille sur
/// disque, et des dimensions déduites d'elle.
fn spec_from_size(name: &str, path: &Path) -> Option<ModelSpec> {
    let bytes = weight_bytes_on_disk(path);
    if bytes == 0 {
        return None;
    }
    let quant = quant::from_label(name).unwrap_or(quant::DEFAULT);
    // Remonter des octets aux paramètres, puisque c'est le seul chemin qui
    // reste. Faux de quelques pour cent, sans conséquence sur le verdict.
    let parameters_b = (bytes as f64 * 8.0 / quant.bits_per_weight) / 1e9;
    // La taille mesurée prime sur la taille déduite : c'est la seule donnée
    // dure dont on dispose.
    Some(ModelSpec::from_params(name, parameters_b, quant).with_weights_bytes(bytes))
}

/// Taille des poids sur disque. Un modèle peut être un fichier unique ou un
/// dossier de fragments.
pub fn weight_bytes_on_disk(path: &Path) -> u64 {
    if path.is_dir() {
        directory_bytes(path)
    } else {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }
}

fn directory_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            total += if path.is_dir() {
                directory_bytes(&path)
            } else {
                std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
            };
        }
    }
    total
}

/// Ce qu'on répond quand le fichier n'existe pas : surtout pas « confortable ».
fn introuvable(name: &str, hardware: &HardwareProfile, options: &RunOptions) -> FitReport {
    let mut report = estimate(
        &ModelSpec::from_params(name, 0.0, quant::DEFAULT),
        hardware,
        options,
    );
    report.verdict = Verdict::Risque;
    report.placement = Placement::Disque;
    report.tokens_per_second = 0.0;
    report.prompt_tokens_per_second = 0.0;
    report.message = format!(
        "Impossible de mesurer « {name} » : le fichier est introuvable dans le dossier des \
         modèles. Le chargement peut échouer."
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un fichier absent ne doit jamais passer pour chargeable.
    #[test]
    fn fichier_absent_nest_pas_chargeable() {
        let report = for_file(
            Path::new("ce-modele-nexiste-pas.gguf"),
            &RunOptions::default(),
        );
        assert_ne!(report.verdict, Verdict::Confortable);
        assert_eq!(report.tokens_per_second, 0.0);
    }

    /// Le chemin catalogue doit répondre sans toucher au disque, et distinguer
    /// nettement un 3B d'un 70B.
    #[test]
    fn le_catalogue_distingue_les_tailles() {
        let options = RunOptions::default();
        let petit = for_catalog("petit", 3.0, Some("Q4_K_M"), &options);
        let gros = for_catalog("gros", 70.0, Some("Q4_K_M"), &options);
        assert!(gros.weights_gb > petit.weights_gb * 5.0);
        assert_eq!(petit.source, SpecSource::Estime);
    }

    /// Le lot doit donner exactement les mêmes verdicts que les appels un par
    /// un — sinon l'optimisation changerait les réponses.
    #[test]
    fn le_lot_donne_les_memes_reponses() {
        let options = RunOptions::default();
        let entries = vec![
            ("a".to_string(), 8.0, Some("Q4_K_M".to_string())),
            ("b".to_string(), 32.0, Some("Q5_K_M".to_string())),
        ];
        let batch = for_catalog_batch(&entries, &options);
        assert_eq!(batch.len(), 2);
        for (report, (name, params, quant)) in batch.iter().zip(entries.iter()) {
            let solo = for_catalog(name, *params, quant.as_deref(), &options);
            assert_eq!(report.verdict, solo.verdict);
            assert_eq!(report.gpu_layers, solo.gpu_layers);
        }
    }
}
