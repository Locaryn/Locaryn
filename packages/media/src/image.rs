//! Text-to-image through stable-diffusion.cpp (`sd.exe`).
//!
//! The invocation logic was proven on a real machine (RTX 4050, 6 GB) and
//! lives here so the daemon and the desktop shell cannot drift apart: same
//! flags, same memory placement, same companion resolution.

use std::path::{Path, PathBuf};

use crate::{GeneratedFile, ProgressFn};

/// Locate the stable-diffusion.cpp binary.
pub fn find_sd_binary() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "sd.exe" } else { "sd" };
    let bin = locaryn_config::bin_dir();
    [bin.join("sd").join(exe), bin.join(exe)]
        .into_iter()
        .find(|candidate| candidate.exists())
}

/// Which weights a model needs alongside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFamily {
    /// Diffusion-only GGUF: needs a standalone VAE and an LLM text encoder.
    ZImage,
    /// Diffusion-only GGUF: needs a VAE plus CLIP-L and T5-XXL encoders.
    Flux,
    /// Self-contained checkpoint: `-m` is enough.
    FullCheckpoint,
}

pub fn classify(file_name: &str) -> ModelFamily {
    let n = file_name.to_ascii_lowercase();
    if n.contains("z_image") || n.contains("z-image") {
        ModelFamily::ZImage
    } else if n.contains("flux") {
        ModelFamily::Flux
    } else {
        ModelFamily::FullCheckpoint
    }
}

/// Sampling defaults that suit the family.
///
/// Z-Image and Flux are flow-matching: CFG above ~1 diverges into noise rather
/// than following the prompt harder, and the turbo variants are distilled for
/// roughly 8 steps.
pub fn default_sampling(file_name: &str) -> (u32, f32) {
    let n = file_name.to_ascii_lowercase();
    let turbo = n.contains("turbo") || n.contains("schnell") || n.contains("lightning");
    match classify(file_name) {
        ModelFamily::ZImage => (if turbo { 8 } else { 20 }, 1.0),
        ModelFamily::Flux => (if turbo { 4 } else { 20 }, 1.0),
        ModelFamily::FullCheckpoint => (if turbo { 6 } else { 20 }, 7.0),
    }
}

/// True for a GGUF that can actually render an image (as opposed to a
/// companion file like a VAE or a text encoder).
fn is_diffusion_checkpoint(file_name: &str) -> bool {
    let n = file_name.to_ascii_lowercase();
    // Only GGUF checkpoints are selectable; a stray test PNG whose name
    // matches a family (z_image_test.png) must not appear as a model.
    if !n.ends_with(".gguf") {
        return false;
    }
    const AUX: &[&str] = &[
        "mmproj-",
        "ae.safetensors",
        "vae",
        "clip",
        "t5xxl",
        "text_encoder",
        "text-encoder",
        "abliterat",
        "qwen",
    ];
    is_image_asset(file_name) && !AUX.iter().any(|p| n.contains(p))
}

fn is_image_asset(file_name: &str) -> bool {
    let n = file_name.to_ascii_lowercase();
    const DIFFUSION: &[&str] = &[
        "stable-diffusion",
        "stable_diffusion",
        "sd_xl",
        "sdxl",
        "sd15",
        "sd-v1",
        "sd_v1",
        "sd3",
        "sd3.5",
        "z_image",
        "z-image",
        "flux",
        "krea",
        "dreamshaper",
        "juggernaut",
        "pony",
        "playground-v",
        "kolors",
        "hunyuan-dit",
        "pixart",
    ];
    const AUX: &[&str] = &[
        "mmproj-",
        "ae.safetensors",
        "vae",
        "clip",
        "t5xxl",
        "text_encoder",
        "text-encoder",
    ];
    DIFFUSION.iter().any(|p| n.contains(p)) || AUX.iter().any(|p| n.contains(p))
}

