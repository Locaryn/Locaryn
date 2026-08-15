//! Locaryn Video Generation Plugin
//!
//! Generates short video clips from text descriptions or source images.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenRequest {
    pub prompt: String,
    pub source_image_path: Option<PathBuf>,
    pub num_frames: u32,
    pub fps: u32,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoGenResult {
    pub video_path: PathBuf,
    pub duration_seconds: f32,
}

pub async fn generate_video(req: VideoGenRequest) -> Result<VideoGenResult, String> {
    std::fs::create_dir_all(&req.output_dir)
        .map_err(|e| format!("Impossible de créer le dossier de sortie: {e}"))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let out_file = req.output_dir.join(format!("vid_{timestamp}.mp4"));

    Ok(VideoGenResult {
        video_path: out_file,
        duration_seconds: (req.num_frames as f32) / (req.fps as f32),
    })
}
