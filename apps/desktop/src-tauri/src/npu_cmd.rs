//! Réglages du NPU des PC Snapdragon (voir `locaryn_provider_supervisor::npu`).

use crate::Core;
use locaryn_provider_supervisor::{npu, runtime_install};
use locaryn_shared_types::ProviderEngine;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct EtatNpu {
    /// Un PC Snapdragon sous Windows.
    pub snapdragon: bool,
    /// Le NPU n'est pas coupé dans les réglages.
    pub voulu: bool,
    /// Le moteur NPU est téléchargé.
    pub installe: bool,
    /// La raison du dernier échec : le moteur Adreno a pris le relais.
    pub echec: Option<String>,
    /// Le dossier du moteur NPU (avec `activer-npu.ps1`).
    pub dossier: String,
    /// Le dernier chargement réussi sur le NPU : génération Hexagon, poids
    /// posés sur le NPU et sur le processeur.
    pub actif: Option<npu::Constat>,
    /// Le modèle actif, et si le NPU calcule son format de poids.
    pub modele: Option<String>,
    pub format_accelere: bool,
    /// Le processeur, tel que Windows le nomme (« Snapdragon X2 Elite… »).
    pub processeur: Option<String>,
}

/// Le nom du processeur, lu dans le registre de Windows.
fn nom_processeur() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let mut cmd = std::process::Command::new("reg");
    cmd.args([
        "query",
        r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "/v",
        "ProcessorNameString",
    ]);
    locaryn_config::hide_console(&mut cmd);
    let sortie = cmd.output().ok()?;
    let texte = String::from_utf8_lossy(&sortie.stdout);
    texte
        .lines()
        .find(|l| l.contains("ProcessorNameString"))
        .and_then(|l| l.split("REG_SZ").nth(1))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

async fn etat(core: &Core) -> EtatNpu {
    let modele = core
        .storage
        .providers
        .active()
        .await
        .ok()
        .flatten()
        .filter(|p| p.engine == ProviderEngine::LlamaCpp)
        .and_then(|p| p.model);
    EtatNpu {
        snapdragon: npu::machine_snapdragon(),
        voulu: npu::voulu(),
        installe: runtime_install::npu_llama_path().is_file(),
        echec: npu::echec(),
        dossier: runtime_install::npu_llama_dir().display().to_string(),
        actif: npu::actif(),
        format_accelere: modele.as_deref().is_some_and(npu::format_accelere),
        modele,
        processeur: nom_processeur(),
    }
}

#[tauri::command]
pub async fn npu_etat(core: State<'_, Core>) -> Result<EtatNpu, String> {
    Ok(etat(&core).await)
}

/// Activer ou couper le NPU. Le moteur en marche s'arrête : le prochain
/// message le relance avec le bon choix.
#[tauri::command]
pub async fn npu_regler(core: State<'_, Core>, actif: bool) -> Result<EtatNpu, String> {
    npu::regler(actif).map_err(|e| e.to_string())?;
    npu::effacer_echec();
    let _ = core.supervisor.shutdown(&ProviderEngine::LlamaCpp).await;
    Ok(etat(&core).await)
}

/// Oublier l'échec (après `activer-npu.ps1` et un redémarrage, par exemple)
/// et retenter le NPU au prochain chargement.
#[tauri::command]
pub async fn npu_reessayer(core: State<'_, Core>) -> Result<EtatNpu, String> {
    npu::effacer_echec();
    let _ = core.supervisor.shutdown(&ProviderEngine::LlamaCpp).await;
    Ok(etat(&core).await)
}

