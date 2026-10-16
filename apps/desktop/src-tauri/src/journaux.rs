//! Les journaux de l'application, à consulter et à transmettre.
//!
//! L'application de bureau n'écrivait son journal que sur la console, invisible
//! une fois installée : un collègue sur Snapdragon qui avait une erreur n'avait
//! rien à envoyer. Le journal va désormais dans `logs/desktop.log` (l'ancien
//! gardé en `desktop.old.log`), et « À propos → Journaux » rassemble en un
//! bloc ce qui sert à comprendre un problème : la machine, la fin de chaque
//! journal, les plantages. Le dossier personnel et tout ce qui
//! ressemble à une clé ou un jeton sont masqués avant.

use std::path::{Path, PathBuf};

/// Dossier des journaux (`~/.locaryn/logs`).
pub fn dossier() -> PathBuf {
    locaryn_config::global_dir().join("logs")
}

/// Le fichier du journal de l'application, préparé pour une nouvelle session.
pub fn fichier_application() -> Option<std::fs::File> {
    let d = dossier();
    std::fs::create_dir_all(&d).ok()?;
    let actuel = d.join("desktop.log");
    if actuel.exists() {
        let _ = std::fs::rename(&actuel, d.join("desktop.old.log"));
    }
    std::fs::File::create(actuel).ok()
}

/// Les dernières lignes d'un fichier (lecture de la fin seulement).
fn fin_de(chemin: &Path, lignes: usize) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(chemin).ok()?;
    let taille = f.metadata().ok()?.len();
    let debut = taille.saturating_sub(256 * 1024);
    f.seek(SeekFrom::Start(debut)).ok()?;
    let mut octets = Vec::new();
    f.read_to_end(&mut octets).ok()?;
    let texte = String::from_utf8_lossy(&octets);
    let toutes: Vec<&str> = texte.lines().collect();
    let depart = toutes.len().saturating_sub(lignes);
    Some(toutes[depart..].join("\n"))
}

/// Masquer ce qui ne doit pas quitter la machine : le dossier personnel (qui
/// porte le nom de la personne), les clés et jetons.
pub fn masquer(texte: &str) -> String {
    let mut t = texte.to_string();
    if let Some(home) = dirs_home() {
        for forme in [
            home.clone(),
            home.replace('\\', "/"),
            home.replace('\\', "\\\\"),
        ] {
            if forme.len() > 3 {
                t = t.replace(&forme, "~");
            }
        }
    }
    let motifs = [
        r"(?i)(bearer\s+)[A-Za-z0-9._\-]{12,}",
        r"(?i)((?:api[_-]?key|token|secret|password|mot_de_passe|authorization)\W{0,3}[:=]\s*\W?)[^\s,;}]{6,}",
        r"\b(sk-[A-Za-z0-9_\-]{6})[A-Za-z0-9_\-]{10,}",
        r"\b(hf_)[A-Za-z0-9]{10,}",
        r"\b(gh[pousr]_)[A-Za-z0-9]{10,}",
        r"\b(AIza)[A-Za-z0-9_\-]{20,}",
    ];
    for m in motifs {
        if let Ok(re) = regex::Regex::new(m) {
            t = re.replace_all(&t, "${1}[masqué]").into_owned();
        }
    }
    t
}

fn dirs_home() -> Option<String> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .filter(|h| !h.is_empty())
}

fn section(out: &mut String, titre: &str, chemin: &Path, lignes: usize) {
    if let Some(texte) = fin_de(chemin, lignes).filter(|t| !t.trim().is_empty()) {
        out.push_str(&format!("\n===== {titre} ({}) =====\n", chemin.display()));
        out.push_str(&texte);
        out.push('\n');
    }
}

/// Le résumé de la machine, en tête de rapport.
pub fn resume_machine() -> String {
    let p = locaryn_llmfit::profile();
    format!(
        "Locaryn {} — {} {} ({})\nProcesseur : {} cœurs — RAM {:.1} Go ({:.1} libres){}\nGPU : {} — VRAM {:.1} Go ({:.1} libres), {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        if locaryn_llmfit::hardware::host_is_arm64() { "machine ARM64" } else { "machine x64" },
        p.cpu_cores,
        p.total_ram_gb,
        p.free_ram_gb,
        if p.unified_memory { ", mémoire unifiée" } else { "" },
        p.gpu_name.clone().unwrap_or_else(|| "aucun".into()),
        p.total_vram_gb,
        p.free_vram_gb,
        p.backend.label(),
    )
}

/// Tout ce qu'il faut pour comprendre un problème, masqué.
pub fn rassembler() -> String {
    let data = locaryn_config::default_data_dir();
    let logs = dossier();
    let mut out = resume_machine();
    section(&mut out, "Application", &logs.join("desktop.log"), 400);
    section(
        &mut out,
        "Application (lancement précédent)",
        &logs.join("desktop.old.log"),
        120,
    );
    section(&mut out, "Plantages", &data.join("crash.log"), 120);
    section(&mut out, "Service", &logs.join("daemon.log"), 200);
    section(
        &mut out,
        "Plantages du service",
        &data.join("daemon-panic.log"),
        80,
    );
    section(&mut out, "Moteur", &data.join("llama-server.log"), 200);
    masquer(&out)
}

#[tauri::command]
pub fn journaux_rassembler() -> String {
    rassembler()
}

#[tauri::command]
pub fn journaux_dossier() -> String {
    dossier().display().to_string()
}

#[cfg(test)]
mod tests {
    use super::masquer;

    #[test]
    fn les_secrets_sont_masques() {
        let t = masquer("Authorization: Bearer abcdefghijklmnopqrstuvwxyz token=supersecret123 sk-proj1234567890abcdefgh hf_abcdefghijklmnop");
        assert!(!t.contains("abcdefghijklmnopqrstuvwxyz"), "{t}");
        assert!(!t.contains("supersecret123"), "{t}");
        assert!(!t.contains("890abcdefgh"), "{t}");
        assert!(!t.contains("hf_abcdefghijklmnop"), "{t}");
        assert!(t.contains("[masqué]"));
    }
}
