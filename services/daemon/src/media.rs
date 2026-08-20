//! Media generation routes — the machine-side engines exposed to thin
//! clients (the phone) that cannot run them.
//!
//! Generation happens where the models live, through `locaryn-media`; the
//! finished file is read back and returned as base64 so a phone can render or
//! play it without needing a file server.

use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures::StreamExt;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::io::AsyncWriteExt;

use crate::DaemonState;

pub(crate) fn is_chat_weight(path: &std::path::Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    // The daemon's managed chat provider is llama.cpp (`-m`), so only GGUF
    // primary weights are runnable. Transformers Safetensors stay available
    // to their dedicated media runtimes but never enter the chat selector.
    if !ext.eq_ignore_ascii_case("gguf") {
        return false;
    }
    let name = path.to_string_lossy().to_ascii_lowercase();
    !name.ends_with(".part")
        && !name.ends_with(".tmp")
        && ![
            "diffusion",
            "stable-diffusion",
            "flux",
            "vae",
            "mmproj",
            "text_encoder",
            "text-encoder",
            "clip",
            "tts",
            "kokoro",
            "xtts",
            "vocoder",
            "musicgen",
            "audio",
            "embed",
            "mtp-",
            "/mtp/",
            "-draft-",
            "_draft_",
        ]
        .iter()
        .any(|part| name.contains(part))
}

fn is_model_weight_path(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let name = name.strip_suffix(".part").unwrap_or(name);
    ["gguf", "safetensors", "onnx", "pth", "pt", "bin"]
        .iter()
        .any(|ext| name.to_ascii_lowercase().ends_with(&format!(".{ext}")))
}

fn model_shard_group(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    for marker in ["-of-", "_of_", "_of-"] {
        if let Some(of_pos) = lower.find(marker) {
            let mut start = of_pos;
            while start > 0 && lower.as_bytes()[start - 1].is_ascii_digit() {
                start -= 1;
            }
            if start < of_pos
                && start > 0
                && matches!(lower.as_bytes()[start - 1], b'-' | b'_' | b'.')
            {
                return path[..start - 1].to_string();
            }
        }
    }
    path.to_string()
}

fn walk_model_files(dir: &std::path::Path, depth: usize) -> Vec<std::path::PathBuf> {
    if depth == 0 {
        return Vec::new();
    }
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_model_files(&path, depth - 1));
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files
}

fn list_chat_models() -> Vec<String> {
    let models_dir = locaryn_config::models_dir();
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if is_chat_weight(&path) {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        names.push(name.to_string());
                    }
                }
                continue;
            }
            if !path.is_dir() {
                continue;
            }
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if dir_name.starts_with('.') {
                continue;
            }
            let mut groups: std::collections::HashMap<String, std::path::PathBuf> =
                std::collections::HashMap::new();
            for file in walk_model_files(&path, 5) {
                if !is_chat_weight(&file) {
                    continue;
                }
                let rel = file
                    .strip_prefix(&path)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let key = model_shard_group(&rel);
                let replace = groups
                    .get(&key)
                    .map(|existing| rel.len() < existing.to_string_lossy().len())
                    .unwrap_or(true);
                if replace {
                    groups.insert(key, file);
                }
            }
            for file in groups.values() {
                let rel = file
                    .strip_prefix(&path)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                names.push(format!("{dir_name}/{rel}"));
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// GET /v1/media/models?kind=audio|chat — what the machine can generate.
pub async fn list_models(
    State(_s): State<Arc<DaemonState>>,
    axum::extract::Query(params): axum::extract::Query<ModelQuery>,
) -> Response {
    let kind = params.kind.as_deref().unwrap_or("audio");
    if kind == "audio" {
        let models = locaryn_media::audio::list_tts_models();
        return Json(serde_json::json!({ "kind": kind, "models": models })).into_response();
    }
    if kind == "chat" {
        let models = list_chat_models();
        return Json(serde_json::json!({ "kind": kind, "models": models })).into_response();
    }
    if kind == "all" {
        let mut chat = list_chat_models();
        let mut audio = locaryn_media::audio::list_tts_models();
        let mut all = Vec::new();
        all.append(&mut chat);
        all.append(&mut audio);
        all.sort();
        all.dedup();
        return Json(serde_json::json!({ "kind": kind, "models": all })).into_response();
    }
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": { "code": "bad_request", "message": format!("kind inconnu : {kind} (audio|chat|all)") }
        })),
    )
        .into_response()
}

/// POST /v1/media/audio — text-to-speech via Kokoro or Qwen3-TTS.
pub async fn generate_audio(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<AudioGenBody>,
) -> Response {
    let req = locaryn_media::audio::TtsRequest {
        model: body.model,
        text: body.text,
        speed: body.speed.unwrap_or(1.0),
        language: body.language,
        // Même raison que les images : gros, refabricable, donc sur le
        // volume de données.
        output_dir: locaryn_config::generated_audio_dir(),
    };
    let progress = |pct: u32, detail: &str| {
        tracing::info!(progress = pct, detail, "audio generation");
    };

    let model_name = req.model.clone();
    let started = std::time::Instant::now();
    match locaryn_media::audio::generate_tts(req, &progress).await {
        Ok(file) => {
            record_speed(&s, &model_name, "audio", started).await;
            respond_file(file.path, "audio/wav", "wav")
        }
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, "generation_failed", &e),
    }
}

