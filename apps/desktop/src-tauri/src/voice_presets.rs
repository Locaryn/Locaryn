//! Saved voice profiles: a reference recording plus how to speak with it.
//!
//! Cloning a voice needs the same three things every time — an audio sample,
//! its transcript, and a set of delivery settings. Re-uploading an MP3 and
//! re-tuning sliders for every sentence is the friction this removes.
//!
//! The reference audio is *copied* into the preset directory rather than
//! referenced in place: a preset that breaks because the user tidied their
//! Downloads folder is not a preset. Everything is stored under the
//! configurable storage root, so voice samples never land on a full system
//! drive and are easy to locate, back up or delete.
//!
//! Parameters are deliberately engine-neutral. Each TTS engine honours a
//! different subset, so a preset describes *intent* (speed, pauses, how varied
//! the intonation should be) and [`applies_to`] reports what a given engine
//! will actually use — rather than silently ignoring half of it.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// How a saved voice should speak.
///
/// Every field has a neutral default so old presets keep loading as the set
/// grows: a preset saved today must not break when a knob is added tomorrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSettings {
    /// Delivery rate. 1.0 = as spoken in the reference.
    #[serde(default = "one")]
    pub speed: f32,
    /// Pitch shift. 1.0 = unchanged.
    #[serde(default = "one")]
    pub pitch: f32,
    /// Vocal energy, 0 = flat, 1 = full.
    #[serde(default = "default_energy")]
    pub energy: f32,
    /// Articulation crispness, 0 = slurred, 1 = crisp.
    #[serde(default = "default_clarity")]
    pub clarity: f32,
    /// Silence stretch. Above 1.0 the delivery becomes more measured, below it
    /// more clipped. Applied as post-processing, so it works on every engine
    /// rather than only those exposing a pause control.
    #[serde(default = "one")]
    pub pause_scale: f32,
    /// Sampling temperature: how much the intonation is allowed to vary.
    /// Low values are what make synthetic speech sound recited.
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_k")]
    pub top_k: u32,
    #[serde(default = "one")]
    pub top_p: f32,
    #[serde(default = "default_rep_penalty")]
    pub repetition_penalty: f32,
    /// In-context cloning: reproduce the reference speaker's rhythm, not only
    /// their timbre. Turning this off audibly changes who the voice sounds
    /// like, so it defaults on.
    #[serde(default = "yes")]
    pub expressive: bool,
    /// Free-form style direction, for engines that accept one.
    #[serde(default)]
    pub instruct: String,
}

fn one() -> f32 {
    1.0
}
fn yes() -> bool {
    true
}
fn default_energy() -> f32 {
    0.7
}
fn default_clarity() -> f32 {
    0.8
}
fn default_temperature() -> f32 {
    0.9
}
fn default_top_k() -> u32 {
    50
}
fn default_rep_penalty() -> f32 {
    1.05
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            speed: one(),
            pitch: one(),
            energy: default_energy(),
            clarity: default_clarity(),
            pause_scale: one(),
            temperature: default_temperature(),
            top_k: default_top_k(),
            top_p: one(),
            repetition_penalty: default_rep_penalty(),
            expressive: yes(),
            instruct: String::new(),
        }
    }
}

