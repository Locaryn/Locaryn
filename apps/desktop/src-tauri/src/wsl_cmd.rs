//! Les distributions WSL de la machine : pour la saisie `@wsl:<distribution>`
//! et pour exécuter un bloc de code dans la bonne distribution.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DistributionWsl {
    pub nom: String,
    pub defaut: bool,
    pub etat: String,
}

/// `wsl.exe` écrit en UTF-16 (sans `WSL_UTF8=1` sur les vieilles versions).
fn decoder(octets: &[u8]) -> String {
    let octets = octets.strip_prefix(&[0xFF, 0xFE]).unwrap_or(octets);
    let nuls = octets
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    if octets.len() >= 2 && nuls * 2 >= octets.len() / 2 {
        let u: Vec<u16> = octets
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u).replace('\0', "");
    }
    String::from_utf8_lossy(octets).into_owned()
}

/// Le tableau de `wsl -l -v` (« * Ubuntu  Running  2 »).
pub fn analyser(texte: &str) -> Vec<DistributionWsl> {
    texte
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("NAME") && !l.contains("NOM"))
        .skip(1)
        .filter_map(|ligne| {
            let defaut = ligne.trim_start().starts_with('*');
            let mut champs = ligne
                .trim_start()
                .trim_start_matches('*')
                .split_whitespace();
            let nom = champs.next()?.to_string();
            let etat = champs.next().unwrap_or("").to_string();
            Some(DistributionWsl { nom, defaut, etat })
        })
        .collect()
}

#[tauri::command]
pub async fn wsl_distributions() -> Vec<DistributionWsl> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut cmd = tokio::process::Command::new("wsl.exe");
    cmd.args(["-l", "-v"])
        .env("WSL_UTF8", "1")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    crate::hide_tokio_console(&mut cmd);
    match tokio::time::timeout(std::time::Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(sortie)) if sortie.status.success() => analyser(&decoder(&sortie.stdout)),
        _ => Vec::new(),
    }
}

/// L'interprète Linux d'un bloc, ou `None` s'il ne se lance pas sous WSL
/// (PowerShell, invite de commandes : ce sont des blocs Windows).
pub fn interprete_linux(langage: &str) -> Option<&'static str> {
    Some(match langage.to_ascii_lowercase().as_str() {
        "bash" | "sh" | "shell" | "zsh" | "console" | "" => "bash",
        "python" | "py" | "python3" => "python3",
        "javascript" | "js" | "node" => "node",
        _ => return None,
    })
}

/// Les arguments de `wsl.exe` qui exécutent le fichier Windows `fichier` dans
/// `distribution` (vide : celle par défaut), depuis le dossier personnel.
pub fn arguments(distribution: &str, interprete: &str, fichier: &str) -> Vec<String> {
    let mut a = Vec::new();
    if !distribution.trim().is_empty() {
        a.extend(["-d".to_string(), distribution.trim().to_string()]);
    }
    a.extend([
        "--cd".into(),
        "~".into(),
        "--exec".into(),
        "bash".into(),
        "-lc".into(),
    ]);
    // Le chemin Windows du fichier, traduit par wslpath dans la distribution.
    a.push(format!(
        r#"f="$(wslpath -u "$1")" && exec {interprete} "$f""#
    ));
    a.push("locaryn".into());
    a.push(fichier.to_string());
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_les_distributions() {
        let t = "  NAME            STATE           VERSION\n* Ubuntu          Stopped         2\n  Ubuntu-22.04    Running         2\n";
        let d = analyser(t);
        assert_eq!(d.len(), 2);
        assert!(d[0].defaut);
        assert_eq!(d[1].nom, "Ubuntu-22.04");
    }

    #[test]
    fn un_bloc_bash_part_dans_la_distribution() {
        let a = arguments("Ubuntu-22.04", "bash", r"C:\Temp\x.sh");
        assert_eq!(&a[..2], ["-d", "Ubuntu-22.04"]);
        assert_eq!(a.last().map(String::as_str), Some(r"C:\Temp\x.sh"));
        assert!(arguments("", "bash", "x")
            .first()
            .is_some_and(|s| s == "--cd"));
        assert_eq!(interprete_linux("ps1"), None);
        assert_eq!(interprete_linux("py"), Some("python3"));
    }
}
