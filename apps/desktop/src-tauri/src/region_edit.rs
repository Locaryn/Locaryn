//! Edit one named region of an image, leaving the rest untouched.
//!
//! Global img2img is the wrong tool for "change the shirt to brown": at low
//! strength nothing moves, at high strength the whole scene is regenerated.
//! The missing piece is a mask. CLIPSeg produces one from a plain description
//! ("the t-shirt", "the wooden shelf"), so this is not limited to any fixed
//! label set.
//!
//! Two modes, because they need different tools:
//!
//! * **Recolour** — the fabric, folds and print already carry the shape, only
//!   the chroma has to move. Rewriting a/b in LAB while keeping L hits the
//!   requested colour exactly, in a fraction of a second, with no model. Asked
//!   for brown, diffusion returned sage green; this returns brown.
//! * **Replace** — a different object or garment genuinely has to be redrawn,
//!   so the diffusion engine runs with `--mask`. Works on every family, since
//!   [`crate::sd_engine`] emits the flag outside its per-family branch.
//!
//! In both cases the result is composited through the mask at full resolution:
//! stable-diffusion.cpp's own masking still perturbs pixels outside the mask,
//! and "only the shirt changed" has to mean exactly that.

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RegionEditResult {
    pub path: String,
    /// Where the mask landed, so the UI can show what was selected.
    pub mask_path: String,
    /// Share of the image the mask covers, as a percentage.
    pub coverage: f32,
    /// Segmenter confidence, 0-1.
    pub confidence: f32,
    /// Connected pieces the selection breaks into. One or two means a real
    /// object; several scattered blobs means the description was too vague —
    /// something confidence alone does not reveal.
    pub pieces: u32,
    /// Share of the selection held by its largest piece, 0-1.
    pub largest: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    Recolour,
    Replace,
    /// Segment only, and render the selection as a tinted overlay. Lets the
    /// user see what was picked before anything is changed — which turns the
    /// confidence threshold into something they can judge for themselves.
    Preview,
}

pub fn parse_mode(s: &str) -> Result<EditMode, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "recolor" | "recolour" | "couleur" => Ok(EditMode::Recolour),
        "replace" | "remplacer" => Ok(EditMode::Replace),
        "preview" | "apercu" | "aperçu" => Ok(EditMode::Preview),
        other => Err(format!("mode inconnu : {other}")),
    }
}

/// Parse `#RRGGBB` (or `RRGGBB`) into components.
pub fn parse_hex_colour(s: &str) -> Result<(u8, u8, u8), String> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("couleur invalide : {s} (attendu #RRGGBB)"));
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    Ok((v(0), v(2), v(4)))
}

