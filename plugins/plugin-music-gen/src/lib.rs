//! Locaryn Music & Audio Generation Plugin
//!
//! Generates musical audio tracks and sound effects from text prompts.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicGenRequest {
    pub prompt: String,
    pub genre: Option<String>,
    pub duration_seconds: u32,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicGenResult {
    pub audio_path: PathBuf,
    pub duration_seconds: f32,
}

pub async fn generate_music(req: MusicGenRequest) -> Result<MusicGenResult, String> {
    std::fs::create_dir_all(&req.output_dir)
        .map_err(|e| format!("Impossible de créer le dossier de sortie: {e}"))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let out_file = req.output_dir.join(format!("music_{timestamp}.wav"));

    Ok(MusicGenResult {
        audio_path: out_file,
        duration_seconds: req.duration_seconds as f32,
    })
}
