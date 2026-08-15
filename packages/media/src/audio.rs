//! Text-to-speech through the Python engines the desktop shell uses:
//! Kokoro (light, instant) and Qwen3-TTS (higher quality, slower).
//!
//! The phone never uploads audio: it sends text and gets back a finished WAV.
//! Voice cloning is out of scope here — the daemon speaks with a model's
//! built-in voice, which is what a chat companion needs.

use std::path::{Path, PathBuf};

use crate::python;
use crate::{GeneratedFile, ProgressFn};

/// The TTS-capable models installed in the models directory.
///
/// Extracted HuggingFace repos are directories whose name carries the engine
/// (`hexgrad__Kokoro-82M`, `Qwen__Qwen3-TTS-...`); Piper models are single
/// `.onnx` files at the top level.
pub fn list_tts_models() -> Vec<String> {
    let models_dir = locaryn_config::models_dir();
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let lower = name.to_ascii_lowercase();
            if path.is_dir() {
                let is_tts = ["kokoro", "qwen3", "xtts", "piper", "parler", "omni"]
                    .iter()
                    .any(|k| lower.contains(k));
                if is_tts {
                    names.push(name);
                }
            } else if lower.ends_with(".onnx") {
                names.push(name);
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// One text-to-speech generation.
pub struct TtsRequest {
    /// Model name as returned by [`list_tts_models`].
    pub model: String,
    pub text: String,
    pub speed: f32,
    /// ISO language code ("fr", "en", "ja"...). `None` = detect from text.
    pub language: Option<String>,
    /// Output directory on the machine running the engine.
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtsEngine {
    Kokoro,
    Qwen3,
}

fn resolve_engine(model: &str) -> Result<TtsEngine, String> {
    let lower = model.to_ascii_lowercase();
    if lower.contains("kokoro") {
        Ok(TtsEngine::Kokoro)
    } else if lower.contains("qwen3") {
        Ok(TtsEngine::Qwen3)
    } else if lower.ends_with(".onnx") && lower.contains("kokoro") {
        Ok(TtsEngine::Kokoro)
    } else {
        Err(format!(
            "Modèle audio non pris en charge par le serveur : {model}. \
             Engines disponibles : Kokoro, Qwen3-TTS."
        ))
    }
}

/// Render `text` to a WAV at `output_dir` and return its path.
pub async fn generate_tts(
    req: TtsRequest,
    progress: ProgressFn<'_>,
) -> Result<GeneratedFile, String> {
    let python = python::find_python()
        .ok_or_else(|| "Python non trouvé sur le serveur. Installez Python 3.10+.".to_string())?;

    let output_dir = &req.output_dir;
    std::fs::create_dir_all(output_dir).map_err(|e| format!("cannot create output dir: {e}"))?;
    let out_file = output_dir.join(format!(
        "gen_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    let models_dir = locaryn_config::models_dir();
    let repo_dir = models_dir.join(&req.model);
    if !repo_dir.exists() {
        return Err(format!("modèle audio introuvable : {}", repo_dir.display()));
    }

    let speed = req.speed.clamp(0.5, 2.0);
    let effective_lang: Option<&str> = req.language.as_deref().filter(|l| !l.is_empty());
    let detected = detect_language(&req.text);
    let lang = effective_lang.unwrap_or(detected);

    match resolve_engine(&req.model)? {
        TtsEngine::Kokoro => {
            run_kokoro(
                &python, &repo_dir, &req.text, &out_file, speed, lang, progress,
            )
            .await
        }
        TtsEngine::Qwen3 => {
            run_qwen3(
                &python, &repo_dir, &req.text, &out_file, speed, lang, progress,
            )
            .await
        }
    }?;

    if !out_file.exists() {
        return Err("Le moteur a terminé sans écrire de fichier audio.".into());
    }
    Ok(GeneratedFile { path: out_file })
}

/// Detect language from text, the same heuristic the desktop shell uses.
fn detect_language(text: &str) -> &'static str {
    if text
        .chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c))
    {
        return "zh";
    }
    if text
        .chars()
        .any(|c| ('\u{3040}'..='\u{309f}').contains(&c) || ('\u{30a0}'..='\u{30ff}').contains(&c))
    {
        return "ja";
    }
    if text.chars().any(|c| ('\u{ac00}'..='\u{d7af}').contains(&c)) {
        return "ko";
    }
    if text.chars().any(|c| ('\u{0600}'..='\u{06ff}').contains(&c)) {
        return "ar";
    }
    if text.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)) {
        return "ru";
    }
    let has_german = text.chars().any(|c| {
        matches!(
            c,
            '\u{e4}' | '\u{f6}' | '\u{fc}' | '\u{c4}' | '\u{d6}' | '\u{dc}'
        )
    });
    let has_french = text.chars().any(|c| {
        matches!(
            c,
            '\u{e0}' | '\u{e8}' | '\u{e9}' | '\u{ea}' | '\u{eb}' | '\u{e7}' | '\u{f4}' | '\u{fb}'
        )
    });
    let has_spanish = text
        .chars()
        .any(|c| matches!(c, '\u{f1}' | '\u{bf}' | '\u{a1}'));
    let has_portuguese = text.chars().any(|c| matches!(c, '\u{e3}' | '\u{f5}'));
    if has_portuguese {
        return "pt";
    }
    if has_german {
        return "de";
    }
    if has_french {
        return "fr";
    }
    if has_spanish {
        return "es";
    }
    let has_italian = text
        .chars()
        .any(|c| matches!(c, '\u{ec}' | '\u{f2}' | '\u{f9}'));
    if has_italian {
        return "it";
    }
    "en"
}

