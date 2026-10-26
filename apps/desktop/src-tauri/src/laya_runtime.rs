//! Laya, installé une fois par l'application et partagé par les morphs.
//!
//! Laya est un petit modèle de décision (classifieur non autorégressif) :
//! « quel élément cliquer pour enregistrer ? », « l'objectif est-il atteint ? »,
//! « cette action est-elle irréversible ? », en une passe. Les extensions qui
//! déclarent la capacité `laya` s'en servent pour décider vite sans faire travailler le
//! modèle de conversation. Elles savaient l'utiliser… à condition que la
//! personne ait installé Python, PyTorch et le paquet `laya` elle-même : sur
//! presque toutes les machines, il manquait, et chaque action retentait de le
//! lancer pour rien.
//!
//! L'application l'installe donc elle-même, sans rien demander d'autre :
//! `uv` (un exécutable autonome, épinglé) installe un Python géré puis Laya
//! dans `python-env`, à côté de la bibliothèque de modèles — le dossier que
//! les morphs cherchent déjà (`LOCARYN_PYTHON_VENV` le leur désigne aussi).
//! PyTorch en version processeur partout, sauf sur une carte NVIDIA (CUDA) :
//! Laya répond en quelques dizaines de millisecondes même sur processeur, et
//! ne dispute pas la carte au modèle de conversation.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

/// Version de `uv` épinglée (publiée le 2026-09-28).
const UV_VERSION: &str = "0.12.20";
/// Python installé pour Laya.
const PYTHON_VERSION: &str = "3.12";
/// Événement de progression de l'installation.
pub const EVENT_PROGRES: &str = "locaryn://laya-progres";

static EN_COURS: AtomicBool = AtomicBool::new(false);

/// Le dossier de l'environnement : à côté de la bibliothèque de modèles, sur
/// le même volume (PyTorch pèse plusieurs centaines de Mo).
pub fn dossier_env() -> PathBuf {
    locaryn_config::python_env_dir()
}

fn dossier_outils() -> PathBuf {
    dossier_env().with_file_name("python-tools")
}

fn python_de(env: &Path) -> PathBuf {
    if cfg!(windows) {
        env.join("Scripts").join("python.exe")
    } else {
        env.join("bin").join("python")
    }
}

fn marqueur() -> PathBuf {
    dossier_env().join("locaryn-laya.json")
}

