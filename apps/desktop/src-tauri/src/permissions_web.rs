//! Les demandes d'autorisation des pages (micro, caméra, position…).
//!
//! Sous Windows, WebView2 affichait sa propre bulle, hors du style de l'app.
//! Chaque vue (la fenêtre principale, le navigateur intégré) passe ici : la
//! demande part vers l'interface, qui pose la question dans une fenêtre de
//! l'app, et la réponse revient par `repondre_permission_web`. Une réponse
//! « retenue » est gardée par origine et par type dans le dossier de données.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::Core;

/// L'événement reçu par l'interface.
pub const EVENEMENT: &str = "web-permission-request";
const FICHIER: &str = "permissions-web.json";

/// Ce que l'interface affiche.
#[derive(Debug, Clone, Serialize)]
pub struct DemandePermission {
    pub id: u64,
    /// `microphone`, `camera`, `geolocation`, `notifications`…
    pub kind: &'static str,
    /// L'origine qui demande (`https://exemple.fr`), vide pour l'app elle-même.
    pub origin: String,
    /// `main` pour l'app, le libellé de la vue sinon.
    pub webview: String,
}

/// Une décision retenue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionRetenue {
    pub origin: String,
    pub kind: String,
    pub allow: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Memoire {
    #[serde(default)]
    decisions: BTreeMap<String, bool>,
}

fn cle(origine: &str, type_: &str) -> String {
    format!("{origine}|{type_}")
}

fn chemin(dossier: &Path) -> PathBuf {
    dossier.join(FICHIER)
}

fn lire(dossier: &Path) -> Memoire {
    match std::fs::read_to_string(chemin(dossier)) {
        Ok(texte) => serde_json::from_str(&texte).unwrap_or_else(|e| {
            tracing::warn!(erreur = %e, "autorisations web illisibles : on repart de zéro");
            Memoire::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Memoire::default(),
        Err(e) => {
            tracing::warn!(erreur = %e, "autorisations web non lues");
            Memoire::default()
        }
    }
}

fn ecrire(dossier: &Path, memoire: &Memoire) -> Result<(), String> {
    let texte = serde_json::to_string_pretty(memoire).map_err(|e| e.to_string())?;
    std::fs::write(chemin(dossier), texte).map_err(|e| e.to_string())
}

/// La décision retenue pour cette origine et ce type, s'il y en a une.
fn decision_retenue(app: &AppHandle, origine: &str, type_: &str) -> Option<bool> {
    let core = app.try_state::<Core>()?;
    lire(&core.data_dir)
        .decisions
        .get(&cle(origine, type_))
        .copied()
}

fn retenir(app: &AppHandle, origine: &str, type_: &str, autoriser: bool) -> Result<(), String> {
    let core = app
        .try_state::<Core>()
        .ok_or_else(|| "l'application démarre encore".to_string())?;
    let mut memoire = lire(&core.data_dir);
    memoire.decisions.insert(cle(origine, type_), autoriser);
    ecrire(&core.data_dir, &memoire)
}

/// `https://site.fr/page?x` → `https://site.fr`. Les pages de l'app elle-même
/// (`tauri://`, `http://tauri.localhost`, le serveur de développement) n'ont
/// pas d'origine à montrer : c'est Locaryn qui demande.
fn origine_de(uri: &str) -> String {
    let Ok(url) = url::Url::parse(uri) else {
        return String::new();
    };
    let hote = url.host_str().unwrap_or_default();
    let app_elle_meme = url.scheme() == "tauri"
        || hote == "tauri.localhost"
        || (cfg!(debug_assertions) && (hote == "localhost" || hote == "127.0.0.1"));
    if app_elle_meme {
        return String::new();
    }
    url.origin().ascii_serialization()
}

/// Les décisions retenues, pour les réglages.
#[tauri::command]
pub async fn permissions_web_retenues(
    core: tauri::State<'_, Core>,
) -> Result<Vec<DecisionRetenue>, String> {
    Ok(lire(&core.data_dir)
        .decisions
        .into_iter()
        .filter_map(|(c, allow)| {
            let (origin, kind) = c.rsplit_once('|')?;
            Some(DecisionRetenue {
                origin: origin.to_string(),
                kind: kind.to_string(),
                allow,
            })
        })
        .collect())
}

/// Oublier une décision (ou toutes, sans origine) : la question reviendra.
#[tauri::command]
pub async fn permissions_web_oublier(
    core: tauri::State<'_, Core>,
    origin: Option<String>,
    kind: Option<String>,
) -> Result<(), String> {
    let mut memoire = lire(&core.data_dir);
    match (origin, kind) {
        (Some(o), Some(k)) => {
            memoire.decisions.remove(&cle(&o, &k));
        }
        _ => memoire.decisions.clear(),
    }
    ecrire(&core.data_dir, &memoire)
}

/// La réponse de la personne à une demande affichée.
#[tauri::command]
pub async fn repondre_permission_web(
    app: AppHandle,
    id: u64,
    allow: bool,
    remember: bool,
) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(imp::repondre(id, allow));
    })
    .map_err(|e| e.to_string())?;
    let demande = rx.await.map_err(|e| e.to_string())?;
    if remember {
        if let Some((origine, type_)) = demande {
            retenir(&app, &origine, type_, allow)?;
        }
    }
    Ok(())
}

