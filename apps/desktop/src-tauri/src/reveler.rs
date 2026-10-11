//! Montrer un fichier ou un dossier dans l'explorateur du système.
//!
//! « Emplacement » passait le chemin du **fichier** d'un modèle à `explorer`,
//! parfois écrit avec des `/` (`D:/…/modele.gguf`) : l'Explorateur Windows ne
//! comprend pas cette forme et s'ouvrait sur Documents — d'où un bouton qui
//! marchait pour certains modèles et pas pour d'autres. On normalise le
//! chemin, on ouvre le dossier avec le fichier sélectionné, et un fichier
//! disparu laisse place à son dossier.

use std::path::{Path, PathBuf};

/// Ce qu'il faut montrer : un fichier à sélectionner dans son dossier, ou un
/// dossier à ouvrir.
#[derive(Debug, PartialEq, Eq)]
pub enum Cible {
    Fichier(PathBuf),
    Dossier(PathBuf),
}

/// Le chemin écrit à la façon du système : `\` sous Windows.
pub fn normaliser(chemin: &str, windows: bool) -> PathBuf {
    let propre = chemin.trim().trim_matches('"');
    if windows {
        PathBuf::from(propre.replace('/', "\\"))
    } else {
        PathBuf::from(propre)
    }
}

/// La cible la plus proche qui existe : le fichier, sinon le premier dossier
/// parent présent, sinon `repli`.
pub fn cible(chemin: &Path, repli: &Path) -> Cible {
    if chemin.is_file() {
        return Cible::Fichier(chemin.to_path_buf());
    }
    if chemin.is_dir() {
        return Cible::Dossier(chemin.to_path_buf());
    }
    let mut parent = chemin.parent();
    while let Some(p) = parent {
        if p.is_dir() {
            return Cible::Dossier(p.to_path_buf());
        }
        parent = p.parent();
    }
    Cible::Dossier(repli.to_path_buf())
}

/// Ouvrir l'explorateur sur `cible`.
pub fn ouvrir(cible: &Cible) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        let mut commande = std::process::Command::new("explorer");
        match cible {
            // `/select,"chemin"` doit arriver tel quel : la mise entre
            // guillemets automatique de la bibliothèque standard enveloppe tout
            // l'argument, et l'Explorateur ne le reconnaît plus.
            Cible::Fichier(f) => {
                commande.raw_arg(format!("/select,\"{}\"", f.display()));
            }
            Cible::Dossier(d) => {
                commande.arg(d);
            }
        }
        commande.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        let mut commande = std::process::Command::new("open");
        match cible {
            Cible::Fichier(f) => commande.arg("-R").arg(f),
            Cible::Dossier(d) => commande.arg(d),
        };
        commande.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Pas de sélection standard sous Linux : on ouvre le dossier.
        let dossier = match cible {
            Cible::Fichier(f) => f.parent().map(Path::to_path_buf).unwrap_or_default(),
            Cible::Dossier(d) => d.clone(),
        };
        std::process::Command::new("xdg-open")
            .arg(dossier)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_barres_obliques_deviennent_des_antislash_sous_windows() {
        assert_eq!(
            normaliser("D:/Nouveau dossier (2)/models/Qwen3.5-4B.gguf", true),
            PathBuf::from(r"D:\Nouveau dossier (2)\models\Qwen3.5-4B.gguf")
        );
        assert_eq!(
            normaliser("/home/a/m.gguf", false),
            PathBuf::from("/home/a/m.gguf")
        );
        assert_eq!(normaliser("\"C:/x\"", true), PathBuf::from(r"C:\x"));
    }

    #[test]
    fn un_fichier_disparu_laisse_place_a_son_dossier() {
        let dossier = std::env::temp_dir().join(format!("locaryn-reveler-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let fichier = dossier.join("modele.gguf");
        std::fs::write(&fichier, b"x").unwrap();
        let repli = Path::new("/repli");

        assert_eq!(cible(&fichier, repli), Cible::Fichier(fichier.clone()));
        assert_eq!(cible(&dossier, repli), Cible::Dossier(dossier.clone()));
        assert_eq!(
            cible(&dossier.join("absent.gguf"), repli),
            Cible::Dossier(dossier.clone())
        );
        let _ = std::fs::remove_dir_all(&dossier);
    }
}
