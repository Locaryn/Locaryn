//! Ce que `locaryn-server` sait faire, sans interface : les commandes et
//! l'écran plein terminal passent tous les deux par ici.
//!
//! Presque tout se lit et s'écrit sans le service : les comptes et les clés
//! dans la base, l'API fournisseur et le port dans la configuration. Le
//! service n'est démarré ou arrêté que pour servir — on peut donc préparer un
//! serveur (comptes, clés) avant de l'allumer.

use anyhow::{anyhow, bail, Context, Result};
use locaryn_config::api_fournisseur;
use locaryn_storage::users::{Role, TokenInfo, TokenKind, User, UserRepo};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

// ── Service ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EtatService {
    Arrete,
    /// En écoute. Le pid, quand c'est `locaryn-server` (ou `locaryn daemon
    /// start`) qui l'a démarré.
    EnMarche {
        pid: Option<u32>,
        /// Joignable depuis le réseau (HTTPS), ou seulement depuis cette
        /// machine (HTTP sur 127.0.0.1).
        reseau: bool,
    },
}

impl EtatService {
    /// L'adresse à laquelle le service répond vraiment.
    pub fn adresse(&self, port: u16) -> Option<String> {
        match self {
            EtatService::EnMarche { reseau: true, .. } => {
                Some(format!("https://{}:{port}", adresse_locale()))
            }
            EtatService::EnMarche { reseau: false, .. } => {
                Some(format!("http://127.0.0.1:{port} (cette machine seulement)"))
            }
            EtatService::Arrete => None,
        }
    }
}

pub fn port() -> u16 {
    locaryn_config::load(None)
        .map(|c| c.daemon.port)
        .unwrap_or(7474)
}

fn fichier_pid() -> PathBuf {
    locaryn_config::global_dir().join("daemon.pid")
}

pub fn journal() -> PathBuf {
    locaryn_config::global_dir().join("logs").join("daemon.log")
}

fn pid_enregistre() -> Option<u32> {
    std::fs::read_to_string(fichier_pid())
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// En écoute sur ce port, quelle que soit la façon dont il a été lancé
/// (cette commande, l'application de bureau, un service système).
pub fn ecoute(port: u16) -> bool {
    let adresse: SocketAddr = ([127, 0, 0, 1], port).into();
    TcpStream::connect_timeout(&adresse, Duration::from_millis(300)).is_ok()
}

pub fn etat_service() -> EtatService {
    let port = port();
    if !ecoute(port) {
        return EtatService::Arrete;
    }
    let reseau = adresse_locale()
        .parse::<std::net::IpAddr>()
        .ok()
        .filter(|ip| !ip.is_loopback())
        .is_some_and(|ip| {
            TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_millis(300))
                .is_ok()
        });
    EtatService::EnMarche {
        pid: pid_enregistre(),
        reseau,
    }
}

fn binaire_du_service() -> Option<PathBuf> {
    let nom = if cfg!(windows) {
        "locaryn-daemon.exe"
    } else {
        "locaryn-daemon"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidat in [dir.join(nom), dir.join("..").join(nom)] {
                if candidat.is_file() {
                    return Some(candidat);
                }
            }
        }
    }
    locaryn_config::program_exists(nom).then(|| PathBuf::from(locaryn_config::resolve_program(nom)))
}

/// Démarrer le service en arrière-plan, ouvert au réseau (sauf `local`), et
/// n'annoncer le succès qu'une fois qu'il écoute vraiment.
pub async fn demarrer(local: bool) -> Result<String> {
    let port = port();
    if ecoute(port) {
        return Ok(format!("Le service est déjà en marche sur le port {port}."));
    }
    if !local && comptes().await?.is_empty() {
        bail!(
            "Aucun compte : un serveur ouvert au réseau sans compte serait ouvert à tous.\n\
             Créez d'abord un administrateur : locaryn-server users add <nom> --admin"
        );
    }
    let bin = binaire_du_service().ok_or_else(|| {
        anyhow!("`locaryn-daemon` est introuvable à côté de locaryn-server et dans le PATH.")
    })?;
    let chemin = journal();
    if let Some(dossier) = chemin.parent() {
        std::fs::create_dir_all(dossier)?;
    }
    let sortie = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&chemin)?;
    let erreurs = sortie.try_clone()?;
    let mut commande = std::process::Command::new(&bin);
    locaryn_config::hide_console(&mut commande);
    commande
        .env(
            "LOCARYN_DAEMON_BIND",
            if local { "127.0.0.1" } else { "0.0.0.0" },
        )
        .stdin(std::process::Stdio::null())
        .stdout(sortie)
        .stderr(erreurs);
    let enfant = commande
        .spawn()
        .with_context(|| format!("impossible de lancer {}", bin.display()))?;
    let pid = enfant.id();
    let _ = std::fs::write(fichier_pid(), pid.to_string());
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if ecoute(port) {
            return Ok(format!(
                "Service démarré (pid {pid}) — {}",
                if local {
                    format!("http://127.0.0.1:{port}")
                } else {
                    format!("https://{}:{port}", adresse_locale())
                }
            ));
        }
    }
    bail!(
        "Le service a été lancé (pid {pid}) mais n'écoute pas sur le port {port}.\n\
         Fin du journal ({}) :\n{}",
        chemin.display(),
        fin_du_journal(15)
    )
}