// ── Kokoro ──────────────────────────────────────────────────────────────────

/// Voices shipped with a Kokoro repo, e.g. `af_heart`, `am_michael`...
fn kokoro_voices_in_repo(repo_dir: &Path) -> Vec<String> {
    let voices_dir = repo_dir.join("voices");
    let mut voices = Vec::new();
    if voices_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&voices_dir) {
            voices = entries
                .flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().and_then(|e| e.to_str()) != Some("pt") {
                        return None;
                    }
                    p.file_stem().and_then(|s| s.to_str()).map(str::to_string)
                })
                .collect();
        }
    }
    voices.sort();
    voices
}

fn kokoro_voice_lang(voice: &str) -> Option<&str> {
    let first = voice.chars().next()?;
    Some(match first {
        'a' | 'b' => "en",
        'f' => "fr",
        'e' => "es",
        'd' => "de",
        'i' => "it",
        'p' => "pt",
        'j' => "ja",
        'z' | 'c' => "zh",
        _ => return None,
    })
}

fn resolve_kokoro_voice(repo_dir: &Path, language: &str) -> Result<String, String> {
    let voices = kokoro_voices_in_repo(repo_dir);
    if voices.is_empty() {
        return Err("Aucune voix Kokoro dans le dossier voices/.".into());
    }
    let candidates: Vec<&String> = voices
        .iter()
        .filter(|v| kokoro_voice_lang(v).unwrap_or("") == language)
        .collect();
    let chosen = if candidates.is_empty() {
        // Fall back to English, then to any voice at all.
        let en: Vec<&String> = voices
            .iter()
            .filter(|v| kokoro_voice_lang(v).unwrap_or("") == "en")
            .collect();
        en.first().or(candidates.first()).map(|v| (*v).clone())
    } else {
        candidates.first().map(|v| (*v).clone())
    };
    chosen.ok_or_else(|| format!("Aucune voix Kokoro utilisable pour la langue '{language}'."))
}

async fn run_kokoro(
    python: &str,
    repo_dir: &Path,
    text: &str,
    out_file: &Path,
    speed: f32,
    language: &str,
    progress: ProgressFn<'_>,
) -> Result<(), String> {
    let pth_path = python::walkdir_recursive(repo_dir, 3)
        .into_iter()
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("pth") || e.eq_ignore_ascii_case("onnx"))
                .unwrap_or(false)
        })
        .ok_or_else(|| "Fichier .pth ou .onnx Kokoro introuvable dans le dépôt.".to_string())?;

    let voice_name = resolve_kokoro_voice(repo_dir, language)?;
    let voice_pt = repo_dir.join("voices").join(format!("{voice_name}.pt"));
    if !voice_pt.exists() {
        return Err(format!(
            "Voix Kokoro sélectionnée introuvable : {}",
            voice_pt.display()
        ));
    }

    let script = format!(
        r#"
import sys

model_path = r"{pth}"
voice_pt = r"{voice_pt}"
out_path = r"{out}"
speed = {speed}
voice_name = "{voice_name}"

text = sys.stdin.read()

try:
    from kokoro import KPipeline
    import soundfile as sf
    pipeline = KPipeline(lang_code='a')
    for i, (gs, ps, audio) in enumerate(pipeline(text, voice=voice_name, speed=speed)):
        sf.write(out_path, audio, 24000)
        break  # first segment is enough for short text
    print("OK")
except ImportError:
    try:
        import kokoro_onnx
        from kokoro_onnx import KokoroOnnx
        import soundfile as sf
        k = KokoroOnnx(model_path=model_path, voice_path=voice_pt)
        audio = k.create(text, voice=voice_name, speed=speed, lang="en-us")
        sf.write(out_path, audio, 24000)
        print("OK")
    except ImportError:
        print("kokoro / kokoro-onnx not installed", file=sys.stderr)
        sys.exit(1)
"#,
        pth = pth_path.display(),
        voice_pt = voice_pt.display(),
        out = out_file.display(),
    );

    progress(10, "Kokoro : initialisation");
    run_python_script(python, &script, text)
        .await
        .map_err(|e| format!("Kokoro a échoué : {e}"))?;
    progress(100, "Kokoro : terminé");
    Ok(())
}

