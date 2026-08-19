//! AirLLM — low-VRAM inference engine.
//!
//! AirLLM (lyric85) runs huge models on small GPUs by loading transformer
//! layers one at a time. This module wires it into Locaryn:
//!
//! 1. `airllm_setup` — installs the `airllm` Python package into the managed
//!    venv (torch/transformers are already there).
//! 2. `airllm_install` — downloads the HuggingFace weights (fp16, un-quantized
//!    — that is AirLLM's design) into `HF_HOME`, so the first launch is
//!    offline.
//! 3. `configure_airllm_provider` — registers the model as the active provider
//!    (`ProviderEngine::AirLlm`); the supervisor then spawns the
//!    OpenAI-compatible Python server.
//!
//! Installed models are tracked in `<storage_root>/airllm/installed.json`.

use crate::Core;
use locaryn_shared_types::{Provider, ProviderEngine};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirllmModelMeta {
    pub repo: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AirllmStatus {
    pub python: bool,
    pub python_path: Option<String>,
    pub torch: bool,
    pub airllm_installed: bool,
    pub installed: Vec<AirllmModelMeta>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SetupEvent {
    Line { text: String },
    Done,
    Error { text: String },
}

fn installed_file() -> std::path::PathBuf {
    locaryn_config::storage_root()
        .join("airllm")
        .join("installed.json")
}

fn read_installed() -> Vec<AirllmModelMeta> {
    std::fs::read_to_string(installed_file())
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<AirllmModelMeta>>(&s).ok())
        .unwrap_or_default()
}

fn write_installed(models: &[AirllmModelMeta]) -> std::io::Result<()> {
    let dir = locaryn_config::storage_root().join("airllm");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(installed_file(), serde_json::to_string_pretty(models)?)
}

/// Report the AirLLM runtime state: Python present, `airllm` package
/// installed, torch present, and the models already downloaded.
#[tauri::command]
pub async fn airllm_status() -> Result<AirllmStatus, String> {
    let python = crate::find_python();
    let python_path = python.clone();
    let (torch, airllm_installed) = match &python {
        Some(py) => {
            let mut command = tokio::process::Command::new(py);
            crate::hide_tokio_console(&mut command);
            let check = command
                .args(["-c", "import importlib.util; print(1 if importlib.util.find_spec('torch') else 0, 1 if importlib.util.find_spec('airllm') else 0)"])
                .output()
                .await;
            match check {
                Ok(out) if out.status.success() => {
                    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    let mut it = s.split_whitespace();
                    (
                        it.next().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0) == 1,
                        it.next().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0) == 1,
                    )
                }
                _ => (false, false),
            }
        }
        None => (false, false),
    };
    Ok(AirllmStatus {
        python: python.is_some(),
        python_path,
        torch,
        airllm_installed,
        installed: read_installed(),
    })
}

/// Install the `airllm` Python package into the managed venv (torch and
/// transformers are reused). Streams pip's output lines.
#[tauri::command]
pub async fn airllm_setup(on_event: Channel<SetupEvent>) -> Result<(), String> {
    let python = crate::find_python().ok_or("Python introuvable — installez Python 3.10+.")?;
    let mut command = tokio::process::Command::new(&python);
    crate::hide_tokio_console(&mut command);
    let mut child = command
        .args(["-m", "pip", "install", "--upgrade", "airllm"])
        .envs(crate::python_env())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("pip spawn: {e}"))?;

    let out = child.stdout.take().ok_or("no stdout")?;
    let err = child.stderr.take().ok_or("no stderr")?;
    let chan = on_event.clone();
    let chan_err = on_event.clone();
    let out_task = tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut lines = BufReader::new(out).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = chan.send(SetupEvent::Line { text: line });
        }
    });
    let err_task = tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut lines = BufReader::new(err).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = chan_err.send(SetupEvent::Line { text: line });
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = out_task.await;
    let _ = err_task.await;
    if !status.success() {
        let _ = on_event.send(SetupEvent::Error {
            text: "pip install airllm a échoué (voir lignes ci-dessus).".into(),
        });
        return Err("pip install airllm a échoué".into());
    }
    let _ = on_event.send(SetupEvent::Done);
    Ok(())
}