/// Le processus qui écoute sur `port`, quand on ne l'a pas démarré nous-mêmes
/// (application de bureau, service système).
fn pid_a_l_ecoute(port: u16) -> Option<u32> {
    if cfg!(windows) {
        let mut c = std::process::Command::new("netstat");
        locaryn_config::hide_console(&mut c);
        let sortie = c.args(["-ano", "-p", "tcp"]).output().ok()?;
        let texte = String::from_utf8_lossy(&sortie.stdout).into_owned();
        texte.lines().find_map(|l| {
            let champs: Vec<&str> = l.split_whitespace().collect();
            (champs.len() >= 5
                && champs[1].ends_with(&format!(":{port}"))
                && champs[3].eq_ignore_ascii_case("LISTENING"))
            .then(|| champs[4].parse().ok())
            .flatten()
        })
    } else {
        let sortie = std::process::Command::new("lsof")
            .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&sortie.stdout)
            .lines()
            .next()?
            .trim()
            .parse()
            .ok()
    }
}

fn tuer(pid: u32) -> bool {
    let mut c = if cfg!(windows) {
        let mut c = std::process::Command::new("taskkill");
        c.args(["/PID", &pid.to_string(), "/T", "/F"]);
        c
    } else {
        let mut c = std::process::Command::new("kill");
        c.args(["-TERM", &pid.to_string()]);
        c
    };
    locaryn_config::hide_console(&mut c);
    c.output().map(|o| o.status.success()).unwrap_or(false)
}

