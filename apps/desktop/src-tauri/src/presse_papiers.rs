//! Le presse-papiers du système, lu et écrit par l'application.
//!
//! Lu depuis la page (`navigator.clipboard.readText`), il déclenchait sous
//! Windows la demande d'autorisation de WebView2 — un bandeau de navigateur,
//! en haut de la fenêtre, à chaque « Coller ». L'application n'est pas un
//! site : elle passe par le système, comme n'importe quel logiciel.

/// Le texte du presse-papiers (vide s'il n'en contient pas).
#[tauri::command]
pub fn lire_presse_papiers() -> Result<String, String> {
    let mut pp = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match pp.get_text() {
        Ok(t) => Ok(t),
        Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Remplacer le contenu du presse-papiers par `texte`.
#[tauri::command]
pub fn ecrire_presse_papiers(texte: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut pp| pp.set_text(texte))
        .map_err(|e| e.to_string())
}