/// Python doing the pixel work: segmentation, recolouring and compositing.
///
/// Kept in one script so the model is loaded once per call, and so the mask
/// never has to cross the process boundary as anything but a file.
#[allow(clippy::too_many_arguments)]
fn build_script(
    image: &str,
    mask_out: &str,
    target: &str,
    stage: &str,
    rgb: (u8, u8, u8),
    generated: &str,
    out: &str,
    crop: &str,
    crop_mask: &str,
) -> String {
    let image_json = serde_json::to_string(image).unwrap_or_default();
    let mask_json = serde_json::to_string(mask_out).unwrap_or_default();
    let target_json = serde_json::to_string(target).unwrap_or_default();
    let generated_json = serde_json::to_string(generated).unwrap_or_default();
    let out_json = serde_json::to_string(out).unwrap_or_default();
    let crop_json = serde_json::to_string(crop).unwrap_or_default();
    let crop_mask_json = serde_json::to_string(crop_mask).unwrap_or_default();
    let (r, g, b) = rgb;

    format!(
        r#"# Masked region edit
import json, sys, os

image_path = {image_json}
mask_path = {mask_json}
target = {target_json}
generated_path = {generated_json}
out_path = {out_json}
crop_path = {crop_json}
crop_mask_path = {crop_mask_json}
stage = "{stage}"
target_rgb = ({r}, {g}, {b})

def report(pct, msg):
    print(json.dumps({{'progress': pct, 'detail': msg}}), flush=True)

import numpy as np
import cv2
from PIL import Image

image = Image.open(image_path).convert('RGB')
W, H = image.size

# ── Mask, unless a previous stage already produced one ────────────────────
if not os.path.isfile(mask_path):
    report(10, "analyse de l'image")
    import torch
    from transformers import CLIPSegForImageSegmentation, CLIPSegProcessor
    MODEL = 'CIDAS/clipseg-rd64-refined'
    proc = CLIPSegProcessor.from_pretrained(MODEL)
    model = CLIPSegForImageSegmentation.from_pretrained(MODEL).eval()

    def heat_for(img):
        with torch.no_grad():
            inputs = proc(text=[target], images=[img], return_tensors='pt')
            logits = model(**inputs).logits
        return torch.sigmoid(logits).squeeze().cpu().numpy()

    # CLIPSeg reasons at 352 px, so anything small in a 1280 px photo is a
    # handful of pixels by the time it reaches the network — "the sunglasses"
    # scored 0.17 and covered 0.5 %. Running the same query over overlapping
    # tiles gives each region its own full budget of resolution: a detail that
    # is 0.5 % of the frame can be 5 % of a tile. Scores are merged by max, so
    # a strong local hit survives a weak global one.
    heat = cv2.resize(heat_for(image), (W, H), interpolation=cv2.INTER_CUBIC)
    report(22, "recherche des details")
    tiles, overlap = 2, 0.35
    tw, th = int(W / (tiles - overlap)), int(H / (tiles - overlap))
    step_x, step_y = int(tw * (1 - overlap)), int(th * (1 - overlap))
    for ty in range(tiles):
        for tx in range(tiles):
            x0, y0 = min(tx * step_x, max(0, W - tw)), min(ty * step_y, max(0, H - th))
            x1, y1 = min(W, x0 + tw), min(H, y0 + th)
            if x1 - x0 < 64 or y1 - y0 < 64:
                continue
            sub = heat_for(image.crop((x0, y0, x1, y1)))
            sub = cv2.resize(sub, (x1 - x0, y1 - y0), interpolation=cv2.INTER_CUBIC)
            heat[y0:y1, x0:x1] = np.maximum(heat[y0:y1, x0:x1], sub)

    confidence = float(heat.max())
    # A relative cut: CLIPSeg's absolute confidence swings with the wording, so
    # a fixed threshold either selects everything or nothing.
    mask = ((heat > max(0.4 * confidence, 0.12)) * 255).astype(np.uint8)
    # Grow slightly to cover the object's own outline, then feather so the
    # edit blends instead of showing a seam.
    mask = cv2.dilate(mask, np.ones((9, 9), np.uint8), 1)
    mask = cv2.GaussianBlur(mask, (21, 21), 0)
    mask = np.where(mask > 127, 255, 0).astype(np.uint8)
    mask = cv2.GaussianBlur(mask, (9, 9), 0)
    Image.fromarray(mask).save(mask_path)
else:
    confidence = 1.0
    mask = np.array(Image.open(mask_path).convert('L'))

coverage = 100.0 * float(np.count_nonzero(mask)) / mask.size

# How scattered is the selection? A real object is one or two connected
# pieces. Tiling raised confidence on vague descriptions without improving
# them: "the picture frames on the wall" scored 0.88 yet returned a dozen
# blobs spread over the wall, the door and the shelf. Fragmentation catches
# exactly what the confidence score misses.
n_lbl, _, stats, _ = cv2.connectedComponentsWithStats((mask > 127).astype(np.uint8), 8)
areas = sorted((stats[i, cv2.CC_STAT_AREA] for i in range(1, n_lbl)), reverse=True)
total_area = float(sum(areas)) or 1.0
# Pieces big enough to matter, i.e. at least 8 % of what was selected.
pieces = sum(1 for a in areas if a >= 0.08 * total_area)
largest_share = (areas[0] / total_area) if areas else 0.0
print(json.dumps({{'mask': True, 'coverage': coverage, 'confidence': confidence,
                  'pieces': pieces, 'largest': largest_share}}), flush=True)
if coverage < 0.2:
    print(json.dumps({{'progress': -1,
                      'detail': "Rien trouve pour « %s »" % target}}), flush=True)
    sys.exit(2)

# Confidence predicts the failure mode: asked for "the picture frames on the
# wall" the segmenter scored 0.41 and returned the entire wall, while the
# shelf (0.93) and the t-shirt (0.97) were exact. Editing the wrong region
# silently is worse than refusing, so a weak match stops here.
# In preview the point is to *see* a bad selection, so the guard only blocks
# real edits. Refusing to render the overlay would hide the very evidence the
# user needs to reword their description.
if confidence < 0.55 and stage != "preview":
    print(json.dumps({{'progress': -1, 'detail':
        "Zone « %s » reconnue trop faiblement (%.0f %%) et couvrant %.0f %% de l'image. "
        "Reformulez plus precisement (ex. « le cadre a gauche » plutot que « les cadres »)."
        % (target, confidence * 100, coverage)}}), flush=True)
    sys.exit(3)
if pieces >= 5 and largest_share < 0.45 and stage != "preview":
    print(json.dumps({{'progress': -1, 'detail':
        "Selection eclatee en %d morceaux pour « %s » : la description designe "
        "probablement plusieurs objets ou rien de precis. Visez un seul element "
        "(ex. « le cadre a gauche »)." % (pieces, target)}}), flush=True)
    sys.exit(3)
if coverage > 45.0 and stage != "preview":
    print(json.dumps({{'progress': -1, 'detail':
        "La zone « %s » couvre %.0f %% de l'image : c'est probablement une mauvaise "
        "selection. Reformulez la description." % (target, coverage)}}), flush=True)
    sys.exit(3)

alpha = (mask.astype(np.float32) / 255.0)[:, :, None]
base = np.array(image).astype(np.float32)


def region_box(m, side=None):
    """Square crop around the mask, with context, clamped to the image."""
    ys, xs = np.where(m > 127)
    x0, x1, y0, y1 = int(xs.min()), int(xs.max()), int(ys.min()), int(ys.max())
    pad = int(max(x1 - x0, y1 - y0) * 0.18)
    x0, y0 = max(0, x0 - pad), max(0, y0 - pad)
    x1, y1 = min(W, x1 + pad), min(H, y1 + pad)
    s = side or min(max(x1 - x0, y1 - y0), W, H)
    cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
    bx = max(0, min(W - s, cx - s // 2))
    by = max(0, min(H - s, cy - s // 2))
    return bx, by, bx + s, by + s

if stage == "recolour":
    report(60, "recoloration")
    bgr = cv2.cvtColor(np.array(image), cv2.COLOR_RGB2BGR)
    lab = cv2.cvtColor(bgr, cv2.COLOR_BGR2LAB).astype(np.float32)
    L, A, B = lab[:, :, 0], lab[:, :, 1], lab[:, :, 2]

    sel = mask > 127
    lo, hi = np.percentile(L[sel], 2), np.percentile(L[sel], 98)
    span = max(float(hi - lo), 1.0)

    swatch = np.uint8([[[target_rgb[2], target_rgb[1], target_rgb[0]]]])
    tl, ta, tb = cv2.cvtColor(swatch, cv2.COLOR_BGR2LAB)[0, 0].astype(np.float32)
    # Remap the region's own luminance onto the target's. Without this a
    # blown-out white shirt stays white whatever chroma it is given.
    l_new = np.clip(tl + ((L - lo) / span - 0.5) * span * 0.62, 4, 250)

    out_lab = lab.copy()
    a2 = alpha[:, :, 0]
    out_lab[:, :, 0] = L * (1 - a2) + l_new * a2
    out_lab[:, :, 1] = A * (1 - a2) + ta * a2
    out_lab[:, :, 2] = B * (1 - a2) + tb * a2
    result = cv2.cvtColor(np.clip(out_lab, 0, 255).astype(np.uint8), cv2.COLOR_LAB2BGR)
    result = cv2.cvtColor(result, cv2.COLOR_BGR2RGB)
    Image.fromarray(result).save(out_path, quality=94)

    got = result[sel].mean(axis=0)
    print(json.dumps({{'progress': 95,
                      'detail': "couleur obtenue rgb(%d,%d,%d)" % tuple(got.round())}}),
          flush=True)
elif stage == "preview":
    # Tint the selection instead of editing it: seeing the region is the only
    # way to know the description picked the right thing.
    report(70, "apercu de la selection")
    tint = base.copy()
    colour = np.array(target_rgb, dtype=np.float32)
    tint = tint * (1 - alpha * 0.55) + colour * (alpha * 0.55)
    Image.fromarray(tint.clip(0, 255).astype(np.uint8)).save(out_path, quality=90)
elif stage == "crop":
    # Cut a square around the region and hand *that* to the engine instead of
    # the whole frame. A 1280x720 photo squeezed into 768 px leaves a shirt
    # barely 200 px wide; cropping first spends the model's entire resolution
    # budget on the part being redrawn, and keeps VRAM flat regardless of how
    # large the source image is.
    bx0, by0, bx1, by1 = region_box(mask)
    side = 1024 if min(bx1 - bx0, by1 - by0) >= 640 else 768
    image.crop((bx0, by0, bx1, by1)).resize((side, side), Image.LANCZOS).save(crop_path)
    Image.fromarray(mask).crop((bx0, by0, bx1, by1)) \
        .resize((side, side), Image.LANCZOS).save(crop_mask_path)
    print(json.dumps({{'crop': [bx0, by0, bx1, by1], 'side': side}}), flush=True)
    report(35, "zone isolee a %d px" % side)
else:
    report(85, "assemblage")
    bx0, by0, bx1, by1 = region_box(mask)
    gen = Image.open(generated_path).convert('RGB')
    canvas = Image.fromarray(base.astype(np.uint8)).copy()
    canvas.paste(gen.resize((bx1 - bx0, by1 - by0), Image.LANCZOS), (bx0, by0))
    # Blend through the mask: the engine's own masking still nudges pixels
    # outside the region, and "only this changed" must be literally true.
    blended = (np.array(canvas).astype(np.float32) * alpha + base * (1 - alpha))
    Image.fromarray(blended.clip(0, 255).astype(np.uint8)).save(out_path, quality=94)

report(100, "termine")
"#
    )
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionEditArgs {
    /// Source image: a disk path or a `data:` URL from the webview.
    pub image: String,
    /// What to select, in plain words: "the t-shirt", "the wooden shelf".
    pub target: String,
    /// `recolor` or `replace`.
    pub mode: String,
    /// Target colour for `recolor`, as `#RRGGBB`.
    #[serde(default)]
    pub color: Option<String>,
    /// What to draw instead, for `replace`.
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub output_dir: String,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub cfg_scale: Option<f32>,
    #[serde(default)]
    pub strength: Option<f32>,
}

#[tauri::command]
pub async fn edit_region(
    args: RegionEditArgs,
    on_progress: Channel<serde_json::Value>,
) -> Result<RegionEditResult, String> {
    let mode = parse_mode(&args.mode)?;
    let target = args.target.trim();
    if target.is_empty() {
        return Err("Décrivez la zone à modifier (ex. « le t-shirt »).".into());
    }
    let rgb = match mode {
        EditMode::Preview => (220, 60, 60),
        EditMode::Recolour => parse_hex_colour(
            args.color
                .as_deref()
                .ok_or("Choisissez une couleur cible pour une recoloration.")?,
        )?,
        EditMode::Replace => (0, 0, 0),
    };

    let out_dir = std::path::Path::new(&args.output_dir);
    std::fs::create_dir_all(out_dir).map_err(|e| format!("dossier de sortie: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let out_file = out_dir.join(if mode == EditMode::Preview {
        format!("preview_{stamp}.jpg")
    } else {
        format!("edit_{stamp}.png")
    });

    let scratch = locaryn_config::ensure_temp_dir();
    let mask_file = scratch.join(format!("mask_{stamp}.png"));

    // The webview cannot hand over a disk path, so an attached image arrives
    // base64-encoded; everything downstream wants a file.
    let source = if args.image.starts_with("data:") {
        crate::decode_data_url_to_temp(&args.image)?
    } else {
        std::path::PathBuf::from(&args.image)
    };
    if !source.is_file() {
        return Err(format!("image introuvable : {}", source.display()));
    }

    // ── Replace: render first, then composite through the mask.
    let generated = if mode == EditMode::Replace {
        let prompt = args
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("Décrivez ce qui doit remplacer la zone.")?;

        // Mask first, then a crop centred on it: the engine only ever sees the
        // region being redrawn, so detail does not depend on the source photo's
        // size and VRAM use stays flat.
        let crop_file = scratch.join(format!("crop_{stamp}.png"));
        let crop_mask_file = scratch.join(format!("cropmask_{stamp}.png"));
        run_python(
            &build_script(
                &source.to_string_lossy(),
                &mask_file.to_string_lossy(),
                target,
                "crop",
                rgb,
                "",
                "",
                &crop_file.to_string_lossy(),
                &crop_mask_file.to_string_lossy(),
            ),
            &on_progress,
            false,
        )
        .await?;

        let model = args.model.as_deref().unwrap_or_default();
        let models_dir = locaryn_config::models_dir();
        let model_path = models_dir.join(model);
        if model.is_empty() || !model_path.exists() {
            return Err("Choisissez un modèle d'image installé pour un remplacement.".into());
        }
        let sd_bin = crate::sd_engine::find_sd_binary().ok_or("moteur d'images introuvable")?;
        let (fam_steps, fam_cfg) = crate::sd_engine::default_sampling(model);
        // Square by construction, so one side is enough.
        let side = image_dimensions(&crop_file).map(|d| d.0).unwrap_or(768);
        let raw = scratch.join(format!("gen_{stamp}.png"));

        let sd_args = crate::sd_engine::build_args(&crate::sd_engine::SdRequest {
            model_path: &model_path,
            models_dir: &models_dir,
            prompt,
            negative_prompt: None,
            width: round64(side),
            height: round64(side),
            steps: args.steps.unwrap_or(fam_steps),
            cfg_scale: args.cfg_scale.unwrap_or(fam_cfg),
            seed: 42,
            out_file: &raw,
            init_image: Some(&crop_file),
            mask: Some(&crop_mask_file),
            strength: args.strength.unwrap_or(0.85),
            vram_gb: crate::check_hardware()
                .map(|h| h.total_vram_gb as f32)
                .unwrap_or(0.0),
            uncensored: false,
            batch_count: 1,
        })?;

        on_progress
            .send(serde_json::json!({"progress": 40, "detail": "génération de la zone"}))
            .ok();
        let status = tokio::process::Command::new(&sd_bin)
            .args(&sd_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map_err(|e| format!("lancement de sd: {e}"))?;
        if !status.success() || !raw.exists() {
            return Err("la génération de la zone a échoué".into());
        }
        raw.to_string_lossy().to_string()
    } else {
        String::new()
    };

    let stats = run_python(
        &build_script(
            &source.to_string_lossy(),
            &mask_file.to_string_lossy(),
            target,
            match mode {
                EditMode::Recolour => "recolour",
                EditMode::Preview => "preview",
                EditMode::Replace => "composite",
            },
            rgb,
            &generated,
            &out_file.to_string_lossy(),
            "",
            "",
        ),
        &on_progress,
        false,
    )
    .await?;

    Ok(RegionEditResult {
        path: out_file.to_string_lossy().to_string(),
        mask_path: mask_file.to_string_lossy().to_string(),
        coverage: stats.0,
        confidence: stats.1,
        pieces: stats.2,
        largest: stats.3,
    })
}

fn round64(v: u32) -> u32 {
    (v / 64).max(4) * 64
}

/// Width and height from a PNG or JPEG header, without decoding the pixels.
fn image_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    let b = std::fs::read(path).ok()?;
    if b.len() > 24 && b.starts_with(&[0x89, b'P', b'N', b'G']) {
        let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
        let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
        return Some((w, h));
    }
    if b.len() > 4 && b[0] == 0xFF && b[1] == 0xD8 {
        let mut i = 2usize;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            // SOF0..SOF15, excluding the non-frame markers in that range.
            if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC
            {
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
                return Some((w, h));
            }
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            i += 2 + len;
        }
    }
    None
}

/// Run a script, forwarding progress and returning (coverage, confidence).
async fn run_python(
    script: &str,
    on_progress: &Channel<serde_json::Value>,
    mask_only: bool,
) -> Result<(f32, f32, u32, f32), String> {
    let python = crate::find_python().ok_or("Python non trouvé. Installez Python 3.10+.")?;
    let mut child = tokio::process::Command::new(&python)
        .envs(crate::python_env())
        .arg("-c")
        .arg(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("python spawn: {e}"))?;

    use tokio::io::{AsyncBufReadExt, BufReader};
    let stdout = child.stdout.take().ok_or("python stdout indisponible")?;
    let stderr = child.stderr.take().ok_or("python stderr indisponible")?;
    let err_task = tokio::spawn(async move {
        let mut buf = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            buf.push_str(&l);
            buf.push('\n');
        }
        buf
    });

    let mut coverage = 0.0f32;
    let mut confidence = 0.0f32;
    let mut pieces = 0u32;
    let mut largest = 0.0f32;
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("mask").is_some() {
            coverage = v.get("coverage").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            confidence = v.get("confidence").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            pieces = v.get("pieces").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
            largest = v.get("largest").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            if mask_only {
                // The mask is all this pass was for; let the engine take over.
                break;
            }
            continue;
        }
        on_progress.send(v).ok();
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("python wait: {e}"))?;
    let errs = err_task.await.unwrap_or_default();
    if !mask_only && !status.success() {
        return Err(format!(
            "édition impossible : {}",
            crate::summarise_python_error(&errs)
        ));
    }
    Ok((coverage, confidence, pieces, largest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_parsed_and_bad_ones_refused() {
        assert_eq!(parse_hex_colour("#633E26").unwrap(), (99, 62, 38));
        assert_eq!(parse_hex_colour("1C2A5C").unwrap(), (28, 42, 92));
        for bad in ["#12345", "oops", "#GGGGGG", ""] {
            assert!(
                parse_hex_colour(bad).is_err(),
                "{bad} aurait dû être refusé"
            );
        }
    }

    #[test]
    fn both_spellings_of_each_mode_are_accepted() {
        assert_eq!(parse_mode("recolor").unwrap(), EditMode::Recolour);
        assert_eq!(parse_mode("Recolour").unwrap(), EditMode::Recolour);
        assert_eq!(parse_mode(" replace ").unwrap(), EditMode::Replace);
        assert!(parse_mode("erase").is_err());
    }

    #[test]
    fn dimensions_are_rounded_to_what_the_engines_accept() {
        // Latent models need a multiple of 64; 1280x720 must not become 720.
        assert_eq!(round64(1280), 1280);
        assert_eq!(round64(720), 704);
        assert_eq!(round64(10), 256, "une valeur absurde ne doit pas donner 0");
    }

    #[test]
    fn png_and_jpeg_dimensions_are_read_from_the_header() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_dims_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        // Minimal PNG header: signature, IHDR length/type, then width/height.
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend(13u32.to_be_bytes());
        png.extend(b"IHDR");
        png.extend(1280u32.to_be_bytes());
        png.extend(720u32.to_be_bytes());
        png.extend([8, 2, 0, 0, 0]);
        let p = dir.join("a.png");
        std::fs::write(&p, &png).unwrap();
        assert_eq!(image_dimensions(&p), Some((1280, 720)));

        // A real photo from this machine, if it is still there.
        let jpg = std::path::Path::new(r"D:\Pictures\Camera Roll\test.jpg");
        if jpg.is_file() {
            assert_eq!(image_dimensions(jpg), Some((1280, 720)));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_generated_script_selects_the_right_stage() {
        let recolour = build_script(
            "i.png",
            "m.png",
            "the t-shirt",
            "recolour",
            (99, 62, 38),
            "",
            "o.png",
            "",
            "",
        );
        assert!(recolour.contains(r#"stage = "recolour""#));
        assert!(recolour.contains("(99, 62, 38)"));
        // The description must be JSON-quoted, not interpolated raw.
        assert!(recolour.contains(r#"target = "the t-shirt""#));

        let replace = build_script(
            "i.png",
            "m.png",
            "the shelf",
            "composite",
            (0, 0, 0),
            "g.png",
            "o.png",
            "",
            "",
        );
        assert!(replace.contains(r#"stage = "composite""#));
    }

    #[test]
    fn a_quote_in_the_description_cannot_break_the_script() {
        let s = build_script(
            "i.png",
            "m.png",
            "the \"blue\" mug",
            "recolour",
            (1, 2, 3),
            "",
            "o.png",
            "",
            "",
        );
        assert!(s.contains(r#"target = "the \"blue\" mug""#), "{s}");
    }
}
