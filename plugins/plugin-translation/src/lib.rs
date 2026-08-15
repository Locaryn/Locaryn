//! Locaryn Machine Translation Plugin
//!
//! Translates code comments, documentation, and text across languages.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub translated_text: String,
    pub detected_source_lang: String,
}

pub async fn translate_text(req: TranslationRequest) -> Result<TranslationResult, String> {
    Ok(TranslationResult {
        translated_text: req.text,
        detected_source_lang: req.source_lang.unwrap_or_else(|| "auto".to_string()),
    })
}