/// Enregistre la durée d'une génération réussie.
///
/// Seules les réussites comptent : une génération qui échoue au bout de deux
/// secondes ferait passer un modèle pour rapide.
async fn record_speed(s: &Arc<DaemonState>, model: &str, kind: &str, started: std::time::Instant) {
    let ms = started.elapsed().as_millis() as u64;
    if let Err(e) = s.storage.metrics.record_generation(model, kind, ms).await {
        tracing::warn!(model, error = %e, "vitesse de génération non enregistrée");
    }
}

/// Read a generated file and answer with its base64 payload.
fn respond_file(path: std::path::PathBuf, mime: &str, ext: &str) -> Response {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("generated.{ext}"));
    match std::fs::read(&path) {
        Ok(bytes) => Json(serde_json::json!({
            "name": name,
            "mime": mime,
            "size": bytes.len(),
            "data_base64": crate::base64_encode(&bytes),
        }))
        .into_response(),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "read_failed",
            &format!("fichier généré illisible : {e}"),
        ),
    }
}

fn err_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
pub struct ModelQuery {
    kind: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct AudioGenBody {
    model: String,
    text: String,
    speed: Option<f32>,
    language: Option<String>,
}

// ─── Installation de modèles (le marketplace) ───────────────────────────────

/// POST /v1/models/pull — installer un modèle de génération sur le serveur.
///
/// Deux formes acceptées, comme le marketplace de l'ordinateur :
///  - une URL directe vers un fichier de poids (.gguf, .safetensors, .onnx,
///    .bin, .pth, .pt) → le fichier tombe dans le dossier des modèles ;
///  - un dépôt HuggingFace (`https://huggingface.co/proprietaire/depot`) → le
///    dépôt complet est téléchargé dans `models_dir/{proprietaire}__{depot}`,
///    la forme qu'attendent les moteurs Kokoro et Qwen3-TTS.
///
/// Des fichiers supplémentaires peuvent être déclarés par le catalogue de
/// l'extension qui possède l'entrée. Le daemon valide puis exécute ce plan sans
/// connaître de famille de modèle ni d'adresse fournisseur.
///
/// La réponse est un flux d'événements (SSE) : chaque `data:` porte un point
/// d'avancement `{ downloaded, total, percentage, message }`, et le dernier
/// `{ "done": true, "name": …, "size": … }` — ou `{ "error": … }` quand le
/// téléchargement casse en route. Les refus qui précèdent tout octet reçu
/// (adresse invalide, déjà installé) restent des réponses JSON ordinaires.
pub async fn pull_model(Json(body): Json<PullBody>) -> Response {
    let raw_url = body.url.or(body.name).or(body.model).unwrap_or_default();
    let mut url = raw_url.trim().to_string();
    if url.starts_with("hf.co/") {
        url = url.replace("hf.co/", "https://huggingface.co/");
    } else if !url.starts_with("http")
        && url.contains('/')
        && !url.contains('\\')
        && !url.contains(' ')
    {
        url = format!("https://huggingface.co/{url}");
    }
    let (file_name, kind) = match classify_model_url(&url) {
        Ok(v) => v,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };
    let declared_companions = match validate_marketplace_companions(body.companions) {
        Ok(companions) => companions,
        Err(message) => return err_response(StatusCode::BAD_REQUEST, "bad_request", &message),
    };
    if kind == ModelKind::Repo && !declared_companions.is_empty() {
        return err_response(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "Les fichiers compagnons ne sont acceptés qu'avec un fichier modèle direct.",
        );
    }

    let models_dir = locaryn_config::models_dir();
    if let Err(e) = std::fs::create_dir_all(&models_dir) {
        return err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "mkdir_failed",
            &format!("dossier des modèles illisible : {e}"),
        );
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .user_agent("locaryn-daemon")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "client_failed",
                &format!("client : {e}"),
            );
        }
    };

    // Tout ce qui peut être refusé l'est ici, avant que le flux commence : une
    // réponse d'erreur ordinaire se lit plus franchement qu'un dernier
    // événement au milieu d'un téléchargement.
    let (prep, total) = match kind {
        // Dépôt complet (TTS multi-fichiers : Kokoro, Qwen3…).
        ModelKind::Repo => {
            let dest_dir = models_dir.join(&file_name);
            if body.selection.is_none() && dest_dir.exists() && dest_dir.is_dir() {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": { "code": "already_installed", "message": format!("Le dépôt {file_name} est déjà installé.") }
                    })),
                )
                    .into_response();
            }
            match list_repo_files(&client, &url).await {
                Ok((repo_id, fichiers)) => {
                    let fichiers = match body.selection.as_ref() {
                        Some(choice) => match select_repo_files(&repo_id, fichiers, choice) {
                            Ok(files) => files,
                            Err(msg) => {
                                return err_response(StatusCode::BAD_REQUEST, "bad_request", &msg);
                            }
                        },
                        None => fichiers,
                    };
                    let total: u64 = fichiers.iter().map(|(_, s)| s).sum();
                    (
                        Preparé::Depot {
                            repo_id,
                            dest_dir,
                            fichiers,
                        },
                        total,
                    )
                }
                Err(msg) => {
                    return err_response(StatusCode::BAD_REQUEST, "download_failed", &msg);
                }
            }
        }
        // Un fichier unique ; déjà présent = déjà installé.
        ModelKind::File => {
            let dest = models_dir.join(&file_name);
            let already_installed = dest.exists();
            let compagnons: Vec<MarketplaceCompanionDownload> = declared_companions
                .into_iter()
                .filter(|comp| !models_dir.join(&comp.file).exists())
                .collect();
            if already_installed && compagnons.is_empty() {
                let _ = std::fs::remove_file(dest.with_extension("part"));
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": { "code": "already_installed", "message": format!("{file_name} est déjà installé.") }
                    })),
                )
                    .into_response();
            }
            // La taille connue d'avance (HEAD) permet à la barre d'afficher un
            // vrai pourcentage dès la première seconde — et les fichiers
            // déclarés comptent dans le même total, pour que 100 % veuille
            // vraiment dire « tout est là ».
            let mut total = if already_installed {
                0
            } else {
                head_length(&client, &url).await.unwrap_or(0)
            };
            for comp in &compagnons {
                total += head_length(&client, &comp.url).await.unwrap_or(0);
            }
            (
                Preparé::Fichier {
                    dest,
                    part: models_dir.join(format!("{file_name}.part")),
                    compagnons,
                    already_installed,
                },
                total,
            )
        }
    };

    // Le téléchargement tourne dans une tâche, le flux SSE la lit : chaque
    // avancement part au client dès qu'il arrive, sans attendre la fin.
    let (tx, rx) = tokio::sync::mpsc::channel::<serde_json::Value>(32);
    let progress = PullProgress::nouvelle(tx, total);
    // D'abord un point à zéro : le client connaît tout de suite la taille
    // totale et sait que le téléchargement a commencé, avant le premier octet.
    progress.emettre(None);
    tokio::spawn(async move {
        let resultat = match prep {
            Preparé::Depot {
                repo_id,
                dest_dir,
                fichiers,
            } => pull_repo_files(&repo_id, &dest_dir, &fichiers, &progress)
                .await
                .map(|size| (file_name, size)),
            Preparé::Fichier {
                dest,
                part,
                compagnons,
                already_installed,
            } => match if already_installed {
                std::fs::metadata(&dest)
                    .map(|metadata| metadata.len())
                    .map_err(|error| error.to_string())
            } else {
                download_to(&client, &url, &dest, &part, &progress).await
            } {
                Ok(size) => {
                    if !compagnons.is_empty() {
                        progress.noter("Installation des compagnons…");
                        if let Err(message) =
                            install_declared_companions(&client, &compagnons, &progress).await
                        {
                            Err(message)
                        } else {
                            Ok((file_name, size))
                        }
                    } else {
                        Ok((file_name, size))
                    }
                }
                Err(msg) => {
                    let _ = std::fs::remove_file(&part);
                    Err(msg)
                }
            },
        };
        match resultat {
            Ok((name, size)) => {
                progress.terminer();
                let _ = progress.tx.try_send(serde_json::json!({
                    "done": true,
                    "name": name,
                    "size": size,
                }));
            }
            Err(msg) => {
                let _ = progress.tx.try_send(serde_json::json!({ "error": msg }));
            }
        }
    });

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|v| (serde_json::to_string(&v).unwrap_or_default(), rx))
    });
    let sse = Sse::new(
        stream.map(|ligne| Ok::<Event, std::convert::Infallible>(Event::default().data(ligne))),
    )
    .keep_alive(KeepAlive::default());
    sse.into_response()
}

