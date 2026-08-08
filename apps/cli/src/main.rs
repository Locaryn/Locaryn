//! Lochor CLI — a thin client over the local daemon (or the remote server).
//!
//! Usage:
//!   lochor                      — the agent, in the current directory
//!   lochor chat                 — plain conversation, no file access
//!   lochor status
//!   lochor projects add <path>
//!   lochor plugin install <path>
//!   lochor import claude-code <path>

use clap::{Parser, Subcommand};
use lochor_sdk::LochorClient;
use std::io::BufRead;

#[derive(Parser)]
#[command(
    name = "lochor",
    version,
    about = "Lochor — agentic coding platform CLI"
)]
struct Cli {
    /// Daemon / server base URL. Defaults to the local daemon.
    #[arg(long, env = "LOCHOR_SERVER_URL")]
    server: Option<String>,
    /// Bearer token (remote server only).
    #[arg(long, env = "LOCHOR_TOKEN")]
    token: Option<String>,
    /// Without a subcommand, Lochor opens its agent in the current directory.
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show status: mode, provider, daemon, projects.
    Status,
    /// Manage projects.
    Projects {
        #[command(subcommand)]
        action: ProjectsCmd,
    },
    /// Manage sessions.
    Sessions {
        #[command(subcommand)]
        action: SessionsCmd,
    },
    /// Plain conversation, with no access to your files.
    Chat {
        /// Resume an existing session.
        #[arg(long)]
        resume: Option<String>,
        /// Use a specific agent profile.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Manage providers.
    Provider {
        #[command(subcommand)]
        action: ProviderCmd,
    },
    /// Manage plugins / extensions.
    Plugin {
        #[command(subcommand)]
        action: PluginCmd,
    },
    /// Manage MCP servers.
    Mcp {
        #[command(subcommand)]
        action: McpCmd,
    },
    /// Reach this machine from elsewhere, without touching the router.
    Travel {
        #[command(subcommand)]
        action: Option<TravelCmd>,
    },
    /// Import a foreign bundle (claude-code, cursor, continue, cline).
    Import {
        /// Source format.
        format: String,
        /// Source path.
        path: String,
        /// Output directory (defaults to ./.lochor/).
        #[arg(long)]
        out: Option<String>,
    },
    /// Produce a ready-to-distribute client configuration.
    Provision {
        /// Server address as employees will reach it: 192.168.1.10,
        /// 192.168.1.10:7474 or a full URL.
        url: String,
        /// Organisation name, shown on the sign-in screen.
        #[arg(long)]
        org: Option<String>,
        /// Note displayed under the sign-in form.
        #[arg(long)]
        note: Option<String>,
        /// Where to write the file. Defaults to the current directory.
        #[arg(long)]
        out: Option<String>,
    },
    /// Manage accounts (shared-server mode).
    Users {
        #[command(subcommand)]
        action: UsersCmd,
    },
    /// Manage the local daemon.
    Daemon {
        #[command(subcommand)]
        action: DaemonCmd,
    },
}

#[derive(Subcommand)]
enum ProjectsCmd {
    List,
    Add {
        path: String,
        #[arg(long, default_value = "untrusted")]
        trust: String,
    },
}

#[derive(Subcommand)]
enum SessionsCmd {
    List,
    New,
}

#[derive(Subcommand)]
enum ProviderCmd {
    List,
    Use {
        target: String,
    },
    Health {
        #[arg(default_value = "")]
        id: String,
    },
    Start {
        engine: String,
        #[arg(long)]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum PluginCmd {
    Install {
        path: String,
        #[arg(long, default_value = "user")]
        scope: String,
    },
    List,
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Remove {
        name: String,
    },
    Reload,
}

#[derive(Subcommand)]
enum McpCmd {
    /// List the registered servers.
    List,
    /// Register a server. `target` is the command to run, or an http(s) URL.
    Add {
        name: String,
        target: String,
        /// Start it whenever Lochor starts.
        #[arg(long)]
        auto: bool,
    },
    /// Unregister a server.
    Remove { name: String },
    /// Run a server once and print the tools it announces, without keeping
    /// it. This is how you check a command before relying on it.
    Test { name: String },
    /// Start a registered server in the running daemon.
    Start { name: String },
    /// Stop it.
    Stop { name: String },
    /// Alias of `test`, kept because the protocol calls it discovery.
    Discover { name: String },
}

#[derive(Subcommand)]
enum TravelCmd {
    /// Open the tunnel and show the code to scan.
    On {
        /// cloudflare (aucun compte), ngrok, ou devtunnel.
        #[arg(long, default_value = "cloudflare")]
        via: String,
    },
    /// Close it.
    Off,
    /// Show the code again — they expire.
    Qr,
    /// Show the code that puts a phone back on the local network.
    Home,
}

#[derive(Subcommand)]
enum UsersCmd {
    /// List accounts.
    List,
    /// Create an account. The password is read from standard input.
    Add {
        username: String,
        /// Grant administrator rights.
        #[arg(long)]
        admin: bool,
    },
    /// Issue a client certificate for mutual TLS. Prints where it was saved.
    Cert {
        username: String,
        /// Validity in days.
        #[arg(long, default_value = "365")]
        days: u32,
    },
    /// Disable an account: its tokens stop working at once.
    Disable { username: String },
    /// Re-enable a disabled account.
    Enable { username: String },
}

#[derive(Subcommand)]
enum DaemonCmd {
    Start,
    Stop,
    Logs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let cfg = lochor_config::load(None)?;
    let base_url = cli.server.unwrap_or(cfg.connection.local_url.clone());
    let client = LochorClient::new(&base_url, cli.token)?;

