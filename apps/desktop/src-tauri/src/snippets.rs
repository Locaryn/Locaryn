//! Exécuter un bloc de code proposé par le modèle (bouton « Exécuter »).
//!
//! L'ancienne version passait tout par une ligne PowerShell fabriquée côté
//! interface : un bloc `cmd` ou sans langage partait vers Python, un bloc
//! `bash` était donné tel quel à `cmd.exe`, et le code transitait par une
//! chaîne entre guillemets que la moindre apostrophe cassait. Ici, le code va
//! dans un fichier temporaire à la bonne extension et l'interprète du langage
//! le lit directement.

use std::path::{Path, PathBuf};

/// Ce qu'il faut pour exécuter un langage : l'extension du fichier, puis le
/// programme et ses arguments, le chemin du fichier venant à la suite.
#[derive(Debug, PartialEq, Eq)]
pub struct Interprete {
    pub extension: &'static str,
    pub programme: String,
    pub arguments: Vec<String>,
}

fn interprete(programme: &str, arguments: &[&str], extension: &'static str) -> Interprete {
    Interprete {
        extension,
        programme: programme.to_string(),
        arguments: arguments.iter().map(|a| a.to_string()).collect(),
    }
}

/// Le bash de Git pour Windows, s'il est installé : `bash` sur le PATH peut être
/// celui de WSL, qui ne voit pas le dossier courant de la même façon.
fn bash_windows() -> String {
    let candidats = [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ];
    candidats
        .iter()
        .find(|c| Path::new(c).is_file())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "bash".to_string())
}

/// L'interprète d'un langage de bloc Markdown, ou `None` s'il ne s'exécute pas
/// ici. `windows` est passé pour pouvoir tester les deux plateformes.
pub fn pour_langage(lang: &str, windows: bool) -> Option<Interprete> {
    let lang = lang.trim().to_lowercase();
    Some(match lang.as_str() {
        "python" | "py" | "python3" => {
            interprete(if windows { "python" } else { "python3" }, &[], "py")
        }
        "javascript" | "js" | "node" | "mjs" => interprete("node", &[], "js"),
        "powershell" | "ps1" | "ps" | "pwsh" => interprete(
            if windows { "powershell" } else { "pwsh" },
            &["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"],
            "ps1",
        ),
        "cmd" | "bat" | "batch" if windows => interprete("cmd", &["/C"], "bat"),
        "bash" | "sh" | "shell" | "zsh" => {
            if windows {
                Interprete {
                    extension: "sh",
                    programme: bash_windows(),
                    arguments: Vec::new(),
                }
            } else {
                interprete("bash", &[], "sh")
            }
        }
        // Un bloc de console sans langage précis : le shell de la machine.
        "console" | "terminal" | "" => {
            if windows {
                interprete("cmd", &["/C"], "bat")
            } else {
                interprete("sh", &[], "sh")
            }
        }
        _ => return None,
    })
}

/// Un argument entre apostrophes pour PowerShell (`'` s'y double).
fn guillemet_powershell(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Un argument entre apostrophes pour un shell POSIX.
fn guillemet_posix(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// La ligne à taper dans le terminal pour lancer `programme` sur `fichier`.
/// `windows` : le terminal est PowerShell (l'appel d'un programme entre
/// apostrophes passe par `&`) ; sinon un shell POSIX.
pub fn ligne_de_terminal(
    programme: &str,
    arguments: &[String],
    fichier: &str,
    windows: bool,
) -> String {
    let guillemet = if windows {
        guillemet_powershell
    } else {
        guillemet_posix
    };
    let mut ligne = String::new();
    if windows {
        ligne.push_str("& ");
    }
    ligne.push_str(&guillemet(programme));
    for a in arguments {
        ligne.push(' ');
        ligne.push_str(&guillemet(a));
    }
    ligne.push(' ');
    ligne.push_str(&guillemet(fichier));
    ligne
}

/// Supprimer les extraits d'il y a plus d'un jour : le terminal les lit et ne
/// les rend pas, ils s'accumulaient dans le dossier temporaire.
fn nettoyer_anciens(dossier: &std::path::Path) {
    let limite = std::time::Duration::from_secs(24 * 3600);
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return;
    };
    for e in entrees.flatten() {
        let vieux = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limite);
        if vieux {
            if let Err(err) = std::fs::remove_file(e.path()) {
                tracing::debug!(erreur = %err, "ancien extrait non supprimé");
            }
        }
    }
}

/// Un fichier temporaire pour ce bloc, dans le dossier temporaire de
/// l'application (sur le volume des données, pas sur le disque système).
pub fn fichier_temporaire(extension: &str, code: &str) -> std::io::Result<PathBuf> {
    let dossier = locaryn_config::ensure_temp_dir().join("extraits");
    std::fs::create_dir_all(&dossier)?;
    nettoyer_anciens(&dossier);
    let chemin = dossier.join(format!(
        "extrait_{}.{extension}",
        uuid::Uuid::new_v4().simple()
    ));
    // Un .bat lit la page de code de la console : sans `chcp 65001`, les accents
    // d'un `echo` sortaient en caractères cassés.
    let corps = if extension == "bat" {
        format!("@chcp 65001 >nul\r\n{}", code.replace('\n', "\r\n"))
    } else {
        code.to_string()
    };
    std::fs::write(&chemin, corps)?;
    Ok(chemin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ligne_de_terminal_protege_les_apostrophes() {
        let ps = ligne_de_terminal(
            r"C:\Program Files\Git\bin\bash.exe",
            &[],
            r"C:\Users\l'ami\x.sh",
            true,
        );
        assert_eq!(
            ps,
            r"& 'C:\Program Files\Git\bin\bash.exe' 'C:\Users\l''ami\x.sh'"
        );
        let posix = ligne_de_terminal("python3", &["-u".into()], "/tmp/l'ami.py", false);
        assert_eq!(posix, r"'python3' '-u' '/tmp/l'\''ami.py'");
    }

    #[test]
    fn chaque_langage_a_son_interprete() {
        assert_eq!(pour_langage("Python", true).unwrap().programme, "python");
        assert_eq!(pour_langage("py", false).unwrap().programme, "python3");
        assert_eq!(pour_langage("js", true).unwrap().extension, "js");
        let ps = pour_langage("powershell", true).unwrap();
        assert_eq!(ps.arguments.last().map(String::as_str), Some("-File"));
        assert_eq!(pour_langage("cmd", true).unwrap().extension, "bat");
        assert!(
            pour_langage("cmd", false).is_none(),
            "pas de cmd hors Windows"
        );
        assert_eq!(pour_langage("bash", false).unwrap().programme, "bash");
        assert_eq!(pour_langage("", true).unwrap().programme, "cmd");
        assert!(pour_langage("rust", true).is_none());
    }

    #[test]
    fn un_extrait_va_dans_un_fichier_a_la_bonne_extension() {
        let f = fichier_temporaire("py", "print('é')").unwrap();
        assert_eq!(f.extension().and_then(|e| e.to_str()), Some("py"));
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "print('é')");
        let _ = std::fs::remove_file(f);
    }
}
