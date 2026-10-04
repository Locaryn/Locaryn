//! Le démarrage de la fenêtre.
//!
//! Avant, `setup` ouvrait la base, nettoyait des dossiers et préparait les
//! serveurs MCP *sur le fil principal*, alors que la fenêtre existait déjà : la
//! boucle de messages de Windows ne tournait plus, la fenêtre restait noire et
//! le système affichait « ne répond pas » — surtout au premier lancement, quand
//! le disque est froid.
//!
//! Désormais :
//! 1. un écran de lancement (`splash.html`, statique) s'ouvre aussitôt ;
//! 2. l'initialisation lourde tourne sur son propre fil ;
//! 3. la fenêtre principale est créée masquée, une fois le cœur prêt ;
//! 4. l'interface dit `app_ready` quand elle a de quoi s'afficher, et seulement
//!    alors la fenêtre principale paraît et l'écran de lancement disparaît.
//!
//! Un minuteur de sécurité révèle la fenêtre principale même si l'interface ne
//! répond jamais : un écran de lancement éternel serait pire que le défaut qu'il
//! remplace.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SPLASH_LABEL: &str = "splashscreen";
pub const MAIN_LABEL: &str = "main";

/// Délai après lequel la fenêtre principale paraît quoi qu'il arrive.
const REVEAL_DEADLINE: Duration = Duration::from_secs(25);

/// Couleur de fond des fenêtres : la même que l'interface, pour qu'aucun éclair
/// blanc ou noir ne précède le premier affichage.
const BACKGROUND: tauri::window::Color = tauri::window::Color(23, 25, 26, 255);

static REVEALED: AtomicBool = AtomicBool::new(false);

/// Ouvrir l'écran de lancement. À appeler le plus tôt possible, depuis `setup`.
pub fn open_splash(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, SPLASH_LABEL, WebviewUrl::App("splash.html".into()))
        .title("Locaryn")
        .inner_size(440.0, 300.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .center()
        .always_on_top(true)
        .focused(true)
        .background_color(BACKGROUND)
        .build()?;
    Ok(())
}

/// Créer la fenêtre principale, masquée : elle charge l'interface pendant que
/// l'écran de lancement reste devant.
pub fn build_main_window(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, MAIN_LABEL, WebviewUrl::App("index.html".into()))
        .title("Locaryn")
        .inner_size(1400.0, 900.0)
        .min_inner_size(1000.0, 600.0)
        .resizable(true)
        .center()
        .background_color(BACKGROUND)
        .visible(false)
        .build()?;
    arm_deadline(app.clone());
    Ok(())
}

fn arm_deadline(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(REVEAL_DEADLINE).await;
        if !REVEALED.load(Ordering::SeqCst) {
            tracing::warn!(
                "l'interface n'a pas signalé qu'elle était prête : fenêtre révélée d'office"
            );
            reveal_main(&app);
        }
    });
}

/// Faire paraître la fenêtre principale et retirer l'écran de lancement.
/// Sans effet la seconde fois.
pub fn reveal_main(app: &AppHandle) {
    if REVEALED.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(main) = app.get_webview_window(MAIN_LABEL) {
        if let Err(e) = main.show() {
            tracing::warn!(error = %e, "impossible d'afficher la fenêtre principale");
        }
        let _ = main.set_focus();
    }
    close_splash(app);
}

fn close_splash(app: &AppHandle) {
    if let Some(splash) = app.get_webview_window(SPLASH_LABEL) {
        // `destroy` et non `close` : la fermeture passe par `CloseRequested`,
        // que l'application intercepte pour ranger la fenêtre dans la zone de
        // notification.
        if let Err(e) = splash.destroy() {
            tracing::warn!(error = %e, "impossible de fermer l'écran de lancement");
        }
    }
}

/// L'interface a de quoi s'afficher : on peut montrer la fenêtre principale.
#[tauri::command]
pub async fn app_ready(app: AppHandle) {
    reveal_main(&app);
}

/// Le démarrage a échoué : le dire clairement, puis quitter. Sans cela, l'écran
/// de lancement resterait affiché devant une application qui ne viendra pas.
pub fn fail(app: &AppHandle, message: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    tracing::error!(error = %message, "démarrage de Locaryn impossible");
    close_splash(app);
    let handle = app.clone();
    app.dialog()
        .message(format!(
            "Locaryn n'a pas pu démarrer.\n\n{message}\n\nSi le problème persiste, le journal se trouve dans le dossier de données de Locaryn."
        ))
        .title("Locaryn")
        .kind(MessageDialogKind::Error)
        .show(move |_| handle.exit(1));
}

/// Remettre au premier plan la fenêtre qui existe : la principale, ou à défaut
/// l'écran de lancement quand une seconde instance se lance pendant le démarrage.
pub fn focus_existing(app: &AppHandle) {
    let window = app
        .get_webview_window(MAIN_LABEL)
        .or_else(|| app.get_webview_window(SPLASH_LABEL));
    if let Some(w) = window {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