/// Remove one local model identity and every shard belonging to that variant.
/// Other quantisations in the same repository directory stay untouched.
fn remove_model_artifacts(name: &str) -> Result<(), String> {
    let models_dir = locaryn_config::models_dir();
    let path = models_dir.join(name);
    let partial = path.with_extension("part");
    let mut targets = Vec::new();
    if path.is_file() || partial.is_file() {
        targets.push(path.clone());
    }

    if let Ok(relative) = path.strip_prefix(&models_dir) {
        if let Some(first) = relative.components().next() {
            let repo_dir = models_dir.join(first.as_os_str());
            if repo_dir.is_dir() {
                let relative_model = path
                    .strip_prefix(&repo_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let model_key = relative_model
                    .strip_suffix(".part")
                    .unwrap_or(&relative_model);
                if is_model_weight_path(std::path::Path::new(model_key)) {
                    let group = model_shard_group(model_key);
                    for file in walk_model_files(&repo_dir, 8) {
                        if !file.is_file() || !is_model_weight_path(&file) {
                            continue;
                        }
                        let rel = file
                            .strip_prefix(&repo_dir)
                            .unwrap_or(&file)
                            .to_string_lossy()
                            .replace('\\', "/");
                        let clean = rel.strip_suffix(".part").unwrap_or(&rel);
                        if model_shard_group(clean) == group && !targets.contains(&file) {
                            targets.push(file);
                        }
                    }
                }
            }
        }
    }

    let mut deleted = false;
    for target in targets {
        if target.is_file() {
            std::fs::remove_file(&target)
                .map_err(|e| format!("suppression de {} impossible : {e}", target.display()))?;
            deleted = true;
        }
        let target_part = target.with_extension("part");
        if target_part.is_file() {
            std::fs::remove_file(&target_part).map_err(|e| {
                format!(
                    "suppression du fichier partiel {} impossible : {e}",
                    target_part.display()
                )
            })?;
            deleted = true;
        }
    }
    if !deleted {
        return Err(format!("{name} n'est pas installé sur ce serveur."));
    }
    if let Some(parent) = path.parent() {
        remove_empty_parent_dirs(parent, &models_dir);
    }
    Ok(())
}

/// DELETE /v1/models/{name} — retirer un modèle installé du serveur.
///
/// Un fichier unique (`.gguf`, `.onnx`…) est effacé ; un dépôt HuggingFace
/// est effacé avec son dossier. Les fichiers partagés déclarés en complément ne
/// sont pas touchés : plusieurs modèles les partagent, et retirer un modèle
/// ne doit pas en casser un autre.
pub async fn remove_model(axum::extract::Path(name): axum::extract::Path<String>) -> Response {
    let name = name.trim().replace('\\', "/");
    if !nom_modele_valide(&name) {
        return err_response(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "Nom de modèle invalide.",
        );
    }
    let models_dir = locaryn_config::models_dir();
    let path = models_dir.join(&name);
    let meta = match tokio::fs::symlink_metadata(&path).await {
        Ok(m) => m,
        Err(_) if path.with_extension("part").is_file() => match remove_model_artifacts(&name) {
            Ok(()) => return Json(serde_json::json!({ "removed": name })).into_response(),
            Err(e) => return err_response(StatusCode::NOT_FOUND, "not_found", &e),
        },
        Err(_) => {
            return err_response(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("{name} n'est pas installé sur ce serveur."),
            );
        }
    };
    // Un lien symbolique est effacé comme tel, jamais suivi : on ne peut pas
    // faire sortir le retrait du dossier des modèles par un détour.
    if meta.file_type().is_dir() {
        return match tokio::fs::remove_dir_all(&path).await {
            Ok(()) => Json(serde_json::json!({ "removed": name })).into_response(),
            Err(e) => err_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "remove_failed",
                &format!("Impossible de retirer {name} : {e}"),
            ),
        };
    }
    match remove_model_artifacts(&name) {
        Ok(()) => Json(serde_json::json!({ "removed": name })).into_response(),
        Err(e) => err_response(StatusCode::NOT_FOUND, "not_found", &e),
    }
}

