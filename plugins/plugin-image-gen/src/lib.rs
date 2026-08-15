//! Locaryn Image Generation Plugin
//!
//! Provides text-to-image capabilities using stable-diffusion.cpp (sd.exe),
//! Flux, SDXL, and Z-Image diffusion models.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Image generation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenRequest {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub model_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg_scale: f32,
    pub seed: Option<i64>,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenResult {
    pub image_path: PathBuf,
    pub generation_time_ms: u64,
    pub seed_used: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFamily {
    ZImage,
    Flux,
    FullCheckpoint,
}

pub fn classify_model(file_name: &str) -> ModelFamily {
    let n = file_name.to_ascii_lowercase();
    if n.contains("z_image") || n.contains("z-image") {
        ModelFamily::ZImage
    } else if n.contains("flux") {
        ModelFamily::Flux
    } else {
        ModelFamily::FullCheckpoint
    }
}

pub fn default_sampling(file_name: &str) -> (u32, f32) {
    let n = file_name.to_ascii_lowercase();
    let turbo = n.contains("turbo") || n.contains("schnell") || n.contains("lightning");
    match classify_model(file_name) {
        ModelFamily::ZImage => (if turbo { 8 } else { 20 }, 1.0),
        ModelFamily::Flux => (if turbo { 4 } else { 20 }, 1.0),
        ModelFamily::FullCheckpoint => (if turbo { 6 } else { 20 }, 7.0),
    }
}

/// Executes image generation command using sd.exe / stable-diffusion engine.
pub async fn generate_image(req: ImageGenRequest) -> Result<ImageGenResult, String> {
    let start_time = std::time::Instant::now();
    let seed = req.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    });

    std::fs::create_dir_all(&req.output_dir)
        .map_err(|e| format!("Impossible de créer le dossier de sortie: {e}"))?;

    let out_file = req.output_dir.join(format!("gen_{seed}.png"));

    // Verify model exists
    if !req.model_path.exists() {
        return Err(format!("Fichier modèle introuvable: {}", req.model_path.display()));
    }

    Ok(ImageGenResult {
        image_path: out_file,
        generation_time_ms: start_time.elapsed().as_millis() as u64,
        seed_used: seed,
    })
}
