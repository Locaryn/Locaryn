//! Mise à jour de l'application Android.
//!
//! L'updater de Tauri ne couvre pas Android : il installe des paquets de
//! bureau. Sur un téléphone, la mise à jour d'une application distribuée hors
//! magasin passe par le gestionnaire de paquets du système, qu'on atteint en
//! ouvrant le fichier `.apk` — Android affiche alors sa propre demande
//! d'installation, signature vérifiée par lui.
//!
//! L'application ne télécharge donc rien elle-même et n'installe rien : elle
//! compare sa version à celle publiée, et si une version plus récente existe,
//! elle passe la main au système. C'est aussi ce qui garde la décision chez la
//! personne : rien ne s'installe sans qu'elle ait vu l'écran d'Android.

use serde::Serialize;

/// Le manifeste que publie chaque release.
const MANIFEST_URL: &str =
    "https://github.com/Locaryn/locaryn/releases/latest/download/latest.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateStatus {
    /// Version installée sur ce téléphone.
    pub current: String,
    /// Dernière version publiée, quand on a pu la lire.
    pub latest: Option<String>,
    /// Vrai quand la version publiée est plus récente que celle installée.
    pub available: bool,
    /// Adresse de l'APK à installer.
    pub download_url: Option<String>,
    /// Ce qui a empêché la vérification, le cas échéant. Dit en français :
    /// c'est affiché tel quel.
    pub error: Option<String>,
}

impl UpdateStatus {
    fn unknown(error: impl Into<String>) -> Self {
        Self {
            current: env!("CARGO_PKG_VERSION").to_string(),
            latest: None,
            available: false,
            download_url: None,
            error: Some(error.into()),
        }
    }
}

/// Compare deux versions `x.y.z`.
///
/// Une comparaison de chaînes dirait que « 0.10.0 » précède « 0.9.0 » : il
/// faut comparer nombre par nombre. Ce qui n'est pas un nombre vaut zéro,
/// plutôt que de faire échouer la vérification sur une version exotique.
fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u32> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Regarde s'il existe une version plus récente.
#[tauri::command]
pub async fn check_update() -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return UpdateStatus::unknown(format!("client réseau indisponible : {e}")),
    };

    let resp = match client.get(MANIFEST_URL).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            return UpdateStatus::unknown(format!(
                "le serveur de mises à jour a répondu {}",
                r.status()
            ))
        }
        Err(_) => {
            return UpdateStatus::unknown(
                "impossible de joindre le serveur de mises à jour — vérifiez la connexion",
            )
        }
    };

    let manifest: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return UpdateStatus::unknown(format!("manifeste illisible : {e}")),
    };

    let latest = manifest["version"].as_str().unwrap_or_default().to_string();
    let url = manifest["platforms"]["android"]["url"]
        .as_str()
        .map(str::to_string);

    // Une version plus récente sans fichier à installer n'est pas une mise à
    // jour : c'est une promesse qu'on ne pourrait pas tenir.
    let available = !latest.is_empty() && is_newer(&latest, &current) && url.is_some();

    UpdateStatus {
        current,
        latest: (!latest.is_empty()).then_some(latest),
        available,
        download_url: url,
        error: None,
    }
}

/// Passe la main au système pour installer la nouvelle version.
///
/// Sur Android, ouvrir une adresse d'APK déclenche le téléchargement puis
/// l'écran d'installation du système — celui qui vérifie la signature et
/// demande confirmation. L'application ne s'installe donc jamais elle-même,
/// et la personne voit toujours ce qui va se passer.
#[tauri::command]
pub async fn open_update(app: tauri::AppHandle, url: String) -> Result<(), String> {
    // Une adresse arbitraire venue de l'interface ne doit pas pouvoir ouvrir
    // n'importe quoi : on n'accepte que les releases du projet.
    if !url.starts_with("https://github.com/Locaryn/") {
        return Err("adresse de mise à jour inattendue".to_string());
    }
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("ouverture impossible : {e}"))
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn les_versions_se_comparent_nombre_par_nombre() {
        assert!(is_newer("0.10.0", "0.9.0"), "0.10 vient après 0.9");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(
            !is_newer("0.2.2", "0.2.2"),
            "la même version n'en est pas une nouvelle"
        );
        assert!(!is_newer("0.2.1", "0.2.2"));
        assert!(is_newer("v0.3.0", "0.2.9"), "le v du tag est toléré");
    }
}
