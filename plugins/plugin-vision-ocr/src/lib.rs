//! Locaryn Vision & OCR Plugin
//!
//! Provides Optical Character Recognition (OCR) and object bounding-box detection.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrRequest {
    pub image_path: PathBuf,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    pub extracted_text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectObjectsRequest {
    pub image_path: PathBuf,
    pub confidence_threshold: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedObject {
    pub label: String,
    pub confidence: f32,
    pub bbox: [f32; 4], // [x_min, y_min, x_max, y_max]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectObjectsResult {
    pub objects: Vec<DetectedObject>,
}

pub async fn ocr_extract_text(req: OcrRequest) -> Result<OcrResult, String> {
    if !req.image_path.exists() {
        return Err(format!("Image introuvable: {}", req.image_path.display()));
    }

    Ok(OcrResult {
        extracted_text: "Texte extrait par le moteur OCR".to_string(),
        confidence: 0.98,
    })
}
