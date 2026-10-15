//! `locaryn` sur le PATH de l'utilisateur Windows.
//!
//! L'application de bureau installe `locaryn.exe` à côté d'elle, mais ce
//! dossier n'était sur le PATH de personne : taper `locaryn` dans un terminal
//! répondait « n'est pas reconnu ». L'installeur appelle donc
//! `locaryn.exe __path-add` après l'installation (et à chaque mise à jour, qui
//! repasse par lui) et `locaryn.exe __path-remove` avant la désinstallation.
//!
//! Pourquoi pas NSIS ou PowerShell : les chaînes de NSIS s'arrêtent à 1024
//! caractères — un PATH plus long serait tronqué en le réécrivant — et
//! `SetEnvironmentVariable` de PowerShell prévient les fenêtres du changement
//! sans délai : une seule application figée suffit à bloquer l'installation.
//! Ici la valeur est lue et réécrite telle quelle dans le registre
//! (`REG_EXPAND_SZ` conservé), puis l'annonce `WM_SETTINGCHANGE` abandonne
//! toute fenêtre qui ne répond pas, et s'arrête au bout de 5 secondes.

/// Traite `__path-add` / `__path-remove`. `None` si ce n'en est pas un.
pub fn depuis_les_arguments() -> Option<i32> {
    let action = std::env::args().nth(1)?;
    let ajouter = match action.as_str() {
        "__path-add" => true,
        "__path-remove" => false,
        _ => return None,
    };
    let dossier = match std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
    {
        Some(d) => d,
        None => return Some(1),
    };
    Some(match imp::modifier(&dossier.to_string_lossy(), ajouter) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("locaryn: PATH non modifié : {e}");
            1
        }
    })
}