#[derive(Debug, Clone, Serialize)]
pub struct EtatLaya {
    pub installe: bool,
    pub en_cours: bool,
    pub dossier: String,
    /// `cpu` ou `cuda`.
    pub appareil: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Progres {
    etape: String,
    detail: String,
    fini: bool,
    erreur: Option<String>,
}

fn emettre(app: &AppHandle, etape: &str, detail: &str) {
    let _ = app.emit(
        EVENT_PROGRES,
        Progres {
            etape: etape.into(),
            detail: detail.into(),
            fini: false,
            erreur: None,
        },
    );
}

/// Laya est-il prêt à servir ?
pub fn est_installe() -> bool {
    marqueur().exists() && python_de(&dossier_env()).exists()
}

#[tauri::command]
pub fn laya_etat() -> EtatLaya {
    let m: Option<serde_json::Value> = std::fs::read_to_string(marqueur())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    EtatLaya {
        installe: est_installe(),
        en_cours: EN_COURS.load(Ordering::SeqCst),
        dossier: dossier_env().display().to_string(),
        appareil: m
            .as_ref()
            .and_then(|v| v.get("appareil"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        version: m
            .as_ref()
            .and_then(|v| v.get("laya"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
}

/// La capacité qu'une extension déclare dans son manifeste pour se servir de
/// Laya. Le socle ne connaît aucune extension par son nom : celle d'un tiers
/// obtient Laya de la même façon.
const CAPACITE_LAYA: &str = "laya";

/// Une extension active déclare `laya`, et Laya manque : on l'installe, en
/// arrière-plan. Appelé au démarrage et à chaque relecture des extensions
/// (donc juste après l'installation d'un morph).
pub async fn verifier(app: AppHandle, core: &crate::Core) {
    if est_installe() || EN_COURS.load(Ordering::SeqCst) {
        return;
    }
    let Ok(rows) = core.storage.extensions.list().await else {
        return;
    };
    let demandee = rows
        .iter()
        .filter(|r| r.enabled)
        .filter_map(|r| crate::extensions::plugin_root(&r.manifest_path))
        .filter_map(|racine| locaryn_extensions::manifest::load(&racine).ok())
        .any(|m| m.capabilities.iter().any(|c| c == CAPACITE_LAYA));
    if demandee {
        tracing::info!("une extension active demande Laya : installation");
        installer_si_besoin(app);
    }
}

/// Lance l'installation en arrière-plan, si elle n'est pas faite ni en cours.
pub fn installer_si_besoin(app: AppHandle) {
    if est_installe() || EN_COURS.load(Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(e) = installer(&app).await {
            tracing::warn!(erreur = %e, "installation de Laya");
        }
    });
}

#[tauri::command]
pub async fn laya_installer(app: AppHandle) -> Result<EtatLaya, String> {
    installer(&app).await?;
    Ok(laya_etat())
}

async fn installer(app: &AppHandle) -> Result<(), String> {
    if EN_COURS.swap(true, Ordering::SeqCst) {
        return Err("L'installation de Laya est déjà en cours.".into());
    }
    let res = installer_vraiment(app).await;
    EN_COURS.store(false, Ordering::SeqCst);
    let _ = app.emit(
        EVENT_PROGRES,
        Progres {
            etape: if res.is_ok() { "fini" } else { "echec" }.into(),
            detail: String::new(),
            fini: true,
            erreur: res.as_ref().err().cloned(),
        },
    );
    res
}

fn cible_uv() -> Result<(&'static str, &'static str), String> {
    let arm = cfg!(target_arch = "aarch64") || locaryn_llmfit::hardware::host_is_arm64();
    Ok(match (std::env::consts::OS, arm) {
        ("windows", false) => ("uv-x86_64-pc-windows-msvc", "zip"),
        ("windows", true) => ("uv-aarch64-pc-windows-msvc", "zip"),
        ("macos", true) => ("uv-aarch64-apple-darwin", "tar.gz"),
        ("macos", false) => ("uv-x86_64-apple-darwin", "tar.gz"),
        ("linux", false) => ("uv-x86_64-unknown-linux-gnu", "tar.gz"),
        ("linux", true) => ("uv-aarch64-unknown-linux-gnu", "tar.gz"),
        (os, _) => return Err(format!("Laya n'est pas proposé sur {os}.")),
    })
}

/// `uv`, téléchargé une fois dans `python-tools`.
async fn obtenir_uv(app: &AppHandle) -> Result<PathBuf, String> {
    let dossier = dossier_outils().join(format!("uv-{UV_VERSION}"));
    let exe = dossier.join(if cfg!(windows) { "uv.exe" } else { "uv" });
    if exe.exists() {
        return Ok(exe);
    }
    let (nom, ext) = cible_uv()?;
    emettre(app, "outil", "Téléchargement de l'installateur (uv)…");
    let url = format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{nom}.{ext}");
    let octets = reqwest::get(&url)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("téléchargement de uv : {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("téléchargement de uv : {e}"))?;
    let attendu = reqwest::get(format!("{url}.sha256"))
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("empreinte de uv : {e}"))?
        .text()
        .await
        .map_err(|e| format!("empreinte de uv : {e}"))?;
    let attendu = attendu
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let calcule = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(&octets))
    };
    if attendu.is_empty() || attendu != calcule {
        return Err("Empreinte de uv invalide : téléchargement refusé.".into());
    }
    std::fs::create_dir_all(&dossier).map_err(|e| e.to_string())?;
    if ext == "zip" {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(octets.to_vec()))
            .map_err(|e| format!("archive de uv : {e}"))?;
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
            let Some(nom_fichier) = Path::new(f.name()).file_name().map(|n| n.to_owned()) else {
                continue;
            };
            if f.is_dir() {
                continue;
            }
            let mut sortie =
                std::fs::File::create(dossier.join(nom_fichier)).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut sortie).map_err(|e| e.to_string())?;
        }
    } else {
        let archive = dossier.join("uv.tar.gz");
        std::fs::write(&archive, &octets).map_err(|e| e.to_string())?;
        let ok = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&dossier)
            .arg("--strip-components=1")
            .status()
            .map_err(|e| format!("extraction de uv : {e}"))?;
        let _ = std::fs::remove_file(&archive);
        if !ok.success() {
            return Err("extraction de uv impossible".into());
        }
    }
    if !exe.exists() {
        return Err("uv introuvable après extraction".into());
    }
    Ok(exe)
}

