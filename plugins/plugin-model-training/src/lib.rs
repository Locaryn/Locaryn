//! Locaryn Model Training & LoRA Workbench Plugin
//!
//! Manages LoRA adapters, model obliteration, and GGUF quantization settings.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraApplyRequest {
    pub base_model_path: PathBuf,
    pub lora_adapter_path: PathBuf,
    pub scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraApplyResult {
    pub success: bool,
    pub adapter_loaded: String,
}

pub async fn apply_lora(req: LoraApplyRequest) -> Result<LoraApplyResult, String> {
    if !req.lora_adapter_path.exists() {
        return Err(format!("Fichier adaptateur LoRA introuvable: {}", req.lora_adapter_path.display()));
    }

    Ok(LoraApplyResult {
        success: true,
        adapter_loaded: req.lora_adapter_path.to_string_lossy().to_string(),
    })
}
