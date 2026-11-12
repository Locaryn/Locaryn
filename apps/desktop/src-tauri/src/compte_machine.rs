//! Le compte du serveur choisi comme utilisateur de cette machine.
//!
//! En mode serveur, l'application de ce PC lit la même base que le service :
//! la lier à un compte, c'est retenir son identifiant. La rubrique Compte
//! montre alors ce compte, et l'onglet « Appareils et sécurité » agit sur lui
//! (mot de passe, appareils connectés). Rien ne passe par le réseau, sauf le
//! téléchargement de la base de géolocalisation, fait seulement sur demande.

use locaryn_storage::connexions::Device;
use locaryn_storage::users::{Role, UserRepo};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::PathBuf;

/// Le fichier qui retient le compte choisi, dans le dossier de données.
fn fichier() -> PathBuf {
    locaryn_config::default_data_dir().join("compte-machine.json")
}

#[derive(Serialize, Deserialize)]
struct Lien {
    user_id: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MachineAccount {
    pub id: String,
    pub username: String,
    pub role: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesReport {
    pub devices: Vec<Device>,
    /// La base de géolocalisation locale est installée.
    pub geolocation: bool,
    pub attribution: &'static str,
}

async fn depot() -> Result<UserRepo, String> {
    let db = locaryn_config::default_data_dir().join("locaryn.db");
    let pool = locaryn_storage::open(&db)
        .await
        .map_err(|e| format!("base des comptes illisible : {e}"))?;
    Ok(UserRepo::new(pool))
}

fn identifiant_lie() -> Option<uuid::Uuid> {
    let texte = match std::fs::read_to_string(fichier()) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(error = %e, "compte de la machine illisible");
            return None;
        }
    };
    match serde_json::from_str::<Lien>(&texte) {
        Ok(l) => uuid::Uuid::parse_str(&l.user_id).ok(),
        Err(e) => {
            tracing::warn!(error = %e, "compte de la machine mal formé");
            None
        }
    }
}

/// Le compte lié, s'il existe toujours et n'est pas désactivé.
async fn compte_lie(repo: &UserRepo) -> Result<Option<MachineAccount>, String> {
    let Some(id) = identifiant_lie() else {
        return Ok(None);
    };
    let compte = repo.get(id).await.map_err(|e| e.to_string())?;
    Ok(compte.filter(|u| !u.disabled).map(|u| MachineAccount {
        id: u.id.to_string(),
        username: u.username,
        role: if u.role == Role::Admin {
            "admin"
        } else {
            "member"
        },
    }))
}

async fn exiger_compte(repo: &UserRepo) -> Result<MachineAccount, String> {
    compte_lie(repo)
        .await?
        .ok_or_else(|| "Aucun compte du serveur n'est lié à cette machine.".to_string())
}

#[tauri::command]
pub async fn machine_account() -> Result<Option<MachineAccount>, String> {
    compte_lie(&depot().await?).await
}

/// Lier l'application à un compte du serveur, ou revenir au profil local
/// (`None`).
#[tauri::command]
pub async fn set_machine_account(
    user_id: Option<String>,
) -> Result<Option<MachineAccount>, String> {
    let Some(user_id) = user_id else {
        return match std::fs::remove_file(fichier()) {
            Ok(()) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("retour au profil local impossible : {e}")),
        };
    };
    let id = uuid::Uuid::parse_str(&user_id).map_err(|e| e.to_string())?;
    let repo = depot().await?;
    match repo.get(id).await.map_err(|e| e.to_string())? {
        Some(u) if u.disabled => return Err("Ce compte est désactivé.".into()),
        Some(_) => {}
        None => return Err("Ce compte n'existe plus sur le serveur.".into()),
    }
    let lien = serde_json::to_string(&Lien { user_id }).map_err(|e| e.to_string())?;
    std::fs::write(fichier(), lien).map_err(|e| format!("compte non retenu : {e}"))?;
    compte_lie(&repo).await
}

