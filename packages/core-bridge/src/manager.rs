//! Supervision des noyaux alternatifs : processus, jetons, statut, skills.
//!
//! Partagé entre le desktop (installation, UI, choix de session) et le daemon
//! (supervision des processus, sessions CLI) — décision D4 du document 14 :
//! **un seul superviseur de processus**, pas deux logiques qui divergent.
//!
//! Un noyau est une extension dont le `plugin.json` porte une section `core`.
//! Locaryn ne réimplémente rien : il lance le processus du noyau (lifecycle),
//! attend sa sonde de santé, lui passe un jeton généré localement, et laisse
//! `send_message` router la session vers son API OpenAI-compatible.
//!
//! Règles de sécurité : URL loopback uniquement (refusée sinon), jeton
//! CSPRNG stocké dans le dossier de données de l'application, commandes de
//! skills exécutées sans shell et seulement avec la permission `shell` de
//! l'extension.

use crate::session::SessionStore;
use crate::CoreAgent;
use locaryn_extensions::manifest::{CoreHealth, CoreManifest};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

// ============================================================================
// Types
// ============================================================================

/// État visible d'un noyau, pour la carte des réglages et la CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreState {
    /// Pas de processus lancé par Locaryn.
    Stopped,
    /// Processus vivant et sonde de santé verte.
    Running,
    /// Le manifeste ne déclare pas de commande de lancement (mode
    /// « existing » : le noyau est installé à la main). Locaryn ne pilote pas
    /// le processus, mais peut encore joindre l'API.
    External,
    /// Le processus est vivant mais la sonde ne répond pas.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoreStatus {
    pub id: String,
    pub state: CoreState,
    pub driver: String,
    pub api_url: String,
    pub error: Option<String>,
}

/// Un skill de l'index déclaré par le noyau.
#[derive(Debug, Clone, Serialize)]
pub struct CoreSkillEntry {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub verified: bool,
}

/// Un processus de noyau supervisé par Locaryn.
struct RunningCore {
    child: Child,
    token: String,
    health: Option<CoreHealth>,
}

/// État des processus de noyaux, par id d'extension, et mappage des
/// sessions confiées (sérialisation par session + clés noyau stables).
pub struct CoreManager {
    running: tokio::sync::Mutex<HashMap<Uuid, RunningCore>>,
    /// Sérialisation par session (D3) + clés noyau stables (D8), partagée
    /// avec le pont : un run par session à la fois, clé `locaryn-{uuid}`.
    pub sessions: Arc<SessionStore>,
}

impl CoreManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            running: tokio::sync::Mutex::new(HashMap::new()),
            sessions: SessionStore::new(),
        })
    }
}

/// Ce dont le superviseur a besoin de l'hôte (desktop ou daemon) pour
/// travailler : où vivent les données, un client HTTP, et la façon de lire le
/// manifeste d'une extension. Le reste — processus, jetons, statut — est
/// commun et vit ici, une seule fois.
#[async_trait::async_trait]
pub trait CoreHost: Send + Sync {
    /// Dossier de données de l'application (jetons de noyaux).
    fn data_dir(&self) -> &Path;
    /// Client HTTP partagé (sondes de santé, pont).
    fn http(&self) -> &reqwest::Client;
    /// Manifeste `core` + dossier racine d'une extension, par id.
    async fn core_manifest(&self, id: Uuid) -> Result<(CoreManifest, PathBuf), String>;
    /// La permission `shell` a-t-elle été accordée à cette extension ?
    async fn shell_granted(&self, id: Uuid) -> Result<bool, String>;
}

fn loopback_only(url: &str) -> Result<(), String> {
    // La règle est la même pour un noyau et pour un moteur : elle vit donc
    // une seule fois, dans le manifeste qui les décrit tous les deux.
    if locaryn_extensions::manifest::is_loopback_url(url) {
        Ok(())
    } else {
        Err(format!(
            "URL du noyau non-loopback refusée : {url} — un noyau ne se joint qu'en local."
        ))
    }
}

// ============================================================================
// Jetons
// ============================================================================

fn tokens_path(data_dir: &Path) -> PathBuf {
    data_dir.join("core-tokens.json")
}