/// Download the HuggingFace weights for an AirLLM model into `HF_HOME`.
/// AirLLM keeps full precision (fp16), so the download is larger than a GGUF.
#[tauri::command]
pub async fn airllm_install(
    repo: String,
    on_event: Channel<SetupEvent>,
) -> Result<AirllmModelMeta, String> {
    let repo = repo.trim().to_string();
    if repo.is_empty() {
        return Err("repo requis".into());
    }
    let python = crate::find_python().ok_or("Python introuvable — installez Python 3.10+.")?;
    let hf = locaryn_config::hf_cache_dir();
    let script = concat!(
        "import os,sys; os.environ['HF_HOME']=sys.argv[2];\n",
        "from huggingface_hub import snapshot_download;\n",
        "p=snapshot_download(repo_id=sys.argv[1]);\n",
        "print('DONE:'+p, flush=True)"
    );
    let mut command = tokio::process::Command::new(&python);
    crate::hide_tokio_console(&mut command);
    let mut child = command
        .args(["-c", script, &repo, &hf.to_string_lossy()])
        .envs(crate::python_env())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("download spawn: {e}"))?;

    let out = child.stdout.take().ok_or("no stdout")?;
    let err = child.stderr.take().ok_or("no stderr")?;
    let chan = on_event.clone();
    let chan_err = on_event.clone();
    let out_task = tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut lines = BufReader::new(out).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = chan.send(SetupEvent::Line { text: line });
        }
    });
    let err_task = tauri::async_runtime::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut lines = BufReader::new(err).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = chan_err.send(SetupEvent::Line { text: line });
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    let _ = out_task.await;
    let _ = err_task.await;
    if !status.success() {
        let _ = on_event.send(SetupEvent::Error {
            text: format!("Téléchargement de {repo} a échoué (voir lignes ci-dessus)."),
        });
        return Err(format!("snapshot_download de {repo} a échoué"));
    }

    let meta = AirllmModelMeta {
        repo: repo.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    let mut models = read_installed();
    if !models.iter().any(|m| m.repo == repo) {
        models.push(meta.clone());
        write_installed(&models).map_err(|e| e.to_string())?;
    }
    let _ = on_event.send(SetupEvent::Done);
    Ok(meta)
}

/// List the models downloaded for AirLLM.
#[tauri::command]
pub async fn airllm_installed() -> Result<Vec<AirllmModelMeta>, String> {
    Ok(read_installed())
}

/// Remove a model from the AirLLM registry (weights stay in the HF cache).
#[tauri::command]
pub async fn airllm_uninstall(repo: String) -> Result<(), String> {
    let models = read_installed();
    let kept: Vec<AirllmModelMeta> = models.into_iter().filter(|m| m.repo != repo).collect();
    write_installed(&kept).map_err(|e| e.to_string())
}

/// Activate an AirLLM model as the chat provider. Like `configure_provider`
/// for llama.cpp: switching model shuts down the running AirLLM server, and
/// the next message respawns it with the new weights.
#[tauri::command]
pub async fn configure_airllm_provider(
    core: State<'_, Core>,
    repo: String,
) -> Result<Provider, String> {
    let repo = repo.trim().to_string();
    if repo.is_empty() {
        return Err("repo requis".into());
    }
    let previous = core.storage.providers.active().await.ok().flatten();
    let changed = previous
        .as_ref()
        .map(|p| p.model.as_deref() != Some(repo.as_str()) || p.engine != ProviderEngine::AirLlm)
        .unwrap_or(true);

    let provider = core
        .storage
        .providers
        .upsert_local(ProviderEngine::AirLlm, "http://127.0.0.1:8090", Some(repo))
        .await
        .map_err(|e| e.to_string())?;

    if changed {
        tracing::info!("AirLLM model changed — restarting AirLLM server");
        let _ = core.supervisor.shutdown(ProviderEngine::AirLlm).await;
    }
    crate::refresh_mcp_runtime_env(&core).await;
    Ok(provider)
}