pub use imp::brancher;

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    use tauri::webview::PlatformWebview;
    use tauri::{AppHandle, Emitter};
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{take_pwstr, PermissionRequestedEventHandler};
    use windows_core::PWSTR;

    use super::{decision_retenue, origine_de, DemandePermission, EVENEMENT};

    struct EnAttente {
        args: ICoreWebView2PermissionRequestedEventArgs,
        report: ICoreWebView2Deferral,
        origine: String,
        type_: &'static str,
    }

    // Les objets COM restent sur le fil de l'interface, où ils sont nés.
    thread_local! {
        static EN_ATTENTE: RefCell<HashMap<u64, EnAttente>> = RefCell::new(HashMap::new());
    }
    static PROCHAIN: AtomicU64 = AtomicU64::new(1);

    fn nom(kind: COREWEBVIEW2_PERMISSION_KIND) -> Option<&'static str> {
        Some(match kind {
            COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => "microphone",
            COREWEBVIEW2_PERMISSION_KIND_CAMERA => "camera",
            COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => "geolocation",
            COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => "notifications",
            COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => "clipboard",
            COREWEBVIEW2_PERMISSION_KIND_OTHER_SENSORS => "sensors",
            COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS => "downloads",
            COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE => "files",
            COREWEBVIEW2_PERMISSION_KIND_LOCAL_FONTS => "fonts",
            COREWEBVIEW2_PERMISSION_KIND_MIDI_SYSTEM_EXCLUSIVE_MESSAGES => "midi",
            COREWEBVIEW2_PERMISSION_KIND_WINDOW_MANAGEMENT => "windows",
            COREWEBVIEW2_PERMISSION_KIND_AUTOPLAY => return None,
            _ => "other",
        })
    }

    /// Prendre en charge les demandes de cette vue. Appelé sur le fil de
    /// l'interface (`with_webview`).
    pub fn brancher(app: AppHandle, vue: String, plateforme: PlatformWebview) {
        let resultat = unsafe {
            plateforme.controller().CoreWebView2().and_then(|coeur| {
                let mut jeton = 0i64;
                let gestionnaire =
                    PermissionRequestedEventHandler::create(Box::new(move |_, args| match args {
                        Some(args) => demande(&app, &vue, &args),
                        None => Ok(()),
                    }));
                coeur.add_PermissionRequested(&gestionnaire, &mut jeton)
            })
        };
        if let Err(e) = resultat {
            tracing::warn!(erreur = %e, "demandes d'autorisation laissées à WebView2");
        }
    }

    fn demande(
        app: &AppHandle,
        vue: &str,
        args: &ICoreWebView2PermissionRequestedEventArgs,
    ) -> windows_core::Result<()> {
        let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
        let mut uri = PWSTR::null();
        unsafe {
            args.PermissionKind(&mut kind)?;
            args.Uri(&mut uri)?;
        }
        let uri = take_pwstr(uri);
        let Some(type_) = nom(kind) else {
            return Ok(());
        };
        let origine = origine_de(&uri);
        // Le presse-papiers de l'app elle-même : wry l'accordait déjà.
        if type_ == "clipboard" && origine.is_empty() {
            return unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW) };
        }
        if let Some(autoriser) = decision_retenue(app, &origine, type_) {
            return unsafe { args.SetState(etat(autoriser)) };
        }
        let report = unsafe { args.GetDeferral()? };
        let id = PROCHAIN.fetch_add(1, Ordering::Relaxed);
        EN_ATTENTE.with(|m| {
            m.borrow_mut().insert(
                id,
                EnAttente {
                    args: args.clone(),
                    report,
                    origine: origine.clone(),
                    type_,
                },
            )
        });
        let message = DemandePermission {
            id,
            kind: type_,
            origin: origine,
            webview: vue.to_string(),
        };
        if let Err(e) = app.emit_to(crate::startup::MAIN_LABEL, EVENEMENT, message) {
            tracing::warn!(erreur = %e, "demande d'autorisation non affichée : refusée");
            repondre(id, false);
        }
        Ok(())
    }

    fn etat(autoriser: bool) -> COREWEBVIEW2_PERMISSION_STATE {
        if autoriser {
            COREWEBVIEW2_PERMISSION_STATE_ALLOW
        } else {
            COREWEBVIEW2_PERMISSION_STATE_DENY
        }
    }

    /// Rendre la réponse à WebView2. Sur le fil de l'interface. Renvoie
    /// l'origine et le type de la demande, pour la retenir.
    pub fn repondre(id: u64, autoriser: bool) -> Option<(String, &'static str)> {
        let attente = EN_ATTENTE.with(|m| m.borrow_mut().remove(&id))?;
        let fin = unsafe {
            attente
                .args
                .SetState(etat(autoriser))
                .and_then(|()| attente.report.Complete())
        };
        if let Err(e) = fin {
            tracing::warn!(erreur = %e, "réponse d'autorisation non transmise");
        }
        Some((attente.origine, attente.type_))
    }
}

