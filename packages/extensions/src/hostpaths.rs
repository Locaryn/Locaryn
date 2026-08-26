//! Les chemins que l'hôte donne à une extension, et leur nom d'assainissement.
//!
//! Ces valeurs voyagent vers deux endroits différents — l'environnement d'un
//! serveur MCP, et celui d'un moteur d'inférence apporté par une extension —
//! depuis trois hôtes : le bureau, le daemon, le superviseur. Elles vivent ici
//! parce qu'elles doivent être **identiques** partout : le dossier privé d'une
//! extension est l'endroit où elle range son état, et deux hôtes qui le
//! calculent différemment donnent deux dossiers à la même extension. Elle
//! réinstalle alors ce qu'elle avait déjà, ou ne retrouve plus ce qu'elle
//! venait d'écrire.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Le nom d'une extension réduit à ce qu'un nom de dossier accepte partout.
///
/// Les tirets bas sont **conservés** : un manifeste peut les employer
/// (`validate` les autorise), et les remplacer ferait de `mon_ext` et
/// `mon-ext` le même dossier.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Le dossier privé d'une extension, sous la racine de stockage choisie par
/// l'utilisateur. Créé s'il n'existe pas — une extension lancée pour la
/// première fois doit trouver son dossier, pas une erreur d'écriture.
pub fn extension_data_dir(name: &str) -> PathBuf {
    let dir = locaryn_config::storage_root()
        .join("extensions")
        .join(sanitize_name(name));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(
            dossier = %dir.display(),
            erreur = %e,
            "dossier privé de l'extension impossible à créer"
        );
    }
    dir
}

/// Le dossier racine d'une extension installée, depuis le chemin de son
/// manifeste. Accepte un chemin de dossier comme un chemin de `plugin.json`.
pub fn plugin_root(manifest_path: &Path) -> Option<PathBuf> {
    if manifest_path.is_dir() {
        return Some(manifest_path.to_path_buf());
    }
    manifest_path.parent().map(Path::to_path_buf)
}

/// Les variables d'environnement génériques qu'un hôte donne à tout code
/// d'extension qu'il lance — serveur MCP comme moteur d'inférence.
///
/// Génériques est le mot important : l'hôte ne sait pas ce que l'extension en
/// fera. Il donne un dossier privé, la bibliothèque de poids de l'utilisateur,
/// les caches qu'il tient hors du disque système, et sa propre racine.
pub fn generic_env(name: &str, plugin_root: &Path) -> HashMap<String, String> {
    let data_dir = extension_data_dir(name);
    let mut env = HashMap::new();
    let mut put = |k: &str, v: String| {
        env.insert(k.to_string(), v);
    };
    put(
        "LOCARYN_DATA_DIR",
        locaryn_config::storage_root().display().to_string(),
    );
    put(
        "LOCARYN_EXTENSION_DATA_DIR",
        data_dir.display().to_string(),
    );
    put(
        "LOCARYN_EXTENSION_MODELS_DIR",
        data_dir.join("models").display().to_string(),
    );
    put(
        "LOCARYN_EXTENSION_MEDIA_DIR",
        data_dir.join("media").display().to_string(),
    );
    // Sans la bibliothèque de poids de l'utilisateur, une extension ne voit
    // que son dossier privé — vide au premier lancement — et annonce
    // qu'aucun modèle n'est installé alors que tout est déjà téléchargé.
    put(
        "LOCARYN_MODELS_DIR",
        locaryn_config::models_dir().display().to_string(),
    );
    // Les caches que le socle tient hors du disque système. Une extension qui
    // les ignore écrit dans `~/.cache`, et c'est ainsi qu'un disque système se
    // remplit.
    put(
        "LOCARYN_HF_CACHE_DIR",
        locaryn_config::hf_cache_dir().display().to_string(),
    );
    put(
        "LOCARYN_TEMP_DIR",
        locaryn_config::ensure_temp_dir().display().to_string(),
    );
    put(
        "LOCARYN_MODEL_PREFERENCES_FILE",
        locaryn_config::default_data_dir()
            .join("model_preferences.json")
            .display()
            .to_string(),
    );
    // Les réglages de l'extension, tels que l'écran des réglages les a
    // enregistrés. Sans ce chemin, le code d'une extension devine où l'hôte
    // range son fichier de configuration — et cesse de lire les réglages le
    // jour où l'hôte le déplace.
    put(
        "LOCARYN_EXTENSION_CONFIG_FILE",
        plugin_root
            .join(".data")
            .join("config.json")
            .display()
            .to_string(),
    );
    put("LOCARYN_PLUGIN_ROOT", plugin_root.display().to_string());
    put(
        "LOCARYN_PLUGIN_BIN_DIR",
        plugin_root.join("bin").display().to_string(),
    );
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le tiret bas survit : un manifeste peut le porter, et le remplacer
    /// confondrait deux extensions dans un seul dossier.
    #[test]
    fn l_assainissement_garde_tirets_et_tirets_bas() {
        assert_eq!(sanitize_name("plugin-image"), "plugin-image");
        assert_eq!(sanitize_name("mon_ext"), "mon_ext");
        assert_eq!(sanitize_name("a/b c:d"), "a-b-c-d");
    }

    #[test]
    fn la_racine_est_le_dossier_du_manifeste() {
        let root = plugin_root(Path::new("/a/b/plugin.json")).unwrap();
        assert_eq!(root, PathBuf::from("/a/b"));
    }

    #[test]
    fn l_environnement_generique_nomme_la_racine_donnee() {
        let env = generic_env("essai", Path::new("/x/y"));
        assert_eq!(
            env.get("LOCARYN_PLUGIN_ROOT").map(String::as_str),
            Some(PathBuf::from("/x/y").display().to_string().as_str())
        );
        assert!(env.contains_key("LOCARYN_MODELS_DIR"));
        assert!(env.contains_key("LOCARYN_EXTENSION_MODELS_DIR"));
    }
}
