use anyhow::Result;
use tokio::fs;

/// Detect the local inference engines at startup.
///
/// Binaries are managed under `data_dir/bin/llama/` (llama-server — installed via
/// Settings → Moteur → Install runtime, pinned to a working build) and
/// `data_dir/bin/sd/` (stable-diffusion.cpp). This used to auto-download from
/// pinned GitHub release URLs on startup, but those are now dead (the sd.cpp
/// `master-829dce5` asset 404s) and the llama URL pulled a broken 2024 build.
/// Installation now goes through the managed runtime flow, so here we only
/// detect what's present and log guidance when something is missing — no
/// startup download that can fail.
pub async fn ensure_engines() -> Result<()> {
    let bin_dir = locaryn_config::bin_dir();
    fs::create_dir_all(&bin_dir).await?;

    #[cfg(windows)]
    let (sd_name, llama_name) = ("sd.exe", "llama-server.exe");
    #[cfg(not(windows))]
    let (sd_name, llama_name) = ("sd", "llama-server");

    // Managed layout is bin/sd/<sd> and bin/llama/<llama-server>; also accept a
    // flat bin/<binary> for older installs.
    let sd_present = bin_dir.join("sd").join(sd_name).exists() || bin_dir.join(sd_name).exists();
    let llama_present =
        bin_dir.join("llama").join(llama_name).exists() || bin_dir.join(llama_name).exists();

    if !sd_present {
        tracing::info!(
            "stable-diffusion.cpp not installed — image generation runs in simulation until it is added under bin/sd/."
        );
    }
    if !llama_present {
        tracing::info!(
            "llama.cpp runtime not installed — install it from Settings → Moteur (Runtime IA)."
        );
    }
    Ok(())
}
