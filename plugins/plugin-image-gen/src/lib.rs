//! Runtime owned by the Locaryn image-generation extension.
//!
//! The Locaryn application only hosts the extension and exposes a generic MCP
//! bridge. Model files, the diffusion executable, output media and all image
//! generation decisions live here.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

const Z_IMAGE_VAE_URL: &str =
    "https://huggingface.co/black-forest-labs/FLUX.1-schnell/resolve/main/ae.safetensors";
#[cfg(test)]
const Z_IMAGE_VAE_FILE: &str = "ae.safetensors";
const Z_IMAGE_ENCODER_URL: &str =
    "https://huggingface.co/second-state/Qwen3-4B-Instruct-2507-GGUF/resolve/main/Qwen3-4B-Instruct-2507-Q4_K_M.gguf";
#[cfg(test)]
const Z_IMAGE_ENCODER_FILE: &str = "Qwen3-4B-Instruct-2507-Q4_K_M.gguf";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenRequest {
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: Option<String>,
    pub model: String,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_steps")]
    pub steps: u32,
    #[serde(default = "default_cfg")]
    pub cfg_scale: f32,
    #[serde(default)]
    pub input_image: Option<String>,
    #[serde(default)]
    pub uncensored: bool,
    #[serde(default = "default_variants")]
    pub variants: u32,
}

fn default_width() -> u32 {
    1024
}
fn default_height() -> u32 {
    1024
}
fn default_steps() -> u32 {
    20
}
fn default_cfg() -> f32 {
    7.0
}
fn default_variants() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenResult {
    pub paths: Vec<PathBuf>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInstallRequest {
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFamily {
    ZImage,
    Flux,
    FullCheckpoint,
}

pub fn classify_model(file_name: &str) -> ModelFamily {
    let name = file_name.to_ascii_lowercase();
    if ["z_image", "z-image", "z-img", "z_img", "zimg"]
        .iter()
        .any(|part| name.contains(part))
    {
        ModelFamily::ZImage
    } else if name.contains("flux") {
        ModelFamily::Flux
    } else {
        ModelFamily::FullCheckpoint
    }
}

pub fn default_sampling(file_name: &str) -> (u32, f32) {
    let name = file_name.to_ascii_lowercase();
    let turbo = ["turbo", "schnell", "lightning"]
        .iter()
        .any(|part| name.contains(part));
    match classify_model(file_name) {
        ModelFamily::ZImage => (if turbo { 8 } else { 20 }, 1.0),
        ModelFamily::Flux => (if turbo { 4 } else { 20 }, 1.0),
        ModelFamily::FullCheckpoint => (if turbo { 6 } else { 20 }, 7.0),
    }
}

/// The host injects this path when it launches the plugin MCP server.
pub fn plugin_root() -> PathBuf {
    std::env::var_os("LOCARYN_PLUGIN_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The plugin keeps its own models instead of using Locaryn's chat model root.
pub fn models_dir() -> PathBuf {
    std::env::var_os("LOCARYN_EXTENSION_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| plugin_root().join("models"))
}

pub fn generated_images_dir() -> PathBuf {
    std::env::var_os("LOCARYN_EXTENSION_MEDIA_DIR")
        .or_else(|| std::env::var_os("LOCARYN_GENERATED_MEDIA_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("LOCARYN_EXTENSION_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| plugin_root().join("data"))
                .join("generated_images")
        })
}

pub fn find_sd_binary() -> Option<PathBuf> {
    let executable = if cfg!(windows) { "sd.exe" } else { "sd" };
    let explicit = std::env::var_os("LOCARYN_SD_BINARY").map(PathBuf::from);
    let mut candidates = explicit
        .into_iter()
        .chain([
            plugin_root().join("bin").join(executable),
            plugin_root().join(executable),
        ])
        .chain(
            std::env::var_os("LOCARYN_PLUGIN_BIN_DIR")
                .map(PathBuf::from)
                .into_iter()
                .map(|dir| dir.join(executable)),
        );
    candidates.find(|path| path.is_file())
}

#[derive(Debug, Clone, Default)]
pub struct Companions {
    pub vae: Option<PathBuf>,
    pub llm: Option<PathBuf>,
    pub clip_l: Option<PathBuf>,
    pub t5xxl: Option<PathBuf>,
}

fn find_companion(dir: &Path, patterns: &[&str], exclude: &[&str]) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if !patterns.iter().any(|p| name.contains(p)) || exclude.iter().any(|p| name.contains(p)) {
            continue;
        }
        let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        if best.as_ref().map(|(old, _)| size > *old).unwrap_or(true) {
            best = Some((size, path));
        }
    }
    best.map(|(_, path)| path)
}

pub fn discover_companions(models: &Path, family: ModelFamily, uncensored: bool) -> Companions {
    if family == ModelFamily::FullCheckpoint {
        return Companions::default();
    }
    // decoder_fp32_fix.onnx is not a VAE accepted by stable-diffusion.cpp.
    let vae = find_companion(models, &["ae.safetensors", "vae"], &[".onnx", "decoder", "taesd"]);
    match family {
        ModelFamily::ZImage => Companions {
            vae,
            llm: if uncensored {
                find_companion(models, &["abliterat", "heretic"], &[])
            } else {
                find_companion(models, &["qwen3-4b", "qwen3_4b"], &["tts", "abliterat"])
            },
            ..Companions::default()
        },
        ModelFamily::Flux => Companions {
            vae,
            clip_l: find_companion(models, &["clip_l", "clip-l"], &[]),
            t5xxl: find_companion(models, &["t5xxl", "t5-xxl"], &[]),
            ..Companions::default()
        },
        ModelFamily::FullCheckpoint => Companions::default(),
    }
}

pub fn missing_companions(family: ModelFamily, companions: &Companions) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match family {
        ModelFamily::ZImage => {
            if companions.vae.is_none() {
                missing.push("ae.safetensors (VAE stable-diffusion.cpp)");
            }
            if companions.llm.is_none() {
                missing.push("un encodeur de texte Qwen3");
            }
        }
        ModelFamily::Flux => {
            if companions.vae.is_none() {
                missing.push("ae.safetensors (VAE)");
            }
            if companions.clip_l.is_none() {
                missing.push("un encodeur CLIP-L");
            }
            if companions.t5xxl.is_none() {
                missing.push("un encodeur T5-XXL");
            }
        }
        ModelFamily::FullCheckpoint => {}
    }
    missing
}

fn image_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "stable-diffusion", "stable_diffusion", "sd_xl", "sdxl", "sd15", "sd-v1", "sd_v1",
        "sd3", "sd3.5", "z_image", "z-image", "z_img", "zimg", "flux", "krea",
        "dreamshaper", "juggernaut", "pony", "playground-v", "kolors", "hunyuan-dit", "pixart",
    ]
    .iter()
    .any(|part| lower.contains(part))
}

