//! Un vrai terminal : un shell persistant dans un pseudo-terminal.
//!
//! L'ancien terminal lançait chaque ligne dans un processus neuf (`cmd /C`) :
//! pas de `cd` qui dure, pas d'entrée pour un programme qui pose une question,
//! pas de couleurs, pas d'édition de ligne — et un bloc proposé par le modèle
//! s'exécutait ailleurs que dans ce que la personne regardait. Ici le shell
//! tourne dans un ConPTY (Windows) ou un PTY (Unix) ; l'interface (xterm.js)
//! lui parle octet pour octet.

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::ipc::Channel;

/// Ce que le terminal envoie à l'interface.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PtyEvent {
    /// Ce que le shell vient d'écrire (séquences d'échappement comprises).
    Data { text: String },
    /// Le shell s'est terminé.
    Exit { code: Option<u32> },
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    ecriture: Box<dyn Write + Send>,
    enfant: Box<dyn Child + Send + Sync>,
}

static SESSIONS: Mutex<Option<HashMap<String, Session>>> = Mutex::new(None);

fn avec_sessions<T>(
    f: impl FnOnce(&mut HashMap<String, Session>) -> Result<T, String>,
) -> Result<T, String> {
    let mut garde = SESSIONS
        .lock()
        .map_err(|_| "terminaux : état illisible".to_string())?;
    f(garde.get_or_insert_with(HashMap::new))
}

/// Un exécutable cherché dans le PATH (extension comprise).
#[cfg(windows)]
fn dans_le_path(nom: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(nom))
        .find(|c| c.is_file())
}

/// Le shell de la personne : PowerShell sous Windows (7 s'il est là), sinon
/// `$SHELL`.
fn shell_par_defaut() -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        if let Some(p) = dans_le_path("pwsh.exe") {
            return (p.display().to_string(), vec!["-NoLogo".into()]);
        }
        let systeme = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let ps = std::path::Path::new(&systeme)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if ps.is_file() {
            return (ps.display().to_string(), vec!["-NoLogo".into()]);
        }
        (
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()),
            Vec::new(),
        )
    }
    #[cfg(not(windows))]
    {
        (
            std::env::var("SHELL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/bin/sh".into()),
            Vec::new(),
        )
    }
}

/// Découper ce qui vient d'être lu en texte, sans couper un caractère en deux :
/// une lecture peut s'arrêter au milieu d'un caractère de plusieurs octets, dont
/// la fin arrive à la lecture suivante.
pub fn decoder(tampon: &mut Vec<u8>) -> String {
    let mut sortie = String::new();
    loop {
        match std::str::from_utf8(tampon) {
            Ok(s) => {
                sortie.push_str(s);
                tampon.clear();
                return sortie;
            }
            Err(e) => {
                let valide = e.valid_up_to();
                sortie.push_str(&String::from_utf8_lossy(&tampon[..valide]));
                match e.error_len() {
                    // Fin de tampon : le caractère est incomplet, on attend la suite.
                    None => {
                        tampon.drain(..valide);
                        return sortie;
                    }
                    // Octets invalides : un caractère de remplacement, et on continue.
                    Some(n) => {
                        sortie.push('\u{fffd}');
                        tampon.drain(..valide + n);
                    }
                }
            }
        }
    }
}

/// Un shell démarré dans un pseudo-terminal : de quoi le lire et lui écrire.
struct Ouvert {
    session: Session,
    lecture: Box<dyn Read + Send>,
}

/// Démarrer le shell de la personne dans un pseudo-terminal, dans `cwd`.
fn ouvrir(cwd: Option<String>, rows: u16, cols: u16) -> Result<Ouvert, String> {
    let paire = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(2),
            cols: cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("terminal indisponible : {e}"))?;

    let (programme, arguments) = shell_par_defaut();
    let mut commande = CommandBuilder::new(&programme);
    commande.args(&arguments);
    commande.env("TERM", "xterm-256color");
    commande.env("COLORTERM", "truecolor");
    if let Some(dir) = cwd
        .filter(|d| !d.is_empty())
        .filter(|d| std::path::Path::new(d).is_dir())
    {
        commande.cwd(dir);
    }
    let enfant = paire
        .slave
        .spawn_command(commande)
        .map_err(|e| format!("« {programme} » n'a pas pu démarrer : {e}"))?;
    // Le côté « esclave » appartient au shell : le garder ici empêcherait la
    // lecture de voir la fin quand il se termine.
    drop(paire.slave);

    let lecture = paire
        .master
        .try_clone_reader()
        .map_err(|e| format!("lecture du terminal impossible : {e}"))?;
    let ecriture = paire
        .master
        .take_writer()
        .map_err(|e| format!("écriture du terminal impossible : {e}"))?;
    Ok(Ouvert {
        session: Session {
            master: paire.master,
            ecriture,
            enfant,
        },
        lecture,
    })
}