    // Most commands talk to the daemon, so an unreachable one is the single
    // most likely first-run failure. A raw reqwest chain tells the user nothing
    // they can act on; name the cause and the fix instead.
    //
    // The exceptions work on files rather than on a running service, and
    // refusing them would be gratuitous — `mcp add` and `mcp test` in
    // particular are what someone runs *before* starting anything.
    let needs_daemon = !matches!(
        cli.cmd,
        Some(Cmd::Daemon { .. }) | Some(Cmd::Users { .. }) | Some(Cmd::Provision { .. })
    ) && !matches!(
        cli.cmd,
        Some(Cmd::Mcp {
            action: McpCmd::List
                | McpCmd::Add { .. }
                | McpCmd::Remove { .. }
                | McpCmd::Test { .. }
                | McpCmd::Discover { .. }
        })
    );
    if needs_daemon
        && client.health().await.is_err()
    {
        anyhow::bail!(
            "Aucun service Lochor n'écoute sur {base_url}.\n\
             Démarrez-le avec `lochor daemon start`, ou lancez l'application Lochor \
             (son mode serveur expose la même interface).\n\
             Pour viser une autre machine : `lochor --server http://IP:7474 …`"
        );
    }

    let Some(cmd) = cli.cmd else {
        // Bare `lochor`: the agent, working in the current directory.
        return converse(&client, None, None, true).await;
    };

