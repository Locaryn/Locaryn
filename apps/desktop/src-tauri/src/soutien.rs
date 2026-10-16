//! Soutenir le projet par une étoile GitHub, une fois, si la personne le veut.
//!
//! Après la première vraie utilisation, une fenêtre propose : « Oui » (une
//! étoile à Locaryn et aux morphs installés), « Je soutiens à fond » (à tous
//! les dépôts de l'organisation), ou « Non ». Rien ne part sans un clic, la
//! fenêtre ne s'affiche que si GitHub est déjà connecté sur la machine (`gh`,
//! ou un identifiant enregistré par git), et la réponse est retenue : on ne
//! redemande jamais.
//!
//! Le jeton n'est ni gardé ni envoyé ailleurs qu'à l'API de GitHub, pour
//! l'action choisie.

use serde::Serialize;
use std::process::Stdio;
use std::time::Duration;
use tauri::State;

use crate::Core;

const ORGANISATION: &str = "Locaryn";
const DEPOT_PRINCIPAL: &str = "Locaryn/Locaryn";
const BRANCHE: &str = "soutien";

/// Comment parler à GitHub au nom de la personne.
enum Acces {
    /// La CLI `gh`, déjà connectée.
    Gh,
    /// Le jeton que git a enregistré pour github.com.
    Jeton(String),
}

fn commande(programme: &str) -> tokio::process::Command {
    let mut c = std::process::Command::new(programme);
    locaryn_config::hide_console(&mut c);
    let mut c = tokio::process::Command::from(c);
    c.stdin(Stdio::null()).kill_on_drop(true);
    c
}

async fn sortie(mut c: tokio::process::Command) -> Option<String> {
    let s = tokio::time::timeout(Duration::from_secs(10), c.output())
        .await
        .ok()?
        .ok()?;
    s.status
        .success()
        .then(|| String::from_utf8_lossy(&s.stdout).trim().to_string())
}

