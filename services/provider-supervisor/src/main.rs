//! Locaryn provider supervisor CLI — `locaryn-supervisor`.
//!
//! Standalone CLI that wraps the `locaryn_provider_supervisor` library to
//! detect, start, healthcheck, and stop local LLM runtimes on loopback.
//! The daemon also uses the library in-process; this CLI is for manual
//! inspection and debugging.

use clap::{Parser, Subcommand};
use locaryn_provider_supervisor::{Supervisor, SupervisorConfig};
use locaryn_shared_types::ProviderEngine;

#[derive(Parser)]
#[command(
    name = "locaryn-supervisor",
    version,
    about = "Supervise local LLM runtimes on loopback"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show status of all known local runtimes.
    Status,
    /// Healthcheck a specific engine.
    Health { engine: String },
    /// Start a runtime (auto-spawns `ollama serve` for Ollama).
    Start {
        engine: String,
        #[arg(long)]
        model: Option<String>,
    },
    /// Stop a runtime we own (spawned by the supervisor).
    Stop { engine: String },
    /// Run the healthcheck + idle-shutdown loop forever (daemon mode).
    Watch,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();

    // Open storage (same DB as the daemon) so the supervisor can update
    // provider statuses.
    let data_dir = locaryn_config::default_data_dir();
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("locaryn.db");
    let pool = locaryn_storage::open(&db_path).await?;
    let storage = locaryn_storage::Storage::new(pool);
    let sup = Supervisor::new(SupervisorConfig::default(), storage);

    match cli.cmd {
        Cmd::Status => {
            let snapshot = sup.status_snapshot().await;
            println!("ENGINE       ENDPOINT                 HEALTHY  OWNED  ALIVE");
            for s in snapshot {
                println!(
                    "{:<12} {:<24} {:<7}  {:<5}  {}",
                    format!("{:?}", s.engine).to_lowercase(),
                    s.endpoint,
                    if s.healthy { "yes" } else { "no" },
                    if s.owned { "yes" } else { "no" },
                    if s.child_alive { "yes" } else { "no" },
                );
            }
            Ok(())
        }
        Cmd::Health { engine } => {
            let e = parse_engine(&engine)?;
            let ok = sup.is_healthy(e).await;
            println!("{engine}: {}", if ok { "healthy" } else { "unhealthy" });
            Ok(())
        }
        Cmd::Start { engine, model } => {
            let e = parse_engine(&engine)?;
            if let Some(m) = &model {
                println!("(model override {m} noted — V1.1 passes it to the runtime)");
            }
            match sup.ensure_running(e).await {
                Ok(endpoint) => {
                    println!("✓ {engine} running on {endpoint}");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("✗ failed to start: {e}");
                    std::process::exit(1);
                }
            }
        }
        Cmd::Stop { engine } => {
            let e = parse_engine(&engine)?;
            sup.shutdown(e).await?;
            println!("stopped {engine}");
            Ok(())
        }
        Cmd::Watch => {
            println!("locaryn-supervisor watching (Ctrl+C to stop)...");
            let _handle = sup.spawn_healthcheck_loop();
            // Run forever until interrupted.
            tokio::signal::ctrl_c().await?;
            println!("\nshutting down supervisor...");
            Ok(())
        }
    }
}

fn parse_engine(s: &str) -> anyhow::Result<ProviderEngine> {
    Ok(match s.to_lowercase().as_str() {
        "ollama" => ProviderEngine::Ollama,
        "llama_cpp" | "llama-cpp" | "llamacpp" => ProviderEngine::LlamaCpp,
        "lmstudio" | "lm_studio" => ProviderEngine::Lmstudio,
        "vllm" => ProviderEngine::Vllm,
        other => anyhow::bail!("unknown engine: {other}"),
    })
}
