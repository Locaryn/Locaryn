//! Locating and configuring the Python used by the TTS engines.
//!
//! The same resolution the desktop shell uses: a managed virtualenv beside the
//! model weights wins, then `python` on PATH, then the per-user install.

/// Find `python.exe` (or `python`) the way the desktop shell does.
pub fn find_python() -> Option<String> {
    for venv in python_venv_candidates() {
        let exe = if cfg!(windows) {
            venv.join("Scripts").join("python.exe")
        } else {
            venv.join("bin").join("python")
        };
        if exe.exists() {
            return Some(exe.to_string_lossy().to_string());
        }
    }
    if let Ok(out) = std::process::Command::new("python")
        .arg("--version")
        .output()
    {
        if out.status.success() {
            return Some("python".to_string());
        }
    }
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        let base = std::path::Path::new(&localappdata)
            .join("Programs")
            .join("Python");
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let python_exe = entry.path().join("python.exe");
                if python_exe.exists() {
                    return Some(python_exe.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// Where a managed Python virtualenv may live, most specific first.
fn python_venv_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Some(v) = std::env::var_os("LOCARYN_PYTHON_VENV") {
        let p = std::path::PathBuf::from(v);
        if !p.as_os_str().is_empty() {
            out.push(p);
        }
    }
    out.push(locaryn_config::storage_root().join("python-env"));
    if let Some(parent) = locaryn_config::models_dir().parent() {
        out.push(parent.join("python-env"));
        out.push(parent.join(".venv"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(".venv"));
    }
    out
}

/// Environment every Python subprocess should inherit.
///
/// `transformers` drags in TensorFlow purely to auto-detect a backend we never
/// use (~20 s per run), and HuggingFace downloads default to `~/.cache`, which
/// is how a system drive ends up full after a few model pulls.
pub fn python_env() -> Vec<(&'static str, String)> {
    let hf = locaryn_config::hf_cache_dir();
    let _ = std::fs::create_dir_all(&hf);
    let tmp = locaryn_config::ensure_temp_dir();
    vec![
        ("HF_HOME", hf.to_string_lossy().to_string()),
        ("TRANSFORMERS_NO_TF", "1".to_string()),
        ("USE_TF", "0".to_string()),
        ("TF_CPP_MIN_LOG_LEVEL", "3".to_string()),
        ("TMPDIR", tmp.to_string_lossy().to_string()),
        ("TEMP", tmp.to_string_lossy().to_string()),
        ("TMP", tmp.to_string_lossy().to_string()),
    ]
}

/// Recursively collect files under `dir`, at most `max_depth` levels deep.
pub fn walkdir_recursive(dir: &std::path::Path, max_depth: usize) -> Vec<std::path::PathBuf> {
    let mut results = Vec::new();
    if max_depth == 0 {
        return results;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                results.extend(walkdir_recursive(&path, max_depth - 1));
            } else if path.is_file() {
                results.push(path);
            }
        }
    }
    results
}
