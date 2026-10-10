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

/// Un fichier temporaire pour ce bloc, dans le dossier temporaire de
/// l'application (sur le volume des données, pas sur le disque système).
pub fn fichier_temporaire(extension: &str, code: &str) -> std::io::Result<PathBuf> {
    let dossier = locaryn_config::ensure_temp_dir().join("extraits");
    std::fs::create_dir_all(&dossier)?;
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
