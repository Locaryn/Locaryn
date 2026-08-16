//! Installation du moteur d'inférence local.
//!
//! Le bouton « installer le runtime » de l'application téléchargeait l'archive
//! puis s'arrêtait là — le code portait le commentaire « extract would go here;
//! for now just report success » et renvoyait `installed: true`. Résultat : le
//! dossier `bin/` restait vide, le superviseur ne trouvait aucun binaire, et le
//! chat repliait sur une réponse factice. Une installation qui se déclare
//! réussie sans rien installer coûte plus cher qu'un échec franc : elle déplace
//! le problème à l'autre bout de la chaîne.
//!
//! L'installation vit ici, dans le paquet qui lance les moteurs, pour que
//! l'application de bureau, la CLI et un serveur sans écran passent tous par le
//! même chemin.

use std::path::{Path, PathBuf};

use crate::SupervisorError;

/// Version épinglée de llama.cpp. Les drapeaux passés au serveur (`--jinja`,
/// `-ngl`, `--mmproj`…) ont été vérifiés contre cette build ; un binaire
/// inconnu qui refuse un drapeau est une erreur fatale au démarrage.
pub const PINNED_LLAMA_BUILD: &str = "b10088";

/// Nom du binaire selon la plateforme.
pub fn llama_server_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Où le superviseur cherche le moteur géré.
pub fn managed_llama_dir() -> PathBuf {
    locaryn_config::bin_dir().join("llama")
}

/// Chemin complet du moteur géré, installé ou non.
pub fn managed_llama_path() -> PathBuf {
    managed_llama_dir().join(llama_server_name())
}

/// L'archive publiée pour cette plateforme.
fn release_url() -> Result<&'static str, SupervisorError> {
    if cfg!(target_os = "windows") {
        Ok("https://github.com/ggml-org/llama.cpp/releases/download/b10088/llama-b10088-bin-win-vulkan-x64.zip")
    } else if cfg!(target_os = "linux") {
        Ok("https://github.com/ggml-org/llama.cpp/releases/download/b10088/llama-b10088-bin-ubuntu-x64.zip")
    } else {
        // macOS : les builds officielles sont publiées par architecture et
        // changent de nom d'une release à l'autre. Plutôt que de deviner, on
        // dit quoi faire.
        Err(SupervisorError::BinaryNotFound(
            "aucune archive llama.cpp épinglée pour cette plateforme — installez \
             llama-server à la main (brew install llama.cpp) puis relancez"
                .into(),
        ))
    }
}

/// Avancement du téléchargement, en octets. `total` vaut 0 quand le serveur ne
/// l'annonce pas.
pub type ProgressFn<'a> = &'a (dyn Fn(u64, u64) + Send + Sync);

/// Télécharge et installe le moteur de conversation, puis rend son chemin.
///
/// Idempotent : si le binaire est déjà là, rien n'est retéléchargé.
pub async fn install_llama_runtime(
    progress: Option<ProgressFn<'_>>,
) -> Result<PathBuf, SupervisorError> {
    let target = managed_llama_path();
    if target.is_file() {
        tracing::info!(path = %target.display(), "moteur déjà installé");
        return Ok(target);
    }

    let dir = managed_llama_dir();
    std::fs::create_dir_all(&dir)?;
    let archive = dir.join(format!("llama-{PINNED_LLAMA_BUILD}.zip"));

    download(release_url()?, &archive, progress).await?;
    extract_zip(&archive, &dir)?;
    let _ = std::fs::remove_file(&archive);

    // L'archive de llama.cpp range parfois les binaires dans un sous-dossier
    // (`build/bin/`). On remonte ce qui compte à la racine du dossier géré :
    // c'est là que le superviseur regarde.
    if !target.is_file() {
        if let Some(found) = find_file(&dir, llama_server_name(), 4) {
            promote_siblings(&found, &dir)?;
        }
    }

    if !target.is_file() {
        return Err(SupervisorError::BinaryNotFound(format!(
            "l'archive a été extraite mais {} reste introuvable",
            target.display()
        )));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(&target)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target, perms)?;
    }

    tracing::info!(path = %target.display(), "moteur installé");
    Ok(target)
}

async fn download(
    url: &str,
    dest: &Path,
    progress: Option<ProgressFn<'_>>,
) -> Result<(), SupervisorError> {
    use futures::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;

    // Un fichier `.part` : une coupure réseau laisse un fichier incomplet, et
    // un `.zip` incomplet portant le bon nom serait pris pour une archive
    // valide au prochain lancement.
    let part = dest.with_extension("part");
    let resp = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "Locaryn-Runtime-Installer")
        .send()
        .await
        .map_err(|e| {
            SupervisorError::SpawnFailed(crate::ProviderEngine::LlamaCpp, e.to_string())
        })?;
    if !resp.status().is_success() {
        return Err(SupervisorError::SpawnFailed(
            crate::ProviderEngine::LlamaCpp,
            format!("téléchargement refusé : HTTP {}", resp.status()),
        ));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut done: u64 = 0;
    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            SupervisorError::SpawnFailed(crate::ProviderEngine::LlamaCpp, e.to_string())
        })?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if let Some(p) = progress {
            p(done, total);
        }
    }
    file.flush().await?;
    drop(file);
    std::fs::rename(&part, dest)?;
    Ok(())
}

fn extract_zip(archive: &Path, dir: &Path) -> Result<(), SupervisorError> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| SupervisorError::BinaryNotFound(format!("archive illisible : {e}")))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| SupervisorError::BinaryNotFound(format!("entrée illisible : {e}")))?;
        // `enclosed_name` refuse les chemins qui sortent du dossier cible :
        // une archive hostile ne doit pas pouvoir écrire ailleurs.
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut dst = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut dst)?;
    }
    Ok(())
}

/// Cherche un fichier par nom, en descendant au plus `depth` niveaux.
fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
        if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.into_iter()
        .find_map(|d| find_file(&d, name, depth - 1))
}

/// Remonte le binaire trouvé et tout ce qui l'accompagne (DLL, bibliothèques
/// partagées) à la racine du dossier géré. Déplacer le seul exécutable ne
/// suffit pas : il ne démarrerait pas sans ses bibliothèques.
fn promote_siblings(found: &Path, dir: &Path) -> Result<(), SupervisorError> {
    let Some(src_dir) = found.parent() else {
        return Ok(());
    };
    if src_dir == dir {
        return Ok(());
    }
    for entry in std::fs::read_dir(src_dir)?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        let dest = dir.join(name);
        if dest.exists() {
            continue;
        }
        std::fs::rename(&path, &dest)?;
    }
    Ok(())
}