#[tauri::command]
pub async fn machine_account_password(current: String, nouveau: String) -> Result<(), String> {
    let repo = depot().await?;
    let compte = exiger_compte(&repo).await?;
    let id = uuid::Uuid::parse_str(&compte.id).map_err(|e| e.to_string())?;
    match repo.change_password(id, &current, &nouveau).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Le mot de passe actuel est incorrect.".into()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn machine_account_devices() -> Result<DevicesReport, String> {
    let repo = depot().await?;
    let compte = exiger_compte(&repo).await?;
    let id = uuid::Uuid::parse_str(&compte.id).map_err(|e| e.to_string())?;
    let base = locaryn_storage::geoip::database_path(&locaryn_config::default_data_dir());
    let installee = base.is_file();
    let devices = repo
        .connections()
        .devices(id, None, installee.then_some(base.as_path()))
        .await
        .map_err(|e| e.to_string())?;
    Ok(DevicesReport {
        devices,
        geolocation: installee,
        attribution: locaryn_storage::geoip::ATTRIBUTION,
    })
}

#[tauri::command]
pub async fn machine_account_revoke(token_id: String) -> Result<(), String> {
    let repo = depot().await?;
    let compte = exiger_compte(&repo).await?;
    let id = uuid::Uuid::parse_str(&compte.id).map_err(|e| e.to_string())?;
    let jeton = uuid::Uuid::parse_str(&token_id).map_err(|e| e.to_string())?;
    if repo
        .revoke_own_token(id, jeton)
        .await
        .map_err(|e| e.to_string())?
    {
        Ok(())
    } else {
        Err("Cet appareil est déjà déconnecté.".into())
    }
}

/// L'adresse de la base « IP to City Lite » de DB-IP pour un mois donné.
fn adresse_base(annee: i32, mois: u32) -> String {
    format!("https://download.db-ip.com/free/dbip-city-lite-{annee:04}-{mois:02}.mmdb.gz")
}

/// Télécharger la base de géolocalisation (environ 130 Mo une fois
/// décompressée). Seul db-ip.com est contacté, pour le fichier : aucune
/// adresse d'appareil ne lui est envoyée. La base du mois n'est pas toujours
/// publiée le 1er : on retombe sur celle du mois précédent.
#[tauri::command]
pub async fn install_geolocation() -> Result<(), String> {
    use chrono::Datelike as _;
    let base = locaryn_storage::geoip::database_path(&locaryn_config::default_data_dir());
    let dossier = base
        .parent()
        .ok_or("dossier de géolocalisation introuvable")?
        .to_path_buf();
    std::fs::create_dir_all(&dossier).map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let auj = chrono::Utc::now().date_naive();
    let precedent = auj - chrono::Duration::days(i64::from(auj.day()) + 1);
    let mut derniere_erreur = String::new();
    for (annee, mois) in [
        (auj.year(), auj.month()),
        (precedent.year(), precedent.month()),
    ] {
        let url = adresse_base(annee, mois);
        let mut reponse = match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                derniere_erreur = format!("{url} : {}", r.status());
                continue;
            }
            Err(e) => {
                derniere_erreur = e.to_string();
                continue;
            }
        };
        let archive = dossier.join("dbip-city-lite.mmdb.gz.part");
        let mut f = std::fs::File::create(&archive).map_err(|e| e.to_string())?;
        while let Some(morceau) = reponse.chunk().await.map_err(|e| e.to_string())? {
            f.write_all(&morceau).map_err(|e| e.to_string())?;
        }
        drop(f);
        let cible = base.clone();
        tokio::task::spawn_blocking(move || decompresser(&archive, &cible))
            .await
            .map_err(|e| e.to_string())??;
        return Ok(());
    }
    Err(format!(
        "Base de géolocalisation introuvable sur db-ip.com ({derniere_erreur})."
    ))
}

fn decompresser(archive: &std::path::Path, cible: &std::path::Path) -> Result<(), String> {
    let entree = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut lecteur = flate2::read::GzDecoder::new(std::io::BufReader::new(entree));
    let provisoire = cible.with_extension("mmdb.part");
    let mut sortie = std::fs::File::create(&provisoire).map_err(|e| e.to_string())?;
    std::io::copy(&mut lecteur, &mut sortie).map_err(|e| format!("archive illisible : {e}"))?;
    drop(sortie);
    std::fs::rename(&provisoire, cible).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::remove_file(archive) {
        tracing::warn!(error = %e, "archive de géolocalisation non supprimée");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_adresse_de_la_base_suit_le_mois() {
        assert_eq!(
            adresse_base(2026, 3),
            "https://download.db-ip.com/free/dbip-city-lite-2026-03.mmdb.gz"
        );
    }
}
