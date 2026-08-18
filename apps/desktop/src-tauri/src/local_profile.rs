//! Persistent identity for the local account shown by the desktop UI.
//!
//! The profile is deliberately independent from a remote login: a local
//! installation still needs a stable name and avatar after restarting Locaryn.
//! Avatar files are copied into the application data directory so moving or
//! deleting the original file does not break the profile.

use crate::Core;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

const MAX_AVATAR_BYTES: u64 = 8 * 1024 * 1024;
const ALLOWED_AVATAR_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalProfile {
    /// Empty means the UI displays the stable default name `Local`.
    pub display_name: String,
    /// Absolute path to the copied avatar, if one was selected.
    pub avatar_path: Option<String>,
}

fn profile_path(data_dir: &Path) -> PathBuf {
    data_dir.join("local_profile.json")
}

fn avatar_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("profile")
}

fn load(data_dir: &Path) -> Result<LocalProfile, String> {
    let path = profile_path(data_dir);
    let profile = match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<LocalProfile>(&raw).map_err(|e| e.to_string())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LocalProfile::default(),
        Err(error) => return Err(error.to_string()),
    };

    Ok(LocalProfile {
        avatar_path: profile.avatar_path.filter(|path| Path::new(path).is_file()),
        ..profile
    })
}

fn save(data_dir: &Path, profile: &LocalProfile) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let path = profile_path(data_dir);
    let temporary = path.with_extension("json.tmp");
    let content = serde_json::to_vec_pretty(profile).map_err(|e| e.to_string())?;
    std::fs::write(&temporary, content).map_err(|e| e.to_string())?;
    // Windows does not replace an existing file with rename, unlike Unix.
    // Remove the old tiny JSON file first; the temporary file prevents a
    // partially written profile if serialization or writing fails.
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&temporary, &path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        e.to_string()
    })
}

fn normalized_name(name: String) -> String {
    name.trim().chars().take(80).collect()
}

fn avatar_extension(source: &Path) -> Result<String, String> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or("l'avatar doit être une image PNG, JPG, JPEG, WEBP ou BMP".to_string())?;
    if ALLOWED_AVATAR_EXTENSIONS.contains(&extension.as_str()) {
        Ok(extension)
    } else {
        Err("l'avatar doit être une image PNG, JPG, JPEG, WEBP ou BMP".into())
    }
}

fn remove_owned_avatar(data_dir: &Path, path: Option<String>) {
    let Some(path) = path else { return };
    let candidate = PathBuf::from(path);
    let root = avatar_dir(data_dir);
    let owned = candidate.parent() == Some(root.as_path())
        && candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("avatar-"));
    if owned {
        let _ = std::fs::remove_file(candidate);
    }
}

#[tauri::command]
pub fn get_local_profile(core: State<'_, Core>) -> Result<LocalProfile, String> {
    load(&core.data_dir)
}

#[tauri::command]
pub fn set_local_profile(
    core: State<'_, Core>,
    display_name: String,
) -> Result<LocalProfile, String> {
    let mut profile = load(&core.data_dir)?;
    profile.display_name = normalized_name(display_name);
    save(&core.data_dir, &profile)?;
    Ok(profile)
}

#[tauri::command]
pub fn set_local_avatar(
    core: State<'_, Core>,
    source_path: String,
) -> Result<LocalProfile, String> {
    let source = PathBuf::from(source_path.trim());
    if !source.is_file() {
        return Err("le fichier d'avatar est introuvable".into());
    }
    let extension = avatar_extension(&source)?;
    let size = std::fs::metadata(&source)
        .map_err(|e| format!("lecture de l'avatar impossible : {e}"))?
        .len();
    if size == 0 {
        return Err("le fichier d'avatar est vide".into());
    }
    if size > MAX_AVATAR_BYTES {
        return Err("l'avatar dépasse 8 Mo".into());
    }

    let directory = avatar_dir(&core.data_dir);
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let destination = directory.join(format!("avatar-{}.{}", Uuid::new_v4(), extension));
    std::fs::copy(&source, &destination)
        .map_err(|e| format!("copie de l'avatar impossible : {e}"))?;

    let mut profile = load(&core.data_dir)?;
    let previous = profile.avatar_path.take();
    profile.avatar_path = Some(destination.to_string_lossy().to_string());
    if let Err(error) = save(&core.data_dir, &profile) {
        let _ = std::fs::remove_file(&destination);
        return Err(error);
    }
    remove_owned_avatar(&core.data_dir, previous);
    Ok(profile)
}

#[tauri::command]
pub fn clear_local_avatar(core: State<'_, Core>) -> Result<LocalProfile, String> {
    let mut profile = load(&core.data_dir)?;
    let previous = profile.avatar_path.take();
    profile.avatar_path = None;
    save(&core.data_dir, &profile)?;
    remove_owned_avatar(&core.data_dir, previous);
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::{load, normalized_name, save, LocalProfile};
    use uuid::Uuid;

    #[test]
    fn le_nom_est_nettoye_et_borne() {
        assert_eq!(normalized_name("  Alice  ".into()), "Alice");
        assert_eq!(normalized_name("x".repeat(100)), "x".repeat(80));
    }

    #[test]
    fn le_profil_reste_apres_un_roundtrip_disque() {
        let directory = std::env::temp_dir().join(format!("locaryn-profile-{}", Uuid::new_v4()));
        let avatar = directory.join("avatar.png");
        std::fs::create_dir_all(&directory).expect("dossier temporaire");
        std::fs::write(&avatar, b"avatar").expect("avatar temporaire");
        let profile = LocalProfile {
            display_name: "Alice".into(),
            avatar_path: Some(avatar.to_string_lossy().to_string()),
        };
        save(&directory, &profile).expect("écriture du profil");
        let loaded = load(&directory).expect("lecture du profil");
        assert_eq!(loaded.display_name, "Alice");
        assert_eq!(loaded.avatar_path, profile.avatar_path);
        let _ = std::fs::remove_dir_all(directory);
    }
}
