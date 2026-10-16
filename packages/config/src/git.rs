//! Le suivi Git du travail de l'agent : un dépôt créé dans un projet qui
//! n'en a pas, et Locaryn nommé co-auteur des commits qu'il fait — comme les
//! assistants de code le font, pour qu'on voie sur GitHub ce qui vient de lui.
//! Les deux sont allumés par défaut et se coupent dans Réglages → Général.

use serde_json::json;

const BRANCHE: &str = "git";

/// La ligne ajoutée aux commits de l'agent.
pub const CO_AUTEUR: &str = "Co-Authored-By: Locaryn <noreply@locaryn.dev>";

fn drapeau(cle: &str) -> bool {
    crate::global_value(BRANCHE)
        .and_then(|v| v.get(cle).and_then(serde_json::Value::as_bool))
        .unwrap_or(true)
}

/// Créer un dépôt Git dans un projet qui n'en a pas, dès que l'agent y écrit.
pub fn init_auto() -> bool {
    drapeau("init_auto")
}

/// Ajouter Locaryn en co-auteur des commits que fait l'agent.
pub fn co_auteur() -> bool {
    drapeau("co_auteur")
}

pub fn regler(init_auto: bool, co_auteur: bool) -> Result<(), crate::ConfigError> {
    crate::set_global(
        BRANCHE,
        json!({ "init_auto": init_auto, "co_auteur": co_auteur }),
    )
}