/// Un nom de modèle est-il sûr à utiliser comme chemin, sans sortir du
/// dossier des modèles ? Les sous-chemins `repo/variant.gguf` sont autorisés
/// pour pouvoir supprimer une quantisation précise d'un dépôt multi-modèles.
fn nom_modele_valide(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !std::path::Path::new(name).is_absolute()
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".." && !part.starts_with('.'))
}

/// Ce qu'il faut télécharger, une fois l'adresse acceptée.
enum Preparé {
    /// Un dépôt HuggingFace entier : la liste des fichiers, avec leur taille.
    Depot {
        repo_id: String,
        dest_dir: std::path::PathBuf,
        fichiers: Vec<(String, u64)>,
    },
    /// Un fichier unique, plus les fichiers déclarés par son extension.
    Fichier {
        dest: std::path::PathBuf,
        part: std::path::PathBuf,
        compagnons: Vec<MarketplaceCompanionDownload>,
        already_installed: bool,
    },
}

/// La fréquence des points d'avancement, comme pour la mise à jour du mobile.
const PAS_MO: u64 = 256 * 1024;

/// Émet l'avancement d'une installation vers le flux SSE.
///
/// La barre n'a besoin que d'un point toutes les ~256 Ko : en envoyer à chaque
/// paquet noierait la liaison sans rien apprendre de plus. Les compteurs sont
/// atomiques parce que le téléchargement d'un dépôt fait plusieurs boucles ;
/// la barre, elle, n'en voit qu'une, continue.
struct PullProgress {
    tx: tokio::sync::mpsc::Sender<serde_json::Value>,
    /// Total connu d'avance ; 0 = inconnu, barre indéterminée. Atomique parce
    /// que le GET peut révéler la taille quand le HEAD ne l'avait pas donnée.
    total: AtomicU64,
    fait: AtomicU64,
    prochain_seuil: AtomicU64,
}

