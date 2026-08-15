//! Locaryn media generation — the machine-side engines shared by the desktop
//! shell and the daemon.
//!
//! Everything here runs where the models live: the phone never sees a weight
//! file, it asks the daemon and gets back the finished PNG or WAV. The daemon
//! and the desktop must produce the same invocation for the same request, so
//! the argument building lives in this one crate instead of drifting apart.
//!
//! Two engines for now:
//! - [`image`] — text-to-image through stable-diffusion.cpp (`sd.exe`).
//! - [`audio`] — text-to-speech through Kokoro or Qwen3-TTS (Python).

pub mod audio;
pub mod image;
pub mod python;

use std::path::PathBuf;

/// A file produced by an engine, on the machine that ran it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedFile {
    pub path: PathBuf,
}

/// Progress callback: a percentage (0-100) and a human phrase.
///
/// Engines call it on every meaningful step. The daemon logs it; the desktop
/// forwards it to its UI channel.
pub type ProgressFn<'a> = &'a (dyn Fn(u32, &str) + Send + Sync);

/// A no-op progress callback, for callers that only care about the result.
pub fn silent_progress() -> impl Fn(u32, &str) + Send + Sync {
    |_pct, _detail| {}
}