/// The diffusion checkpoints installed in the models directory.
pub fn list_image_models() -> Vec<String> {
    let models_dir = locaryn_config::models_dir();
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                let lower = name.to_ascii_lowercase();
                let is_partial = lower.ends_with(".part") || lower.ends_with(".tmp");
                if is_diffusion_checkpoint(name) && !is_partial {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Un modèle d'image et ce qui lui manque, s'il lui manque quelque chose.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageModelInfo {
    pub name: String,
    /// Faux si des fichiers compagnons manquent : le proposer mènerait à un
    /// échec au moment de générer.
    pub ready: bool,
    /// Ce qui manque, nommé comme l'utilisateur le voit. Vide si `ready`.
    pub missing: Vec<String>,
}

/// La même liste, mais en disant lesquels peuvent réellement produire une
/// image.
///
/// Un dossier de modèles contient couramment des poids de diffusion seuls —
/// Flux, Z-Image — qui exigent un VAE et un ou deux encodeurs de texte. Les
/// lister comme les autres, c'est proposer un choix qui échouera : c'est
/// exactement ce qu'a vu l'utilisateur, un modèle offert en premier dans la
/// liste du téléphone et un message d'erreur au moment de générer.
pub fn list_image_models_detailed() -> Vec<ImageModelInfo> {
    let models_dir = locaryn_config::models_dir();
    list_image_models()
        .into_iter()
        .map(|name| {
            let family = classify(&name);
            let companions = discover_companions(&models_dir, family);
            let missing: Vec<String> = missing_companions(family, &companions)
                .into_iter()
                .map(str::to_string)
                .collect();
            ImageModelInfo {
                name,
                ready: missing.is_empty(),
                missing,
            }
        })
        .collect()
}

/// Find a companion weight file in `dir` whose name matches any pattern and
/// none of the exclusions. Returns the largest match, since the useful file
/// (a 2.3 GB encoder) sits beside smaller decoys with similar names.
fn find_companion(dir: &Path, patterns: &[&str], exclude: &[&str]) -> Option<PathBuf> {
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if !patterns.iter().any(|p| name.contains(p)) {
            continue;
        }
        if exclude.iter().any(|p| name.contains(p)) {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if best.as_ref().map(|(s, _)| size > *s).unwrap_or(true) {
            best = Some((size, path));
        }
    }
    best.map(|(_, p)| p)
}

/// Everything a family needs beyond the diffusion weights themselves.
#[derive(Debug, Clone, Default)]
pub struct Companions {
    pub vae: Option<PathBuf>,
    pub llm: Option<PathBuf>,
    pub clip_l: Option<PathBuf>,
    pub t5xxl: Option<PathBuf>,
}

/// Which companion files are missing, named the way the user sees them.
pub fn missing_companions(family: ModelFamily, c: &Companions) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match family {
        ModelFamily::ZImage => {
            if c.vae.is_none() {
                missing.push("un VAE (ae.safetensors)");
            }
            if c.llm.is_none() {
                missing.push("un encodeur de texte Qwen3 (Qwen3-4B-*.gguf)");
            }
        }
        ModelFamily::Flux => {
            if c.vae.is_none() {
                missing.push("un VAE (ae.safetensors)");
            }
            if c.clip_l.is_none() {
                missing.push("un encodeur CLIP-L");
            }
            if c.t5xxl.is_none() {
                missing.push("un encodeur T5-XXL");
            }
        }
        ModelFamily::FullCheckpoint => {}
    }
    missing
}

pub fn discover_companions(models_dir: &Path, family: ModelFamily) -> Companions {
    let mut c = Companions::default();
    if family == ModelFamily::FullCheckpoint {
        return c;
    }
    c.vae = find_companion(models_dir, &["ae.safetensors", "vae"], &["taesd"]);
    match family {
        ModelFamily::ZImage => {
            c.llm = find_companion(models_dir, &["qwen3-4b", "qwen3_4b"], &["tts", "abliterat"]);
        }
        ModelFamily::Flux => {
            c.clip_l = find_companion(models_dir, &["clip_l", "clip-l"], &[]);
            c.t5xxl = find_companion(models_dir, &["t5xxl", "t5-xxl"], &[]);
        }
        ModelFamily::FullCheckpoint => {}
    }
    c
}

/// Inputs for one generation.
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
    /// img2img / inpainting source.
    pub init_image: Option<&'a Path>,
    /// White = repaint, black = keep. Requires `init_image`.
    pub mask: Option<&'a Path>,
    pub strength: f32,
    /// Free VRAM in GiB, used to decide what may stay on the GPU.
    pub vram_gb: f32,
    /// How many variants to render in this one process.
    pub batch_count: u32,
}

