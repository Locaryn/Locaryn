//! Le coffre vu de l'interface : lister, définir, effacer les secrets que le
//! modèle utilise sans les voir. Les valeurs ne repartent jamais vers
//! l'interface : on sait seulement qu'un secret existe.

use locaryn_agent_runtime::coffre::{self, SecretInfo};

#[tauri::command]
pub fn secrets_lister() -> Vec<SecretInfo> {
    coffre::lister()
}

#[tauri::command]
pub fn secret_definir(nom: String, libelle: String, valeur: String) -> Result<(), String> {
    coffre::definir(nom.trim(), &valeur, &libelle)
}

#[tauri::command]
pub fn secret_effacer(nom: String) -> Result<(), String> {
    coffre::effacer(&nom)
}
