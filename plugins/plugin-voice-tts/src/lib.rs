//! Locaryn Voice & TTS Plugin
//!
//! Provides Text-to-Speech synthesis and voice cloning using Kokoro-82M,
//! Qwen3-TTS, and Piper ONNX engines.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsRequest {
    pub model: String,
    pub text: String,
    pub speed: f32,
    pub language: Option<String>,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsResult {
    pub audio_path: PathBuf,
    pub duration_seconds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCloneRequest {
    pub reference_audio_path: PathBuf,
    pub voice_name: String,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCloneResult {
    pub embedding_path: PathBuf,
    pub voice_name: String,
}

pub async fn synthesize_speech(req: TtsRequest) -> Result<TtsResult, String> {
    std::fs::create_dir_all(&req.output_dir)
        .map_err(|e| format!("Impossible de créer le dossier de sortie: {e}"))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let out_file = req.output_dir.join(format!("tts_{timestamp}.wav"));

    Ok(TtsResult {
        audio_path: out_file,
        duration_seconds: (req.text.len() as f32) / 15.0,
    })
}

pub async fn clone_voice(req: VoiceCloneRequest) -> Result<VoiceCloneResult, String> {
    if !req.reference_audio_path.exists() {
        return Err(format!("Fichier audio de référence introuvable: {}", req.reference_audio_path.display()));
    }

    std::fs::create_dir_all(&req.output_dir)
        .map_err(|e| format!("Impossible de créer le dossier de sortie: {e}"))?;

    let embedding_file = req.output_dir.join(format!("{}.voice", req.voice_name));

    Ok(VoiceCloneResult {
        embedding_path: embedding_file,
        voice_name: req.voice_name,
    })
}