/// Output path for a batch, and the paths its images will land on.
///
/// sd.cpp fills the printf `%d` in `-o` starting at **zero** — verified
/// against a real 3-image run, which produced `_0`, `_1`, `_2`.
pub fn batch_output(out_file: &Path, batch_count: u32) -> (PathBuf, Vec<PathBuf>) {
    if batch_count <= 1 {
        return (out_file.to_path_buf(), vec![out_file.to_path_buf()]);
    }
    let dir = out_file.parent().unwrap_or(Path::new("."));
    let stem = out_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = out_file
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "png".to_string());
    let pattern = dir.join(format!("{stem}_%d.{ext}"));
    let files = (0..batch_count)
        .map(|i| dir.join(format!("{stem}_{i}.{ext}")))
        .collect();
    (pattern, files)
}

/// Assemble the full argument list. Pure so it can be asserted without a GPU
/// or a multi-GB download.
pub fn build_args(req: &SdRequest<'_>) -> Result<Vec<String>, String> {
    let file_name = req
        .model_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let family = classify(&file_name);
    let companions = discover_companions(req.models_dir, family);

    let missing = missing_companions(family, &companions);
    if !missing.is_empty() {
        return Err(format!(
            "{file_name} est un modèle de diffusion seul : il lui faut aussi {}. \
             Placez ces fichiers dans le dossier des modèles.",
            missing.join(" et ")
        ));
    }

    let mut a: Vec<String> = Vec::new();
    a.push("-M".into());
    a.push("img_gen".into());

    match family {
        ModelFamily::FullCheckpoint => {
            a.push("-m".into());
            a.push(req.model_path.to_string_lossy().to_string());
        }
        _ => {
            a.push("--diffusion-model".into());
            a.push(req.model_path.to_string_lossy().to_string());
            if let Some(v) = &companions.vae {
                a.push("--vae".into());
                a.push(v.to_string_lossy().to_string());
            }
            if let Some(l) = &companions.llm {
                a.push("--llm".into());
                a.push(l.to_string_lossy().to_string());
            }
            if let Some(c) = &companions.clip_l {
                a.push("--clip_l".into());
                a.push(c.to_string_lossy().to_string());
            }
            if let Some(t) = &companions.t5xxl {
                a.push("--t5xxl".into());
                a.push(t.to_string_lossy().to_string());
            }
        }
    }

    a.push("-p".into());
    a.push(req.prompt.to_string());
    if let Some(n) = req.negative_prompt.filter(|s| !s.is_empty()) {
        a.push("-n".into());
        a.push(n.to_string());
    }

    if let Some(init) = req.init_image {
        a.push("-i".into());
        a.push(init.to_string_lossy().to_string());
        a.push("--strength".into());
        a.push(format!("{:.2}", req.strength));
        if let Some(m) = req.mask {
            a.push("--mask".into());
            a.push(m.to_string_lossy().to_string());
        }
    }

    a.push("-W".into());
    a.push(req.width.to_string());
    a.push("-H".into());
    a.push(req.height.to_string());
    a.push("--steps".into());
    a.push(req.steps.to_string());
    a.push("--cfg-scale".into());
    a.push(format!("{:.2}", req.cfg_scale));
    a.push("-s".into());
    a.push(req.seed.to_string());

    // ── Memory placement ────────────────────────────────────────────────
    let weights_gb = std::fs::metadata(req.model_path)
        .map(|m| m.len() as f32 / (1024.0 * 1024.0 * 1024.0))
        .unwrap_or(0.0);

    if weights_gb > 0.0 && weights_gb >= req.vram_gb * 0.85 {
        // Weights alone exceed what the card can hold: stream them from RAM
        // and cap the working set so the allocator does not overshoot.
        a.push("--offload-to-cpu".into());
        a.push("--max-vram".into());
        a.push(format!("{:.1}", (req.vram_gb * 0.55).max(1.5)));
        // The text encoder runs once per generation, so the CPU costs little.
        // The VAE is the opposite: on CPU its decode dominated the run
        // (167 s of 310 s), so it stays on the GPU.
        a.push("--backend".into());
        a.push("te=cpu".into());
    }
    a.push("--diffusion-fa".into());
    a.push("--vae-tiling".into());

    // One process, N variants: the weight load and prompt encoding are paid
    // once instead of once per image.
    let batch = req.batch_count.max(1);
    if batch > 1 {
        a.push("-b".into());
        a.push(batch.to_string());
    }
    let (out_pattern, _) = batch_output(req.out_file, batch);

    a.push("-o".into());
    a.push(out_pattern.to_string_lossy().to_string());
    Ok(a)
}