async fn lancer(uv: &Path, args: &[&str], dossier_python: &Path) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new(uv);
    cmd.args(args)
        .env("UV_PYTHON_INSTALL_DIR", dossier_python)
        .env("UV_CACHE_DIR", dossier_outils().join("cache"))
        .env("UV_NO_PROGRESS", "1")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let sortie = cmd.output().await.map_err(|e| format!("uv : {e}"))?;
    let texte = format!(
        "{}{}",
        String::from_utf8_lossy(&sortie.stdout),
        String::from_utf8_lossy(&sortie.stderr)
    );
    if !sortie.status.success() {
        let fin: String = texte
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("uv {} : {fin}", args.first().unwrap_or(&"")));
    }
    Ok(texte)
}

async fn installer_vraiment(app: &AppHandle) -> Result<(), String> {
    let uv = obtenir_uv(app).await?;
    let env = dossier_env();
    let dossier_python = dossier_outils().join("python");
    let env_s = env.display().to_string();

    emettre(app, "python", "Installation de Python…");
    lancer(
        &uv,
        &[
            "venv",
            "--python",
            PYTHON_VERSION,
            "--managed-python",
            "--allow-existing",
            &env_s,
        ],
        &dossier_python,
    )
    .await?;

    let python = python_de(&env);
    let python_s = python.display().to_string();
    let cuda = matches!(
        locaryn_llmfit::profile().backend,
        locaryn_llmfit::hardware::Backend::Cuda
    );
    let (appareil, index) = if cuda {
        ("cuda", "https://download.pytorch.org/whl/cu128")
    } else {
        ("cpu", "https://download.pytorch.org/whl/cpu")
    };
    emettre(
        app,
        "torch",
        if cuda {
            "Installation de PyTorch (CUDA)…"
        } else {
            "Installation de PyTorch (processeur)…"
        },
    );
    lancer(
        &uv,
        &[
            "pip",
            "install",
            "--python",
            &python_s,
            "--index-url",
            index,
            "torch",
        ],
        &dossier_python,
    )
    .await?;

    emettre(app, "laya", "Installation du modèle de décision…");
    lancer(
        &uv,
        &["pip", "install", "--python", &python_s, "laya"],
        &dossier_python,
    )
    .await?;

    // Les poids de Laya, téléchargés maintenant plutôt qu'à la première
    // action d'un morph (qui attendrait des minutes).
    emettre(app, "poids", "Téléchargement du modèle de décision…");
    let mut cmd = tokio::process::Command::new(&python);
    cmd.args([
        "-c",
        // Le seul modèle que chargent les morphs (multilingue, 650 Mo) : le
        // `Router` les préchargeait tous, 2,3 Go. Mesuré ensuite sur
        // processeur (RX 570, Ryzen) : 90 à 110 ms par décision.
        "import laya; laya.load('convaiinnovations/laya', subfolder='multilingual', device='cpu'); print(getattr(laya, '__version__', ''))",
    ])
    .envs(crate::python_env())
    .env("LAYA_DEVICE", appareil)
    .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let sortie = cmd.output().await.map_err(|e| format!("Laya : {e}"))?;
    if !sortie.status.success() {
        return Err(format!(
            "Laya ne démarre pas : {}",
            crate::summarise_python_error(&String::from_utf8_lossy(&sortie.stderr))
        ));
    }
    let version = String::from_utf8_lossy(&sortie.stdout).trim().to_string();

    std::fs::write(
        marqueur(),
        serde_json::json!({ "appareil": appareil, "laya": version, "uv": UV_VERSION }).to_string(),
    )
    .map_err(|e| e.to_string())?;
    tracing::info!(%appareil, %version, dossier = %env.display(), "Laya installé");
    Ok(())
}
