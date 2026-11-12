//! Les processus lancés par Locaryn meurent avec lui, même tué net.
//!
//! `kill_on_drop` ne vaut que pour une sortie ordinaire : une application
//! arrêtée par le gestionnaire des tâches, ou qui plante, laissait son
//! `llama-server` tourner, le modèle chargé en mémoire vidéo, sans plus rien
//! pour l'arrêter. Un « objet de tâche » Windows réglé pour tuer à la
//! fermeture y remédie : quand le processus qui le tient disparaît, quelle
//! qu'en soit la raison, le noyau arrête tout ce qui y est rattaché.
//!
//! Ailleurs que sous Windows, ces fonctions ne font rien.

#[cfg(windows)]
mod windows {
    use std::os::windows::io::RawHandle;
    use std::sync::OnceLock;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct IO_COUNTERS {
        ReadOperationCount: u64,
        WriteOperationCount: u64,
        OtherOperationCount: u64,
        ReadTransferCount: u64,
        WriteTransferCount: u64,
        OtherTransferCount: u64,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        PerProcessUserTimeLimit: i64,
        PerJobUserTimeLimit: i64,
        LimitFlags: u32,
        MinimumWorkingSetSize: usize,
        MaximumWorkingSetSize: usize,
        ActiveProcessLimit: u32,
        Affinity: usize,
        PriorityClass: u32,
        SchedulingClass: u32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        IoInfo: IO_COUNTERS,
        ProcessMemoryLimit: usize,
        JobMemoryLimit: usize,
        PeakProcessMemoryLimit: usize,
        PeakJobMemoryLimit: usize,
    }

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;

    type Handle = *mut std::ffi::c_void;
    type Bool = i32;

    extern "system" {
        fn CreateJobObjectW(attributs: *const std::ffi::c_void, nom: *const u16) -> Handle;
        fn SetInformationJobObject(
            tache: Handle,
            classe: u32,
            information: *const std::ffi::c_void,
            taille: u32,
        ) -> Bool;
        fn AssignProcessToJobObject(tache: Handle, processus: Handle) -> Bool;
        fn CloseHandle(objet: Handle) -> Bool;
    }

    /// La poignée de la tâche, gardée ouverte toute la vie du processus : sa
    /// fermeture (à la sortie, quelle qu'elle soit) est ce qui tue les enfants.
    struct Tache(Handle);
    // SAFETY: une poignée de noyau se partage entre fils ; elle n'est jamais
    // fermée tant que le processus vit (elle est dans un `OnceLock` statique).
    unsafe impl Send for Tache {}
    // SAFETY: voir ci-dessus ; les appels qui la lisent sont sûrs en parallèle.
    unsafe impl Sync for Tache {}

    fn tache() -> Option<Handle> {
        static TACHE: OnceLock<Tache> = OnceLock::new();
        let t = TACHE.get_or_init(|| {
            // SAFETY: appels Win32 documentés, paramètres nuls permis, et la
            // structure est mise à zéro avant d'être remplie.
            unsafe {
                let poignee = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if poignee.is_null() {
                    tracing::warn!("objet de tâche Windows non créé : les moteurs survivront à un arrêt forcé");
                    return Tache(std::ptr::null_mut());
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    poignee,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    std::ptr::from_ref(&info).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    tracing::warn!("objet de tâche Windows non réglé : les moteurs survivront à un arrêt forcé");
                    CloseHandle(poignee);
                    return Tache(std::ptr::null_mut());
                }
                Tache(poignee)
            }
        });
        (!t.0.is_null()).then_some(t.0)
    }

    pub fn rattacher(processus: RawHandle) {
        let Some(t) = tache() else { return };
        // SAFETY: `processus` est la poignée d'un enfant vivant que l'appelant
        // tient ; la tâche est ouverte pour toute la vie du processus.
        let ok = unsafe { AssignProcessToJobObject(t, processus.cast()) };
        if ok == 0 {
            tracing::warn!(
                erreur = %std::io::Error::last_os_error(),
                "processus non rattaché à l'objet de tâche : il survivra à un arrêt forcé"
            );
        }
    }
}

/// Rattacher un processus lancé (`std`) : il mourra avec Locaryn.
pub fn rattacher(enfant: &std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle as _;
        windows::rattacher(enfant.as_raw_handle());
    }
    #[cfg(not(windows))]
    let _ = enfant;
}

/// Rattacher un processus lancé (`tokio`) : il mourra avec Locaryn.
pub fn rattacher_tokio(enfant: &tokio::process::Child) {
    #[cfg(windows)]
    if let Some(poignee) = enfant.raw_handle() {
        windows::rattacher(poignee);
    }
    #[cfg(not(windows))]
    let _ = enfant;
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Un enfant rattaché tourne normalement : le rattachement ne le tue pas.
    #[test]
    fn un_enfant_rattache_reste_vivant() {
        let mut enfant = std::process::Command::new("cmd")
            .args(["/C", "ping -n 3 127.0.0.1 >NUL"])
            .spawn()
            .expect("cmd se lance");
        rattacher(&enfant);
        assert!(enfant.try_wait().expect("état lisible").is_none());
        enfant.kill().expect("arrêt");
        enfant.wait().expect("fin");
    }
}