fn load_tokens(data_dir: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(tokens_path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_tokens(data_dir: &Path, tokens: &HashMap<String, String>) {
    let path = tokens_path(data_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(tokens) {
        let _ = std::fs::write(path, raw);
    }
}

/// Jeton stable par noyau : généré une fois, réutilisé entre les sessions.
/// Un UUID simple (32 hex) suffit comme secret local ; il ne sort jamais de
/// la machine et n'est pas envoyé ailleurs que sur loopback.
fn token_for(host: &dyn CoreHost, id: Uuid) -> String {
    let mut tokens = load_tokens(host.data_dir());
    let token = tokens
        .entry(id.to_string())
        .or_insert_with(|| Uuid::new_v4().simple().to_string())
        .clone();
    save_tokens(host.data_dir(), &tokens);
    token
}

fn interpolate(s: &str, port: u16, token: &str) -> String {
    s.replace("{{port}}", &port.to_string())
        .replace("{{token}}", token)
}

// ============================================================================
// Statut
// ============================================================================

pub async fn status(
    manager: &CoreManager,
    host: &dyn CoreHost,
    id: Uuid,
) -> Result<CoreStatus, String> {
    let (manifest, _) = host.core_manifest(id).await?;
    loopback_only(&manifest.api_url)?;

    let running = manager.running.lock().await;
    let Some(rc) = running.get(&id) else {
        return Ok(CoreStatus {
            id: id.to_string(),
            state: if manifest.lifecycle.start.is_empty() {
                CoreState::External
            } else {
                CoreState::Stopped
            },
            driver: manifest.driver.clone(),
            api_url: manifest.api_url.clone(),
            error: None,
        });
    };

    // Le processus est vivant (Locaryn l'a lancé). La sonde décide de la
    // couleur : un noyau qui refuse de répondre est en erreur, pas en marche.
    let healthy = match &rc.health {
        Some(h) if !h.url.is_empty() => {
            let url = interpolate(&h.url, manifest.port, &rc.token);
            let probe = host.http().get(&url).send().await;
            matches!(probe, Ok(r) if r.status().is_success())
        }
        _ => true, // pas de sonde déclarée : vivant = en marche
    };

    Ok(CoreStatus {
        id: id.to_string(),
        state: if healthy {
            CoreState::Running
        } else {
            CoreState::Error
        },
        driver: manifest.driver.clone(),
        api_url: manifest.api_url.clone(),
        error: (!healthy).then(|| {
            rc.health
                .as_ref()
                .map(|h| format!("la sonde {} ne répond pas", h.url))
                .unwrap_or_else(|| "le processus ne répond pas".into())
        }),
    })
}

// ============================================================================
// Démarrer / arrêter
// ============================================================================

pub async fn start(
    manager: &CoreManager,
    host: &dyn CoreHost,
    id: Uuid,
) -> Result<CoreStatus, String> {
    let (manifest, _) = host.core_manifest(id).await?;
    loopback_only(&manifest.api_url)?;

    {
        let running = manager.running.lock().await;
        if running.contains_key(&id) {
            return status(manager, host, id).await;
        }
    }

    if manifest.lifecycle.start.is_empty() {
        // Mode « existing » : Locaryn ne lance pas le processus. Le statut le
        // dit, et l'utilisateur démarre son noyau lui-même.
        return Ok(CoreStatus {
            id: id.to_string(),
            state: CoreState::External,
            driver: manifest.driver.clone(),
            api_url: manifest.api_url.clone(),
            error: Some(
                "Aucune commande de lancement déclarée : démarrez le noyau vous-même (mode existing)."
                    .into(),
            ),
        });
    }

    let token = token_for(host, id);
    let port = manifest.port;

    // Command line: arguments only, never a shell. `{{port}}`/`{{token}}`
    // are interpolated; the process inherits our environment plus the
    // manifest's own env entries (also interpolated).
    let mut args = manifest
        .lifecycle
        .start
        .iter()
        .map(|a| interpolate(a, port, &token));
    let program = args.next().ok_or_else(|| "commande vide".to_string())?;
    let mut cmd = Command::new(program);
    cmd.args(args);
    for (k, v) in &manifest.lifecycle.env {
        cmd.env(k, interpolate(v, port, &token));
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    let child = cmd.spawn().map_err(|e| {
        format!(
            "impossible de lancer le noyau ({e}) — vérifiez que « {} » est installé et dans le PATH.",
            manifest.lifecycle.start[0]
        )
    })?;

    let health = manifest.lifecycle.health.clone();
    let probe_url = health
        .as_ref()
        .map(|h| interpolate(&h.url, port, &token))
        .unwrap_or_default();
    let http = host.http().clone();

    {
        let mut running = manager.running.lock().await;
        running.insert(
            id,
            RunningCore {
                child,
                token: token.clone(),
                health: health.clone(),
            },
        );
    }

    // Attendre la sonde : c'est elle qui rend la main à l'utilisateur.
    if !probe_url.is_empty() {
        let retries = health.as_ref().map(|h| h.retries).unwrap_or(30);
        let interval = health
            .as_ref()
            .map(|h| h.interval_ms)
            .unwrap_or(1000)
            .max(200);
        let mut ok = false;
        for _ in 0..retries {
            if let Ok(r) = http.get(&probe_url).send().await {
                if r.status().is_success() {
                    ok = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(interval)).await;
        }
        if !ok {
            // Un noyau qui ne répond pas au démarrage est arrêté proprement :
            // il ne doit pas rester un processus fantôme dont personne ne sait
            // pourquoi il tourne.
            let _ = stop(manager, host, id).await;
            return Ok(CoreStatus {
                id: id.to_string(),
                state: CoreState::Error,
                driver: manifest.driver.clone(),
                api_url: manifest.api_url.clone(),
                error: Some(format!("la sonde de santé {probe_url} ne répond pas")),
            });
        }
    }

    tracing::info!(id = %id, port, driver = %manifest.driver, "noyau démarré");
    status(manager, host, id).await
}

pub async fn stop(
    manager: &CoreManager,
    host: &dyn CoreHost,
    id: Uuid,
) -> Result<CoreStatus, String> {
    let (manifest, _) = host.core_manifest(id).await?;
    let mut running = manager.running.lock().await;
    if let Some(mut rc) = running.remove(&id) {
        let _ = rc.child.kill();
        let _ = rc.child.wait();
    }
    drop(running);
    Ok(CoreStatus {
        id: id.to_string(),
        state: if manifest.lifecycle.start.is_empty() {
            CoreState::External
        } else {
            CoreState::Stopped
        },
        driver: manifest.driver.clone(),
        api_url: manifest.api_url.clone(),
        error: None,
    })
}

/// Agent prêt à parler au noyau pour une session. `Err` = le noyau n'est
/// pas joignable — l'appelant affiche le message sans fallback silencieux
/// vers le noyau natif (D2).
///
/// Le driver est choisi par le manifeste (`responses` / `runs` /
/// `chat_completions`) ; le pont du noyau implémente le même trait `Agent`
/// que les agents natifs, donc l'aval (streaming, tool cards, persistance)
/// ne change pas.
pub async fn agent_for_core(
    manager: &CoreManager,
    host: &dyn CoreHost,
    id: &str,
) -> Result<(CoreAgent, Option<String>), String> {
    let uid = Uuid::parse_str(id).map_err(|_| "id de noyau invalide".to_string())?;
    let (manifest, _) = host.core_manifest(uid).await?;
    loopback_only(&manifest.api_url)?;

    let running = manager.running.lock().await;
    let rc = running
        .get(&uid)
        .ok_or_else(|| {
            "Le noyau de cette conversation n'est pas démarré. Ouvrez Réglages → Extensions et démarrez-le."
                .to_string()
        })?;

    // L'`api_url` du manifeste peut porter le chemin (`…/v1/responses`) ou
    // pas : le pont ramène la base et reconstruit ses chemins.
    let base = crate::base_url_of(&manifest.api_url);
    let agent = CoreAgent::new(crate::CoreAgentConfig {
        manifest: manifest.clone(),
        base_url: base,
        bearer: rc.token.clone(),
        client: host.http().clone(),
        sessions: manager.sessions.clone(),
    });
    Ok((agent, Some(rc.token.clone())))
}

// ============================================================================
// Skills
// ============================================================================

pub async fn skills(
    _manager: &CoreManager,
    host: &dyn CoreHost,
    id: Uuid,
) -> Result<Vec<CoreSkillEntry>, String> {
    let (manifest, root) = host.core_manifest(id).await?;
    let Some(index_rel) = manifest.skills.index else {
        return Ok(Vec::new());
    };
    let index_path = root.join(index_rel);
    let raw = std::fs::read_to_string(&index_path)
        .map_err(|e| format!("index de skills illisible ({e})"))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("index de skills invalide ({e})"))?;
    let mut out = Vec::new();
    if let Some(list) = value.get("skills").and_then(|s| s.as_array()) {
        for item in list {
            let Some(slug) = item.get("slug").and_then(|s| s.as_str()) else {
                continue;
            };
            out.push(CoreSkillEntry {
                slug: slug.to_string(),
                name: item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(slug)
                    .to_string(),
                description: item
                    .get("description")
                    .and_then(|d| d.as_str())
                    .map(str::to_string),
                verified: item
                    .get("verified")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            });
        }
    }
    Ok(out)
}

/// Installer un skill de l'écosystème du noyau. La commande vient du
/// manifeste (`skills.install`, avec `{{slug}}`) et n'est exécutée que si
/// l'utilisateur a accordé la permission `shell` à l'extension.
pub async fn install_skill(
    _manager: &CoreManager,
    host: &dyn CoreHost,
    id: Uuid,
    slug: &str,
) -> Result<String, String> {
    let (manifest, _) = host.core_manifest(id).await?;
    if !host.shell_granted(id).await? {
        return Err(
            "La permission shell n'a pas été accordée à cette extension — impossible d'installer des skills."
                .into(),
        );
    }
    let template = manifest
        .skills
        .install
        .clone()
        .ok_or_else(|| "cette extension ne déclare pas d'installation de skills".to_string())?;
    let mut parts = template.split_whitespace().map(str::to_string);
    let program = parts.next().ok_or_else(|| "commande vide".to_string())?;
    let args: Vec<String> = parts.map(|a| a.replace("{{slug}}", slug)).collect();

    let out = Command::new(&program).args(&args).output().map_err(|e| {
        format!("impossible de lancer « {program} » ({e}) — vérifiez qu'il est dans le PATH")
    })?;
    if !out.status.success() {
        return Err(format!(
            "l'installation du skill a échoué : {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if stdout.is_empty() {
        format!("skill « {slug} » installé")
    } else {
        stdout
    })
}