// ── Qwen3-TTS ───────────────────────────────────────────────────────────────

async fn run_qwen3(
    python: &str,
    repo_dir: &Path,
    text: &str,
    out_file: &Path,
    speed: f32,
    language: &str,
    progress: ProgressFn<'_>,
) -> Result<(), String> {
    let dirname = repo_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // The Base variant has no default voice: it only clones from a reference
    // recording. Say so rather than failing inside Python with a stack trace.
    if dirname.contains("base") {
        return Err(
            "Le modèle Qwen3-TTS « Base » exige un audio de référence (clonage vocal), \
             ce que le serveur ne propose pas encore. Choisissez la variante \
             « CustomVoice » (ou Kokoro) depuis le téléphone."
                .into(),
        );
    }

    let repo_json =
        serde_json::to_string(&repo_dir.to_string_lossy().as_ref()).map_err(|e| e.to_string())?;
    let out_json =
        serde_json::to_string(&out_file.to_string_lossy().as_ref()).map_err(|e| e.to_string())?;
    let lang_json = serde_json::to_string(language).map_err(|e| e.to_string())?;

    // The desktop shell ships this same script (minus cloning/voice design);
    // `generate_custom_voice` with the model's first speaker is the plain
    // text-to-speech path.
    let script = format!(
        r#"# Qwen3-TTS inference (simple voice, no cloning)
import sys, os, json, subprocess

repo_dir = {repo_json}
out_path = {out_json}
lang = {lang_json}
speed = {speed}

text = sys.stdin.read()

def report(pct, msg):
    print(json.dumps({{'progress': pct, 'detail': msg}}), flush=True)

try:
    from qwen_tts import Qwen3TTSModel, Qwen3TTSTokenizer
except ImportError:
    report(5, "Installation de qwen-tts...")
    subprocess.check_call([sys.executable, "-m", "pip", "install", "-q", "qwen-tts", "soundfile"])
    from qwen_tts import Qwen3TTSModel, Qwen3TTSTokenizer

import torch
dtype = torch.float16 if torch.cuda.is_available() else torch.float32
device = "cuda" if torch.cuda.is_available() else "cpu"
report(15, "Qwen3-TTS : chargement du modèle")

model = Qwen3TTSModel.from_pretrained(repo_dir, dtype=dtype, device_map=device)
report(30, "Qwen3-TTS : modèle chargé")

temperature = max(0.05, 0.7 + (1.0 - speed) * 0.1)
spk_list = model.get_supported_speakers()
spk = spk_list[0] if spk_list else None
report(50, "Qwen3-TTS : synthèse")

wavs, sr = model.generate_custom_voice(
    text=text,
    speaker=spk,
    language=lang,
    temperature=temperature,
)

wav = wavs[0] if isinstance(wavs, list) else wavs
if hasattr(wav, 'numpy'):
    wav = wav.numpy()
elif hasattr(wav, 'detach'):
    wav = wav.detach().cpu().numpy()

import soundfile as sf
sf.write(out_path, wav, sr)
report(100, "Qwen3-TTS : terminé")
"#,
    );

    progress(5, "Qwen3-TTS : initialisation");
    run_python_script(python, &script, text)
        .await
        .map_err(|e| format!("Qwen3-TTS a échoué : {e}"))?;
    progress(100, "Qwen3-TTS : terminé");
    Ok(())
}

/// Spawn `python -c <script>`, feed `text` on stdin, wait for completion.
///
/// Returns the trimmed stderr on failure so the caller can phrase its own
/// error; the output type is deliberately not surfaced (tokio keeps it
/// private).
async fn run_python_script(python: &str, script: &str, text: &str) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new(python)
        .envs(python::python_env())
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| format!("python stdin: {e}"))?;
        stdin.shutdown().await.ok();
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("python wait: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