pub async fn arreter() -> Result<String> {
    let port = port();
    let pid = pid_enregistre().or_else(|| pid_a_l_ecoute(port));
    let _ = std::fs::remove_file(fichier_pid());
    let Some(pid) = pid else {
        return Ok(if ecoute(port) {
            format!("Le port {port} est occupé, mais par quel processus : impossible de le savoir.")
        } else {
            "Le service n'est pas en marche.".to_string()
        });
    };
    if !tuer(pid) && !ecoute(port) {
        return Ok("Le service n'était plus en marche.".to_string());
    }
    for _ in 0..40 {
        if !ecoute(port) {
            return Ok(format!("Service arrêté (pid {pid})."));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    bail!("Le processus {pid} ne s'est pas arrêté.")
}

pub fn fin_du_journal(lignes: usize) -> String {
    let texte = std::fs::read_to_string(journal()).unwrap_or_default();
    let gardees: Vec<&str> = texte.lines().rev().take(lignes).collect();
    if gardees.is_empty() {
        return "(journal vide)".to_string();
    }
    gardees.into_iter().rev().collect::<Vec<_>>().join("\n")
}

/// L'adresse de la machine sur le réseau local, pour les URL à donner.
pub fn adresse_locale() -> String {
    // Une « connexion » UDP ne part pas sur le réseau : elle choisit seulement
    // l'interface qui mènerait dehors, et donc son adresse.
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("192.0.2.1:9")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

// ── Comptes ────────────────────────────────────────────────────────────────

async fn depot() -> Result<UserRepo> {
    let db = locaryn_config::default_data_dir().join("locaryn.db");
    if let Some(dossier) = db.parent() {
        std::fs::create_dir_all(dossier)?;
    }
    let pool = locaryn_storage::open(&db)
        .await
        .with_context(|| format!("base illisible : {}", db.display()))?;
    Ok(UserRepo::new(pool))
}

pub async fn comptes() -> Result<Vec<User>> {
    Ok(depot().await?.list().await?)
}

async fn compte(nom: &str) -> Result<User> {
    comptes()
        .await?
        .into_iter()
        .find(|u| u.username.eq_ignore_ascii_case(nom))
        .ok_or_else(|| anyhow!("aucun compte « {nom} »"))
}

pub async fn creer_compte(nom: &str, mot_de_passe: &str, admin: bool) -> Result<String> {
    let repo = depot().await?;
    // Le premier compte est administrateur : un serveur en a besoin.
    let premier = repo.count().await? == 0;
    let est_admin = admin || premier;
    let role = if est_admin { Role::Admin } else { Role::Member };
    repo.create(nom.trim(), mot_de_passe, role).await?;
    Ok(format!(
        "Compte « {} » créé ({}).",
        nom.trim(),
        if est_admin {
            "administrateur"
        } else {
            "membre"
        }
    ))
}

pub async fn supprimer_compte(nom: &str) -> Result<String> {
    let u = compte(nom).await?;
    let repo = depot().await?;
    if u.role == Role::Admin
        && !repo
            .list()
            .await?
            .iter()
            .any(|x| x.role == Role::Admin && !x.disabled && x.id != u.id)
    {
        bail!(
            "« {} » est le dernier administrateur : nommez-en un autre d'abord.",
            u.username
        );
    }
    repo.delete(u.id).await?;
    Ok(format!("Compte « {} » supprimé.", u.username))
}

pub async fn activer_compte(nom: &str, actif: bool) -> Result<String> {
    let u = compte(nom).await?;
    depot().await?.set_disabled(u.id, !actif).await?;
    Ok(format!(
        "Compte « {} » {}.",
        u.username,
        if actif {
            "réactivé"
        } else {
            "désactivé (ses jetons ne marchent plus)"
        }
    ))
}

pub async fn changer_role(nom: &str, admin: bool) -> Result<String> {
    let u = compte(nom).await?;
    depot()
        .await?
        .set_role(u.id, if admin { Role::Admin } else { Role::Member })
        .await?;
    Ok(format!(
        "« {} » est maintenant {}.",
        u.username,
        if admin { "administrateur" } else { "membre" }
    ))
}

pub async fn changer_mot_de_passe(nom: &str, mot_de_passe: &str) -> Result<String> {
    let u = compte(nom).await?;
    depot().await?.reset_password(u.id, mot_de_passe).await?;
    Ok(format!("Mot de passe de « {} » remplacé.", u.username))
}

// ── API fournisseur et clés ─────────────────────────────────────────────────

pub fn api_active() -> bool {
    api_fournisseur::active()
}

pub fn regler_api(actif: bool) -> Result<String> {
    api_fournisseur::regler(actif)?;
    Ok(if actif {
        "API fournisseur activée : les applications munies d'une clé peuvent utiliser les modèles."
            .to_string()
    } else {
        "API fournisseur désactivée : les clés d'API sont refusées.".to_string()
    })
}

pub struct AdressesApi {
    pub locale: String,
    pub reseau: String,
}

pub fn adresses_api() -> AdressesApi {
    let p = port();
    AdressesApi {
        locale: format!("http://127.0.0.1:{}/v1", api_fournisseur::port_local(p)),
        reseau: format!("https://{}:{p}/v1", adresse_locale()),
    }
}

/// Le compte qui porte les clés : celui nommé, sinon le premier
/// administrateur.
async fn porteur(nom: Option<&str>) -> Result<User> {
    match nom {
        Some(n) => {
            let u = compte(n).await?;
            if u.role != Role::Admin {
                bail!("« {} » n'est pas administrateur : seul un administrateur porte des clés d'API.", u.username);
            }
            Ok(u)
        }
        None => {
            let repo = depot().await?;
            let id = repo
                .first_admin_id()
                .await
                .ok_or_else(|| anyhow!("aucun compte administrateur"))?;
            repo.list()
                .await?
                .into_iter()
                .find(|u| u.id == id)
                .ok_or_else(|| anyhow!("aucun compte administrateur"))
        }
    }
}

pub struct Cle {
    pub proprietaire: String,
    pub info: TokenInfo,
}

/// Les clés d'API en service, de tous les administrateurs.
pub async fn cles() -> Result<Vec<Cle>> {
    let repo = depot().await?;
    let mut out = Vec::new();
    for u in repo.list().await? {
        for t in repo.list_tokens(u.id).await? {
            if t.kind == TokenKind::Api && t.revoked_at.is_none() {
                out.push(Cle {
                    proprietaire: u.username.clone(),
                    info: t,
                });
            }
        }
    }
    Ok(out)
}

pub struct CleCreee {
    pub id: String,
    pub cle: String,
    pub expire: Option<String>,
}

pub async fn creer_cle(nom: &str, jours: Option<i64>, compte: Option<&str>) -> Result<CleCreee> {
    if !api_active() {
        bail!("L'API fournisseur est désactivée : locaryn-server api on");
    }
    let u = porteur(compte).await?;
    let nom = nom.trim();
    let c = depot()
        .await?
        .issue_api_token(
            u.id,
            (!nom.is_empty()).then_some(nom),
            jours.filter(|j| *j > 0),
        )
        .await?;
    Ok(CleCreee {
        id: c.id.to_string(),
        cle: c.plaintext,
        expire: c.expires_at,
    })
}

/// Révoquer une clé désignée par son identifiant (ou son début) ou par son
/// nom.
pub async fn revoquer_cle(qui: &str) -> Result<String> {
    let q = qui.trim().to_lowercase();
    let toutes = cles().await?;
    let trouvees: Vec<&Cle> = toutes
        .iter()
        .filter(|c| {
            c.info.id.to_string().starts_with(&q)
                || c.info.label.as_deref().map(str::to_lowercase).as_deref() == Some(q.as_str())
        })
        .collect();
    match trouvees.as_slice() {
        [] => bail!("aucune clé « {qui} »"),
        [c] => {
            depot().await?.revoke_token(c.info.id).await?;
            Ok(format!(
                "Clé « {} » révoquée : l'application qui l'utilise perd l'accès.",
                c.info.label.as_deref().unwrap_or(&c.info.hint)
            ))
        }
        _ => bail!("plusieurs clés correspondent à « {qui} » : donnez l'identifiant"),
    }
}

// ── Modèles ────────────────────────────────────────────────────────────────

pub struct Modele {
    pub nom: String,
    pub octets: u64,
}

/// Les modèles installés (fichiers GGUF), projecteurs vision exclus.
pub fn modeles() -> Vec<Modele> {
    fn parcourir(dossier: &Path, racine: &Path, out: &mut Vec<Modele>) {
        let Ok(entrees) = std::fs::read_dir(dossier) else {
            return;
        };
        for e in entrees.flatten() {
            let chemin = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => parcourir(&chemin, racine, out),
                Ok(t) if t.is_file() => {
                    let nom = chemin
                        .strip_prefix(racine)
                        .unwrap_or(&chemin)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let bas = nom.to_lowercase();
                    if bas.ends_with(".gguf") && !bas.contains("mmproj") {
                        out.push(Modele {
                            nom,
                            octets: e.metadata().map(|m| m.len()).unwrap_or(0),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    let racine = locaryn_config::models_dir();
    let mut out = Vec::new();
    parcourir(&racine, &racine, &mut out);
    out.sort_by(|a, b| a.nom.to_lowercase().cmp(&b.nom.to_lowercase()));
    out
}

pub fn go(octets: u64) -> String {
    format!("{:.1} Go", octets as f64 / (1024.0 * 1024.0 * 1024.0))
}

// ── Configuration ──────────────────────────────────────────────────────────

pub fn regler_port(p: u16) -> Result<String> {
    locaryn_config::set_global("daemon", serde_json::json!({ "port": p }))?;
    Ok(format!(
        "Port {p} retenu (API locale sur {}). Redémarrez le service pour l'appliquer.",
        api_fournisseur::port_local(p)
    ))
}

pub fn resume_configuration() -> String {
    let p = port();
    format!(
        "Port du service      {p} (HTTPS sur le réseau)\n\
         API locale           http://127.0.0.1:{}/v1\n\
         Données              {}\n\
         Modèles              {}\n\
         Journal              {}\n\
         Configuration        {}",
        api_fournisseur::port_local(p),
        locaryn_config::default_data_dir().display(),
        locaryn_config::models_dir().display(),
        journal().display(),
        locaryn_config::global_config_path().display(),
    )
}

/// Une date ISO, raccourcie au jour.
pub fn jour(iso: Option<&str>) -> String {
    iso.map(|d| d.get(..10).unwrap_or(d).to_string())
        .unwrap_or_else(|| "—".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn une_date_se_lit_au_jour() {
        assert_eq!(super::jour(Some("2026-10-16T08:12:00+00:00")), "2026-10-16");
        assert_eq!(super::jour(None), "—");
        assert_eq!(super::go(3 * 1024 * 1024 * 1024 / 2), "1.5 Go");
    }
}