impl VoiceSettings {
    /// Clamp every field into the range the engines accept. Presets are
    /// user-editable JSON on disk, so a hand-typed value must not reach the
    /// sampler unchecked.
    pub fn sanitised(&self) -> Self {
        Self {
            speed: self.speed.clamp(0.5, 2.0),
            pitch: self.pitch.clamp(0.5, 2.0),
            energy: self.energy.clamp(0.0, 1.0),
            clarity: self.clarity.clamp(0.0, 1.0),
            pause_scale: self.pause_scale.clamp(0.3, 3.0),
            temperature: self.temperature.clamp(0.05, 2.0),
            top_k: self.top_k.clamp(1, 200),
            top_p: self.top_p.clamp(0.05, 1.0),
            repetition_penalty: self.repetition_penalty.clamp(1.0, 2.0),
            expressive: self.expressive,
            instruct: self.instruct.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoicePreset {
    pub id: String,
    pub name: String,
    /// Free-text note: whose voice it is, what it suits.
    #[serde(default)]
    pub note: String,
    /// Absolute path to the copied reference recording.
    pub reference_audio: String,
    /// Transcript of the reference. Required for in-context cloning; produced
    /// by speech recognition when the user does not supply one.
    #[serde(default)]
    pub reference_text: String,
    /// Language of the reference, as an ISO code (`fr`, `en`, …).
    #[serde(default)]
    pub language: String,
    /// Seconds of reference audio — the single best predictor of clone
    /// quality, so it is worth showing in the picker.
    #[serde(default)]
    pub duration_s: f32,
    #[serde(default)]
    pub settings: VoiceSettings,
    /// Engine the preset was captured with. Only a hint: the reference audio
    /// is portable, so any cloning-capable engine can use it.
    #[serde(default)]
    pub engine: String,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Which of a preset's settings a given engine will actually honour.
///
/// Reported so the UI can grey out what a model ignores instead of letting the
/// user tune a slider that does nothing — the complaint that keeps recurring
/// about controls not matching their label.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineSupport {
    pub engine: String,
    /// Can clone from a reference recording at all.
    pub cloning: bool,
    /// Honours the reference transcript (in-context cloning).
    pub reference_text: bool,
    pub temperature: bool,
    pub speed: bool,
    pub pitch: bool,
    /// Always true: pause scaling is post-processing on the rendered audio.
    pub pause_scale: bool,
    pub instruct: bool,
}

/// What a model can do with a preset, keyed off its name.
pub fn applies_to(model: &str) -> EngineSupport {
    let m = model.to_ascii_lowercase();
    let qwen3 = m.contains("qwen3") && m.contains("tts");
    let qwen3_base = qwen3 && m.contains("base");
    let xtts = m.contains("xtts") || m.contains("coqui");
    let f5 = m.contains("f5-tts") || m.contains("f5_tts") || m.contains("f5tts");
    let piper = m.contains("piper") || m.ends_with(".onnx");
    let kokoro = m.contains("kokoro");
    let parler = m.contains("parler");

    EngineSupport {
        engine: if qwen3 {
            "Qwen3-TTS"
        } else if xtts {
            "XTTS"
        } else if f5 {
            "F5-TTS"
        } else if piper {
            "Piper"
        } else if kokoro {
            "Kokoro"
        } else if parler {
            "Parler-TTS"
        } else {
            "générique"
        }
        .to_string(),
        // Qwen3-TTS clones only with the Base variant; the others ship fixed
        // speakers.
        cloning: qwen3_base || xtts || f5,
        // Only Qwen3-TTS Base takes a reference transcript.
        reference_text: qwen3_base,
        temperature: qwen3 || f5,
        speed: !f5,
        pitch: piper || qwen3,
        pause_scale: true,
        instruct: qwen3 || parler,
    }
}

// ============================================================================
// Storage
// ============================================================================

fn presets_dir() -> PathBuf {
    locaryn_config::storage_root().join("voice_presets")
}

fn preset_dir(id: &str) -> PathBuf {
    presets_dir().join(id)
}

fn manifest_path(id: &str) -> PathBuf {
    preset_dir(id).join("preset.json")
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Read every preset on disk, newest first.
///
/// A malformed manifest is skipped rather than failing the whole listing: one
/// hand-edited file should not hide the rest of the library.
pub fn load_all() -> Vec<VoicePreset> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(presets_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let manifest = entry.path().join("preset.json");
        if !manifest.is_file() {
            continue;
        }
        match std::fs::read_to_string(&manifest)
            .ok()
            .and_then(|raw| serde_json::from_str::<VoicePreset>(&raw).ok())
        {
            Some(p) => out.push(p),
            None => tracing::warn!("preset illisible, ignoré: {}", manifest.display()),
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

#[tauri::command]
pub fn list_voice_presets() -> Result<Vec<VoicePreset>, String> {
    Ok(load_all())
}

#[tauri::command]
pub fn voice_preset_support(model: String) -> Result<EngineSupport, String> {
    Ok(applies_to(&model))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePresetArgs {
    /// Present when updating an existing preset; absent creates a new one.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub note: String,
    /// Source recording. Only required when creating, or when replacing the
    /// sample of an existing preset.
    #[serde(default)]
    pub reference_audio: Option<String>,
    #[serde(default)]
    pub reference_text: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub settings: VoiceSettings,
}

/// Duration of a WAV, read from its header.
///
/// Only WAV is parsed here; other containers report 0 rather than pulling in a
/// decoder, since the figure is informational.
fn wav_duration_seconds(path: &Path) -> f32 {
    let Ok(bytes) = std::fs::read(path) else {
        return 0.0;
    };
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return 0.0;
    }
    let mut pos = 12;
    let mut byte_rate = 0u32;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]])
            as usize;
        if id == b"fmt " && pos + 16 + 8 <= bytes.len() {
            byte_rate = u32::from_le_bytes([
                bytes[pos + 16],
                bytes[pos + 17],
                bytes[pos + 18],
                bytes[pos + 19],
            ]);
        } else if id == b"data" && byte_rate > 0 {
            return size as f32 / byte_rate as f32;
        }
        pos += 8 + size + (size & 1);
    }
    0.0
}

#[tauri::command]
pub fn save_voice_preset(args: SavePresetArgs) -> Result<VoicePreset, String> {
    let name = args.name.trim();
    if name.is_empty() {
        return Err("Donnez un nom au préréglage.".into());
    }

    let creating = args.id.is_none();
    let id = args.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let dir = preset_dir(&id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("création du dossier: {e}"))?;

    let existing: Option<VoicePreset> = std::fs::read_to_string(manifest_path(&id))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());

    // Copy the sample in so the preset survives the source being moved.
    let stored_audio = match args.reference_audio.as_deref().filter(|s| !s.is_empty()) {
        Some(src) => {
            let src_path = Path::new(src);
            if !src_path.is_file() {
                return Err(format!("enregistrement introuvable : {src}"));
            }
            let ext = src_path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "wav".to_string());
            let dst = dir.join(format!("reference.{ext}"));
            if src_path != dst {
                // Remove any sample from a previous save, whatever its format.
                for old in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let p = old.path();
                    if p.file_stem().map(|s| s == "reference").unwrap_or(false) && p != dst {
                        let _ = std::fs::remove_file(p);
                    }
                }
                std::fs::copy(src_path, &dst).map_err(|e| format!("copie de l'enregistrement: {e}"))?;
            }
            dst.to_string_lossy().to_string()
        }
        None => existing
            .as_ref()
            .map(|p| p.reference_audio.clone())
            .filter(|s| !s.is_empty())
            .ok_or("Aucun enregistrement de référence fourni.")?,
    };

    let preset = VoicePreset {
        duration_s: wav_duration_seconds(Path::new(&stored_audio)),
        id: id.clone(),
        name: name.to_string(),
        note: args.note.trim().to_string(),
        reference_audio: stored_audio,
        reference_text: args.reference_text.trim().to_string(),
        language: args.language.trim().to_string(),
        settings: args.settings.sanitised(),
        engine: args.engine.trim().to_string(),
        created_at: existing
            .as_ref()
            .map(|p| p.created_at.clone())
            .unwrap_or_else(now),
        updated_at: if creating { String::new() } else { now() },
    };

    let json = serde_json::to_string_pretty(&preset).map_err(|e| format!("sérialisation: {e}"))?;
    std::fs::write(manifest_path(&id), json).map_err(|e| format!("écriture: {e}"))?;
    Ok(preset)
}

