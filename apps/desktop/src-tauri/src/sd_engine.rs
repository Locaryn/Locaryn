//! Invoking stable-diffusion.cpp correctly, per model family.
//!
//! A GGUF is not self-describing enough to just hand to `-m`. Z-Image ships as
//! a *diffusion-model-only* file — 453 tensors, zero metadata — so `-m` fails
//! with "get sd version from file failed"; it needs `--diffusion-model` plus a
//! standalone VAE and a Qwen3 text encoder. Full checkpoints (SD1.5, SDXL) do
//! take `-m`. Getting this wrong looks exactly like "the model is broken".
//!
//! Memory placement matters as much as the flags. On a 6 GB card the Q8
//! Z-Image weights (6.13 GB) do not fit, so they must stream from RAM — but
//! pushing the *VAE* to the CPU as well turns a 3 s decode into 167 s. The
//! split encoded here (text encoder on CPU, VAE on GPU, diffusion streamed)
//! took one generation from 310 s to 59 s on an RTX 4050.

use std::path::{Path, PathBuf};

/// Locate the stable-diffusion.cpp binary.
///
/// Managed installs put it in `bin/sd/`, older ones dropped it straight in
/// `bin/`. Checking only the flat path meant the engine was reported missing
/// on every managed install, silently downgrading generation to a stub.
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
/// Z-Image is flow-matching: CFG above ~1 diverges into noise rather than
/// following the prompt harder, and the turbo variants are distilled for
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
            // Z-Image conditions on Qwen3. The abliterated encoder is the same
            // architecture, so it is a valid drop-in — `uncensored` picks it.
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

/// The abliterated Qwen3 encoder, when the caller asked for uncensored output.
pub fn abliterated_encoder(models_dir: &Path) -> Option<PathBuf> {
    find_companion(models_dir, &["abliterat"], &["tts"])
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
    pub uncensored: bool,
    /// How many variants to render in this one process.
    ///
    /// The fixed cost of a run — loading 6 GB of weights and encoding the
    /// prompt — is paid once regardless, so asking for several up front is far
    /// cheaper than several separate runs. Worth offering whenever the user
    /// expects the prompt to need a few attempts.
    pub batch_count: u32,
}

/// Output path for a batch, and the paths its images will land on.
///
/// sd.cpp fills the printf `%d` in `-o` starting at **zero** — verified
/// against a real 3-image run, which produced `_0`, `_1`, `_2`. Assuming
/// one-indexing silently loses the first image of every batch.
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