pub fn is_diffusion_checkpoint(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let valid_extension = [".gguf", ".safetensors", ".ckpt"]
        .iter()
        .any(|suffix| lower.ends_with(suffix));
    valid_extension
        && image_asset(name)
        && [
            "mmproj-", "ae.safetensors", "vae", "clip", "t5xxl", "text_encoder",
            "text-encoder", "abliterat", "qwen", "embed",
        ]
        .iter()
        .all(|part| !lower.contains(part))
}

pub fn list_image_models() -> Vec<String> {
    let mut names = std::fs::read_dir(models_dir())
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            let lower = name.to_ascii_lowercase();
            if path.is_dir()
                && (path.join("model_index.json").is_file() || path.join("unet").is_dir())
            {
                return Some(name);
            }
            (!lower.ends_with(".part") && !lower.ends_with(".tmp") && is_diffusion_checkpoint(&name))
                .then_some(name)
        })
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

pub fn resolve_model_path(raw: &str) -> PathBuf {
    let root = models_dir();
    let direct = root.join(raw);
    if direct.exists() {
        return direct;
    }
    let clean = raw
        .split(['/', '\\'])
        .next_back()
        .unwrap_or(raw);
    root.join(clean)
}

fn validate_model_path(path: &Path) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(models_dir()).map_err(|e| format!("dossier modèles illisible : {e}"))?;
    let model = std::fs::canonicalize(path).map_err(|e| format!("modèle introuvable : {e}"))?;
    if !model.starts_with(&root) {
        return Err("le modèle doit se trouver dans le stockage de l'extension".into());
    }
    Ok(model)
}

pub struct SdRequest<'a> {
    pub model_path: &'a Path,
    pub models_dir: &'a Path,
    pub prompt: &'a str,
    pub negative_prompt: Option<&'a str>,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg_scale: f32,
    pub seed: i64,
    pub out_file: &'a Path,
    pub init_image: Option<&'a Path>,
    pub batch_count: u32,
    pub uncensored: bool,
}