/// A partial file is not an installed model. Remove it on every failed or
/// cancelled download; only the final rename makes the guard commit.
struct PartialDownloadGuard {
    path: std::path::PathBuf,
    committed: bool,
}

impl PartialDownloadGuard {
    fn new(path: &std::path::Path) -> Self {
        Self {
            path: path.to_path_buf(),
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for PartialDownloadGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn remove_empty_parent_dirs(start: &std::path::Path, stop: &std::path::Path) {
    let mut current = start.to_path_buf();
    while current.starts_with(stop) && current != stop {
        let empty = std::fs::read_dir(&current)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if !empty || std::fs::remove_dir(&current).is_err() {
            break;
        }
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
}

impl PullProgress {
    fn nouvelle(tx: tokio::sync::mpsc::Sender<serde_json::Value>, total: u64) -> Self {
        Self {
            tx,
            total: AtomicU64::new(total),
            fait: AtomicU64::new(0),
            prochain_seuil: AtomicU64::new(PAS_MO),
        }
    }

    /// La taille totale connue, ou 0 si on ne la sait pas encore.
    fn total_connu(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// Apprendre la taille en cours de route (le GET la connaît parfois quand
    /// le HEAD ne l'a pas donnée) et le faire savoir au client.
    fn definir_total(&self, n: u64) {
        if n > 0 && self.total_connu() == 0 {
            self.total.store(n, Ordering::Relaxed);
            self.emettre(None);
        }
    }

    fn pourcentage(&self) -> Option<u8> {
        let total = self.total_connu();
        (total > 0).then(|| {
            let fait = self.fait.load(Ordering::Relaxed);
            ((fait as f64 / total as f64) * 100.0).min(100.0) as u8
        })
    }

    fn emettre(&self, message: Option<&str>) {
        let total = self.total_connu();
        let _ = self.tx.try_send(serde_json::json!({
            "downloaded": self.fait.load(Ordering::Relaxed),
            "total": (total > 0).then_some(total),
            "percentage": self.pourcentage(),
            "message": message,
        }));
    }

    /// Compter des octets reçus, en émettant au plus tous les `PAS_MO`.
    fn ajouter(&self, n: u64, message: Option<&str>) {
        let fait = self.fait.fetch_add(n, Ordering::Relaxed) + n;
        if fait >= self.prochain_seuil.load(Ordering::Relaxed) {
            self.prochain_seuil.store(fait + PAS_MO, Ordering::Relaxed);
            self.emettre(message);
        }
    }

    /// Annoncer une phase sans attendre le prochain seuil.
    fn noter(&self, message: &str) {
        self.emettre(Some(message));
    }

    /// Le dernier point : 100 % quand le total est connu. L'événement `done`
    /// (ou `error`) suit, et le flux se ferme.
    fn terminer(&self) {
        let total = self.total_connu();
        let _ = self.tx.try_send(serde_json::json!({
            "downloaded": self.fait.load(Ordering::Relaxed),
            "total": (total > 0).then_some(total),
            "percentage": (total > 0).then_some(100u8),
            "message": None::<String>,
        }));
    }
}

/// La taille qu'annonce le serveur pour un fichier, sans le télécharger.
///
/// Absente quand le serveur ne l'annonce pas (ou répond autre chose qu'un
/// succès : une page de redirection n'est pas une taille) : la barre devient
/// indéterminée, ce qui vaut mieux qu'un pourcentage inventé.
async fn head_length(client: &reqwest::Client, url: &str) -> Option<u64> {
    client
        .head(url)
        .send()
        .await
        .ok()
        .filter(|r| r.status().is_success())
        .and_then(|r| r.content_length())
}

/// Keep only the model files selected in the desktop selector, plus its small
/// tokenizer/config support files. The server rechecks the tree so a stale UI
/// cannot make it download a different path (or a whole repository).
fn select_repo_files(
    repo_id: &str,
    available: Vec<(String, u64)>,
    choice: &serde_json::Value,
) -> Result<Vec<(String, u64)>, String> {
    let selected_repo = choice
        .get("repo")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .trim_start_matches("https://huggingface.co/")
        .trim_start_matches("hf.co/")
        .trim_end_matches('/')
        .to_string();
    if selected_repo != repo_id {
        return Err("La variante sélectionnée ne vient pas de ce dépôt HuggingFace.".into());
    }
    let mut paths = Vec::new();
    for key in ["files", "support_files"] {
        if let Some(items) = choice.get(key).and_then(|value| value.as_array()) {
            for item in items {
                let path = item
                    .as_str()
                    .ok_or_else(|| "Chemin de variante HuggingFace invalide.".to_string())?;
                if path.is_empty()
                    || path.starts_with('/')
                    || path.contains("..")
                    || path.contains('\\')
                {
                    return Err("Chemin de variante HuggingFace invalide.".into());
                }
                paths.push(path.to_string());
            }
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err("La variante HuggingFace ne contient aucun fichier.".into());
    }
    let mut result = Vec::with_capacity(paths.len());
    for path in paths {
        let Some((_, size)) = available.iter().find(|(candidate, _)| candidate == &path) else {
            return Err(format!(
                "Le fichier sélectionné n'existe plus dans le dépôt : {path}"
            ));
        };
        result.push((path, *size));
    }
    Ok(result)
}

/// Ce qu'on peut installer : un fichier unique, ou un dépôt HuggingFace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelKind {
    File,
    Repo,
}

#[derive(serde::Deserialize)]
pub struct PullBody {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Optional candidate selected after inspecting a multi-variant HF repo.
    /// Without it, legacy callers may still request a complete repository.
    #[serde(default)]
    pub selection: Option<serde_json::Value>,
    #[serde(default)]
    pub companions: Vec<MarketplaceCompanionDownload>,
}

/// Décider de ce que désigne une adresse, et du nom sous lequel le modèle
/// apparaîtra dans la liste des installés.
fn classify_model_url(url: &str) -> Result<(String, ModelKind), String> {
    if !url.starts_with("https://") {
        return Err("Seules les adresses https:// sont acceptées.".into());
    }
    // Un dépôt HuggingFace : pas de /resolve/ ni /blob/ — c'est le dépôt entier
    // qu'on veut, pas un de ses fichiers.
    if url.starts_with("https://huggingface.co/")
        && !url.contains("/resolve/")
        && !url.contains("/blob/")
    {
        let repo = url
            .trim_end_matches('/')
            .strip_prefix("https://huggingface.co/")
            .unwrap_or("")
            .to_string();
        if repo.is_empty() {
            return Err("Adresse HuggingFace incomplète.".into());
        }
        let nom = repo.replace('/', "__");
        return Ok((nom, ModelKind::Repo));
    }
    let name = filename_from_url(url)
        .ok_or_else(|| "Impossible de tirer un nom de fichier de cette adresse.".to_string())?;
    let lower = name.to_ascii_lowercase();
    let acceptee = [".gguf", ".safetensors", ".onnx", ".bin", ".pth", ".pt"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    if !acceptee {
        return Err(
            "L'adresse doit pointer vers un fichier de poids (.gguf, .safetensors, .onnx, \
             .bin, .pth, .pt) ou vers un dépôt HuggingFace."
                .into(),
        );
    }
    Ok((name, ModelKind::File))
}

/// Le nom de fichier d'une URL directe : dernier segment, décodé, sans la
/// requête. Refusé s'il est vide, caché, ou contient un séparateur.
fn filename_from_url(url: &str) -> Option<String> {
    let raw = url.split('?').next().unwrap_or(url);
    let seg = raw.rsplit('/').next()?;
    let decoded = percent_decode(seg);
    if decoded.is_empty() || decoded.starts_with('.') || decoded == ".." || decoded.contains('/') {
        return None;
    }
    Some(decoded)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Télécharger un fichier vers `dest`, via un `.part` pour qu'un téléchargement
/// interrompu ne soit jamais pris pour un modèle installé. Chaque bloc reçu est
/// compté dans `progress`, qui décide lui-même à quelle fréquence émettre.
async fn download_to(
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
    part: &std::path::Path,
    progress: &PullProgress,
) -> Result<u64, String> {
    let mut partial_cleanup = PartialDownloadGuard::new(part);
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("téléchargement impossible : {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("le serveur a répondu {status}."));
    }
    // Le HEAD n'a pas toujours donné la taille : la réponse du GET, elle, la
    // connaît presque toujours. Dès qu'on l'apprend, la barre devient exacte.
    progress.definir_total(resp.content_length().unwrap_or(0));
    let mut out = tokio::fs::File::create(part)
        .await
        .map_err(|e| format!("fichier : {e}"))?;
    let mut stream = resp.bytes_stream();
    let mut total: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("lecture : {e}"))?;
        total += chunk.len() as u64;
        progress.ajouter(chunk.len() as u64, None);
        out.write_all(&chunk)
            .await
            .map_err(|e| format!("écriture : {e}"))?;
    }
    out.flush().await.map_err(|e| format!("écriture : {e}"))?;
    drop(out);
    std::fs::rename(part, dest).map_err(|e| format!("finalisation : {e}"))?;
    partial_cleanup.commit();
    Ok(total)
}

/// Lister les fichiers d'un dépôt HuggingFace, avec la taille que l'API
/// annonce pour chacun.
///
/// La taille est connue d'avance : c'est elle qui permet à la barre de couvrir
/// tout le dépôt — et pas seulement le fichier en cours —, et de donner un
/// vrai pourcentage dès le départ.
async fn list_repo_files(
    client: &reqwest::Client,
    url: &str,
) -> Result<(String, Vec<(String, u64)>), String> {
    let repo_id = url
        .trim_end_matches('/')
        .strip_prefix("https://huggingface.co/")
        .unwrap_or(url)
        .split("/tree/")
        .next()
        .unwrap_or(url)
        .split("/blob/")
        .next()
        .unwrap_or(url);
    if repo_id.is_empty() {
        return Err("Adresse HuggingFace incomplète.".into());
    }

    let tree_url = format!("https://huggingface.co/api/models/{repo_id}/tree/main?recursive=true");
    let resp = client
        .get(&tree_url)
        .send()
        .await
        .map_err(|e| format!("liste des fichiers : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Impossible de lister les fichiers du dépôt {repo_id} (HTTP {}).",
            resp.status()
        ));
    }
    let entries: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| format!("réponse du dépôt illisible : {e}"))?;
    let fichiers: Vec<(String, u64)> = entries
        .iter()
        .filter(|e| e.get("type").and_then(|v| v.as_str()) == Some("file"))
        .filter_map(|e| {
            let path = e.get("path")?.as_str()?.to_string();
            let size = e.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            Some((path, size))
        })
        .filter(|(p, _)| {
            !p.starts_with("/")
                && !p.contains("..")
                && !p.contains('\\')
                && !p.starts_with("eval/")
                && !p.starts_with("samples/")
                && p != ".gitattributes"
        })
        .collect();
    if fichiers.is_empty() {
        return Err(format!("Aucun fichier trouvé dans le dépôt {repo_id}."));
    }
    Ok((repo_id.to_string(), fichiers))
}

/// Télécharger un dépôt HuggingFace entier, fichier par fichier (l'archive ZIP
/// du dépôt renvoie un 404 pour la plupart d'entre eux).
async fn pull_repo_files(
    repo_id: &str,
    dest_dir: &std::path::Path,
    fichiers: &[(String, u64)],
    progress: &PullProgress,
) -> Result<u64, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("dossier : {e}"))?;
    let dl_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .user_agent("locaryn-daemon")
        .build()
        .map_err(|e| format!("client : {e}"))?;
    let models_dir = locaryn_config::models_dir();
    let mut created_files: Vec<std::path::PathBuf> = Vec::new();
    let result = async {
        let mut total: u64 = 0;
        for (i, (path, expected_size)) in fichiers.iter().enumerate() {
            let out = dest_dir.join(path);
            let part = out.with_extension("part");
            let existed = out.exists();
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("dossier : {e}"))?;
            }
            progress.noter(&format!("Fichier {}/{}…", i + 1, fichiers.len()));
            tracing::info!(file = %path, position = i + 1, total = fichiers.len(), "dépôt HuggingFace");
            let dl = format!("https://huggingface.co/{repo_id}/resolve/main/{path}");
            if existed {
                // A completed model never needs a stale resume marker left by
                // an earlier crash.
                let _ = std::fs::remove_file(&part);
                total = total.saturating_add(*expected_size);
                continue;
            }
            match download_to(&dl_client, &dl, &out, &part, progress).await {
                Ok(n) => {
                    total += n;
                    if !existed {
                        created_files.push(out);
                    }
                }
                Err(e) => return Err(format!("{path} : {e}")),
            }
        }
        Ok::<u64, String>(total)
    }
    .await;

    if let Err(error) = result {
        // A repository install is one transaction from the user's point of
        // view. Remove files completed by this attempt, but preserve files
        // belonging to another already-installed variant in the same repo.
        for file in &created_files {
            let _ = std::fs::remove_file(file);
        }
        remove_empty_parent_dirs(dest_dir, &models_dir);
        return Err(error);
    }
    result
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceCompanionDownload {
    url: String,
    file: String,
    label: Option<String>,
}