#[cfg(not(windows))]
mod imp {
    use tauri::webview::PlatformWebview;
    use tauri::AppHandle;

    /// Ailleurs, le moteur du système pose la question lui-même.
    pub fn brancher(_app: AppHandle, _vue: String, _plateforme: PlatformWebview) {}

    pub fn repondre(_id: u64, _autoriser: bool) -> Option<(String, &'static str)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_app_elle_meme_n_a_pas_d_origine() {
        assert_eq!(origine_de("http://tauri.localhost/index.html"), "");
        assert_eq!(origine_de("tauri://localhost/"), "");
        assert_eq!(
            origine_de("https://meet.exemple.fr/salle?x=1"),
            "https://meet.exemple.fr"
        );
        assert_eq!(origine_de("pas une adresse"), "");
    }

    #[test]
    fn une_decision_retenue_se_relit_et_s_oublie() {
        let dossier = std::env::temp_dir().join(format!("locaryn-perm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dossier).unwrap();
        let mut m = lire(&dossier);
        m.decisions.insert(cle("https://a.fr", "camera"), false);
        ecrire(&dossier, &m).unwrap();
        assert_eq!(
            lire(&dossier).decisions.get(&cle("https://a.fr", "camera")),
            Some(&false)
        );
        std::fs::remove_dir_all(&dossier).unwrap();
    }
}
