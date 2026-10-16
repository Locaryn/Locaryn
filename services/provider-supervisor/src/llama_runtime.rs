//! Un programme fourni par une extension, lancé à la place de llama-server.
//!
//! Certaines machines demandent une autre build de llama.cpp, ou des
//! arguments que seul celui qui connaît la puce sait choisir. Plutôt que
//! d'apprendre chaque cas à l'application, une extension déclare dans son
//! manifeste (`llama_runtime`) un programme qui parle la ligne de commande de
//! llama-server, pour les plateformes où il vaut quelque chose.
//!
//! L'application garde tout ce qu'elle sait faire — contexte, cache, marge de
//! mémoire, reprise après plantage — et lance simplement ce programme avec les
//! mêmes arguments. Il reçoit aussi le chemin du moteur géré, pour s'y replier
//! s'il ne peut rien faire de mieux, et le dossier privé de son extension.
//! Rien ici ne nomme une extension ni une puce.

use crate::extension_engine::EngineSource;
use locaryn_extensions::hostpaths;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Variable d'environnement : le llama-server géré par l'application.
pub const ENV_MOTEUR_GERE: &str = "LOCARYN_LLAMA_SERVER";
/// Variable d'environnement : le dossier privé de l'extension.
pub const ENV_DONNEES: &str = "LOCARYN_EXTENSION_DATA_DIR";

/// Le programme de remplacement retenu, avec ce qu'il faut pour le lancer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlamaRuntime {
    pub program: PathBuf,
    pub extension_name: String,
    pub extension_data_dir: PathBuf,
}

static ACTIF: Mutex<Option<LlamaRuntime>> = Mutex::new(None);

/// Le programme qui remplace llama-server en ce moment, s'il y en a un.
pub fn actif() -> Option<LlamaRuntime> {
    ACTIF.lock().ok().and_then(|g| g.clone())
}

/// Retenir le programme de remplacement. Rend vrai s'il a changé.
pub(crate) fn retenir(nouveau: Option<LlamaRuntime>) -> bool {
    let mut g = ACTIF.lock().unwrap_or_else(|e| e.into_inner());
    if *g == nouveau {
        return false;
    }
    *g = nouveau;
    true
}

/// Cette machine, au vocabulaire des manifestes : `windows-aarch64`,
/// `linux-x86_64`, `macos-aarch64`…
///
/// L'architecture est celle de la machine, pas celle de l'application : une
/// application x64 émulée sur un PC ARM doit pouvoir lancer un programme ARM.
pub fn plateforme() -> String {
    format!(
        "{}-{}",
        std::env::consts::OS,
        locaryn_llmfit::hardware::host_arch()
    )
}

/// Le chemin réel d'un programme déclaré : marqueurs remplacés, et suffixe
/// `.exe` ajouté sous Windows quand le manifeste l'a omis.
fn resoudre(gabarit: &str, racine: &Path) -> PathBuf {
    let brut = gabarit
        .replace("{{plugin_root}}", &racine.display().to_string())
        .replace(
            "{{plugin_bin_dir}}",
            &racine.join("bin").display().to_string(),
        );
    let chemin = PathBuf::from(brut);
    let chemin = if chemin.is_absolute() {
        chemin
    } else {
        racine.join(chemin)
    };
    if !chemin.is_file() && !std::env::consts::EXE_SUFFIX.is_empty() {
        let avec = chemin.with_extension(std::env::consts::EXE_EXTENSION);
        if avec.is_file() {
            return avec;
        }
    }
    chemin
}

/// Le programme de remplacement que déclare la première extension active qui
/// en propose un pour cette machine. Un programme absent du paquet est
/// ignoré : mieux vaut le moteur géré qu'un lancement voué à l'échec.
pub fn collect(sources: &[EngineSource]) -> Option<LlamaRuntime> {
    let ici = plateforme();
    for source in sources.iter().filter(|s| s.enabled) {
        let Some(racine) = hostpaths::plugin_root(&source.manifest_path) else {
            continue;
        };
        let Ok(manifest) = locaryn_extensions::manifest::load(&racine) else {
            continue;
        };
        let Some(decl) = manifest.llama_runtime.as_ref() else {
            continue;
        };
        if !decl.platforms.iter().any(|p| p.eq_ignore_ascii_case(&ici)) {
            continue;
        }
        let program = resoudre(&decl.program, &racine);
        if !program.is_file() {
            tracing::warn!(
                extension = %manifest.name,
                programme = %program.display(),
                "programme de remplacement de llama-server absent du paquet — ignoré"
            );
            continue;
        }
        return Some(LlamaRuntime {
            program,
            extension_data_dir: hostpaths::extension_data_dir(&manifest.name),
            extension_name: manifest.name,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paquet(nom: &str, plateformes: &[&str]) -> PathBuf {
        let racine = std::env::temp_dir().join(format!("locaryn-llama-runtime-{nom}"));
        let _ = std::fs::remove_dir_all(&racine);
        std::fs::create_dir_all(racine.join("bin")).unwrap();
        std::fs::write(racine.join("bin").join("relais.exe"), b"").unwrap();
        std::fs::write(racine.join("bin").join("relais"), b"").unwrap();
        let manifeste = serde_json::json!({
            "apiVersion": "0.1",
            "name": nom,
            "version": "1.0.0",
            "llama_runtime": { "program": "{{plugin_bin_dir}}/relais", "platforms": plateformes },
        });
        std::fs::write(racine.join("morph.json"), manifeste.to_string()).unwrap();
        racine
    }

    fn source(racine: &Path, enabled: bool) -> EngineSource {
        EngineSource {
            manifest_path: racine.join("morph.json"),
            enabled,
        }
    }

    #[test]
    fn une_extension_active_de_cette_plateforme_fournit_le_programme() {
        let racine = paquet("relais-ici", &[&plateforme()]);
        let r = collect(&[source(&racine, true)]).expect("programme retenu");
        assert!(r.program.starts_with(&racine), "{}", r.program.display());
        assert_eq!(r.extension_name, "relais-ici");
        let _ = std::fs::remove_dir_all(&racine);
    }

    #[test]
    fn une_autre_plateforme_ou_une_extension_eteinte_ne_change_rien() {
        let ailleurs = paquet("relais-ailleurs", &["plan9-mips"]);
        let eteinte = paquet("relais-eteint", &[&plateforme()]);
        assert!(collect(&[source(&ailleurs, true), source(&eteinte, false)]).is_none());
        let _ = std::fs::remove_dir_all(&ailleurs);
        let _ = std::fs::remove_dir_all(&eteinte);
    }
}
