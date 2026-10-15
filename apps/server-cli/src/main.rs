//! `locaryn-server` — gérer un serveur Locaryn depuis le terminal.
//!
//! Sans argument, dans un terminal interactif : l'interface plein écran
//! (service, comptes, API fournisseur, clés, modèles, journal). Avec une
//! commande : la même chose en une ligne, pour un script ou un terminal qui
//! n'affiche pas d'interface — `locaryn-server --help` les liste toutes.
//!
//! La même commande sur Windows, macOS et Linux, x64 comme ARM64 : c'est la
//! seule qu'installe le paquet serveur, et l'application de bureau l'installe
//! aussi, pour qui voudrait un jour se passer de l'interface graphique.

mod serveur;
mod tui;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::io::{BufRead, IsTerminal};

#[derive(Parser, Debug)]
#[command(
    name = "locaryn-server",
    version,
    about = "Gérer un serveur Locaryn : service, comptes, API fournisseur, clés d'API, modèles.",
    long_about = "Gérer un serveur Locaryn depuis le terminal.\n\n\
                  Sans commande, dans un terminal interactif, locaryn-server ouvre son \
                  interface plein écran. Chaque action y existe aussi en commande, pour \
                  les scripts et les terminaux sans interface.",
    after_help = "Exemples :\n  \
                  locaryn-server                          interface plein écran\n  \
                  locaryn-server users add marie --admin  créer un administrateur\n  \
                  locaryn-server start                    démarrer, ouvert au réseau\n  \
                  locaryn-server api on                   ouvrir les modèles aux autres applications\n  \
                  locaryn-server keys create \"Cursor\"     créer une clé d'API\n  \
                  locaryn-server status --json            état lisible par un script"
)]
struct Cli {
    /// Sortie en JSON (status, users list, keys list, models).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// L'état du serveur : service, adresses, comptes, API, clés, modèles.
    Status,
    /// Démarrer le service en arrière-plan.
    Start {
        /// Seulement pour cette machine (127.0.0.1), sans ouvrir le réseau.
        #[arg(long)]
        local: bool,
    },
    /// Arrêter le service.
    Stop,
    /// Redémarrer le service (pour appliquer un changement de port).
    Restart {
        #[arg(long)]
        local: bool,
    },
    /// La fin du journal du service.
    Logs {
        /// Nombre de lignes.
        #[arg(short = 'n', long, default_value_t = 40)]
        lines: usize,
        /// Suivre le journal en continu (Ctrl+C pour sortir).
        #[arg(short, long)]
        follow: bool,
    },
    /// Les comptes qui se connectent au serveur.
    Users {
        #[command(subcommand)]
        action: UsersCmd,
    },
    /// L'API fournisseur : les modèles servis aux autres applications.
    Api {
        #[command(subcommand)]
        action: ApiCmd,
    },
    /// Les clés d'API des applications (Cursor, VS Code, Claude Code…).
    Keys {
        #[command(subcommand)]
        action: KeysCmd,
    },
    /// Les modèles installés.
    Models,
    /// La configuration (dossiers, port) ; `config port <n>` change le port.
    Config {
        #[command(subcommand)]
        action: Option<ConfigCmd>,
    },
    /// Ouvrir l'interface plein écran.
    Tui,
}

#[derive(Subcommand, Debug)]
pub enum UsersCmd {
    /// Lister les comptes.
    List,
    /// Créer un compte (le mot de passe est demandé, ou lu sur l'entrée standard).
    Add {
        name: String,
        /// Droits d'administrateur : gérer les comptes, créer des clés d'API.
        #[arg(long)]
        admin: bool,
    },
    /// Supprimer un compte et ses jetons.
    Remove { name: String },
    /// Désactiver un compte : ses jetons cessent aussitôt de marcher.
    Disable { name: String },
    /// Réactiver un compte.
    Enable { name: String },
    /// Donner les droits d'administrateur (`--remove` pour les retirer).
    Admin {
        name: String,
        #[arg(long)]
        remove: bool,
    },
    /// Remplacer le mot de passe d'un compte.
    Passwd { name: String },
}

#[derive(Subcommand, Debug)]
pub enum ApiCmd {
    /// Allumée ou non, et les adresses à donner aux applications.
    Status,
    /// Allumer l'API fournisseur.
    On,
    /// L'éteindre : les clés d'API sont refusées.
    Off,
}