fn validate_marketplace_companions(
    companions: Vec<MarketplaceCompanionDownload>,
) -> Result<Vec<MarketplaceCompanionDownload>, String> {
    if companions.len() > 16 {
        return Err("Un plan d'installation ne peut pas ajouter plus de 16 fichiers.".into());
    }
    let mut seen = std::collections::HashSet::new();
    for companion in &companions {
        let parsed = reqwest::Url::parse(&companion.url)
            .map_err(|_| format!("Adresse de fichier compagnon invalide : {}", companion.url))?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            return Err(format!(
                "Le fichier compagnon {} doit utiliser une adresse HTTPS.",
                companion.file
            ));
        }
        let path = std::path::Path::new(&companion.file);
        let mut components = path.components();
        if path.is_absolute()
            || !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
            || companion.file.ends_with(".part")
        {
            return Err(format!(
                "Nom de fichier compagnon non sûr : {}",
                companion.file
            ));
        }
        if !seen.insert(companion.file.to_ascii_lowercase()) {
            return Err(format!(
                "Fichier compagnon déclaré deux fois : {}",
                companion.file
            ));
        }
    }
    Ok(companions)
}

/// Install every validated extra file declared by the extension catalogue.
async fn install_declared_companions(
    client: &reqwest::Client,
    compagnons: &[MarketplaceCompanionDownload],
    progress: &PullProgress,
) -> Result<(), String> {
    let models_dir = locaryn_config::models_dir();
    for comp in compagnons {
        let label = comp.label.as_deref().unwrap_or(&comp.file);
        let dest = models_dir.join(&comp.file);
        if dest.exists() {
            let _ = std::fs::remove_file(dest.with_extension("part"));
            continue;
        }
        progress.noter(&format!("Compagnon : {label}…"));
        tracing::info!(file = comp.file, "compagnon déclaré : {label}");
        let part = models_dir.join(format!("{}.part", comp.file));
        download_to(client, &comp.url, &dest, &part, progress).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_adresses_valides_sont_classées() {
        // Un fichier direct : le nom du fichier est retenu.
        let (name, kind) =
            classify_model_url("https://models.example/owner/repo/resolve/main/model-Q4_K_M.gguf")
                .unwrap();
        assert_eq!(kind, ModelKind::File);
        assert_eq!(name, "model-Q4_K_M.gguf");

        // Un dépôt HuggingFace : le nom devient propriétaire__dépôt.
        let (name, kind) = classify_model_url("https://huggingface.co/hexgrad/Kokoro-82M").unwrap();
        assert_eq!(kind, ModelKind::Repo);
        assert_eq!(name, "hexgrad__Kokoro-82M");

        // Une requête ne gêne pas le nom du fichier.
        let (name, _) =
            classify_model_url("https://huggingface.co/o/r/resolve/main/model.gguf?download=true")
                .unwrap();
        assert_eq!(name, "model.gguf");

        // Refusés : pas de https, extension inconnue, nom vide.
        assert!(classify_model_url("http://example.com/model.gguf").is_err());
        assert!(classify_model_url("https://example.com/model.txt").is_err());
        assert!(classify_model_url("https://example.com/").is_err());
        assert!(classify_model_url("https://huggingface.co/juste/un").is_ok()); // dépôt
    }

    #[test]
    fn le_nom_de_fichier_est_décodé_et_sécurisé() {
        assert_eq!(
            filename_from_url("https://x/y/mon%20mod%C3%A8le.gguf").as_deref(),
            Some("mon modèle.gguf")
        );
        assert!(filename_from_url("https://x/.cache").is_none());
        assert!(filename_from_url("https://x/..").is_none());
    }

    #[test]
    fn les_compagnons_declares_restent_generiques_et_surs() {
        let valid = validate_marketplace_companions(vec![MarketplaceCompanionDownload {
            url: "https://models.example/encoder.gguf".into(),
            file: "encoder.gguf".into(),
            label: Some("encodeur".into()),
        }])
        .unwrap();
        assert_eq!(valid[0].file, "encoder.gguf");

        for file in ["../outside.gguf", "nested/file.gguf", "partial.gguf.part"] {
            assert!(
                validate_marketplace_companions(vec![MarketplaceCompanionDownload {
                    url: "https://models.example/file.gguf".into(),
                    file: file.into(),
                    label: None,
                }])
                .is_err()
            );
        }
    }

    #[test]
    fn les_variantes_et_les_shards_restent_distincts() {
        assert_eq!(
            model_shard_group("model-Q4_K_M-00002-of-00003.gguf"),
            "model-Q4_K_M"
        );
        assert_ne!(
            model_shard_group("model-Q4_K_M.gguf"),
            model_shard_group("model-Q8_0.gguf")
        );
    }

    #[test]
    fn seuls_les_gguf_principaux_apparaissent_dans_le_chat() {
        assert!(is_chat_weight(std::path::Path::new(
            "Qwen3.8-27B-Q4_K_M.gguf"
        )));
        assert!(!is_chat_weight(std::path::Path::new(
            "Qwen__Qwen3.8-27B/model-00001-of-00018.safetensors"
        )));
        assert!(!is_chat_weight(std::path::Path::new(
            "mmproj-Qwen3.8-27B-Q8_0.gguf"
        )));
        assert!(!is_chat_weight(std::path::Path::new(
            "mtp-Qwen3.8-27B-Q4_0.gguf"
        )));
    }

    #[test]
    fn les_fichiers_partiels_sont_supprimes_si_le_telechargement_echoue() {
        let path = std::env::temp_dir().join(format!(
            "locaryn-daemon-part-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"partial").unwrap();
        {
            let _guard = super::PartialDownloadGuard::new(&path);
        }
        assert!(!path.exists());

        std::fs::write(&path, b"complete").unwrap();
        {
            let mut guard = super::PartialDownloadGuard::new(&path);
            guard.commit();
        }
        assert!(path.exists());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn les_noms_de_modeles_restent_dans_le_dossier() {
        assert!(nom_modele_valide("z_image_turbo-Q8_0.gguf"));
        assert!(nom_modele_valide("hexgrad__Kokoro-82M"));
        assert!(nom_modele_valide("hexgrad__repo/model-Q4_K_M.gguf"));
        // Les sous-chemins sont nécessaires pour supprimer une variante ; les
        // remontées, fichiers cachés et séparateurs Windows restent interdits.
        assert!(!nom_modele_valide("../secret.gguf"));
        assert!(!nom_modele_valide("a/../b.gguf"));
        assert!(!nom_modele_valide("a\\b.gguf"));
        assert!(!nom_modele_valide("C:/outside/model.gguf"));
        assert!(!nom_modele_valide(".cache"));
        assert!(!nom_modele_valide(""));
        assert!(!nom_modele_valide(".."));
    }
}
