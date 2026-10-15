//! Le NPU Hexagon des PC Snapdragon.
//!
//! Sur Windows ARM, le moteur Snapdragon de Locaryn (llama.cpp avec le backend
//! Hexagon) est essayé en premier : le NPU lit un prompt bien plus vite que le
//! processeur, sans occuper le GPU. Windows n'en charge les bibliothèques que
//! si leur signature est approuvée (mode test-signing, voir
//! `activer-npu.ps1` dans le dossier du moteur). Quand il refuse — ou que le
//! modèle ne va pas sur le NPU —, l'échec est noté et le moteur Adreno
//! habituel reprend la main ; « Réessayer le NPU » dans les réglages l'efface.

use std::path::PathBuf;

const FICHIER_ECHEC: &str = "npu_echec.json";
/// Mémoire d'une session NPU (Hexagon v75+ : environ 2 Go adressables).
const MEMOIRE_SESSION: u64 = 1_800 * 1024 * 1024;
const SESSIONS_MAX: u64 = 4;

fn chemin_echec() -> PathBuf {
    locaryn_config::default_data_dir().join(FICHIER_ECHEC)
}

const FICHIER_REGLAGE: &str = "npu_reglage.json";

/// Un PC Snapdragon sous Windows : le moteur NPU existe pour lui.
pub fn machine_snapdragon() -> bool {
    cfg!(windows) && locaryn_llmfit::hardware::host_is_arm64()
}

/// La personne n'a pas coupé le NPU dans les réglages.
pub fn voulu() -> bool {
    std::fs::read_to_string(locaryn_config::default_data_dir().join(FICHIER_REGLAGE))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("actif").and_then(|a| a.as_bool()))
        .unwrap_or(true)
}

pub fn regler(actif: bool) -> std::io::Result<()> {
    std::fs::write(
        locaryn_config::default_data_dir().join(FICHIER_REGLAGE),
        serde_json::json!({ "actif": actif }).to_string(),
    )
}

/// Cette machine a un NPU Hexagon utilisable par Locaryn, et la personne ne
/// l'a pas coupé.
pub fn disponible() -> bool {
    machine_snapdragon() && voulu()
}

/// La raison du dernier échec du NPU, s'il a échoué.
pub fn echec() -> Option<String> {
    let t = std::fs::read_to_string(chemin_echec()).ok()?;
    serde_json::from_str::<serde_json::Value>(&t)
        .ok()?
        .get("raison")
        .and_then(|r| r.as_str())
        .map(str::to_string)
}

pub fn noter_echec(raison: &str) {
    tracing::warn!(%raison, "NPU indisponible — retour au moteur Adreno");
    let _ = std::fs::write(
        chemin_echec(),
        serde_json::json!({ "raison": raison }).to_string(),
    );
}

pub fn effacer_echec() {
    let _ = std::fs::remove_file(chemin_echec());
}

/// À essayer pour ce lancement ?
pub fn a_essayer() -> bool {
    disponible() && echec().is_none()
}

/// Nombre de sessions NPU pour porter un modèle de cette taille.
pub fn sessions_pour(octets: u64) -> u64 {
    octets.div_ceil(MEMOIRE_SESSION).clamp(1, SESSIONS_MAX)
}

/// `HTP0,HTP1,…` pour `--device`.
pub fn appareils(sessions: u64) -> String {
    (0..sessions)
        .map(|i| format!("HTP{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Le journal montre-t-il des poids posés sur le NPU ? Sans backend Hexagon
/// chargé (signature refusée, pilote absent), llama.cpp continue sur le
/// processeur sans échouer : il faut le lire pour le savoir.
pub fn journal_montre_le_npu(journal: &str) -> bool {
    journal
        .lines()
        .any(|l| l.contains("HTP") && l.contains("model buffer size"))
}

/// Les formats de poids que le NPU calcule lui-même ; les autres retombent
/// sur le processeur, et le NPU n'apporte alors presque rien.
pub fn format_accelere(nom_fichier: &str) -> bool {
    let n = nom_fichier.to_ascii_uppercase();
    ["Q4_0", "Q4_1", "Q8_0", "MXFP4", "IQ4_NL", "F16"]
        .iter()
        .any(|q| n.contains(q))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_selon_la_taille() {
        assert_eq!(sessions_pour(700 * 1024 * 1024), 1);
        assert_eq!(sessions_pour(4 * 1024 * 1024 * 1024), 3);
        assert_eq!(sessions_pour(40 * 1024 * 1024 * 1024), 4);
        assert_eq!(appareils(3), "HTP0,HTP1,HTP2");
    }

    #[test]
    fn le_journal_dit_si_le_npu_porte_le_modele() {
        assert!(journal_montre_le_npu(
            "load_tensors:      HTP0:0 model buffer size  =  1400.26 MiB"
        ));
        assert!(!journal_montre_le_npu(
            "load_tensors:          CPU model buffer size =   300.00 MiB"
        ));
    }

    #[test]
    fn formats_acceleres() {
        assert!(format_accelere("Llama-3.2-1B-Instruct-Q4_0.gguf"));
        assert!(!format_accelere("Qwen3.5-9B-UD-IQ3_XXS.gguf"));
    }
}
