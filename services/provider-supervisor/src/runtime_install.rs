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
/// `-ngl`, `--mmproj`, `-fa`, `-ctk`/`-ctv`, `-cmoe`/`-ncmoe`, `-md`…)
/// ont été vérifiés contre le `--help` de cette build (b11003, 16/09/2026) :
/// un binaire inconnu qui refuse un drapeau est une erreur fatale au
/// démarrage. Changement notable par rapport à b10088 : `--no-mmap` n'existe
/// plus, remplacé par le mode de chargement `-lm none` — le spawn (lib.rs)
/// utilise déjà la nouvelle forme.
pub const PINNED_LLAMA_BUILD: &str = "b11003";

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

/// L'archive publiée pour un système et une architecture de *machine*.
///
/// Noms vérifiés sur les assets de la release b11003. Windows x64 prend la
/// build Vulkan (elle sert aussi les cartes AMD et Intel) ; Windows ARM64 la
/// build OpenCL Adreno — la build CPU qu'il prenait laissait le GPU des
/// Snapdragon X inutilisé, alors que l'archive Adreno contient exactement les
/// mêmes fichiers plus `ggml-opencl.dll` (sans pilote OpenCL, llama.cpp
/// retombe sur le processeur, comme avant). Linux et macOS prennent la build
/// de leur architecture (Metal est inclus dans celle de macOS ARM64).
///
/// Une build CUDA gagnait 12 % en génération seulement sur une RTX 4050, au
/// prix d'un second téléchargement lié à la version du pilote : elle reste un
/// choix du morph cluster, qui sait le mesurer.
fn asset_for(os: &str, arm: bool) -> Option<&'static str> {
    Some(match (os, arm) {
        ("windows", true) => "llama-b11003-bin-win-opencl-adreno-arm64.zip",
        ("windows", false) => "llama-b11003-bin-win-vulkan-x64.zip",
        // Depuis ~b11000 les publications Ubuntu sont des `.tar.gz`.
        ("linux", true) => "llama-b11003-bin-ubuntu-arm64.tar.gz",
        ("linux", false) => "llama-b11003-bin-ubuntu-x64.tar.gz",
        ("macos", true) => "llama-b11003-bin-macos-arm64.tar.gz",
        ("macos", false) => "llama-b11003-bin-macos-x64.tar.gz",
        _ => return None,
    })
}

/// L'archive qui convient à *cette machine*.
///
/// L'architecture est celle du processeur, pas celle de l'application : la
/// version x64 tourne aussi sur Snapdragon (émulée), et un processus émulé
/// peut lancer un `llama-server` ARM64 natif — c'est même le seul moyen
/// d'atteindre l'Adreno.
fn release_asset() -> Result<&'static str, SupervisorError> {
    asset_for(
        std::env::consts::OS,
        locaryn_llmfit::hardware::host_is_arm64(),
    )
    .ok_or_else(|| {
        SupervisorError::BinaryNotFound(
            "aucune archive llama.cpp épinglée pour cette plateforme — installez \
             llama-server à la main puis relancez"
                .into(),
        )
    })
}

fn release_url() -> Result<String, SupervisorError> {
    Ok(format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{PINNED_LLAMA_BUILD}/{}",
        release_asset()?
    ))
}

/// Fichier qui note, à côté du moteur, l'archive dont il vient.
const VARIANT_FILE: &str = "variant.txt";

/// Le moteur installé est-il celui qui convient à cette machine et à ce pin ?
///
/// Faux pour une installation d'avant ce marqueur : elle peut venir d'une
/// ancienne build (dont les drapeaux ne sont plus ceux du lancement) ou de la
/// build CPU qu'on posait sur les Snapdragon.
pub fn installed_variant_is_current() -> bool {
    let Ok(wanted) = release_asset() else {
        return true;
    };
    std::fs::read_to_string(managed_llama_dir().join(VARIANT_FILE))
        .is_ok_and(|v| v.trim() == wanted)
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
    let present = target.is_file();
    if present && installed_variant_is_current() {
        tracing::info!(path = %target.display(), "moteur déjà installé");
        return Ok(target);
    }

    // Installation dans un dossier à part, puis échange : un moteur qui
    // fonctionne n'est jamais retiré avant que son remplaçant soit prêt. Sans
    // réseau, ou si l'ancien est en cours d'exécution (Windows verrouille ses
    // fichiers), on garde l'ancien.
    let dir = managed_llama_dir();
    match install_into(&staging_dir(), progress).await {
        Ok(()) => {}
        Err(e) if present => {
            tracing::warn!(erreur = %e, "mise à niveau du moteur impossible — l'installation existante est conservée");
            let _ = std::fs::remove_dir_all(staging_dir());
            return Ok(target);
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(staging_dir());
            return Err(e);
        }
    }
    if let Err(e) = swap_into_place(&staging_dir(), &dir) {
        let _ = std::fs::remove_dir_all(staging_dir());
        if present {
            tracing::warn!(erreur = %e, "moteur en cours d'utilisation — la mise à niveau attendra le prochain lancement");
            return Ok(target);
        }
        return Err(e);
    }

    tracing::info!(path = %target.display(), variante = %release_asset()?, "moteur installé");
    Ok(target)
}