pub fn batch_output(out_file: &Path, count: u32) -> (PathBuf, Vec<PathBuf>) {
    if count <= 1 {
        return (out_file.to_path_buf(), vec![out_file.to_path_buf()]);
    }
    let dir = out_file.parent().unwrap_or_else(|| Path::new("."));
    let stem = out_file.file_stem().unwrap_or_default().to_string_lossy();
    let extension = out_file.extension().unwrap_or_default().to_string_lossy();
    let pattern = dir.join(format!("{stem}_%d.{extension}"));
    let paths = (0..count)
        .map(|index| dir.join(format!("{stem}_{index}.{extension}")))
        .collect();
    (pattern, paths)
}

pub fn build_args(request: &SdRequest<'_>) -> Result<Vec<String>, String> {
    let file_name = request
        .model_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let family = classify_model(&file_name);
    let companions = discover_companions(request.models_dir, family, request.uncensored);
    let mut args = vec!["-M".into(), "img_gen".into()];

    if request.model_path.is_dir() {
        let checkpoint = std::fs::read_dir(request.model_path)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.is_file()
                    && matches!(
                        path.extension().and_then(|ext| ext.to_str()),
                        Some("gguf") | Some("safetensors") | Some("ckpt")
                    )
            });
        let Some(checkpoint) = checkpoint else {
            return Err(format!("dossier de modèle invalide : {}", request.model_path.display()));
        };
        args.extend(["-m".into(), checkpoint.to_string_lossy().to_string()]);
    } else {
        let missing = missing_companions(family, &companions);
        if !missing.is_empty() {
            return Err(format!("{} nécessite : {}", file_name, missing.join(", ")));
        }
        match family {
            ModelFamily::FullCheckpoint => {
                args.extend(["-m".into(), request.model_path.to_string_lossy().to_string()]);
            }
            ModelFamily::ZImage | ModelFamily::Flux => {
                args.extend(["--diffusion-model".into(), request.model_path.to_string_lossy().to_string()]);
                if let Some(vae) = &companions.vae {
                    args.extend(["--vae".into(), vae.to_string_lossy().to_string()]);
                }
                if let Some(llm) = &companions.llm {
                    args.extend(["--llm".into(), llm.to_string_lossy().to_string()]);
                }
                if let Some(clip) = &companions.clip_l {
                    args.extend(["--clip_l".into(), clip.to_string_lossy().to_string()]);
                }
                if let Some(t5) = &companions.t5xxl {
                    args.extend(["--t5xxl".into(), t5.to_string_lossy().to_string()]);
                }
            }
        }
    }

    args.extend(["-p".into(), request.prompt.to_string()]);
    if let Some(negative) = request.negative_prompt.filter(|prompt| !prompt.is_empty()) {
        args.extend(["-n".into(), negative.to_string()]);
    }
    if let Some(input) = request.init_image {
        args.extend([
            "-i".into(),
            input.to_string_lossy().to_string(),
            "--strength".into(),
            "0.75".into(),
        ]);
    }
    args.extend([
        "-W".into(),
        request.width.to_string(),
        "-H".into(),
        request.height.to_string(),
        "--steps".into(),
        request.steps.to_string(),
        "--cfg-scale".into(),
        format!("{:.2}", request.cfg_scale),
        "-s".into(),
        request.seed.to_string(),
        "--diffusion-fa".into(),
        "--vae-tiling".into(),
    ]);
    let count = request.batch_count.clamp(1, 8);
    if count > 1 {
        args.extend(["-b".into(), count.to_string()]);
    }
    let (output, _) = batch_output(request.out_file, count);
    args.extend(["-o".into(), output.to_string_lossy().to_string()]);
    Ok(args)
}

