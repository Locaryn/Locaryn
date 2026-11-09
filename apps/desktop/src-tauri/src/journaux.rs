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

/// Les motifs masqués avant tout envoi, et ce qui les remplace.
static MOTIFS: std::sync::LazyLock<Vec<(regex::Regex, &'static str)>> = std::sync::LazyLock::new(
    || {
        [
            (r"(?i)(bearer\s+)[A-Za-z0-9._\-]{12,}", "${1}[masqué]"),
            (
                r"(?i)((?:api[_-]?key|token|secret|password|mot_de_passe|authorization)\W{0,3}[:=]\s*\W?)[^\s,;}]{6,}",
                "${1}[masqué]",
            ),
            (r"\b(sk-[A-Za-z0-9_\-]{6})[A-Za-z0-9_\-]{10,}", "${1}[masqué]"),
            (r"\b(hf_)[A-Za-z0-9]{10,}", "${1}[masqué]"),
            (r"\b(gh[pousr]_)[A-Za-z0-9]{10,}", "${1}[masqué]"),
            (r"\b(AIza)[A-Za-z0-9_\-]{20,}", "${1}[masqué]"),
            (r"\b(xox[abp]-)[A-Za-z0-9\-]{10,}", "${1}[masqué]"),
            (
                r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}",
                "[jeton]",
            ),
            (r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}", "[e-mail]"),
            // Le dossier d'une autre session ou d'un autre compte que le courant.
            (r#"(?i)\b[A-Z]:(\\\\?|/)Users(\\\\?|/)[^\\/\s"'<>]+"#, "~"),
            (r#"/(?:home|Users)/[^/\s"'<>]+"#, "~"),
            (
                r"\b(?:10|127|192\.168|172\.(?:1[6-9]|2\d|3[01])|169\.254)(?:\.\d{1,3}){2,3}\b",
                "[ip]",
            ),
            (r"(?i)\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b", "[mac]"),
        ]
        .into_iter()
        .filter_map(|(m, r)| match regex::Regex::new(m) {
            Ok(re) => Some((re, r)),
            Err(e) => {
                tracing::error!(motif = m, erreur = %e, "motif de masquage invalide");
                None
            }
        })
        .collect()
    },
);

/// Masquer ce qui ne doit pas quitter la machine : le dossier personnel (qui
/// porte le nom de la personne), le nom de session, les e-mails, les adresses
/// du réseau local, les clés et jetons.
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
    for (re, remplacement) in MOTIFS.iter() {
        t = re.replace_all(&t, *remplacement).into_owned();
    }
    for nom in noms_de_session() {
        t = t.replace(&nom, "[utilisateur]");
    }
    t
}

/// Le nom de session et celui de la machine : ils apparaissent hors des chemins
/// (invites de commande, messages d'erreur). Trop courts, ils masqueraient des
/// mots ordinaires.
fn noms_de_session() -> Vec<String> {
    ["USERNAME", "USER", "COMPUTERNAME", "HOSTNAME"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .filter(|n| n.chars().count() >= 4)
        .collect()
}

/// Indices d'une ligne qui porte le contenu d'une demande au modèle : corps
/// JSON d'une requête, gabarit de conversation, prompt rendu.
const MARQUES_DE_PROMPT: &[&str] = &[
    "\"messages\"",
    "\"prompt\"",
    "\"content\"",
    "\"system\"",
    "<|im_start|>",
    "<|start_header_id|>",
    "<start_of_turn>",
    "[INST]",
    "prompt:",
    "formatted prompt",
    "chat_template",
];

/// Retirer du journal du modèle tout ce qui pourrait recopier une demande ou
/// une réponse : seules les lignes techniques partent.
pub fn retirer_prompts(texte: &str) -> String {
    let mut out = String::with_capacity(texte.len());
    let mut retire = false;
    for ligne in texte.lines() {
        let basse = ligne.to_ascii_lowercase();
        let suspecte = MARQUES_DE_PROMPT
            .iter()
            .any(|m| basse.contains(&m.to_ascii_lowercase()));
        if suspecte {
            if !retire {
                out.push_str("[contenu de conversation retiré]\n");
            }
            retire = true;
            continue;
        }
        retire = false;
        out.push_str(ligne);
        out.push('\n');
    }
    out
}

/// Nettoyer un texte avant qu'il quitte la machine : prompts, puis secrets.
pub fn nettoyer_pour_envoi(texte: &str) -> String {
    masquer(&retirer_prompts(texte))
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

/// Ce qui part vers le point de collecte : la machine, les journaux du service
/// et du modèle, les plantages. Pas le journal de l'interface, et aucune ligne
/// qui pourrait porter une demande au modèle.
pub fn rassembler_pour_envoi() -> String {
    let data = locaryn_config::default_data_dir();
    let logs = dossier();
    let mut out = resume_machine();
    section(&mut out, "Service", &logs.join("daemon.log"), 300);
    section(
        &mut out,
        "Plantages du service",
        &data.join("daemon-panic.log"),
        80,
    );
    section(&mut out, "Moteur", &data.join("llama-server.log"), 300);
    section(&mut out, "Plantages", &data.join("crash.log"), 120);
    nettoyer_pour_envoi(&out)
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
    use super::{masquer, nettoyer_pour_envoi, retirer_prompts};

    #[test]
    fn chemins_mails_et_ip_sont_masques() {
        let t = masquer(
            r"ouvre C:\Users\JeanDupont\Documents\a.gguf et /home/marie/x, écris à jean.dupont@exemple.fr depuis 192.168.1.42 (aa:bb:cc:dd:ee:ff) jeton eyJhbGciOiJIUzI1.eyJzdWIiOiIxMjM0.SflKxwRJSMeKKF2QT4",
        );
        assert!(!t.contains("JeanDupont"), "{t}");
        assert!(!t.contains("marie"), "{t}");
        assert!(!t.contains("jean.dupont@"), "{t}");
        assert!(!t.contains("192.168.1.42"), "{t}");
        assert!(!t.contains("aa:bb:cc"), "{t}");
        assert!(!t.contains("SflKxwRJ"), "{t}");
        assert!(t.contains(r"~\Documents\a.gguf"), "{t}");
    }

    #[test]
    fn les_prompts_ne_partent_pas() {
        let journal = "srv  load_model: loaded 32 layers\n{\"messages\":[{\"role\":\"user\",\"content\":\"mon secret\"}]}\n<|im_start|>user\nprompt: raconte\nslot release: id 0 | task 3\n";
        let t = retirer_prompts(journal);
        assert!(!t.contains("mon secret"), "{t}");
        assert!(!t.contains("raconte"), "{t}");
        assert!(t.contains("loaded 32 layers"), "{t}");
        assert!(t.contains("slot release"), "{t}");
        assert!(t.contains("[contenu de conversation retiré]"), "{t}");
    }

    #[test]
    fn le_nettoyage_complet_enchaine_les_deux() {
        let t = nettoyer_pour_envoi("token=abcdef123456\n\"prompt\": \"bonjour\"\nok");
        assert!(!t.contains("abcdef123456") && !t.contains("bonjour"), "{t}");
        assert!(t.ends_with("ok\n"), "{t}");
    }

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
