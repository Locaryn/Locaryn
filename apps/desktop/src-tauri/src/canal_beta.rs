//! Le canal bêta de l'application.
//!
//! Le canal stable lit `releases/latest/download/latest.json` (tauri.conf.json)
//! : GitHub n'y sert jamais une pré-version. Le canal bêta prend la release la
//! plus récente, pré-versions comprises, qui publie un `latest.json`, et
//! interroge l'updater avec ce manifeste-là. Tant qu'aucune pré-version n'est
//! sortie, les deux canaux donnent la même version.
//!
//! L'interface ne peut pas changer l'adresse interrogée par le `check()` du
//! plugin : la vérification et l'installation passent donc par ici, et la
//! mise à jour trouvée attend son installation côté Rust.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Les releases du dépôt, les plus récentes d'abord, pré-versions comprises.
const RELEASES_API: &str = "https://api.github.com/repos/Locaryn/Locaryn/releases?per_page=15";

/// Le manifeste que chaque release publie pour l'updater.
const MANIFESTE: &str = "latest.json";

/// La mise à jour trouvée par la dernière vérification, en attente.
static EN_ATTENTE: Mutex<Option<Update>> = Mutex::new(None);

#[derive(Debug, Deserialize)]
struct ReleaseGh {
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<AssetGh>,
}

#[derive(Debug, Deserialize)]
struct AssetGh {
    name: String,
    browser_download_url: String,
}

/// Ce que l'interface montre d'une version bêta trouvée.
#[derive(Debug, Clone, Serialize)]
pub struct MajBeta {
    pub version: String,
    pub body: Option<String>,
    /// RFC 3339, tel que le manifeste le donne.
    pub date: Option<String>,
}

/// La progression du téléchargement, sous la forme des événements du plugin
/// (`DownloadEvent`) : l'interface les traite de la même façon.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum ProgressionMaj {
    Started {
        #[serde(rename = "contentLength")]
        content_length: Option<u64>,
    },
    Progress {
        #[serde(rename = "chunkLength")]
        chunk_length: usize,
    },
    Finished,
}

/// L'adresse du manifeste le plus récent : la première release publiée (pas
/// un brouillon) qui en porte un.
fn adresse_manifeste(releases: &[ReleaseGh]) -> Option<&str> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .find_map(|r| r.assets.iter().find(|a| a.name == MANIFESTE))
        .map(|a| a.browser_download_url.as_str())
}

async fn manifeste_beta() -> Result<url::Url, String> {
    let releases: Vec<ReleaseGh> = reqwest::Client::new()
        .get(RELEASES_API)
        // L'API GitHub refuse une requête sans User-Agent.
        .header(reqwest::header::USER_AGENT, "Locaryn")
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("liste des versions illisible : {e}"))?
        .json()
        .await
        .map_err(|e| format!("liste des versions illisible : {e}"))?;
    let adresse = adresse_manifeste(&releases)
        .ok_or_else(|| "aucune version publiée ne porte de manifeste de mise à jour".to_string())?;
    url::Url::parse(adresse).map_err(|e| format!("adresse du manifeste invalide : {e}"))
}

fn garder(update: Option<Update>) -> Result<(), String> {
    let mut en_attente = EN_ATTENTE
        .lock()
        .map_err(|_| "mise à jour en attente illisible".to_string())?;
    *en_attente = update;
    Ok(())
}

/// Chercher une version plus récente sur le canal bêta.
#[tauri::command]
pub async fn chercher_maj_beta(app: AppHandle) -> Result<Option<MajBeta>, String> {
    let manifeste = manifeste_beta().await?;
    let update = app
        .updater_builder()
        .endpoints(vec![manifeste])
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let trouvee = update.as_ref().map(|u| MajBeta {
        version: u.version.clone(),
        body: u.body.clone(),
        date: u
            .raw_json
            .get("pub_date")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    });
    garder(update)?;
    Ok(trouvee)
}

/// Télécharger et poser la version bêta trouvée par la dernière vérification.
#[tauri::command]
pub async fn installer_maj_beta(progression: Channel<ProgressionMaj>) -> Result<(), String> {
    let update = EN_ATTENTE
        .lock()
        .map_err(|_| "mise à jour en attente illisible".to_string())?
        .take()
        .ok_or_else(|| "aucune version bêta en attente : relancez la vérification".to_string())?;
    let mut commence = false;
    let envoyer = |p: ProgressionMaj| {
        if let Err(e) = progression.send(p) {
            tracing::debug!(erreur = %e, "progression de la mise à jour non transmise");
        }
    };
    update
        .download_and_install(
            |morceau, total| {
                if !commence {
                    commence = true;
                    envoyer(ProgressionMaj::Started {
                        content_length: total,
                    });
                }
                envoyer(ProgressionMaj::Progress {
                    chunk_length: morceau,
                });
            },
            || envoyer(ProgressionMaj::Finished),
        )
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(draft: bool, assets: &[&str]) -> ReleaseGh {
        ReleaseGh {
            draft,
            assets: assets
                .iter()
                .map(|n| AssetGh {
                    name: (*n).to_string(),
                    browser_download_url: format!("https://exemple/{n}"),
                })
                .collect(),
        }
    }

    #[test]
    fn la_premiere_release_publiee_avec_manifeste_gagne() {
        let releases = vec![
            release(true, &["latest.json"]),
            release(false, &["Locaryn.msi"]),
            release(false, &["Locaryn.msi", "latest.json"]),
            release(false, &["latest.json"]),
        ];
        assert_eq!(
            adresse_manifeste(&releases),
            Some("https://exemple/latest.json")
        );
        assert!(std::ptr::eq(
            adresse_manifeste(&releases).unwrap_or_default(),
            releases[2].assets[1].browser_download_url.as_str()
        ));
    }

    #[test]
    fn sans_manifeste_rien_n_est_propose() {
        assert_eq!(adresse_manifeste(&[release(false, &["notes.md"])]), None);
        assert_eq!(adresse_manifeste(&[]), None);
    }

    #[test]
    fn la_progression_a_la_forme_du_plugin() {
        let debut = serde_json::to_value(ProgressionMaj::Started {
            content_length: Some(10),
        })
        .unwrap_or_default();
        assert_eq!(
            debut,
            serde_json::json!({"event": "Started", "data": {"contentLength": 10}})
        );
        let fin = serde_json::to_value(ProgressionMaj::Finished).unwrap_or_default();
        assert_eq!(fin, serde_json::json!({"event": "Finished"}));
    }
}