/// Les lignes du journal du moteur qui concernent le NPU, à transmettre.
#[tauri::command]
pub fn npu_diagnostic() -> String {
    let journal =
        std::fs::read_to_string(locaryn_config::default_data_dir().join("llama-server.log"))
            .unwrap_or_default();
    let mut out = format!(
        "Locaryn {} — NPU voulu : {}, échec : {}\n",
        env!("CARGO_PKG_VERSION"),
        npu::voulu(),
        npu::echec().unwrap_or_else(|| "aucun".into())
    );
    if let Some(a) = npu::actif() {
        out.push_str(&format!(
            "Dernier chargement NPU : {} — {} sessions, {:.0} Mio sur le NPU, {:.0} Mio sur le processeur, arch {}\n",
            a.modele,
            a.sessions,
            a.npu_mib,
            a.cpu_mib,
            a.arch.unwrap_or_else(|| "?".into())
        ));
    }
    out.push_str("--- llama-server.log ---\n");
    out.push_str(&npu::diagnostic(&journal));
    out
}

/// Le nom du modèle converti en Q4_0, à côté de l'original.
pub fn nom_q4_0(modele: &str) -> String {
    let (dossier, fichier) = match modele.rsplit_once('/') {
        Some((d, f)) => (format!("{d}/"), f.to_string()),
        None => (String::new(), modele.to_string()),
    };
    let base = fichier.trim_end_matches(".gguf");
    let haut = base.to_ascii_uppercase();
    // Le format d'origine dans le nom (Q4_K_M, UD-IQ3_XXS…) est remplacé.
    let reperes = [
        "UD-", "IQ", "Q2_", "Q3_", "Q4_", "Q5_", "Q6_", "Q8_", "F16", "BF16",
    ];
    let coupe = reperes
        .iter()
        .filter_map(|r| haut.rfind(r))
        .min()
        .filter(|&i| i > 0);
    let tronc = match coupe {
        Some(i) => base[..i].trim_end_matches(['-', '_', '.']),
        None => base,
    };
    format!("{dossier}{tronc}-Q4_0.gguf")
}

/// Créer une version Q4_0 du modèle, que le NPU calcule lui-même, avec
/// `llama-quantize` du moteur NPU. Rend le nom du nouveau modèle.
#[tauri::command]
pub async fn npu_convertir(modele: String) -> Result<String, String> {
    let outil = runtime_install::npu_llama_dir().join("llama-quantize.exe");
    if !outil.is_file() {
        return Err("Le moteur NPU n'est pas encore installé : chargez d'abord un modèle.".into());
    }
    let dossier = locaryn_config::models_dir();
    let source = dossier.join(&modele);
    if !source.is_file() {
        return Err(format!("Modèle introuvable : {}", source.display()));
    }
    let nouveau = nom_q4_0(&modele);
    let cible = dossier.join(&nouveau);
    if cible.is_file() {
        return Ok(nouveau);
    }
    let mut cmd = tokio::process::Command::new(&outil);
    cmd.arg("--allow-requantize")
        .arg(&source)
        .arg(&cible)
        .arg("Q4_0")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    crate::hide_tokio_console(&mut cmd);
    let sortie = cmd
        .output()
        .await
        .map_err(|e| format!("llama-quantize : {e}"))?;
    if !sortie.status.success() {
        let _ = std::fs::remove_file(&cible);
        let err = String::from_utf8_lossy(&sortie.stderr);
        let fin: Vec<&str> = err.lines().rev().take(4).collect();
        return Err(format!(
            "Conversion impossible : {}",
            fin.into_iter().rev().collect::<Vec<_>>().join(" ")
        ));
    }
    Ok(nouveau)
}

#[cfg(test)]
mod tests {
    use super::nom_q4_0;

    #[test]
    fn le_nom_converti_remplace_le_format() {
        assert_eq!(
            nom_q4_0("unsloth__Qwen3.5-9B-GGUF/Qwen3.5-9B-UD-IQ3_XXS.gguf"),
            "unsloth__Qwen3.5-9B-GGUF/Qwen3.5-9B-Q4_0.gguf"
        );
        assert_eq!(nom_q4_0("Qwen3.5-4B-Q4_K_M.gguf"), "Qwen3.5-4B-Q4_0.gguf");
        assert_eq!(nom_q4_0("modele.gguf"), "modele-Q4_0.gguf");
    }
}
