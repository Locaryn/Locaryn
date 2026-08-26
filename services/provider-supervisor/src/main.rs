//! Locaryn provider supervisor CLI — `locaryn-supervisor`.
//!
//! Standalone CLI that wraps the `locaryn_provider_supervisor` library to
//! detect, start, healthcheck, and stop local LLM runtimes on loopback.
//! The daemon also uses the library in-process; this CLI is for manual
//! inspection and debugging.

use clap::{Parser, Subcommand};
use locaryn_provider_supervisor::{Supervisor, SupervisorConfig};
use locaryn_provider_supervisor::extension_engine::EngineSource as ExtensionEngineSpecSource;
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
    let sup = Supervisor::new(SupervisorConfig::default(), storage.clone());

    // Les moteurs apportés par les extensions installées. Sans cette étape,
    // `locaryn-supervisor start ext:<id>` répondrait « moteur inconnu » alors
    // que l'extension est bien là : le superviseur ne lit pas le disque des
    // extensions, on le lui donne.
    match storage.extensions.list().await {
        Ok(rows) => {
            let sources: Vec<ExtensionEngineSpecSource> = rows
                .into_iter()
                .map(|row| ExtensionEngineSpecSource {
                    manifest_path: std::path::PathBuf::from(row.manifest_path),
                    enabled: row.enabled,
                })
                .collect();
            sup.set_extension_engines(locaryn_provider_supervisor::extension_engine::collect(
                &sources,
            ))
            .await;
        }
        Err(e) => {
            tracing::warn!(erreur = %e, "extensions illisibles — aucun moteur d'extension");
        }
    }

    match cli.cmd {
        Cmd::Status => {
            let snapshot = sup.status_snapshot().await;
            println!("ENGINE            ENDPOINT                 HEALTHY  OWNED  ALIVE");
            for s in snapshot {
                println!(
                    "{:<17} {:<24} {:<7}  {:<5}  {}",
                    s.engine.as_token(),
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
            let ok = sup.is_healthy(&e).await;
            println!("{engine}: {}", if ok { "healthy" } else { "unhealthy" });
            Ok(())
        }
        Cmd::Start { engine, model } => {
            let e = parse_engine(&engine)?;
            if let Some(m) = &model {
                println!("(model override {m} noted — V1.1 passes it to the runtime)");
            }
            match sup.ensure_running(&e).await {
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
            sup.shutdown(&e).await?;
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

/// Lit un nom de moteur d'argument. La table des jetons vit dans
/// `shared-types` — la recopier ici a déjà produit des CLI qui acceptaient un
/// moteur que l'application ne connaissait pas.
fn parse_engine(s: &str) -> anyhow::Result<ProviderEngine> {
    ProviderEngine::from_token(s).ok_or_else(|| {
        anyhow::anyhow!(
            "moteur inconnu : {s} — attendus : ollama, llama_cpp, lmstudio, vllm,              open_ai_compat, airllm, ou ext:<id> pour un moteur d'extension"
        )
    })
}