pub async fn generate_image(request: ImageGenRequest) -> Result<ImageGenResult, String> {
    if request.prompt.trim().is_empty() {
        return Err("le prompt ne peut pas être vide".into());
    }
    let model_path = validate_model_path(&resolve_model_path(&request.model))?;
    let model_name = model_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let (family_steps, family_cfg) = default_sampling(&model_name);
    let steps = if request.steps == 20 || request.steps == 8 {
        family_steps
    } else {
        request.steps.clamp(1, 100)
    };
    let cfg = if (request.cfg_scale - 7.0).abs() < f32::EPSILON {
        family_cfg
    } else {
        request.cfg_scale.clamp(0.1, 30.0)
    };

    let output_dir = generated_images_dir();
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("dossier de sortie : {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let out_file = output_dir.join(format!("img_{stamp}.png"));
    let input_path = request
        .input_image
        .as_deref()
        .map(|data| {
            let path = std::env::temp_dir().join(format!("locaryn-image-input-{stamp}.png"));
            std::fs::write(&path, decode_data_url(data)?).map_err(|e| format!("image source : {e}"))?;
            Ok::<PathBuf, String>(path)
        })
        .transpose()?;
    let models = models_dir();
    let args = build_args(&SdRequest {
        model_path: &model_path,
        models_dir: &models,
        prompt: request.prompt.trim(),
        negative_prompt: request.negative_prompt.as_deref(),
        width: request.width.clamp(64, 2048),
        height: request.height.clamp(64, 2048),
        steps,
        cfg_scale: cfg,
        seed: stamp as i64,
        out_file: &out_file,
        init_image: input_path.as_deref(),
        batch_count: request.variants.clamp(1, 8),
        uncensored: request.uncensored,
    })?;
    let binary = find_sd_binary().ok_or_else(|| {
        "le moteur image du plugin est introuvable. Installez le runtime depuis l'extension."
            .to_string()
    })?;

    let mut command = tokio::process::Command::new(binary);
    command
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    hide_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("lancement de stable-diffusion.cpp : {e}"))?;
    let stderr = child.stderr.take().ok_or("stderr du moteur indisponible")?;
    let mut lines = tokio::io::BufReader::new(stderr).lines();
    let mut errors = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.contains("[ERROR]") {
            errors.push(line.trim().to_string());
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|e| format!("attente du moteur : {e}"))?;
    if let Some(path) = &input_path {
        let _ = std::fs::remove_file(path);
    }
    let (_, expected) = batch_output(&out_file, request.variants.clamp(1, 8));
    let paths = expected.into_iter().filter(|path| path.is_file()).collect::<Vec<_>>();
    if !status.success() || paths.is_empty() {
        return Err(format!(
            "génération échouée : {}",
            errors.last().map(String::as_str).unwrap_or("aucune image écrite")
        ));
    }
    Ok(ImageGenResult {
        paths,
        model: request.model,
    })
}

fn hide_console(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn decode_data_url(input: &str) -> Result<Vec<u8>, String> {
    decode_base64(input.split_once(',').map(|(_, payload)| payload).unwrap_or(input))
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut values = Vec::new();
    for byte in input.bytes().filter(|byte| !b" \r\n\t".contains(byte)) {
        if byte == b'=' {
            break;
        }
        values.push(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("image source base64 invalide".into()),
        });
    }
    let mut output = Vec::with_capacity(values.len() * 3 / 4);
    let mut index = 0;
    while index + 3 < values.len() {
        output.push((values[index] << 2) | (values[index + 1] >> 4));
        output.push((values[index + 1] << 4) | (values[index + 2] >> 2));
        output.push((values[index + 2] << 6) | values[index + 3]);
        index += 4;
    }
    if values.len() - index >= 2 {
        output.push((values[index] << 2) | (values[index + 1] >> 4));
    }
    if values.len() - index >= 3 {
        output.push((values[index + 1] << 4) | (values[index + 2] >> 2));
    }
    Ok(output)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub token: Option<String>,
}

pub async fn install_models(request: InstallRequest) -> Result<Vec<String>, String> {
    let mut sources = request.sources;
    if let Some(source) = request.source.filter(|source| !source.trim().is_empty()) {
        sources.push(source);
    }
    if sources.is_empty() {
        return Err("aucune source de modèle".into());
    }
    let mut expanded = Vec::new();
    for source in sources {
        expanded.push(source.clone());
        let lower = source.to_ascii_lowercase();
        if lower.contains("z_image") || lower.contains("z-image") {
            expanded.push(Z_IMAGE_VAE_URL.into());
            expanded.push(Z_IMAGE_ENCODER_URL.into());
        }
    }
    expanded.sort();
    expanded.dedup();

    let token = request
        .token
        .or_else(|| std::env::var("HF_TOKEN").ok())
        .unwrap_or_default();
    let client = reqwest::Client::builder()
        .user_agent("locaryn-plugin-image-gen")
        .build()
        .map_err(|e| format!("client HTTP : {e}"))?;
    let mut installed = Vec::new();
    for source in expanded {
        let url = normalize_huggingface_source(&source)?;
        let file_name = filename_from_url(&url)?;
        let destination = models_dir().join(&file_name);
        if !destination.is_file() {
            download_file(&client, &url, &destination, &token).await?;
        }
        installed.push(file_name);
    }
    Ok(installed)
}