/// Le dossier où se prépare une installation, à côté du dossier géré.
fn staging_dir() -> PathBuf {
    managed_llama_dir().with_extension("new")
}

/// Télécharge, extrait et vérifie le moteur dans `dir`, marqueur compris.
async fn install_into(dir: &Path, progress: Option<ProgressFn<'_>>) -> Result<(), SupervisorError> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    let asset = release_asset()?;
    // L'extension suit l'asset réel : `.zip` côté Windows, `.tar.gz` ailleurs.
    let is_targz = asset.ends_with(".tar.gz");
    let archive = dir.join(format!(
        "llama-{PINNED_LLAMA_BUILD}.{}",
        if is_targz { "tar.gz" } else { "zip" }
    ));

    download(&release_url()?, &archive, progress).await?;
    if is_targz {
        extract_tar_gz(&archive, dir)?;
    } else {
        extract_zip(&archive, dir)?;
    }
    let _ = std::fs::remove_file(&archive);

    // L'archive de llama.cpp range parfois les binaires dans un sous-dossier
    // (`build/bin/`). On remonte ce qui compte à la racine du dossier : c'est
    // là que le superviseur regarde.
    let target = dir.join(llama_server_name());
    if !target.is_file() {
        if let Some(found) = find_file(dir, llama_server_name(), 4) {
            promote_siblings(&found, dir)?;
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

    std::fs::write(dir.join(VARIANT_FILE), asset)?;
    Ok(())
}

/// Remplace `dest` par `staged`. L'ancien dossier est d'abord mis de côté :
/// si le renommage échoue (fichier verrouillé), rien n'a bougé.
fn swap_into_place(staged: &Path, dest: &Path) -> Result<(), SupervisorError> {
    let old = dest.with_extension("old");
    let _ = std::fs::remove_dir_all(&old);
    if dest.exists() {
        std::fs::rename(dest, &old)?;
    }
    if let Err(e) = std::fs::rename(staged, dest) {
        // Remettre l'ancien en place plutôt que de laisser un trou.
        let _ = std::fs::rename(&old, dest);
        return Err(e.into());
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
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

/// Extraire une archive `.tar.gz` (les publications Ubuntu de llama.cpp
/// depuis b11003). Mêmes règles que `extract_zip` : on écrase à plat dans
/// `dir`, le promoteur de binaires (`find_file` + `promote_siblings`) gère
/// les sous-dossiers éventuels. Sous Unix, les permissions du tar (dont le
/// bit exécutable) sont conservées.
fn extract_tar_gz(archive: &Path, dir: &Path) -> Result<(), SupervisorError> {
    let file = std::fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.set_preserve_permissions(true);
    for entry in tar
        .entries()
        .map_err(|e| SupervisorError::BinaryNotFound(format!("archive illisible : {e}")))?
    {
        let mut entry = entry
            .map_err(|e| SupervisorError::BinaryNotFound(format!("entrée illisible : {e}")))?;
        // Même garde-fou que `enclosed_name` côté zip : une entrée qui
        // sortirait du dossier cible est ignorée, pas réécrite.
        let Some(rel) = entry
            .path()
            .ok()
            .and_then(|p| p.file_name().map(std::path::PathBuf::from))
        else {
            continue;
        };
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            continue;
        }
        let out = dir.join(rel);
        // Les bibliothèques partagées d'Ubuntu sont des liens
        // (`libggml-base.so` → `.so.0` → `.so.0.24.0`). Les écrire comme des
        // fichiers vides laissait `llama-server` incapable de démarrer.
        if kind.is_symlink() || kind.is_hard_link() {
            link_entry(&entry, &out)?;
            continue;
        }
        let mut dst = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut dst)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mode) = entry.header().mode() {
                let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

/// Recréer un lien de l'archive à côté de sa cible. Les cibles sont toutes
/// dans le même dossier : seul le nom de fichier est conservé, ce qui écarte
/// aussi un lien qui pointerait hors du dossier géré.
#[cfg(unix)]
fn link_entry<R: std::io::Read>(
    entry: &tar::Entry<'_, R>,
    out: &Path,
) -> Result<(), SupervisorError> {
    let Some(target) = entry
        .link_name()
        .ok()
        .flatten()
        .and_then(|t| t.file_name().map(std::path::PathBuf::from))
    else {
        return Ok(());
    };
    match std::fs::remove_file(out) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::os::unix::fs::symlink(target, out)?;
    Ok(())
}

#[cfg(not(unix))]
fn link_entry<R: std::io::Read>(
    _entry: &tar::Entry<'_, R>,
    _out: &Path,
) -> Result<(), SupervisorError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// « Installer le moteur » téléchargeait l'archive et s'arrêtait là, en
    /// annonçant une réussite. Les binaires doivent finir côte à côte à la
    /// racine du dossier géré, quel que soit le sous-dossier de l'archive :
    /// sous Windows l'exécutable ne démarre pas sans ses DLL.
    #[test]
    fn l_archive_du_moteur_est_mise_a_plat() {
        let base = std::env::temp_dir().join(format!(
            "locaryn_runtime_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let archive = base.join("llama.zip");

        let fichier = std::fs::File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(fichier);
        let options: zip::write::SimpleFileOptions = Default::default();
        for chemin in [
            "build/bin/llama-server.exe",
            "build/bin/ggml-base.dll",
            "build/bin/ggml-vulkan.dll",
        ] {
            zip.start_file(chemin, options).unwrap();
            zip.write_all(b"binaire").unwrap();
        }
        zip.finish().unwrap();

        let dest = base.join("llama");
        std::fs::create_dir_all(&dest).unwrap();
        extract_zip(&archive, &dest).unwrap();
        let found = find_file(&dest, "llama-server.exe", 4).expect("binaire extrait");
        promote_siblings(&found, &dest).unwrap();
        for nom in ["llama-server.exe", "ggml-base.dll", "ggml-vulkan.dll"] {
            assert!(dest.join(nom).is_file(), "{nom} devrait être à la racine");
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Les publications Ubuntu livrent `libggml-base.so` comme lien vers
    /// `.so.0.24.0` : extrait en fichier vide, le moteur ne démarrait pas.
    #[cfg(unix)]
    #[test]
    fn les_liens_de_l_archive_tar_sont_recrees() {
        let base = std::env::temp_dir().join(format!(
            "locaryn_tar_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let archive = base.join("llama.tar.gz");

        let gz = flate2::write::GzEncoder::new(
            std::fs::File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        let mut builder = tar::Builder::new(gz);
        let contenu = b"vraie bibliotheque";
        let mut header = tar::Header::new_gnu();
        header.set_size(contenu.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "llama/libggml-base.so.0.24.0", &contenu[..])
            .unwrap();
        for (nom, cible) in [
            ("llama/libggml-base.so.0", "libggml-base.so.0.24.0"),
            ("llama/libggml-base.so", "libggml-base.so.0"),
        ] {
            let mut lien = tar::Header::new_gnu();
            lien.set_entry_type(tar::EntryType::Symlink);
            lien.set_size(0);
            lien.set_mode(0o777);
            builder.append_link(&mut lien, nom, cible).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();

        let dest = base.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        extract_tar_gz(&archive, &dest).unwrap();
        assert_eq!(
            std::fs::read(dest.join("libggml-base.so")).unwrap(),
            contenu,
            "le lien doit mener à la bibliothèque, pas à un fichier vide"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn l_archive_epinglee_porte_la_version_du_pin() {
        if let Ok(url) = release_url() {
            assert!(url.contains(PINNED_LLAMA_BUILD), "{url}");
        }
        for os in ["windows", "linux", "macos"] {
            for arm in [false, true] {
                let asset = asset_for(os, arm).expect("plateforme couverte");
                assert!(asset.contains(PINNED_LLAMA_BUILD), "{asset}");
                assert_eq!(asset.contains("arm64"), arm, "{asset}");
            }
        }
    }

    /// Un Snapdragon X recevait la build CPU : l'Adreno ne servait jamais.
    #[test]
    fn un_snapdragon_recoit_le_moteur_adreno() {
        assert_eq!(
            asset_for("windows", true),
            Some("llama-b11003-bin-win-opencl-adreno-arm64.zip")
        );
        // Les PC x64 gardent Vulkan, qui sert NVIDIA, AMD et Intel.
        assert_eq!(
            asset_for("windows", false),
            Some("llama-b11003-bin-win-vulkan-x64.zip")
        );
    }

    /// Une mise à niveau ne doit jamais laisser le dossier du moteur vide.
    #[test]
    fn l_echange_remplace_le_moteur_sans_trou() {
        let base = std::env::temp_dir().join(format!(
            "locaryn_swap_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dest = base.join("llama");
        let staged = base.join("llama.new");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(dest.join("ancien"), b"x").unwrap();
        std::fs::write(staged.join("nouveau"), b"y").unwrap();

        swap_into_place(&staged, &dest).unwrap();
        assert!(dest.join("nouveau").is_file());
        assert!(!dest.join("ancien").exists());
        assert!(!staged.exists());
        assert!(!dest.with_extension("old").exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}