/// La nouvelle valeur du PATH, ou `None` si elle ne change pas.
fn recompose(actuel: &str, dossier: &str, ajouter: bool) -> Option<String> {
    let normal = |s: &str| s.trim().trim_end_matches(['\\', '/']).to_lowercase();
    let cible = normal(dossier);
    let present = actuel.split(';').any(|p| normal(p) == cible);
    if ajouter {
        if present {
            return None;
        }
        let base = actuel.trim_end_matches(';');
        return Some(if base.is_empty() {
            dossier.to_string()
        } else {
            format!("{base};{dossier}")
        });
    }
    if !present {
        return None;
    }
    Some(
        actuel
            .split(';')
            .filter(|p| !p.is_empty() && normal(p) != cible)
            .collect::<Vec<_>>()
            .join(";"),
    )
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_SZ,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    fn large(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct Cle(HKEY);
    impl Drop for Cle {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    /// Prévient Windows que l'environnement a changé, en 5 s au plus.
    ///
    /// Explorer d'abord : c'est lui qui transmet l'environnement aux
    /// terminaux ouverts ensuite depuis le menu Démarrer ou la barre des
    /// tâches. Puis toutes les fenêtres — mais le délai de l'annonce générale
    /// s'applique fenêtre par fenêtre : quelques applications lentes la
    /// faisaient durer 45 s. Elle part donc sur un fil à part, qu'on
    /// n'attend que 5 s ; le registre, lui, est déjà écrit.
    fn annoncer() {
        fn envoyer(fenetre: *mut c_void, delai_ms: u32) {
            let zone = large("Environment");
            let mut resultat = 0usize;
            unsafe {
                SendMessageTimeoutW(
                    fenetre as _,
                    WM_SETTINGCHANGE,
                    0,
                    zone.as_ptr() as isize,
                    SMTO_ABORTIFHUNG,
                    delai_ms,
                    &mut resultat as *mut usize as _,
                )
            };
        }
        for classe in ["Progman", "Shell_TrayWnd"] {
            let nom = large(classe);
            let fenetre = unsafe { FindWindowW(nom.as_ptr(), std::ptr::null()) };
            if !fenetre.is_null() {
                envoyer(fenetre, 2000);
            }
        }
        let (fini, attente) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            envoyer(HWND_BROADCAST, 1000);
            let _ = fini.send(());
        });
        let _ = attente.recv_timeout(Duration::from_secs(5));
    }

    pub fn modifier(dossier: &str, ajouter: bool) -> Result<(), String> {
        let sous_cle = large("Environment");
        let nom = large("Path");
        let mut hkey: HKEY = std::ptr::null_mut();
        let r = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                sous_cle.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                &mut hkey,
            )
        };
        if r != ERROR_SUCCESS {
            return Err(format!("HKCU\\Environment inaccessible (code {r})"));
        }
        let cle = Cle(hkey);

        // Lecture : taille d'abord, puis la valeur, sans limite de longueur.
        let mut genre = 0u32;
        let mut octets = 0u32;
        let r = unsafe {
            RegQueryValueExW(
                cle.0,
                nom.as_ptr(),
                std::ptr::null(),
                &mut genre,
                std::ptr::null_mut(),
                &mut octets,
            )
        };
        let (actuel, genre) = if r == ERROR_FILE_NOT_FOUND {
            (String::new(), REG_EXPAND_SZ)
        } else if r != ERROR_SUCCESS {
            return Err(format!("lecture du PATH impossible (code {r})"));
        } else {
            let mut tampon = vec![0u16; (octets as usize).div_ceil(2) + 1];
            let r = unsafe {
                RegQueryValueExW(
                    cle.0,
                    nom.as_ptr(),
                    std::ptr::null(),
                    &mut genre,
                    tampon.as_mut_ptr().cast(),
                    &mut octets,
                )
            };
            if r != ERROR_SUCCESS {
                return Err(format!("lecture du PATH impossible (code {r})"));
            }
            let fin = tampon.iter().position(|&c| c == 0).unwrap_or(tampon.len());
            let genre = if genre == REG_SZ {
                REG_SZ
            } else {
                REG_EXPAND_SZ
            };
            (String::from_utf16_lossy(&tampon[..fin]), genre)
        };

        let Some(nouveau) = super::recompose(&actuel, dossier, ajouter) else {
            return Ok(());
        };
        let valeur = large(&nouveau);
        let r = unsafe {
            RegSetValueExW(
                cle.0,
                nom.as_ptr(),
                0,
                genre,
                valeur.as_ptr().cast(),
                (valeur.len() * 2) as u32,
            )
        };
        if r != ERROR_SUCCESS {
            return Err(format!("écriture du PATH impossible (code {r})"));
        }
        drop(cle);

        annoncer();
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn modifier(_dossier: &str, _ajouter: bool) -> Result<(), String> {
        Err("réservé à Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::recompose;

    #[test]
    fn ajoute_une_seule_fois() {
        let d = r"C:\Users\a\AppData\Local\Locaryn";
        assert_eq!(
            recompose(r"C:\x;C:\y", d, true).as_deref(),
            Some(r"C:\x;C:\y;C:\Users\a\AppData\Local\Locaryn")
        );
        assert_eq!(
            recompose(r"C:\x;c:\users\a\appdata\local\locaryn\", d, true),
            None
        );
        assert_eq!(recompose("", d, true).as_deref(), Some(d));
        assert_eq!(
            recompose(r"C:\x;", d, true).as_deref(),
            Some(r"C:\x;C:\Users\a\AppData\Local\Locaryn")
        );
    }

    #[test]
    fn retire_sans_toucher_au_reste() {
        let d = r"C:\Users\a\AppData\Local\Locaryn";
        assert_eq!(
            recompose(
                r"C:\x;C:\Users\a\AppData\Local\Locaryn;%USERPROFILE%\bin",
                d,
                false
            )
            .as_deref(),
            Some(r"C:\x;%USERPROFILE%\bin")
        );
        assert_eq!(recompose(r"C:\x", d, false), None);
    }
}