/// Assemble the full argument list. Pure so it can be asserted in tests
/// without a GPU or a 6 GB download.
pub fn build_args(req: &SdRequest<'_>) -> Result<Vec<String>, String> {
    let file_name = req
        .model_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let family = classify(&file_name);
    let mut companions = discover_companions(req.models_dir, family);

    if req.uncensored && family == ModelFamily::ZImage {
        if let Some(abl) = abliterated_encoder(req.models_dir) {
            companions.llm = Some(abl);
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "locaryn_sd_test_{tag}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(dir: &Path, name: &str, size: usize) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, vec![0u8; size]).unwrap();
        p
    }

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
        // CFG 7 on Z-Image produces noise, not a stronger prompt match.
        assert_eq!(default_sampling("z_image_turbo-Q8_0.gguf"), (8, 1.0));
        assert_eq!(default_sampling("z_image-Q8_0.gguf"), (20, 1.0));
        assert_eq!(default_sampling("flux1-schnell-Q4_0.gguf"), (4, 1.0));
        assert_eq!(default_sampling("sd_xl_turbo_1.0.q8_0.gguf"), (6, 7.0));
    }

    #[test]
    fn z_image_gets_diffusion_model_vae_and_llm_not_dash_m() {
        let dir = scratch("zimage");
        let model = touch(&dir, "z_image_turbo-Q8_0.gguf", 64);
        touch(&dir, "ae.safetensors", 32);
        touch(&dir, "Qwen3-4B-Instruct-2507-Q4_K_M.gguf", 48);
        let out = dir.join("out.png");

        let args = build_args(&SdRequest {
            model_path: &model,
            models_dir: &dir,
            prompt: "a brown t-shirt",
            negative_prompt: None,
            width: 1024,
            height: 576,
            steps: 8,
            cfg_scale: 1.0,
            seed: 42,
            out_file: &out,
            init_image: None,
            mask: None,
            strength: 0.8,
            vram_gb: 6.0,
            uncensored: false,
            batch_count: 1,
        })
        .expect("z-image with companions must build");

        assert!(args.contains(&"--diffusion-model".to_string()));
        assert!(
            !args.contains(&"-m".to_string()),
            "-m fails on a metadata-less GGUF"
        );
        assert!(args.contains(&"--vae".to_string()));
        assert!(args.contains(&"--llm".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_companions_are_named_not_silently_ignored() {
        let dir = scratch("bare");
        let model = touch(&dir, "z_image_turbo-Q8_0.gguf", 64);
        let out = dir.join("out.png");

        let err = build_args(&SdRequest {
            model_path: &model,
            models_dir: &dir,
            prompt: "x",
            negative_prompt: None,
            width: 512,
            height: 512,
            steps: 8,
            cfg_scale: 1.0,
            seed: 1,
            out_file: &out,
            init_image: None,
            mask: None,
            strength: 0.8,
            vram_gb: 6.0,
            uncensored: false,
            batch_count: 1,
        })
        .unwrap_err();

        assert!(err.contains("VAE"), "got: {err}");
        assert!(err.contains("Qwen3"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn weights_larger_than_vram_stream_from_ram_with_vae_left_on_gpu() {
        let dir = scratch("vram");
        // 3 MB stands in for the real 6.13 GB; the ratio is what matters.
        let model = touch(&dir, "z_image_turbo-Q8_0.gguf", 3 * 1024 * 1024);
        touch(&dir, "ae.safetensors", 16);
        touch(&dir, "Qwen3-4B-Instruct-2507-Q4_K_M.gguf", 16);
        let out = dir.join("out.png");

        let build = |vram_gb: f32| {
            build_args(&SdRequest {
                model_path: &model,
                models_dir: &dir,
                prompt: "x",
                negative_prompt: None,
                width: 512,
                height: 512,
                steps: 8,
                cfg_scale: 1.0,
                seed: 1,
                out_file: &out,
                init_image: None,
                mask: None,
                strength: 0.8,
                vram_gb,
                uncensored: false,
                batch_count: 1,
            })
            .unwrap()
        };

        // Tiny card: the 3 MB "weights" exceed 0.002 GiB, so streaming kicks in.
        let tight = build(0.002);
        assert!(tight.contains(&"--offload-to-cpu".to_string()));
        assert!(
            tight.contains(&"te=cpu".to_string()),
            "text encoder belongs on CPU"
        );
        assert!(
            !tight.iter().any(|a| a.contains("vae=cpu")),
            "VAE on CPU turned a 3 s decode into 167 s"
        );

        // Roomy card: no offloading needed.
        let roomy = build(24.0);
        assert!(!roomy.contains(&"--offload-to-cpu".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn uncensored_swaps_in_the_abliterated_encoder() {
        let dir = scratch("abl");
        let model = touch(&dir, "z_image_turbo-Q8_0.gguf", 64);
        touch(&dir, "ae.safetensors", 16);
        touch(&dir, "Qwen3-4B-Instruct-2507-Q4_K_M.gguf", 16);
        touch(&dir, "Z-Image-AbliteratedV1.Q4_K_M.gguf", 24);
        let out = dir.join("out.png");

        let mk = |uncensored: bool| {
            build_args(&SdRequest {
                model_path: &model,
                models_dir: &dir,
                prompt: "x",
                negative_prompt: None,
                width: 512,
                height: 512,
                steps: 8,
                cfg_scale: 1.0,
                seed: 1,
                out_file: &out,
                init_image: None,
                mask: None,
                strength: 0.8,
                vram_gb: 24.0,
                uncensored,
                batch_count: 1,
            })
            .unwrap()
            .join(" ")
        };

        assert!(mk(true).to_lowercase().contains("abliterat"));
        assert!(!mk(false).to_lowercase().contains("abliterat"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_mask_is_only_passed_alongside_an_init_image() {
        let dir = scratch("mask");
        let model = touch(&dir, "stable-diffusion-v1-5-Q4_0.gguf", 64);
        let init = touch(&dir, "in.png", 8);
        let mask = touch(&dir, "mask.png", 8);
        let out = dir.join("out.png");

        let mk = |init_image: Option<&Path>, m: Option<&Path>| {
            build_args(&SdRequest {
                model_path: &model,
                models_dir: &dir,
                prompt: "x",
                negative_prompt: None,
                width: 512,
                height: 512,
                steps: 20,
                cfg_scale: 7.0,
                seed: 1,
                out_file: &out,
                init_image,
                mask: m,
                strength: 0.8,
                vram_gb: 24.0,
                uncensored: false,
                batch_count: 1,
            })
            .unwrap()
        };

        assert!(mk(Some(&init), Some(&mask)).contains(&"--mask".to_string()));
        // sd.cpp ignores --mask without -i; emitting it would only mislead.
        assert!(!mk(None, Some(&mask)).contains(&"--mask".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod real_machine {
    use super::*;
    use std::path::Path;

    /// Builds against this machine's actual model library and asserts the
    /// result matches the invocation verified by hand — the one that took a
    /// Z-Image garment edit from "generate failed" to a 59 s render.
    #[test]
    fn z_image_args_match_the_invocation_proven_on_this_machine() {
        let models = Path::new(r"D:\Documents\Syncho\models");
        let model = models.join("z_image_turbo-Q8_0.gguf");
        if !model.exists() {
            eprintln!("modeles absents, test ignore");
            return;
        }

        let args = build_args(&SdRequest {
            model_path: &model,
            models_dir: models,
            prompt: "a man wearing a dark brown t-shirt",
            negative_prompt: None,
            width: 1024,
            height: 576,
            steps: 8,
            cfg_scale: 1.0,
            seed: 42,
            out_file: Path::new(r"D:\Documents\Syncho\clothes_test\out.png"),
            init_image: Some(Path::new(r"D:\tmp_pip\in.png")),
            mask: Some(Path::new(r"D:\tmp_pip\mask.png")),
            strength: 0.8,
            vram_gb: 6.0,
            uncensored: false,
            batch_count: 1,
        })
        .expect("this machine has the VAE and the Qwen3 encoder");

        let line = args.join(" ");
        println!("\n  sd.exe {line}\n");

        for expected in [
            "--diffusion-model",
            "--vae",
            "--llm",
            "-i",
            "--mask",
            "--offload-to-cpu",
            "te=cpu",
            "--diffusion-fa",
        ] {
            assert!(line.contains(expected), "flag manquant: {expected}\n{line}");
        }
        assert!(line.contains("ae.safetensors"), "VAE non trouve\n{line}");
        assert!(
            line.to_lowercase().contains("qwen3-4b"),
            "encodeur non trouve\n{line}"
        );
        // `-m` on a metadata-less GGUF is the exact failure we are fixing.
        assert!(
            !args.iter().any(|a| a == "-m"),
            "-m ne doit pas etre utilise"
        );
        // The VAE must stay on the GPU: on CPU its decode took 167 s of 310 s.
        assert!(!line.contains("vae=cpu"), "VAE renvoye sur CPU\n{line}");
    }
}

#[cfg(test)]
mod env_checks {
    /// The Python that actually gets used on this machine, and where its
    /// downloads and scratch will land. Guards the whole point of the change:
    /// nothing heavy may resolve back onto the system drive.
    #[test]
    fn resolved_python_and_cache_stay_off_the_system_drive() {
        let py = crate::find_python().expect("un interpreteur Python doit etre trouve");
        println!("\n  python   {py}");
        for (k, v) in crate::python_env() {
            println!("  {k:<20} {v}");
        }

        let env: std::collections::HashMap<_, _> = crate::python_env().into_iter().collect();
        for key in ["HF_HOME", "TMPDIR"] {
            let v = &env[key];
            assert!(!v.is_empty(), "{key} vide");
        }
        assert_eq!(
            env["TRANSFORMERS_NO_TF"], "1",
            "TensorFlow doit rester non importe"
        );
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use std::path::Path;

    /// Names asserted against a real 3-image run, which wrote batch_0/1/2.
    #[test]
    fn batch_paths_are_zero_indexed_like_sd_cpp_writes_them() {
        let out = Path::new(r"D:\out\img_1785.png");
        let (pattern, files) = batch_output(out, 3);

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
    fn a_single_image_keeps_its_plain_name() {
        let out = Path::new(r"D:\out\img.png");
        let (pattern, files) = batch_output(out, 1);
        assert_eq!(pattern, out);
        assert_eq!(files, vec![out.to_path_buf()]);
        // 0 must behave like 1, not produce an empty list.
        assert_eq!(batch_output(out, 0).1.len(), 1);
    }

    #[test]
    fn batch_flag_is_only_emitted_when_more_than_one_is_asked_for() {
        let dir = std::env::temp_dir().join("locaryn_batch_flag_test");
        std::fs::create_dir_all(&dir).unwrap();
        let model = dir.join("stable-diffusion-v1-5-Q4_0.gguf");
        std::fs::write(&model, [0u8; 32]).unwrap();
        let out = dir.join("o.png");

        let mk = |n: u32| {
            build_args(&SdRequest {
                model_path: &model,
                models_dir: &dir,
                prompt: "x",
                negative_prompt: None,
                width: 512,
                height: 512,
                steps: 20,
                cfg_scale: 7.0,
                seed: 1,
                out_file: &out,
                init_image: None,
                mask: None,
                strength: 0.75,
                vram_gb: 24.0,
                uncensored: false,
                batch_count: n,
            })
            .unwrap()
            .join(" ")
        };

        assert!(!mk(1).contains(" -b "), "pas de -b pour une seule image");
        assert!(!mk(1).contains("%d"), "pas de motif pour une seule image");
        let three = mk(3);
        assert!(three.contains(" -b 3"), "{three}");
        assert!(three.contains("o_%d.png"), "{three}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
