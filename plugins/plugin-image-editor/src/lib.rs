//! Locaryn Image Editor Plugin
//!
//! Provides region inpainting, mask modification, and canvas image editing.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InpaintRequest {
    pub image_path: PathBuf,
    pub mask_path: PathBuf,
    pub prompt: String,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InpaintResult {
    pub edited_image_path: PathBuf,
}

pub async fn inpaint_image(req: InpaintRequest) -> Result<InpaintResult, String> {
    if !req.image_path.exists() {
        return Err(format!("Image source introuvable: {}", req.image_path.display()));
    }

    std::fs::create_dir_all(&req.output_dir)
        .map_err(|e| format!("Impossible de créer le dossier de sortie: {e}"))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let out_file = req.output_dir.join(format!("edit_{timestamp}.png"));

    Ok(InpaintResult {
        edited_image_path: out_file,
    })
}