pub async fn install_runtime(request: RuntimeInstallRequest) -> Result<String, String> {
    let source = request.source.trim();
    if !(source.starts_with("https://") && source.contains("github.com/")) {
        return Err("le runtime doit être un asset HTTPS de GitHub".into());
    }
    let file_name = filename_from_url(source)?;
    let destination = plugin_root().join("bin").join(file_name);
    let client = reqwest::Client::builder()
        .user_agent("locaryn-plugin-image-gen")
        .build()
        .map_err(|e| format!("client HTTP : {e}"))?;
    download_file(&client, source, &destination, "").await?;
    Ok(destination.to_string_lossy().to_string())
}

fn normalize_huggingface_source(source: &str) -> Result<String, String> {
    let source = source.trim();
    let url = if source.starts_with("hf.co/") {
        source.replacen("hf.co/", "https://huggingface.co/", 1)
    } else {
        source.to_string()
    };
    if !(url.starts_with("https://") && url.contains("huggingface.co/")) {
        return Err("les modèles doivent venir de huggingface.co en HTTPS".into());
    }
    Ok(url)
}

fn filename_from_url(url: &str) -> Result<String, String> {
    let name = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err("nom de fichier invalide".into());
    }
    Ok(name.to_string())
}

async fn download_file(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    token: &str,
) -> Result<(), String> {
    let mut request = client.get(url);
    if !token.is_empty() {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("téléchargement : {e}"))?;
    if !response.status().is_success() {
        return Err(format!("téléchargement HTTP {} pour {}", response.status(), url));
    }
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("dossier de destination : {e}"))?;
    }
    let part = destination.with_file_name(format!(
        "{}.part",
        destination.file_name().unwrap().to_string_lossy()
    ));
    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|e| format!("fichier temporaire : {e}"))?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk.map_err(|e| format!("flux de téléchargement : {e}"))?)
            .await
            .map_err(|e| format!("écriture : {e}"))?;
    }
    file.flush().await.map_err(|e| format!("flush : {e}"))?;
    drop(file);
    tokio::fs::rename(&part, destination)
        .await
        .map_err(|e| format!("finalisation : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_families_and_sampling_are_stable() {
        assert_eq!(classify_model("z_image_turbo-Q8.gguf"), ModelFamily::ZImage);
        assert_eq!(classify_model("z-img-Q4.gguf"), ModelFamily::ZImage);
        assert_eq!(classify_model("flux1-schnell-Q4.gguf"), ModelFamily::Flux);
        assert_eq!(classify_model("sdxl-turbo-Q4.gguf"), ModelFamily::FullCheckpoint);
        assert_eq!(default_sampling("z_image_turbo-Q8.gguf"), (8, 1.0));
        assert_eq!(default_sampling("flux1-schnell-Q4.gguf"), (4, 1.0));
    }

    #[test]
    fn old_onnx_vae_is_never_selected() {
        let root = temp_dir("companions");
        std::fs::write(root.join("decoder_fp32_fix.onnx"), b"bad").unwrap();
        std::fs::write(root.join(Z_IMAGE_VAE_FILE), b"good").unwrap();
        let companions = discover_companions(&root, ModelFamily::ZImage, false);
        assert_eq!(companions.vae, Some(root.join(Z_IMAGE_VAE_FILE)));
    }

    #[test]
    fn catalog_hides_companions() {
        assert!(is_diffusion_checkpoint("z_image_turbo-Q8.gguf"));
        assert!(!is_diffusion_checkpoint("ae.safetensors"));
        assert!(!is_diffusion_checkpoint("decoder_fp32_fix.onnx"));
        assert!(!is_diffusion_checkpoint(Z_IMAGE_ENCODER_FILE));
    }

    #[test]
    fn data_url_and_batch_paths_are_valid() {
        assert_eq!(decode_data_url("data:image/png;base64,aGVsbG8=").unwrap(), b"hello");
        let (pattern, files) = batch_output(Path::new("out/img.png"), 3);
        assert!(pattern.to_string_lossy().contains("img_%d.png"));
        assert_eq!(files[0], PathBuf::from("out/img_0.png"));
        assert_eq!(files[2], PathBuf::from("out/img_2.png"));
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("locaryn-image-plugin-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