/// Free VRAM in GiB. `LOCARYN_VRAM_GB` overrides the conservative default, so
/// a machine with a bigger card does not needlessly stream weights from RAM.
pub fn vram_gb() -> f32 {
    std::env::var("LOCARYN_VRAM_GB")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(6.0)
}

/// Pull the current step out of a stable-diffusion.cpp progress line.
///
/// The bar is drawn with carriage returns and looks like
/// `  |====>      | 3/8 - 6.15s/it`, so the whole render can arrive as one
/// long line. We scan for every `n/total` and take the last.
fn parse_sd_step(line: &str) -> Option<u32> {
    let mut latest = None;
    for chunk in line.split('|') {
        let chunk = chunk.trim();
        let Some((left, right)) = chunk.split_once('/') else {
            continue;
        };
        let done: u32 = left.trim().parse().ok()?;
        let total: u32 = right
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .ok()?;
        if total > 0 && done <= total {
            latest = Some(done);
        }
    }
    latest
}

/// One text-to-image generation.
pub struct ImageRequest {
    /// File name of the GGUF in the models directory.
    pub model: String,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub width: u32,
    pub height: u32,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f32>,
    /// How many variants to render in one process (1-8).
    pub variants: u32,
    /// Output directory on the machine running the engine.
    pub output_dir: PathBuf,
}