#[tauri::command]
pub fn delete_voice_preset(id: String) -> Result<(), String> {
    let dir = preset_dir(&id);
    if !dir.is_dir() {
        return Ok(());
    }
    // Refuse anything that is not a preset directory we created, so a bad id
    // cannot delete an unrelated tree.
    if !manifest_path(&id).is_file() {
        return Err("Ce dossier ne contient pas de préréglage.".into());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("suppression: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_match_the_engine_not_the_old_hardcoded_values() {
        let d = VoiceSettings::default();
        // 0.7 with timbre-only cloning is what made clones sound recited.
        assert_eq!(d.temperature, 0.9);
        assert!(d.expressive, "in-context cloning must be the default");
        assert_eq!(d.pause_scale, 1.0);
    }

    #[test]
    fn hand_edited_values_are_clamped_before_reaching_the_sampler() {
        let wild = VoiceSettings {
            speed: 99.0,
            pitch: -4.0,
            energy: 8.0,
            clarity: -1.0,
            pause_scale: 50.0,
            temperature: 0.0,
            top_k: 0,
            top_p: 3.0,
            repetition_penalty: 0.1,
            expressive: false,
            instruct: "x".into(),
        };
        let s = wild.sanitised();
        assert_eq!(s.speed, 2.0);
        assert_eq!(s.pitch, 0.5);
        assert_eq!(s.energy, 1.0);
        assert_eq!(s.clarity, 0.0);
        assert_eq!(s.pause_scale, 3.0);
        assert!(s.temperature >= 0.05);
        assert_eq!(s.top_k, 1);
        assert_eq!(s.top_p, 1.0);
        assert_eq!(s.repetition_penalty, 1.0);
        assert!(!s.expressive, "an explicit choice must survive clamping");
    }

    #[test]
    fn old_presets_keep_loading_when_fields_are_added() {
        // A manifest written before the sampling knobs existed.
        let raw = r#"{
            "id": "abc",
            "name": "Ma soeur",
            "referenceAudio": "D:/x/reference.wav",
            "createdAt": "2026-07-31T10:00:00Z"
        }"#;
        let p: VoicePreset = serde_json::from_str(raw).expect("doit se charger");
        assert_eq!(p.name, "Ma soeur");
        assert_eq!(p.settings.temperature, 0.9);
        assert!(p.settings.expressive);
        assert_eq!(p.reference_text, "");
    }

    #[test]
    fn engine_support_reports_what_each_model_really_honours() {
        let base = applies_to("Qwen__Qwen3-TTS-12Hz-0.6B-Base");
        assert!(base.cloning);
        assert!(base.reference_text, "only the Base variant takes a transcript");
        assert!(base.temperature);

        // CustomVoice ships fixed speakers: it cannot clone from a recording.
        let custom = applies_to("Qwen__Qwen3-TTS-12Hz-1.7B-CustomVoice");
        assert!(!custom.cloning);
        assert!(!custom.reference_text);

        let piper = applies_to("fr_FR-siwis-medium.onnx");
        assert!(!piper.cloning);
        assert!(piper.speed);

        // Pause scaling is post-processing, so it applies everywhere.
        for m in ["piper.onnx", "kokoro", "xtts_v2", "whatever"] {
            assert!(applies_to(m).pause_scale, "{m}");
        }
    }

    #[test]
    fn wav_duration_is_read_from_the_header() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_preset_wav_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.wav");

        // 24 kHz, mono, 16-bit => byte rate 48000; 96000 bytes = 2.0 s.
        let data_len: u32 = 96_000;
        let mut w = Vec::new();
        w.extend(b"RIFF");
        w.extend((36 + data_len).to_le_bytes());
        w.extend(b"WAVEfmt ");
        w.extend(16u32.to_le_bytes());
        w.extend(1u16.to_le_bytes()); // PCM
        w.extend(1u16.to_le_bytes()); // mono
        w.extend(24_000u32.to_le_bytes());
        w.extend(48_000u32.to_le_bytes()); // byte rate
        w.extend(2u16.to_le_bytes());
        w.extend(16u16.to_le_bytes());
        w.extend(b"data");
        w.extend(data_len.to_le_bytes());
        w.extend(vec![0u8; data_len as usize]);
        std::fs::write(&p, &w).unwrap();

        assert!((wav_duration_seconds(&p) - 2.0).abs() < 0.01);
        assert_eq!(wav_duration_seconds(&dir.join("absent.wav")), 0.0);
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod roundtrip {
    use super::*;

    /// Save, list, reload and delete against the real storage root.
    ///
    /// Uses a temporary root so the user's own presets are untouched; the env
    /// override is the only knob that redirects it in-process.
    #[test]
    fn a_preset_survives_the_source_recording_being_deleted() {
        let base = std::env::temp_dir().join(format!(
            "locaryn_preset_rt_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let root = base.join("root");
        let src_dir = base.join("downloads");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&src_dir).unwrap();
        std::env::set_var("LOCARYN_STORAGE_ROOT", &root);

        // A throwaway "recording" the user might later move or delete.
        let src = src_dir.join("ma_soeur.wav");
        std::fs::write(&src, b"RIFF....WAVEfake").unwrap();

        let saved = save_voice_preset(SavePresetArgs {
            id: None,
            name: "Ma petite soeur".into(),
            note: "voix douce".into(),
            reference_audio: Some(src.to_string_lossy().to_string()),
            reference_text: "et ca m'enerve genre pendant le chargement".into(),
            language: "fr".into(),
            engine: "Qwen3-TTS".into(),
            settings: VoiceSettings { temperature: 0.95, pause_scale: 0.9, ..Default::default() },
        })
        .expect("l'enregistrement doit reussir");

        // The sample must have been copied into the preset, not linked.
        assert!(Path::new(&saved.reference_audio).is_file());
        assert!(Path::new(&saved.reference_audio).starts_with(&root));
        std::fs::remove_file(&src).unwrap();
        assert!(
            Path::new(&saved.reference_audio).is_file(),
            "le preset doit survivre a la suppression de la source"
        );

        let listed = load_all();
        assert_eq!(listed.len(), 1);
        let p = &listed[0];
        assert_eq!(p.name, "Ma petite soeur");
        assert_eq!(p.settings.temperature, 0.95);
        assert_eq!(p.settings.pause_scale, 0.9);
        assert!(p.settings.expressive);
        assert_eq!(p.reference_text, "et ca m'enerve genre pendant le chargement");

        // Updating keeps the id, the creation date and the stored recording.
        let updated = save_voice_preset(SavePresetArgs {
            id: Some(saved.id.clone()),
            name: "Petite soeur".into(),
            note: String::new(),
            reference_audio: None,
            reference_text: p.reference_text.clone(),
            language: "fr".into(),
            engine: "Qwen3-TTS".into(),
            settings: VoiceSettings { temperature: 1.1, ..p.settings.clone() },
        })
        .expect("la mise a jour doit reussir");
        assert_eq!(updated.id, saved.id);
        assert_eq!(updated.created_at, saved.created_at);
        assert!(!updated.updated_at.is_empty(), "une mise a jour est horodatee");
        assert_eq!(updated.reference_audio, saved.reference_audio);
        assert_eq!(load_all().len(), 1, "une mise a jour ne cree pas de doublon");

        delete_voice_preset(saved.id.clone()).unwrap();
        assert!(load_all().is_empty());
        // Deleting twice is not an error.
        delete_voice_preset(saved.id).unwrap();

        std::env::remove_var("LOCARYN_STORAGE_ROOT");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn saving_without_a_name_or_a_recording_is_refused() {
        let e = save_voice_preset(SavePresetArgs {
            id: None,
            name: "   ".into(),
            note: String::new(),
            reference_audio: None,
            reference_text: String::new(),
            language: String::new(),
            engine: String::new(),
            settings: VoiceSettings::default(),
        })
        .unwrap_err();
        assert!(e.contains("nom"), "got: {e}");
    }
}