    match cmd {
        Cmd::Status => print_status(&client).await,
        Cmd::Projects { action } => match action {
            ProjectsCmd::List => {
                let ps = client.list_projects().await?;
                println!("{:<36} {:<24} {:<10}", "ID", "NAME", "TRUST");
                for p in ps {
                    println!(
                        "{:<36} {:<24} {:<10}",
                        p.id,
                        p.name,
                        format!("{:?}", p.trust_level).to_lowercase()
                    );
                }
                Ok(())
            }
            ProjectsCmd::Add { path, trust } => {
                let t = parse_trust(&trust)?;
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("project")
                    .to_string();
                let p = client.create_project(&path, &name, t).await?;
                println!("added project {} ({})", p.name, p.id);
                Ok(())
            }
        },
        Cmd::Sessions { action } => match action {
            SessionsCmd::List => {
                println!("(sessions list — V1 wires project selection)");
                Ok(())
            }
            SessionsCmd::New => {
                println!("(new session — V1 wires project selection)");
                Ok(())
            }
        },
        Cmd::Chat { resume, agent } => converse(&client, resume, agent, false).await,
        Cmd::Provider { action } => match action {
            ProviderCmd::List => {
                let ps = client.list_providers().await?;
                for p in ps {
                    println!(
                        "{:?} {:?} {} active={}",
                        p.kind, p.engine, p.endpoint, p.is_active
                    );
                }
                Ok(())
            }
            ProviderCmd::Use { target } => {
                let mode = match target.as_str() {
                    "auto" => lochor_shared_types::ConnectionMode::Auto,
                    "local" => lochor_shared_types::ConnectionMode::Local,
                    "remote" => lochor_shared_types::ConnectionMode::Remote,
                    _ => anyhow::bail!("invalid mode: {target} (use auto|local|remote)"),
                };
                let p = client.switch_provider(mode).await?;
                println!("switched to {:?} {:?}", p.kind, p.engine);
                Ok(())
            }
            ProviderCmd::Health { id } => {
                println!("health {id} (V1 wires per-provider healthcheck)");
                Ok(())
            }
            ProviderCmd::Start { engine, model } => {
                let e = parse_engine(&engine)?;
                let p = client.start_local(e, model.as_deref()).await?;
                println!("started {:?} at {}", p.engine, p.endpoint);
                Ok(())
            }
        },
        Cmd::Plugin { action } => match action {
            PluginCmd::Install { path, scope } => {
                let scope = parse_scope(&scope)?;
                let reg = lochor_extensions::ExtensionRegistry::new();
                let entry = reg.install_from_dir(std::path::Path::new(&path), scope)?;
                println!(
                    "installed {} v{} ({}), permissions pending approval",
                    entry.name,
                    entry.version,
                    format!("{:?}", entry.scope).to_lowercase()
                );
                Ok(())
            }
            PluginCmd::List => {
                println!("(plugin list — V1 wires the registry query)");
                Ok(())
            }
            PluginCmd::Enable { name } => {
                println!("enabled {name}");
                Ok(())
            }
            PluginCmd::Disable { name } => {
                println!("disabled {name}");
                Ok(())
            }
            PluginCmd::Remove { name } => {
                println!("removed {name}");
                Ok(())
            }
            PluginCmd::Reload => {
                println!("reloaded");
                Ok(())
            }
        },
        Cmd::Mcp { action } => mcp_cmd(action, &client).await,
        Cmd::Travel { action } => travel_cmd(action, &client).await,
        Cmd::Import { format, path, out } => {
            let out = out.unwrap_or_else(|| "./.lochor/imported".into());
            let out_path = std::path::Path::new(&out);
            let summary = match format.as_str() {
                "claude-code" | "claude_code" => lochor_extensions::registry::import_claude_code(
                    std::path::Path::new(&path),
                    out_path,
                )?,
                "cursor" => lochor_extensions::registry::import_cursor(
                    std::path::Path::new(&path),
                    out_path,
                )?,
                other => {
                    anyhow::bail!("unsupported import format: {other} (try claude-code|cursor)")
                }
            };
            println!(
                "imported {format}: {} agents, {} commands, {} skills, {} hooks, {} rules, {} mcp → {out}",
                summary.agents, summary.commands, summary.skills, summary.hooks_files,
                summary.rules_files, summary.mcp_servers
            );
            Ok(())
        }
        Cmd::Provision { url, org, note, out } => provision_cmd(url, org, note, out).await,
        Cmd::Users { action } => users_cmd(action).await,
        Cmd::Daemon { action } => match action {
            DaemonCmd::Start => {
                println!("use `cargo run -p lochor-daemon` to start the daemon in dev");
                Ok(())
            }
            DaemonCmd::Stop => {
                println!("(daemon stop — V1 wires signal to PID file)");
                Ok(())
            }
            DaemonCmd::Logs => {
                println!("(daemon logs — V1 wires log tail)");
                Ok(())
            }
        },
    }
}

async fn print_status(client: &LochorClient) -> anyhow::Result<()> {
    let h = client.health().await?;
    println!("Lochor status");
    println!("  version : {}", h.version);
    println!("  mode    : {:?}", h.mode);
    if let Some(p) = h.active_provider {
        println!(
            "  provider: {:?} {:?} {} model={:?}",
            p.kind, p.engine, p.endpoint, p.model
        );
    }
    Ok(())
}

/// Resolve the session to talk in: the one asked for, or a fresh one in the
/// project covering the current directory.
///
/// The previous version posted to an all-zero session id, so nothing was ever
/// stored and the agent had no project context to work from.
async fn resolve_session(
    client: &LochorClient,
    resume: Option<String>,
    agentic: bool,
) -> anyhow::Result<String> {
    if let Some(id) = resume {
        let s = client.get_session(&id).await?;
        println!("reprise de la session {}", s.id);
        return Ok(s.id.to_string());
    }

    if !agentic {
        // The container the desktop uses for chats that belong to no project.
        // Its path is a marker, not a directory, so the runtime stays in plain
        // conversation mode.
        const FREE: &str = "__lochor_free_chats__";
        let projects = client.list_projects().await?;
        let free = match projects.iter().find(|p| p.path == FREE) {
            Some(p) => p.clone(),
            None => {
                client
                    .create_project(FREE, "Conversations libres",
                                    lochor_shared_types::TrustLevel::Sandbox)
                    .await?
            }
        };
        let session = client.create_session(&free.id.to_string()).await?;
        return Ok(session.id.to_string());
    }

    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.to_string_lossy().replace('\\', "/");
    let projects = client.list_projects().await?;
    // Longest matching root wins, so a nested project beats its parent.
    let project = projects
        .iter()
        .filter(|p| {
            let root = p.path.replace('\\', "/");
            cwd_str == root || cwd_str.starts_with(&format!("{root}/"))
        })
        .max_by_key(|p| p.path.len())
        .cloned();

    let project = match project {
        Some(p) => {
            println!("projet : {} ({})", p.name, p.path);
            p
        }
        None => {
            let p = client
                // Name it after the directory, as the desktop does.
                .create_project(
                    &cwd_str,
                    cwd.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "projet".into())
                        .as_str(),
                    lochor_shared_types::TrustLevel::Untrusted,
                )
                .await?;
            println!("nouveau projet : {} ({})", p.name, p.path);
            p
        }
    };