#[derive(Subcommand, Debug)]
pub enum KeysCmd {
    /// Lister les clés en service.
    List,
    /// Créer une clé (affichée une seule fois).
    Create {
        /// Le nom de l'application qui l'utilisera.
        name: String,
        /// Validité en jours (sans : jamais d'expiration).
        #[arg(long)]
        days: Option<i64>,
        /// Le compte administrateur qui la porte (par défaut le premier).
        #[arg(long)]
        user: Option<String>,
    },
    /// Révoquer une clé, par son identifiant (ou son début) ou son nom.
    Revoke { key: String },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Changer le port du service (l'API locale prend le suivant).
    Port { port: u16 },
}

/// Lire un mot de passe : masqué dans un terminal, une ligne sinon (script).
fn lire_mot_de_passe(nom: &str) -> Result<String> {
    if std::io::stdin().is_terminal() {
        let a = rpassword::prompt_password(format!("Mot de passe de « {nom} » : "))?;
        let b = rpassword::prompt_password("Le même, pour confirmer : ")?;
        if a != b {
            bail!("les deux mots de passe diffèrent");
        }
        Ok(a)
    } else {
        let mut ligne = String::new();
        std::io::stdin().lock().read_line(&mut ligne)?;
        Ok(ligne.trim_end_matches(['\r', '\n']).to_string())
    }
}

/// Exécuter une commande et rendre ce qu'elle a à dire. `mot_de_passe` sert
/// à l'interface plein écran, qui le demande elle-même.
pub async fn executer(cmd: Cmd, json: bool, mot_de_passe: Option<String>) -> Result<String> {
    use serveur as s;
    Ok(match cmd {
        Cmd::Status => statut(json).await?,
        Cmd::Start { local } => s::demarrer(local).await?,
        Cmd::Stop => s::arreter().await?,
        Cmd::Restart { local } => {
            let a = s::arreter().await?;
            format!("{a}\n{}", s::demarrer(local).await?)
        }
        Cmd::Logs { lines, .. } => s::fin_du_journal(lines),
        Cmd::Users { action } => match action {
            UsersCmd::List => {
                let comptes = s::comptes().await?;
                if json {
                    serde_json::to_string_pretty(
                        &comptes
                            .iter()
                            .map(|u| {
                                serde_json::json!({
                                    "name": u.username,
                                    "admin": u.role == locaryn_storage::users::Role::Admin,
                                    "disabled": u.disabled,
                                })
                            })
                            .collect::<Vec<_>>(),
                    )?
                } else if comptes.is_empty() {
                    "Aucun compte. Créez un administrateur : locaryn-server users add <nom> --admin"
                        .to_string()
                } else {
                    comptes
                        .iter()
                        .map(|u| {
                            format!(
                                "{:<24} {:<15} {}",
                                u.username,
                                if u.role == locaryn_storage::users::Role::Admin {
                                    "administrateur"
                                } else {
                                    "membre"
                                },
                                if u.disabled { "désactivé" } else { "actif" }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            UsersCmd::Add { name, admin } => {
                let mdp = match mot_de_passe {
                    Some(m) => m,
                    None => lire_mot_de_passe(&name)?,
                };
                s::creer_compte(&name, &mdp, admin).await?
            }
            UsersCmd::Remove { name } => s::supprimer_compte(&name).await?,
            UsersCmd::Disable { name } => s::activer_compte(&name, false).await?,
            UsersCmd::Enable { name } => s::activer_compte(&name, true).await?,
            UsersCmd::Admin { name, remove } => s::changer_role(&name, !remove).await?,
            UsersCmd::Passwd { name } => {
                let mdp = match mot_de_passe {
                    Some(m) => m,
                    None => lire_mot_de_passe(&name)?,
                };
                s::changer_mot_de_passe(&name, &mdp).await?
            }
        },
        Cmd::Api { action } => match action {
            ApiCmd::On => s::regler_api(true)?,
            ApiCmd::Off => s::regler_api(false)?,
            ApiCmd::Status => {
                let a = s::adresses_api();
                format!(
                    "API fournisseur {}\n\
                     Sur cette machine   {}\n\
                     Depuis le réseau    {}\n\n\
                     OpenAI (Cursor, VS Code, OpenCode) : adresse de base ci-dessus, clé d'API.\n\
                     Anthropic (Claude Code) : ANTHROPIC_BASE_URL={} et ANTHROPIC_AUTH_TOKEN=<clé>.",
                    if s::api_active() {
                        "activée"
                    } else {
                        "désactivée (locaryn-server api on)"
                    },
                    a.locale,
                    a.reseau,
                    a.locale.trim_end_matches("/v1"),
                )
            }
        },
        Cmd::Keys { action } => match action {
            KeysCmd::List => {
                let cles = s::cles().await?;
                if json {
                    serde_json::to_string_pretty(
                        &cles
                            .iter()
                            .map(|c| {
                                serde_json::json!({
                                    "id": c.info.id.to_string(),
                                    "name": c.info.label,
                                    "owner": c.proprietaire,
                                    "created_at": c.info.created_at,
                                    "last_used_at": c.info.last_used_at,
                                    "expires_at": c.info.expires_at,
                                })
                            })
                            .collect::<Vec<_>>(),
                    )?
                } else if cles.is_empty() {
                    "Aucune clé d'API. Créez-en une : locaryn-server keys create \"<application>\""
                        .to_string()
                } else {
                    cles.iter()
                        .map(|c| {
                            format!(
                                "{}  {:<26} {:<12} utilisée {}  expire {}",
                                &c.info.id.to_string()[..8],
                                c.info.label.as_deref().unwrap_or("(sans nom)"),
                                c.proprietaire,
                                s::jour(c.info.last_used_at.as_deref()),
                                s::jour(c.info.expires_at.as_deref()),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            KeysCmd::Create { name, days, user } => {
                let c = s::creer_cle(&name, days, user.as_deref()).await?;
                if json {
                    serde_json::to_string_pretty(&serde_json::json!({
                        "id": c.id, "key": c.cle, "expires_at": c.expire,
                    }))?
                } else {
                    format!(
                        "Clé créée pour « {name} » — copiez-la maintenant, elle ne sera plus affichée :\n\n  {}\n",
                        c.cle
                    )
                }
            }
            KeysCmd::Revoke { key } => s::revoquer_cle(&key).await?,
        },
        Cmd::Models => {
            let m = s::modeles();
            if json {
                serde_json::to_string_pretty(
                    &m.iter()
                        .map(|x| serde_json::json!({ "name": x.nom, "bytes": x.octets }))
                        .collect::<Vec<_>>(),
                )?
            } else if m.is_empty() {
                format!(
                    "Aucun modèle dans {}.",
                    locaryn_config::models_dir().display()
                )
            } else {
                m.iter()
                    .map(|x| format!("{:>8}  {}", s::go(x.octets), x.nom))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        Cmd::Config { action } => match action {
            None => s::resume_configuration(),
            Some(ConfigCmd::Port { port }) => s::regler_port(port)?,
        },
        Cmd::Tui => String::new(),
    })
}

async fn statut(json: bool) -> Result<String> {
    use serveur as s;
    let etat = s::etat_service();
    let comptes = s::comptes().await.unwrap_or_default();
    let cles = s::cles().await.map(|c| c.len()).unwrap_or(0);
    let modeles = s::modeles();
    let a = s::adresses_api();
    let port = s::port();
    let en_marche = matches!(etat, s::EtatService::EnMarche { .. });
    if json {
        return Ok(serde_json::to_string_pretty(&serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "running": en_marche,
            "pid": match etat { s::EtatService::EnMarche { pid, .. } => pid, _ => None },
            "network": matches!(etat, s::EtatService::EnMarche { reseau: true, .. }),
            "port": port,
            "url": format!("https://{}:{port}", s::adresse_locale()),
            "accounts": comptes.len(),
            "provider_api": { "enabled": s::api_active(), "local_url": a.locale, "network_url": a.reseau },
            "api_keys": cles,
            "models": modeles.len(),
        }))?);
    }
    let admins = comptes
        .iter()
        .filter(|u| u.role == locaryn_storage::users::Role::Admin)
        .count();
    Ok(format!(
        "Locaryn Server {}\n\
         Service           {}\n\
         Comptes           {} ({} administrateur{})\n\
         API fournisseur   {}\n\
         Clés d'API        {}\n\
         Modèles           {}",
        env!("CARGO_PKG_VERSION"),
        match etat {
            s::EtatService::EnMarche { pid, .. } => format!(
                "en marche — {}{}",
                etat.adresse(port).unwrap_or_default(),
                pid.map(|p| format!("  (pid {p})")).unwrap_or_default()
            ),
            s::EtatService::Arrete => "arrêté (locaryn-server start)".to_string(),
        },
        comptes.len(),
        admins,
        if admins > 1 { "s" } else { "" },
        if s::api_active() {
            format!("activée — {}", a.locale)
        } else {
            "désactivée".to_string()
        },
        cles,
        modeles.len(),
    ))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let resultat = match cli.cmd {
        // Sans commande : l'interface, si le terminal en affiche une.
        None if std::io::stdout().is_terminal() && std::io::stdin().is_terminal() => {
            tui::lancer().await
        }
        None => statut(cli.json).await.map(|t| println!("{t}")),
        Some(Cmd::Tui) => tui::lancer().await,
        Some(Cmd::Logs {
            lines,
            follow: true,
        }) => suivre_le_journal(lines).await,
        Some(cmd) => executer(cmd, cli.json, None)
            .await
            .map(|t| println!("{}", t.trim_end())),
    };
    if let Err(e) = resultat {
        eprintln!("Erreur : {e:#}");
        std::process::exit(1);
    }
}

async fn suivre_le_journal(lignes: usize) -> Result<()> {
    println!("{}", serveur::fin_du_journal(lignes));
    let chemin = serveur::journal();
    let mut deja = std::fs::metadata(&chemin).map(|m| m.len()).unwrap_or(0);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let taille = std::fs::metadata(&chemin).map(|m| m.len()).unwrap_or(0);
        if taille < deja {
            deja = 0;
        }
        if taille > deja {
            use std::io::{Read, Seek, SeekFrom};
            let mut f = std::fs::File::open(&chemin)?;
            f.seek(SeekFrom::Start(deja))?;
            let mut nouveau = String::new();
            f.read_to_string(&mut nouveau)?;
            print!("{nouveau}");
            deja = taille;
        }
    }
}