/// `gh` connecté, sinon un identifiant github.com déjà enregistré par git —
/// sans jamais ouvrir de fenêtre de connexion ni poser de question.
async fn acces() -> Option<Acces> {
    if locaryn_config::program_exists("gh") {
        let mut c = commande("gh");
        c.args(["auth", "status", "--hostname", "github.com"]);
        if sortie(c).await.is_some() {
            return Some(Acces::Gh);
        }
    }
    if !locaryn_config::program_exists("git") {
        return None;
    }
    let mut c = std::process::Command::new("git");
    locaryn_config::hide_console(&mut c);
    let mut c = tokio::process::Command::from(c);
    c.args(["credential", "fill"])
        // Seulement ce qui est déjà enregistré : aucune invite, aucune
        // fenêtre du gestionnaire d'identifiants.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("GIT_ASKPASS", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut enfant = c.spawn().ok()?;
    if let Some(mut entree) = enfant.stdin.take() {
        use tokio::io::AsyncWriteExt as _;
        let _ = entree
            .write_all(b"protocol=https\nhost=github.com\n\n")
            .await;
    }
    let s = tokio::time::timeout(Duration::from_secs(8), enfant.wait_with_output())
        .await
        .ok()?
        .ok()?;
    if !s.status.success() {
        return None;
    }
    String::from_utf8_lossy(&s.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("password="))
        .map(|j| Acces::Jeton(j.trim().to_string()))
        .filter(|a| matches!(a, Acces::Jeton(j) if !j.is_empty()))
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("locaryn-desktop/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

async fn identifiant(a: &Acces) -> Option<String> {
    match a {
        Acces::Gh => {
            let mut c = commande("gh");
            c.args(["api", "user", "--jq", ".login"]);
            sortie(c).await.filter(|l| !l.is_empty())
        }
        Acces::Jeton(j) => {
            let v: serde_json::Value = client()
                .get("https://api.github.com/user")
                .bearer_auth(j)
                .send()
                .await
                .ok()?
                .error_for_status()
                .ok()?
                .json()
                .await
                .ok()?;
            v["login"].as_str().map(str::to_string)
        }
    }
}

/// Mettre une étoile à `depot` (`owner/repo`).
async fn etoiler(a: &Acces, depot: &str) -> bool {
    match a {
        Acces::Gh => {
            let mut c = commande("gh");
            c.args([
                "api",
                "-X",
                "PUT",
                &format!("/user/starred/{depot}"),
                "--silent",
            ]);
            sortie(c).await.is_some()
        }
        Acces::Jeton(j) => client()
            .put(format!("https://api.github.com/user/starred/{depot}"))
            .bearer_auth(j)
            .header("Content-Length", "0")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false),
    }
}

/// Les dépôts publics de l'organisation (`owner/repo`).
async fn depots_de_l_organisation() -> Vec<String> {
    let mut out = Vec::new();
    for page in 1..=5 {
        let Ok(r) = client()
            .get(format!(
                "https://api.github.com/orgs/{ORGANISATION}/repos?per_page=100&page={page}&type=public"
            ))
            .send()
            .await
        else {
            break;
        };
        let Ok(v) = r.json::<serde_json::Value>().await else {
            break;
        };
        let noms: Vec<String> = v
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|d| !d["archived"].as_bool().unwrap_or(false))
                    .filter_map(|d| d["full_name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let fin = noms.len() < 100;
        out.extend(noms);
        if fin {
            break;
        }
    }
    out
}

/// `owner/repo` d'une source ou d'une page d'accueil GitHub.
pub fn depot_github(texte: &str) -> Option<String> {
    let t = texte.trim().trim_end_matches('/').trim_end_matches(".git");
    let chemin = t
        .strip_prefix("https://github.com/")
        .or_else(|| t.strip_prefix("http://github.com/"))
        .or_else(|| t.strip_prefix("git@github.com:"))
        .or_else(|| t.strip_prefix("github:"))
        .unwrap_or(t);
    let morceaux: Vec<&str> = chemin.split('/').collect();
    let valide = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match morceaux.as_slice() {
        [proprio, depot, ..] if valide(proprio) && valide(depot) && !chemin.contains(':') => {
            // Un chemin local (« C:/… », « ./morph ») n'est pas un dépôt.
            (!texte.contains('\\') && !texte.starts_with('.') && !texte.starts_with('/'))
                .then(|| format!("{proprio}/{depot}"))
        }
        _ => None,
    }
}

/// Locaryn et les morphs installés depuis GitHub.
async fn depots_utilises(core: &Core) -> Vec<String> {
    let mut depots = vec![DEPOT_PRINCIPAL.to_string()];
    if let Ok(exts) = core.storage.extensions.list().await {
        for e in exts {
            // La source d'installation, sinon le dépôt que déclare le manifeste.
            let manifeste: Option<serde_json::Value> = std::fs::read_to_string(&e.manifest_path)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok());
            let declares: Vec<String> = ["repository", "homepage"]
                .iter()
                .filter_map(|k| {
                    let v = manifeste.as_ref()?.get(*k)?;
                    v.as_str()
                        .or_else(|| v.get("url").and_then(|u| u.as_str()))
                        .map(str::to_string)
                })
                .collect();
            let candidats = e.source.iter().chain(declares.iter());
            if let Some(d) = candidats.map(String::as_str).find_map(depot_github) {
                if !depots.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
                    depots.push(d);
                }
            }
        }
    }
    depots
}

fn decision() -> Option<String> {
    locaryn_config::global_value(BRANCHE).and_then(|v| {
        v.get("decision")
            .and_then(|d| d.as_str())
            .map(str::to_string)
    })
}

fn retenir(choix: &str) {
    let _ = locaryn_config::set_global(
        BRANCHE,
        serde_json::json!({ "decision": choix, "date": chrono::Utc::now().to_rfc3339() }),
    );
}

#[derive(Serialize)]
pub struct EtatSoutien {
    /// La fenêtre peut s'afficher : pas encore de réponse, GitHub connecté.
    pub proposer: bool,
    pub login: Option<String>,
    /// Les dépôts qu'« Oui » étoilerait.
    pub depots: Vec<String>,
}