/// Run `sd.exe` and wait for the finished PNG.
pub async fn generate_image(
    req: ImageRequest,
    progress: ProgressFn<'_>,
) -> Result<GeneratedFile, String> {
    let sd_bin = find_sd_binary().ok_or_else(|| {
        "Moteur d'images non installé (stable-diffusion.cpp introuvable). \
         Téléchargez-le depuis l'application de bureau Locaryn."
            .to_string()
    })?;

    let output_dir = &req.output_dir;
    std::fs::create_dir_all(output_dir).map_err(|e| format!("cannot create output dir: {e}"))?;
    let out_file = output_dir.join(format!(
        "img_{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    let models_dir = locaryn_config::models_dir();
    let model_path = models_dir.join(&req.model);
    if !model_path.exists() {
        return Err(format!("modèle introuvable : {}", model_path.display()));
    }

    // The caller's steps/cfg come from generic defaults that suit Stable
    // Diffusion. Flow-matching models diverge into noise at CFG 7, so when the
    // request still carries those defaults, use what the family actually wants.
    let (fam_steps, fam_cfg) = default_sampling(&req.model);
    let steps = match req.steps {
        Some(s) if s != 20 && s != 8 => s,
        _ => fam_steps,
    };
    let cfg_scale = match req.cfg_scale {
        Some(c) if (c - 7.0).abs() >= f32::EPSILON => c,
        _ => fam_cfg,
    };

    let args = build_args(&SdRequest {
        model_path: &model_path,
        models_dir: &models_dir,
        prompt: req.prompt.trim(),
        negative_prompt: req.negative_prompt.as_deref(),
        width: req.width,
        height: req.height,
        steps,
        cfg_scale,
        seed: 42,
        out_file: &out_file,
        init_image: None,
        mask: None,
        strength: 0.75,
        vram_gb: vram_gb(),
        batch_count: req.variants.clamp(1, 8),
    })?;

    progress(5, "chargement du modèle");

    let mut child = tokio::process::Command::new(&sd_bin)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("lancement de sd: {e}"))?;

    // sd.cpp writes its step counter to stderr; stream it so the caller's UI
    // advances instead of sitting still for the whole render. stderr closes
    // when the child exits, so reading it to EOF then waiting is safe and
    // avoids a spawned task that would need to own the progress callback.
    use tokio::io::{AsyncBufReadExt, BufReader};
    let stderr = child.stderr.take().ok_or("sd stderr indisponible")?;
    let total_steps = steps.max(1);
    let mut errors = String::new();
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(done) = parse_sd_step(&line) {
            let pct = 5 + ((done as f64 / total_steps as f64) * 90.0).round() as u32;
            progress(pct.min(95), &format!("étape {done}/{total_steps}"));
        }
        if line.contains("[ERROR]") {
            errors.push_str(line.trim());
            errors.push('\n');
        }
    }

    // The exit status is not the verdict: what matters is which files landed,
    // so a partially-failed batch still returns its usable images.
    let _status = child
        .wait()
        .await
        .map_err(|e| format!("attente de sd: {e}"))?;

    let (_, expected) = batch_output(&out_file, req.variants.clamp(1, 8));
    let produced: Vec<PathBuf> = expected.iter().filter(|p| p.exists()).cloned().collect();

    let first = produced.first().cloned().ok_or_else(|| {
        format!(
            "génération échouée : {}",
            errors.lines().last().unwrap_or("aucune image écrite")
        )
    })?;

    let detail = if produced.len() > 1 {
        format!("{} variantes générées", produced.len())
    } else {
        "terminé".to_string()
    };
    progress(100, &detail);
    Ok(GeneratedFile { path: first })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_image_is_recognised_as_diffusion_only() {
        assert_eq!(classify("z_image_turbo-Q8_0.gguf"), ModelFamily::ZImage);
        assert_eq!(
            classify("Z-Image-AbliteratedV1.Q4_K_M.gguf"),
            ModelFamily::ZImage
        );
        assert_eq!(classify("flux1-schnell-Q4_0.gguf"), ModelFamily::Flux);
        assert_eq!(
            classify("stable-diffusion-v1-5-pruned-emaonly-Q4_0.gguf"),
            ModelFamily::FullCheckpoint
        );
    }

    #[test]
    fn flow_matching_models_get_cfg_one() {
        assert_eq!(default_sampling("z_image_turbo-Q8_0.gguf"), (8, 1.0));
        assert_eq!(default_sampling("z_image-Q8_0.gguf"), (20, 1.0));
        assert_eq!(default_sampling("flux1-schnell-Q4_0.gguf"), (4, 1.0));
        assert_eq!(default_sampling("sd_xl_turbo_1.0.q8_0.gguf"), (6, 7.0));
    }

    #[test]
    fn batch_paths_are_zero_indexed_like_sd_cpp_writes_them() {
        let dir = std::path::PathBuf::from("out");
        let out_buf = dir.join("img_1785.png");
        let (pattern, files) = batch_output(&out_buf, 3);
        assert!(pattern.to_string_lossy().contains("img_1785_%d.png"));
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            names,
            ["img_1785_0.png", "img_1785_1.png", "img_1785_2.png"]
        );
    }

    #[test]
    fn list_excludes_companion_files() {
        // is_diffusion_checkpoint is what list_image_models filters with.
        assert!(is_diffusion_checkpoint("flux1-schnell-Q4_0.gguf"));
        assert!(is_diffusion_checkpoint("z_image_turbo-Q8_0.gguf"));
        assert!(!is_diffusion_checkpoint("ae.safetensors"));
        assert!(!is_diffusion_checkpoint(
            "Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
        ));
    }
}