    let session = client.create_session(&project.id.to_string()).await?;
    Ok(session.id.to_string())
}

/// One interactive loop for both modes.
///
/// `agentic` decides which project the session belongs to, and that is what
/// gives the runtime a workspace: a real directory turns on the tool loop, the
/// internal free-chat container leaves it off.
async fn converse(
    client: &LochorClient,
    resume: Option<String>,
    agent: Option<String>,
    agentic: bool,
) -> anyhow::Result<()> {
    use futures::StreamExt;
    use lochor_agent_runtime::reasoning::{peek, split_reasoning};

    let session_id = resolve_session(client, resume, agentic).await?;
    if let Some(a) = &agent {
        println!("[agent : {a}]");
    }
    if agentic {
        println!(
            "Agent Lochor — il peut lire et modifier les fichiers de ce dossier.\n\
             /exit pour quitter, /think pour voir le raisonnement\n"
        );
    } else {
        println!(
            "Conversation simple — aucun accès à vos fichiers.\n\
             /exit pour quitter, /think pour voir le raisonnement\n"
        );
    }

    // Off by default: on a reasoning model the scratchpad is several times the
    // length of the reply, and it is not what the user asked for.
    let mut show_reasoning = false;
    let mut lines = std::io::BufReader::new(std::io::stdin()).lines();

    loop {
        print!("> ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let Some(Ok(line)) = lines.next() else { break };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        match line.as_str() {
            "/exit" | "/quit" => break,
            "/think" => {
                show_reasoning = !show_reasoning;
                println!(
                    "raisonnement : {}\n",
                    if show_reasoning { "affiché" } else { "masqué" }
                );
                continue;
            }
            _ => {}
        }

        let mut stream = client.send_message(&session_id, &line).await?;
        // Accumulate rather than printing tokens directly: a `<think>` block
        // only reveals itself once its tag is complete, so raw passthrough
        // would leak the opening tag before we could suppress it.
        let mut full = String::new();
        let mut printed = 0usize;
        let mut thinking_shown = false;

        while let Some(ev) = stream.next().await {
            match ev? {
                lochor_events::StreamEvent::Token { text } => {
                    full.push_str(&text);
                    let split = split_reasoning(&full);

                    if split.in_progress && !show_reasoning {
                        // One rewritten status line, so the scratchpad shows
                        // activity without scrolling the terminal away.
                        let p = peek(&split.reasoning, 68);
                        print!("\r\x1b[2K  réflexion… {p}");
                        std::io::Write::flush(&mut std::io::stdout())?;
                        thinking_shown = true;
                        continue;
                    }
                    if thinking_shown {
                        print!("\r\x1b[2K");
                        thinking_shown = false;
                        if show_reasoning && !split.reasoning.is_empty() {
                            println!("[raisonnement]\n{}\n", split.reasoning);
                        }
                    }
                    // Emit only what is new, so re-splitting never reprints.
                    if split.answer.len() > printed {
                        print!("{}", &split.answer[printed..]);
                        std::io::Write::flush(&mut std::io::stdout())?;
                        printed = split.answer.len();
                    }
                }
                lochor_events::StreamEvent::ToolCall { tool, .. } => {
                    if thinking_shown {
                        print!("\r\x1b[2K");
                        thinking_shown = false;
                    }
                    println!("\n  · {tool}");
                }
                lochor_events::StreamEvent::MessageEnd { .. } => {
                    if thinking_shown {
                        print!("\r\x1b[2K");
                    }
                    println!("\n");
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Read the certificate this server presents, so clients can pin it.
///
/// Without the fingerprint a client facing a self-signed certificate has only
/// two options: refuse every connection, or accept any certificate at all. The
/// second is what makes an interception trivial.
fn certificate_fingerprint() -> Option<String> {
    let cfg = lochor_config::load(None).ok()?;
    let data_dir = cfg
        .daemon
        .data_dir
        .clone()
        .unwrap_or_else(lochor_config::default_data_dir);
    let pem = std::fs::read_to_string(data_dir.join("tls").join("daemon-cert.pem")).ok()?;
    lochor_config::provision::certificate_fingerprint(&pem)
}

async fn provision_cmd(
    url: String,
    org: Option<String>,
    note: Option<String>,
    out: Option<String>,
) -> anyhow::Result<()> {
    let cfg = lochor_config::load(None)?;
    let server_url = lochor_config::provision::normalise_url(&url, cfg.daemon.port)
        .map_err(|e| anyhow::anyhow!(e))?;

    let fingerprint = certificate_fingerprint();
    if fingerprint.is_none() {
        eprintln!(
            "Aucun certificat trouvé sur cette machine : le fichier est écrit sans empreinte.\n\
             Générez-la en démarrant le serveur une fois, puis relancez cette commande — sans \
             elle les clients ne peuvent pas distinguer votre serveur d'un autre."
        );
    }

    // The authority travels with the file. It is public — it vouches for
    // others rather than authorising anything — and without it a phone cannot
    // check that a scanned pairing code came from this deployment.
    let data_dir = cfg
        .daemon
        .data_dir
        .clone()
        .unwrap_or_else(lochor_config::default_data_dir);
    let authority_pem = lochor_config::mtls::authority(&data_dir)
        .map(|a| a.cert_pem)
        .ok();
    if authority_pem.is_none() {
        eprintln!(
            "Autorité locale introuvable : le fichier est écrit sans elle. Les téléphones 
             ne pourront pas vérifier les codes du mode voyage."
        );
    }

    let p = lochor_config::provision::Provisioning {
        server_url: server_url.clone(),
        organisation: org.unwrap_or_default(),
        certificate_fingerprint: fingerprint,
        authority_pem,
        note: note.unwrap_or_default(),
    };
    let dir = std::path::PathBuf::from(out.unwrap_or_else(|| ".".into()));
    let path = lochor_config::provision::write(&dir, &p).map_err(|e| anyhow::anyhow!(e))?;

    println!("Configuration écrite : {}", path.display());
    println!("Serveur : {server_url}");
    println!();
    println!("À distribuer aux postes, avec l'installeur :");
    println!("  • placez ce fichier à côté du .msi, ou");
    println!("  • déposez-le dans C:\\ProgramData\\Lochor\\ sur chaque poste");
    println!();
    println!("Les employés n'auront qu'à ouvrir l'application et saisir leurs identifiants.");
    Ok(())
}

/// Open the same database the daemon uses.
async fn open_users() -> anyhow::Result<lochor_storage::users::UserRepo> {
    let cfg = lochor_config::load(None)?;
    let data_dir = cfg
        .daemon
        .data_dir
        .clone()
        .unwrap_or_else(lochor_config::default_data_dir);
    let pool = lochor_storage::open(&data_dir.join("lochor.db")).await?;
    Ok(lochor_storage::users::UserRepo::new(pool))
}

/// MCP servers.
///
/// Reading and writing the registry happens locally, against the same
/// `mcp.json` the application uses, so it works on a machine where the daemon
/// is not running. Only start and stop need the daemon: a server is a child
/// process, and a command that exits would take it with it.
async fn mcp_cmd(action: McpCmd, client: &LochorClient) -> anyhow::Result<()> {
    use lochor_mcp::{build_client, McpConfig, McpServerEntry, Transport};

    let path = lochor_mcp::config_path(lochor_shared_types::ExtensionScope::Global, None);
    let load = || McpConfig::load(&path).unwrap_or_default();

    match action {
        McpCmd::List => {
            let cfg = load();
            if cfg.mcp_servers.is_empty() {
                println!("Aucun serveur MCP enregistré.");
                println!("  lochor mcp add <nom> \"npx -y @modelcontextprotocol/server-filesystem /chemin\"");
                return Ok(());
            }
            let mut names: Vec<_> = cfg.mcp_servers.keys().cloned().collect();
            names.sort();
            println!("{:<20} {:<8} {}", "NOM", "AUTO", "CIBLE");
            for n in names {
                let e = &cfg.mcp_servers[&n];
                let target = match e.transport {
                    Transport::Stdio => {
                        let mut p = vec![e.command.clone().unwrap_or_default()];
                        p.extend(e.args.clone());
                        p.join(" ")
                    }
                    Transport::Http => e.url.clone().unwrap_or_default(),
                };
                println!("{:<20} {:<8} {}", n, if e.auto_start { "oui" } else { "non" }, target);
            }
            println!("\nFichier : {}", path.display());
            Ok(())
        }

        McpCmd::Add { name, target, auto } => {
            anyhow::ensure!(
                name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "le nom préfixe les outils vus par le modèle : lettres, chiffres, « - » et « _ » uniquement"
            );
            let mut cfg = load();
            anyhow::ensure!(!cfg.mcp_servers.contains_key(&name), "« {name} » existe déjà");

            let entry = if target.starts_with("http://") || target.starts_with("https://") {
                McpServerEntry {
                    command: None,
                    args: Vec::new(),
                    env: Default::default(),
                    url: Some(target.clone()),
                    headers: Default::default(),
                    transport: Transport::Http,
                    auto_start: auto,
                    scope: None,
                    owner: None,
                }
            } else {
                let mut parts = target.split_whitespace().map(str::to_string);
                let command = parts.next().unwrap_or_default();
                anyhow::ensure!(!command.is_empty(), "commande vide");
                McpServerEntry {
                    command: Some(command),
                    args: parts.collect(),
                    env: Default::default(),
                    url: None,
                    headers: Default::default(),
                    transport: Transport::Stdio,
                    auto_start: auto,
                    scope: None,
                    owner: None,
                }
            };
            cfg.mcp_servers.insert(name.clone(), entry);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            cfg.save(&path)?;
            println!("« {name} » enregistré dans {}", path.display());
            println!("Vérifiez-le avec :  lochor mcp test {name}");
            Ok(())
        }

        McpCmd::Remove { name } => {
            let mut cfg = load();
            anyhow::ensure!(cfg.mcp_servers.remove(&name).is_some(), "« {name} » n'est pas enregistré");
            cfg.save(&path)?;
            println!("« {name} » retiré.");
            Ok(())
        }

        McpCmd::Test { name } | McpCmd::Discover { name } => {
            let cfg = load();
            let entry = cfg
                .mcp_servers
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("« {name} » n'est pas enregistré"))?;
            let c = build_client(entry);
            let caps = c
                .discover()
                .await
                .map_err(|e| anyhow::anyhow!("{name} n'a pas répondu : {e}"))?;
            let _ = c.shutdown().await;

            if caps.tools.is_empty() {
                println!("{name} répond, mais n'annonce aucun outil.");
            } else {
                println!("{} outil(s) :", caps.tools.len());
                for t in &caps.tools {
                    match &t.description {
                        Some(d) => {
                            let line: String = d.lines().next().unwrap_or_default().chars().take(70).collect();
                            println!("  {:<28} {}", t.name, line);
                        }
                        None => println!("  {}", t.name),
                    }
                }
            }
            if !caps.resources.is_empty() {
                println!("{} ressource(s).", caps.resources.len());
            }
            Ok(())
        }

        McpCmd::Start { name } => {
            client
                .start_mcp(&name)
                .await
                .map_err(|e| anyhow::anyhow!("{e}. Le serveur Lochor doit tourner pour héberger un serveur MCP."))?;
            println!("« {name} » démarré.");
            Ok(())
        }

        McpCmd::Stop { name } => {
            client.stop_mcp(&name).await?;
            println!("« {name} » arrêté.");
            Ok(())
        }
    }
}

/// Travel mode.
///
/// Everything goes through the daemon: the tunnel is a child process that has
/// to outlive this command, which exits in a second.
async fn travel_cmd(action: Option<TravelCmd>, client: &LochorClient) -> anyhow::Result<()> {
    /// Print the code full width, with what it is for above it.
    fn show(link: &str, title: &str, footer: &str) -> anyhow::Result<()> {
        let code = lochor_travel::qr::terminal(link)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        println!();
        println!("  {title}");
        println!();
        for l in code.lines() {
            println!("  {l}");
        }
        println!();
        println!("  {footer}");
        println!();
        Ok(())
    }

    match action.unwrap_or(TravelCmd::Qr) {
        TravelCmd::On { via } => {
            let st = client.set_travel(Some(&via)).await?;
            let link = st
                .get("link")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("le serveur n'a pas renvoyé de lien"))?;
            show(
                link,
                "Scannez ce code avec l'appareil photo du téléphone :",
                "Ce code expire dans 10 minutes. Pour en réafficher un : lochor travel qr",
            )
        }

        TravelCmd::Off => {
            client.set_travel(None).await?;
            println!("Mode voyage désactivé.");
            println!("Sur le téléphone, scannez le code de retour : lochor travel home");
            Ok(())
        }

        TravelCmd::Qr => {
            let st = client.travel_status().await?;
            if !st.get("active").and_then(|v| v.as_bool()).unwrap_or(false) {
                match st.get("blocker").and_then(|v| v.as_str()) {
                    Some(b) => println!("Mode voyage inactif : {b}"),
                    None => {
                        println!("Mode voyage inactif.");
                        println!("Pour l'activer :  lochor travel on --via cloudflare");
                    }
                }
                return Ok(());
            }
            let link = st
                .get("link")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("aucun lien disponible"))?;
            let via = st.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
            show(
                link,
                &format!("Mode voyage actif via {via}. Scannez :"),
                "Ce code expire dans 10 minutes.",
            )
        }

        TravelCmd::Home => {
            let body = client.travel_home().await?;
            let link = body
                .get("link")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("aucun lien disponible"))?;
            show(
                link,
                "Scannez pour revenir au réseau local :",
                "À faire une fois rentré ; le téléphone se reconfigure tout seul.",
            )
        }
    }
}

async fn users_cmd(action: UsersCmd) -> anyhow::Result<()> {
    use lochor_storage::users::Role;
    let repo = open_users().await?;
    match action {
        UsersCmd::List => {
            let users = repo.list().await?;
            if users.is_empty() {
                println!("Aucun compte. Le daemon reste donc limité à un usage local.");
                return Ok(());
            }
            println!("{:<24} {:<8} {}", "NOM", "RÔLE", "ÉTAT");
            for u in users {
                println!(
                    "{:<24} {:<8} {}",
                    u.username,
                    if u.role == Role::Admin { "admin" } else { "membre" },
                    if u.disabled { "désactivé" } else { "actif" }
                );
            }
            Ok(())
        }
        UsersCmd::Add { username, admin } => {
            // Read from stdin rather than an argument: a password on the
            // command line lands in the shell history and in the process list.
            eprint!("Mot de passe pour « {username} » : ");
            use std::io::Write as _;
            std::io::stderr().flush().ok();
            let mut pass = String::new();
            std::io::stdin().read_line(&mut pass)?;
            let pass = pass.trim_end_matches(['\n', '\r']);

            let role = if admin { Role::Admin } else { Role::Member };
            let u = repo.create(&username, pass, role).await?;
            println!(
                "Compte « {} » créé ({}).",
                u.username,
                if admin { "administrateur" } else { "membre" }
            );
            Ok(())
        }
        UsersCmd::Cert { ref username, days } => {
            let cfg = lochor_config::load(None)?;
            let data_dir = cfg
                .daemon
                .data_dir
                .clone()
                .unwrap_or_else(lochor_config::default_data_dir);
            let cred = lochor_config::mtls::issue_client(&data_dir, username, days)?;
            let ca_path = data_dir.join("tls").join("ca-cert.pem");

            // On a headless server the path is the whole point: nothing will
            // pop up a dialog offering to install this.
            println!("Certificat client émis pour « {username} », valide {days} jours.");
            println!();
            println!("  Certificat + clé : {}", cred.path.display());
            println!("  Autorité         : {}", ca_path.display());
            println!();
            println!("À transmettre au poste de l'utilisateur — c'est un secret :");
            println!("  • le premier fichier prouve son identité au serveur ;");
            println!("  • le second lui permet de vérifier qu'il parle au bon serveur.");
            println!();
            println!("Activez ensuite l'exigence côté serveur :");
            println!("  require_client_cert = true   (ou LOCHOR_REQUIRE_CLIENT_CERT=1)");
            println!("Attention : les clients sans certificat cesseront de se connecter.");
            Ok(())
        }
        UsersCmd::Disable { ref username } | UsersCmd::Enable { ref username } => {
            let want_disabled = matches!(action, UsersCmd::Disable { .. });
            let username = username.clone();
            let users = repo.list().await?;
            let Some(u) = users
                .into_iter()
                .find(|u| u.username.eq_ignore_ascii_case(&username))
            else {
                anyhow::bail!("compte introuvable : {username}");
            };
            repo.set_disabled(u.id, want_disabled).await?;
            println!(
                "Compte « {} » {}.",
                u.username,
                if want_disabled {
                    "désactivé — ses jetons ne fonctionnent plus"
                } else {
                    "réactivé"
                }
            );
            Ok(())
        }
    }
}

fn parse_trust(s: &str) -> anyhow::Result<lochor_shared_types::TrustLevel> {
    Ok(match s.to_lowercase().as_str() {
        "trusted" => lochor_shared_types::TrustLevel::Trusted,
        "untrusted" => lochor_shared_types::TrustLevel::Untrusted,
        "sandbox" => lochor_shared_types::TrustLevel::Sandbox,
        _ => anyhow::bail!("invalid trust: {s} (use trusted|untrusted|sandbox)"),
    })
}

fn parse_engine(s: &str) -> anyhow::Result<lochor_shared_types::ProviderEngine> {
    Ok(match s.to_lowercase().as_str() {
        "ollama" => lochor_shared_types::ProviderEngine::Ollama,
        "llama_cpp" | "llama-cpp" => lochor_shared_types::ProviderEngine::LlamaCpp,
        "lmstudio" | "lm_studio" => lochor_shared_types::ProviderEngine::Lmstudio,
        "vllm" => lochor_shared_types::ProviderEngine::Vllm,
        _ => anyhow::bail!("unknown engine: {s}"),
    })
}

fn parse_scope(s: &str) -> anyhow::Result<lochor_shared_types::ExtensionScope> {
    Ok(match s.to_lowercase().as_str() {
        "global" => lochor_shared_types::ExtensionScope::Global,
        "user" => lochor_shared_types::ExtensionScope::User,
        "workspace" => lochor_shared_types::ExtensionScope::Workspace,
        "session" => lochor_shared_types::ExtensionScope::Session,
        _ => anyhow::bail!("invalid scope: {s}"),
    })
}
