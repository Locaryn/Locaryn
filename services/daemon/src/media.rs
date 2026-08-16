//! Media generation routes — the machine-side engines exposed to thin
//! clients (the phone) that cannot run them.
//!
//! Generation happens where the models live, through `locaryn-media`; the
//! finished file is read back and returned as base64 so a phone can render or
//! play it without needing a file server.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

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

/// POST /v1/media/image — text-to-image via stable-diffusion.cpp.
pub async fn generate_image(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<ImageGenBody>,
) -> Response {
    let width = body.width.unwrap_or(1024);
    let height = body.height.unwrap_or(1024);
    let variants = body.variants.unwrap_or(1).clamp(1, 8);

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
}

#[derive(serde::Deserialize)]
pub struct AudioGenBody {
    model: String,
    text: String,
    speed: Option<f32>,
    language: Option<String>,
}