#[tauri::command]
pub async fn soutien_etat(core: State<'_, Core>) -> Result<EtatSoutien, String> {
    if decision().is_some() {
        return Ok(EtatSoutien {
            proposer: false,
            login: None,
            depots: Vec::new(),
        });
    }
    let Some(a) = acces().await else {
        return Ok(EtatSoutien {
            proposer: false,
            login: None,
            depots: Vec::new(),
        });
    };
    let login = identifiant(&a).await;
    Ok(EtatSoutien {
        proposer: login.is_some(),
        login,
        depots: depots_utilises(&core).await,
    })
}

#[derive(Serialize, Clone)]
pub struct ResultatSoutien {
    pub etoiles: u32,
    pub echecs: u32,
    /// Les étoiles se posent encore, en arrière-plan.
    pub en_cours: bool,
}

/// `oui`, `tout` ou `non`. Retenu dans tous les cas : on ne redemande pas.
#[tauri::command]
pub async fn soutien_repondre(
    app: tauri::AppHandle,
    core: State<'_, Core>,
    choix: String,
) -> Result<ResultatSoutien, String> {
    retenir(&choix);
    if choix != "oui" && choix != "tout" {
        return Ok(ResultatSoutien {
            etoiles: 0,
            echecs: 0,
            en_cours: false,
        });
    }
    // Les dépôts des morphs installés se lisent dans la base locale : tout de
    // suite. Le reste — la liste de l'organisation, une requête GitHub par
    // étoile — prenait 15 à 30 secondes pendant lesquelles la fenêtre restait
    // figée. Il part en arrière-plan ; le résultat arrive par un évènement.
    let utilises = depots_utilises(&core).await;
    tauri::async_runtime::spawn(async move {
        let resultat = poser_les_etoiles(&choix, utilises).await;
        use tauri::Emitter as _;
        let _ = app.emit("locaryn://soutien-etoiles", resultat);
    });
    Ok(ResultatSoutien {
        etoiles: 0,
        echecs: 0,
        en_cours: true,
    })
}

async fn poser_les_etoiles(choix: &str, utilises: Vec<String>) -> ResultatSoutien {
    let mut depots = if choix == "tout" {
        depots_de_l_organisation().await
    } else {
        Vec::new()
    };
    for x in utilises {
        if !depots.iter().any(|y| y.eq_ignore_ascii_case(&x)) {
            depots.push(x);
        }
    }
    let Some(a) = acces().await else {
        tracing::warn!("soutien GitHub : GitHub n'est plus connecté sur cette machine");
        return ResultatSoutien {
            etoiles: 0,
            echecs: depots.len() as u32,
            en_cours: false,
        };
    };
    let (mut etoiles, mut echecs) = (0, 0);
    for d in &depots {
        if etoiler(&a, d).await {
            etoiles += 1;
        } else {
            echecs += 1;
        }
    }
    tracing::info!(choix, etoiles, echecs, "soutien GitHub");
    ResultatSoutien {
        etoiles,
        echecs,
        en_cours: false,
    }
}

#[cfg(test)]
mod tests {
    use super::depot_github;

    #[test]
    fn un_depot_github_se_reconnait_sous_toutes_ses_formes() {
        assert_eq!(
            depot_github("Locaryn/morph-wsl").as_deref(),
            Some("Locaryn/morph-wsl")
        );
        assert_eq!(
            depot_github("https://github.com/Locaryn/morph-browser").as_deref(),
            Some("Locaryn/morph-browser")
        );
        assert_eq!(
            depot_github("https://github.com/Locaryn/morph-ssh.git").as_deref(),
            Some("Locaryn/morph-ssh")
        );
        assert_eq!(
            depot_github("git@github.com:a-b/c_d.git").as_deref(),
            Some("a-b/c_d")
        );
        assert_eq!(depot_github(r"C:\Users\x\morph"), None);
        assert_eq!(depot_github("./morph-local"), None);
        assert_eq!(depot_github("https://gitlab.com/a/b"), None);
        assert_eq!(depot_github("morph-seul"), None);
    }
}
