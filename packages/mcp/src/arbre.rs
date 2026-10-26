//! L'arborescence d'un processus, pour ne rien laisser tourner derrière lui.
//!
//! Arrêter le serveur MCP d'un morph n'arrête pas ce qu'il a lancé : un
//! Python de transcription, un `node` de passerelle continuaient seuls, et
//! sous Windows ils gardaient les fichiers du morph ouverts. Désactiver puis
//! désinstaller ou mettre à jour échouait alors sur « Accès refusé ».
//!
//! On relève les descendants **avant** d'arrêter le parent : une fois celui-ci
//! parti, plus rien ne relie ses enfants à lui.

use std::path::Path;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System, UpdateKind};

/// Un processus repéré : son numéro et l'heure de son démarrage, pour ne pas
/// tuer un inconnu qui aurait hérité du même numéro entre-temps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repere {
    pub pid: u32,
    pub debut: u64,
}

/// Une photographie des processus de la machine.
pub fn photographier() -> System {
    System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()))
}

/// Tous les descendants de `racine` (enfants, petits-enfants…), sans elle.
pub fn descendants(racine: u32) -> Vec<Repere> {
    descendants_dans(&photographier(), &[racine])
}

/// Les descendants de plusieurs racines dans une photographie donnée.
pub fn descendants_dans(sys: &System, racines: &[u32]) -> Vec<Repere> {
    let mut trouves: Vec<Repere> = Vec::new();
    let mut a_voir: Vec<u32> = racines.to_vec();
    while let Some(parent) = a_voir.pop() {
        for (pid, p) in sys.processes() {
            let enfant = pid.as_u32();
            let deja = racines.contains(&enfant) || trouves.iter().any(|r| r.pid == enfant);
            if !deja && p.parent().map(Pid::as_u32) == Some(parent) {
                trouves.push(Repere {
                    pid: enfant,
                    debut: p.start_time(),
                });
                a_voir.push(enfant);
            }
        }
    }
    trouves
}

/// Tuer ceux qui vivent encore, et rendre combien l'ont été.
pub fn tuer(reperes: &[Repere]) -> usize {
    if reperes.is_empty() {
        return 0;
    }
    let sys = photographier();
    reperes
        .iter()
        .filter(|r| {
            sys.process(Pid::from_u32(r.pid))
                .filter(|p| p.start_time() == r.debut)
                .is_some_and(|p| p.kill())
        })
        .inspect(|r| tracing::info!(pid = r.pid, "processus resté derrière son parent arrêté"))
        .count()
}

/// Arrêter tout ce qui s'exécute depuis `dossier`, avec sa descendance, et
/// rendre combien de processus l'ont été.
///
/// « Depuis » : son exécutable y est, son dossier courant y est, ou sa ligne
/// de commande y pointe. Le dernier cas compte : un pont lancé par
/// `node <morph>/bridge/server.mjs` ou un `python` lancé par le morph ont
/// leur exécutable ailleurs, et gardaient pourtant le dossier ouvert. Ce
/// processus et ses ancêtres sont épargnés.
pub fn liberer_dossier(dossier: &Path) -> usize {
    let Ok(base) = dossier.canonicalize() else {
        return 0;
    };
    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_exe(UpdateKind::Always)
                .with_cwd(UpdateKind::Always)
                .with_cmd(UpdateKind::Always),
        ),
    );
    let epargnes = moi_et_mes_ancetres(&sys);
    let racines: Vec<u32> = sys
        .processes()
        .iter()
        .filter(|(pid, p)| !epargnes.contains(&pid.as_u32()) && tourne_depuis(p, &base))
        .map(|(pid, _)| pid.as_u32())
        .collect();
    if racines.is_empty() {
        return 0;
    }
    let mut cibles: Vec<Repere> = racines
        .iter()
        .filter_map(|pid| sys.process(Pid::from_u32(*pid)))
        .map(|p| Repere {
            pid: p.pid().as_u32(),
            debut: p.start_time(),
        })
        .collect();
    cibles.extend(
        descendants_dans(&sys, &racines)
            .into_iter()
            .filter(|r| !epargnes.contains(&r.pid)),
    );
    let tues = tuer(&cibles);
    if tues > 0 {
        tracing::info!(tues, dossier = %dossier.display(), "processus du dossier arrêtés pour libérer ses fichiers");
        // Le temps que Windows relâche les fichiers.
        std::thread::sleep(std::time::Duration::from_millis(600));
    }
    tues
}

fn dans(chemin: &Path, base: &Path) -> bool {
    chemin
        .canonicalize()
        .map(|c| c.starts_with(base))
        .unwrap_or_else(|_| chemin.starts_with(base))
}

fn tourne_depuis(p: &sysinfo::Process, base: &Path) -> bool {
    p.exe().is_some_and(|e| dans(e, base))
        || p.cwd().is_some_and(|c| dans(c, base))
        || p.cmd().iter().skip(1).any(|a| {
            let a = Path::new(a);
            a.is_absolute() && dans(a, base)
        })
}

fn moi_et_mes_ancetres(sys: &System) -> Vec<u32> {
    let mut chaine = vec![std::process::id()];
    let mut courant = Pid::from_u32(std::process::id());
    while let Some(parent) = sys.process(courant).and_then(sysinfo::Process::parent) {
        if chaine.contains(&parent.as_u32()) {
            break;
        }
        chaine.push(parent.as_u32());
        courant = parent;
    }
    chaine
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un processus dont seul le dossier courant est dans le dossier libéré
    /// est arrêté ; ce processus-ci, lui aussi dans ce dossier, est épargné.
    #[test]
    fn un_processus_au_dossier_courant_dedans_est_arrete() {
        let dossier = std::env::temp_dir().join(format!("locaryn-arbre-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).expect("dossier d'essai");
        let mut enfant = if cfg!(windows) {
            std::process::Command::new("ping")
                .args(["-n", "30", "127.0.0.1"])
                .current_dir(&dossier)
                .stdout(std::process::Stdio::null())
                .spawn()
        } else {
            std::process::Command::new("sleep")
                .arg("30")
                .current_dir(&dossier)
                .spawn()
        }
        .expect("processus d'essai");
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(liberer_dossier(&dossier) >= 1);
        assert!(!enfant.wait().expect("attente").success());
        if let Err(e) = std::fs::remove_dir_all(&dossier) {
            eprintln!("dossier d'essai non retiré : {e}");
        }
    }

    /// Un enfant lancé par ce processus de test est bien vu comme descendant,
    /// et il est tué.
    #[test]
    fn un_enfant_est_retrouve_puis_arrete() {
        let mut enfant = if cfg!(windows) {
            std::process::Command::new("ping")
                .args(["-n", "30", "127.0.0.1"])
                .stdout(std::process::Stdio::null())
                .spawn()
        } else {
            std::process::Command::new("sleep").arg("30").spawn()
        }
        .expect("processus d'essai");
        let moi = std::process::id();
        let vus = descendants(moi);
        assert!(vus.iter().any(|r| r.pid == enfant.id()), "{vus:?}");
        let cible: Vec<Repere> = vus.into_iter().filter(|r| r.pid == enfant.id()).collect();
        assert_eq!(tuer(&cible), 1);
        let statut = enfant.wait().expect("attente de l'enfant");
        assert!(!statut.success());
    }

    #[test]
    fn un_repere_perime_n_est_pas_tue() {
        let faux = Repere {
            pid: std::process::id(),
            debut: 1,
        };
        assert_eq!(tuer(&[faux]), 0);
    }
}