/// Ouvrir un terminal dans `cwd`. Rend son identifiant.
#[tauri::command]
pub async fn pty_open(
    cwd: Option<String>,
    rows: u16,
    cols: u16,
    on_event: Channel<PtyEvent>,
) -> Result<String, String> {
    let Ouvert {
        session,
        mut lecture,
    } = ouvrir(cwd, rows, cols)?;

    let id = uuid::Uuid::new_v4().to_string();
    avec_sessions(|s| {
        s.insert(id.clone(), session);
        Ok(())
    })?;

    let id_lecteur = id.clone();
    std::thread::spawn(move || {
        let mut tampon: Vec<u8> = Vec::new();
        let mut bloc = [0u8; 8192];
        loop {
            match lecture.read(&mut bloc) {
                Ok(0) => break,
                Ok(n) => {
                    tampon.extend_from_slice(&bloc[..n]);
                    let texte = decoder(&mut tampon);
                    if !texte.is_empty() && on_event.send(PtyEvent::Data { text: texte }).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!(erreur = %e, "terminal : lecture terminée");
                    break;
                }
            }
        }
        // Le code de sortie, si le shell est encore dans la table : un terminal
        // fermé par la personne y a déjà été retiré.
        let code = avec_sessions(|s| {
            Ok(s.get_mut(&id_lecteur)
                .and_then(|session| session.enfant.try_wait().ok().flatten())
                .map(|statut| statut.exit_code()))
        })
        .ok()
        .flatten();
        let _ = on_event.send(PtyEvent::Exit { code });
    });
    Ok(id)
}

/// Écrire ce que la personne tape (ou colle, ou ce que « Exécuter » envoie).
#[tauri::command]
pub async fn pty_write(id: String, data: String) -> Result<(), String> {
    avec_sessions(|s| {
        let session = s.get_mut(&id).ok_or("terminal fermé")?;
        session
            .ecriture
            .write_all(data.as_bytes())
            .and_then(|()| session.ecriture.flush())
            .map_err(|e| format!("écriture dans le terminal : {e}"))
    })
}

/// Le terminal a changé de taille.
#[tauri::command]
pub async fn pty_resize(id: String, rows: u16, cols: u16) -> Result<(), String> {
    avec_sessions(|s| {
        let session = s.get_mut(&id).ok_or("terminal fermé")?;
        session
            .master
            .resize(PtySize {
                rows: rows.max(2),
                cols: cols.max(10),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("redimensionnement : {e}"))
    })
}

/// Fermer le terminal et ce qui y tourne.
#[tauri::command]
pub async fn pty_close(id: String) -> Result<(), String> {
    let session = avec_sessions(|s| Ok(s.remove(&id)))?;
    if let Some(mut session) = session {
        if let Err(e) = session.enfant.kill() {
            tracing::debug!(erreur = %e, "terminal : shell déjà terminé");
        }
        // `master` tombe ici : le pseudo-terminal se ferme.
        drop(session);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decoder, ouvrir};
    use std::io::{Read, Write};

    /// Le vrai chemin : le shell de cette machine démarre dans un
    /// pseudo-terminal, reçoit une commande tapée et sa sortie revient.
    #[test]
    fn le_shell_execute_une_commande_tapee() {
        let dossier = std::env::temp_dir();
        let mut ouvert = ouvrir(Some(dossier.display().to_string()), 24, 100).expect("shell");
        let (envoi, reception) = std::sync::mpsc::channel::<String>();
        let mut lecture = ouvert.lecture;
        std::thread::spawn(move || {
            let mut tampon = Vec::new();
            let mut bloc = [0u8; 4096];
            while let Ok(n) = lecture.read(&mut bloc) {
                if n == 0 {
                    break;
                }
                tampon.extend_from_slice(&bloc[..n]);
                if envoi.send(decoder(&mut tampon)).is_err() {
                    break;
                }
            }
        });
        // Calculé par le shell : la sortie ne peut pas être le simple écho de
        // la frappe.
        ouvert
            .session
            .ecriture
            .write_all(b"echo locaryn-$(1+1)-ok\r")
            .expect("écriture");
        ouvert.session.ecriture.flush().expect("vidage");
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut vu = String::new();
        let mut repondu = false;
        while std::time::Instant::now() < limite && !vu.contains("locaryn-2-ok") {
            if let Ok(t) = reception.recv_timeout(std::time::Duration::from_millis(500)) {
                vu.push_str(&t);
            }
            // Le pseudo-terminal de Windows demande où est le curseur avant de
            // démarrer, et attend la réponse : xterm.js la donne dans
            // l'application, le test la donne ici.
            if !repondu && vu.contains("\x1b[6n") {
                repondu = true;
                ouvert
                    .session
                    .ecriture
                    .write_all(b"\x1b[1;1R")
                    .expect("réponse au terminal");
                ouvert.session.ecriture.flush().expect("vidage");
            }
        }
        let _ = ouvert.session.enfant.kill();
        assert!(vu.contains("locaryn-2-ok"), "sortie reçue : {vu:?}");
    }

    #[test]
    fn un_caractere_coupe_en_deux_lectures_est_recolle() {
        let mut t: Vec<u8> = Vec::new();
        // « é » = C3 A9, reçu en deux fois.
        t.extend_from_slice(b"caf\xC3");
        assert_eq!(decoder(&mut t), "caf");
        t.extend_from_slice(b"\xA9 ok");
        assert_eq!(decoder(&mut t), "é ok");
        assert!(t.is_empty());
    }

    #[test]
    fn un_octet_invalide_devient_un_caractere_de_remplacement() {
        let mut t: Vec<u8> = b"a\xFFb".to_vec();
        assert_eq!(decoder(&mut t), "a\u{fffd}b");
        assert!(t.is_empty());
    }

    #[test]
    fn les_sequences_d_echappement_passent_telles_quelles() {
        let mut t: Vec<u8> = b"\x1b[32mvert\x1b[0m".to_vec();
        assert_eq!(decoder(&mut t), "\x1b[32mvert\x1b[0m");
    }
}
