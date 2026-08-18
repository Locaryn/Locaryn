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

/// GET /v1/media/models?kind=image|audio — what the machine can generate.
pub async fn list_models(
    State(_s): State<Arc<DaemonState>>,
    axum::extract::Query(params): axum::extract::Query<ModelQuery>,
) -> Response {
    let kind = params.kind.as_deref().unwrap_or("image");
    // Les images se disent en détail : un modèle de diffusion seul apparaît
    // dans la liste mais annonce ce qui lui manque, pour que le client ne le
    // propose pas comme s'il pouvait produire quelque chose.
    if kind == "image" {
        let details = locaryn_media::image::list_image_models_detailed();
        let names: Vec<&str> = details.iter().map(|d| d.name.as_str()).collect();
        return Json(serde_json::json!({
            "kind": kind,
            "models": names,
            "details": details,
        }))
        .into_response();
    }
    let models = match kind {
        "audio" => locaryn_media::audio::list_tts_models(),
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": format!("kind inconnu : {other} (image|audio)") }
                })),
            )
                .into_response();
        }
    };
    Json(serde_json::json!({ "kind": kind, "models": models })).into_response()
}

/// Décode une image source envoyée par un client mince (base64, avec ou
/// sans le préfixe `data:...;base64,`) vers un fichier temporaire sur cette
/// machine — c'est elle qui a le moteur, la vue web du client n'a pas accès
/// à son propre disque de la même façon.
fn decode_input_image(data: &str) -> Result<std::path::PathBuf, String> {
    let payload = data.split_once(',').map(|(_, p)| p).unwrap_or(data);
    let bytes = locaryn_shared_types::base64_decode(payload)?;
    let ext = if data.contains("jpeg") || data.contains("jpg") {
        "jpg"
    } else if data.contains("webp") {
        "webp"
    } else {
        "png"
    };
    let path = locaryn_config::ensure_temp_dir().join(format!(
        "media_input_{}.{ext}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    std::fs::write(&path, bytes).map_err(|e| format!("écriture image source : {e}"))?;
    Ok(path)
}

/// POST /v1/media/image — text-to-image (ou img2img si `input_image` est
/// fourni) via stable-diffusion.cpp.
pub async fn generate_image(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<ImageGenBody>,
) -> Response {
    let width = body.width.unwrap_or(1024);
    let height = body.height.unwrap_or(1024);
    let variants = body.variants.unwrap_or(1).clamp(1, 8);

    let input_image = match body.input_image.as_deref().filter(|s| !s.is_empty()) {
        Some(data) => match decode_input_image(data) {
            Ok(p) => Some(p),
            Err(e) => return err_response(StatusCode::BAD_REQUEST, "bad_request", &e),
        },
        None => None,
    };

    let req = locaryn_media::image::ImageRequest {
        model: body.model,
        prompt: body.prompt,
        negative_prompt: body.negative_prompt,
        width,
        height,
        steps: body.steps,
        cfg_scale: body.cfg_scale,
        variants,
        // Volumineux et refabricable : suit la racine de stockage, pas le
        // disque système.
        output_dir: locaryn_config::generated_images_dir(),
        input_image,
    };
    let progress = |pct: u32, detail: &str| {
        tracing::info!(progress = pct, detail, "image generation");
    };

    // Chronométré pour de vrai : c'est ce temps-là, sur cette machine, qui
    // permettra de comparer deux modèles dans le catalogue.
    let model_name = req.model.clone();
    let started = std::time::Instant::now();
    match locaryn_media::image::generate_image(req, &progress).await {
        Ok(file) => {
            record_speed(&s, &model_name, "image", started).await;
            respond_file(file.path, "image/png", "png")
        }
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, "generation_failed", &e),
    }
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
pub struct ImageGenBody {
    model: String,
    prompt: String,
    negative_prompt: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    steps: Option<u32>,
    cfg_scale: Option<f32>,
    variants: Option<u32>,
    /// Image source pour une édition (img2img) : base64, avec ou sans le
    /// préfixe `data:image/...;base64,`. Absent = texte vers image ordinaire.
    input_image: Option<String>,
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
/// Un checkpoint d'image seul ne produit rien : ses compagnons (VAE, encodeur
/// de texte) sont récupérés automatiquement, comme sur l'ordinateur.
///
/// La réponse est un flux d'événements (SSE) : chaque `data:` porte un point
/// d'avancement `{ downloaded, total, percentage, message }`, et le dernier
/// `{ "done": true, "name": …, "size": … }` — ou `{ "error": … }` quand le
/// téléchargement casse en route. Les refus qui précèdent tout octet reçu
/// (adresse invalide, déjà installé) restent des réponses JSON ordinaires.
pub async fn pull_model(Json(body): Json<PullBody>) -> Response {
    let url = body.url.trim().to_string();
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
            if dest_dir.exists() && dest_dir.is_dir() {
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
            if dest.exists() {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": { "code": "already_installed", "message": format!("{file_name} est déjà installé.") }
                    })),
                )
                    .into_response();
            }
            // La taille connue d'avance (HEAD) permet à la barre d'afficher un
            // vrai pourcentage dès la première seconde — et les compagnons
            // d'image comptent dans le même total, pour que 100 % veuille
            // vraiment dire « tout est là ».
            let mut total = head_length(&client, &url).await.unwrap_or(0);
            let compagnons = companions_for(&file_name);
            for comp in &compagnons {
                total += head_length(&client, comp.url).await.unwrap_or(0);
            }
            (
                Preparé::Fichier {
                    dest,
                    part: models_dir.join(format!("{file_name}.part")),
                    compagnons,
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
            } => match download_to(&client, &url, &dest, &part, &progress).await {
                Ok(size) => {
                    if !compagnons.is_empty() {
                        progress.noter("Installation des compagnons…");
                        let _ = install_image_companions(&client, &compagnons, &progress).await;
                    }
                    Ok((file_name, size))
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

/// DELETE /v1/models/{name} — retirer un modèle installé du serveur.
///
/// Un fichier unique (`.gguf`, `.onnx`…) est effacé ; un dépôt HuggingFace
/// est effacé avec son dossier. Les compagnons d'image (VAE, encodeur) ne
/// sont pas touchés : plusieurs modèles les partagent, et retirer un modèle
/// ne doit pas en casser un autre.
pub async fn remove_model(axum::extract::Path(name): axum::extract::Path<String>) -> Response {
    let name = name.trim().to_string();
    if !nom_modele_valide(&name) {
        return err_response(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "Nom de modèle invalide.",
        );
    }
    let path = locaryn_config::models_dir().join(&name);
    let meta = match tokio::fs::symlink_metadata(&path).await {
        Ok(m) => m,
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
    let resultat = if meta.file_type().is_dir() {
        tokio::fs::remove_dir_all(&path).await
    } else {
        tokio::fs::remove_file(&path).await
    };
    match resultat {
        Ok(()) => Json(serde_json::json!({ "removed": name })).into_response(),
        Err(e) => err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "remove_failed",
            &format!("Impossible de retirer {name} : {e}"),
        ),
    }
}

/// Un nom de modèle est-il sûr à utiliser comme chemin, sans sortir du
/// dossier des modèles ? Ni séparateur, ni remontée, ni fichier caché.
fn nom_modele_valide(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
}

/// Ce qu'il faut télécharger, une fois l'adresse acceptée.
enum Preparé {
    /// Un dépôt HuggingFace entier : la liste des fichiers, avec leur taille.
    Depot {
        repo_id: String,
        dest_dir: std::path::PathBuf,
        fichiers: Vec<(String, u64)>,
    },
    /// Un fichier unique, plus les compagnons d'image éventuels.
    Fichier {
        dest: std::path::PathBuf,
        part: std::path::PathBuf,
        compagnons: Vec<Companion>,
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

/// Ce qu'on peut installer : un fichier unique, ou un dépôt HuggingFace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelKind {
    File,
    Repo,
}

#[derive(serde::Deserialize)]
pub struct PullBody {
    url: String,
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
            !p.starts_with("eval/") && !p.starts_with("samples/") && p != ".gitattributes"
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
    let mut total: u64 = 0;
    for (i, (path, _)) in fichiers.iter().enumerate() {
        let out = dest_dir.join(path);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("dossier : {e}"))?;
        }
        progress.noter(&format!("Fichier {}/{}…", i + 1, fichiers.len()));
        tracing::info!(file = %path, position = i + 1, total = fichiers.len(), "dépôt HuggingFace");
        let dl = format!("https://huggingface.co/{repo_id}/resolve/main/{path}");
        let part = out.with_extension("part");
        match download_to(&dl_client, &dl, &out, &part, progress).await {
            Ok(n) => total += n,
            Err(e) => return Err(format!("{path} : {e}")),
        }
    }
    Ok(total)
}

/// Les poids compagnons qu'un checkpoint d'image exige pour générer.
#[derive(Clone, Copy)]
struct Companion {
    url: &'static str,
    file: &'static str,
    label: &'static str,
}

const Z_IMAGE_VAE: Companion = Companion {
    url: "https://huggingface.co/onnx-community/z_image-vae-fp32-fix/resolve/main/decoder_fp32_fix.onnx",
    file: "z_image-vae-fp32-fix.onnx",
    label: "décodeur VAE",
};

const Z_IMAGE_ENCODER: Companion = Companion {
    url: "https://huggingface.co/second-state/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
    file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
    label: "encodeur de texte",
};

/// Quels compagnons un checkpoint d'image exige, s'il en exige : Z-Image et
/// Stable Diffusion ne génèrent rien sans leur VAE ; Z-Image exige en plus son
/// encodeur de texte.
fn companions_for(installed_file: &str) -> Vec<Companion> {
    let lower = installed_file.to_ascii_lowercase();
    let is_z_image = lower.contains("z_image") || lower.contains("z-image");
    let is_sd =
        lower.contains("stable-diffusion") || lower.contains("sd_xl") || lower.contains("sd15");
    if is_z_image {
        vec![Z_IMAGE_VAE, Z_IMAGE_ENCODER]
    } else if is_sd {
        vec![Z_IMAGE_VAE]
    } else {
        Vec::new()
    }
}

/// Poser les poids compagnons d'un checkpoint d'image à côté de lui, comme le
/// fait le bureau, sans faire échouer l'installation si un compagnon refuse.
/// Leurs tailles comptent déjà dans le total de la barre, annoncé d'avance.
async fn install_image_companions(
    client: &reqwest::Client,
    compagnons: &[Companion],
    progress: &PullProgress,
) -> Result<(), String> {
    let models_dir = locaryn_config::models_dir();
    for comp in compagnons {
        let dest = models_dir.join(comp.file);
        if dest.exists() {
            continue;
        }
        progress.noter(&format!("Compagnon : {}…", comp.label));
        tracing::info!(file = comp.file, "compagnon : {}", comp.label);
        let part = models_dir.join(format!("{}.part", comp.file));
        if let Err(e) = download_to(client, comp.url, &dest, &part, progress).await {
            tracing::warn!(error = %e, file = comp.file, "compagnon non installé");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_adresses_valides_sont_classées() {
        // Un fichier direct : le nom du fichier est retenu.
        let (name, kind) = classify_model_url(
            "https://huggingface.co/leejet/Z-Image-Turbo-GGUF/resolve/main/z_image_turbo-Q8_0.gguf",
        )
        .unwrap();
        assert_eq!(kind, ModelKind::File);
        assert_eq!(name, "z_image_turbo-Q8_0.gguf");

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
    fn les_noms_de_modèles_restent_dans_le_dossier() {
        assert!(nom_modele_valide("z_image_turbo-Q8_0.gguf"));
        assert!(nom_modele_valide("hexgrad__Kokoro-82M"));
        // Jamais de remontée, de séparateur ou de fichier caché.
        assert!(!nom_modele_valide("../secret.gguf"));
        assert!(!nom_modele_valide("a/b.gguf"));
        assert!(!nom_modele_valide("a\\b.gguf"));
        assert!(!nom_modele_valide(".cache"));
        assert!(!nom_modele_valide(""));
        assert!(!nom_modele_valide(".."));
    }
}
