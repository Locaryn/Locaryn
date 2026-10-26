//! La passerelle qui tourne sur la machine : l'installer, la sonder, la
//! démarrer, l'arrêter.
//!
//! Rien n'est deviné. Les commandes viennent du manifeste de l'extension,
//! jamais de l'interface ni du panneau : une commande choisie ailleurs qu'au
//! manifeste ferait de l'application un exécuteur de commandes arbitraires.
//!
//! Une passerelle absente est le premier cas d'échec du dossier — catalogue
//! vide, choix de modèle refusé, sans qu'un écran dise pourquoi. D'où les
//! réponses tenues ici : est-elle installée, répond-elle, et sinon comment y
//! remédier.
//!
//! # Trois choses que l'hôte fait à la place de l'utilisateur
//!
//! **L'installer chez Locaryn.** Un paquet npm va dans un dossier réservé à la
//! passerelle ([`gateway_dir`]), sur le volume des données lourdes, et non dans
//! le dossier global de npm. Une passerelle Node pèse des centaines de Mo : `-g` la posait sur le
//! disque système, sur le `PATH` de tout le poste, et hors de portée de la
//! désinstallation du morph.
//!
//! **Lui donner ses propres secrets.** Une passerelle publiée avec des secrets
//! par défaut est ouverte à qui connaît le paquet. Ceux que le manifeste nomme
//! sont générés une fois, gardés dans le trousseau, et passés à chaque
//! démarrage.
//!
//! **Obtenir sa clé.** Quand la passerelle sait en émettre une depuis sa ligne
//! de commande, l'hôte la demande et la range lui-même : sans elle, la liste
//! des modèles reste fermée et le dossier s'ouvre vide.
//!
//! # Windows
//!
//! `Command::new("npm")` y échoue sur « program not found » : npm n'est pas un
//! exécutable mais `npm.cmd`, et la bibliothèque standard ne cherche que les
//! `.exe`. Le bouton « Installer » ne pouvait donc jamais fonctionner sous
//! Windows, ni « Démarrer » une passerelle installée par npm. D'où
//! [`programme`], et le lancement direct du script du paquet par `node`.

use crate::{set_key, stored_key, DeclaredProvider, Host};
use locaryn_extensions::manifest::{CloudLocalInstall, CloudLocalRuntime};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// L'état d'une passerelle locale, sondé à la demande.
#[derive(Debug, Clone, Serialize)]
pub struct CloudProviderStatus {
    pub running: bool,
    /// Le programme est-il présent sur la machine ?
    pub installed: bool,
    /// Ce qu'il faut faire — jamais un code.
    pub detail: String,
    pub dashboard_url: Option<String>,
    /// Un démarrage est-il en cours ? Une passerelle Node peut mettre
    /// plusieurs minutes à servir sa première requête : sans ce drapeau,
    /// l'écran qui revient pendant ce temps la croyait éteinte et proposait
    /// de la démarrer encore (Locaryn/Locaryn#89).
    pub starting: bool,
}

// ============================================================================
// Démarrages en cours
// ============================================================================

/// Les passerelles dont un démarrage est en cours, quel qu'en soit l'auteur :
/// le lancement de l'application, un clic, une mise à jour du morph.
fn en_demarrage() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static EN_COURS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    EN_COURS.get_or_init(Default::default)
}

fn ensemble_en_demarrage() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
    match en_demarrage().lock() {
        Ok(g) => g,
        Err(empoisonne) => empoisonne.into_inner(),
    }
}

/// Un démarrage tant que cette valeur vit : la sortie de `start`, par
/// n'importe quel chemin, le retire.
struct Demarrage(String);

impl Demarrage {
    fn inscrire(id: &str) -> Self {
        ensemble_en_demarrage().insert(id.to_string());
        Self(id.to_string())
    }
}

impl Drop for Demarrage {
    fn drop(&mut self) {
        ensemble_en_demarrage().remove(&self.0);
    }
}

/// Un démarrage de cette passerelle est-il en cours ?
pub fn demarrage_en_cours(id: &str) -> bool {
    ensemble_en_demarrage().contains(id)
}

// ============================================================================
// Où elle vit
// ============================================================================

/// Le dossier que Locaryn réserve à cette passerelle.
///
/// L'identifiant vient d'un manifeste : il est filtré avant de devenir un
/// chemin, pour qu'un `../` ne fasse pas installer ailleurs.
pub fn gateway_dir(provider_id: &str) -> PathBuf {
    let sur: String = provider_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    locaryn_config::gateways_dir().join(if sur.is_empty() { "_".into() } else { sur })
}

/// Le dossier du morph qui déclare chaque passerelle, par dossier de
/// passerelle. Un morph peut ainsi lancer un script qu'il apporte lui-même
/// (`{{morph_dir}}/bridge/server.mjs`) sans le publier sur npm.
static DOSSIERS_DES_MORPHS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, PathBuf>>,
> = std::sync::LazyLock::new(Default::default);

/// Le dossier de la passerelle de `p`, en retenant celui de son morph.
fn dossier_de(p: &DeclaredProvider) -> PathBuf {
    let dir = gateway_dir(&p.id);
    if let Ok(mut m) = DOSSIERS_DES_MORPHS.lock() {
        m.insert(dir.clone(), p.plugin_root.clone());
    }
    dir
}

/// Remplacer les marqueurs d'une commande ou d'une valeur d'environnement.
fn interpoler(texte: &str, dir: &Path) -> String {
    let mut t = texte
        .replace("{{gateway_dir}}", &dir.display().to_string())
        .replace("{{data_dir}}", &dir.join("data").display().to_string());
    if t.contains("{{morph_dir}}") {
        let morph = DOSSIERS_DES_MORPHS
            .lock()
            .ok()
            .and_then(|m| m.get(dir).cloned())
            .unwrap_or_else(|| dir.to_path_buf());
        t = t.replace("{{morph_dir}}", &morph.display().to_string());
    }
    t
}

/// Le script qu'expose un paquet npm installé dans `dir`.
///
/// Lu dans son `package.json` : `bin` est soit une chaîne, soit une table dont
/// on prend l'entrée nommée comme l'exécutable sondé, sinon la première.
fn script_du_paquet(dir: &Path, install: &CloudLocalInstall) -> Option<PathBuf> {
    if install.kind != "npm" {
        return None;
    }
    let paquet = install.package.as_deref()?.trim();
    if paquet.is_empty() {
        return None;
    }
    let racine = dir.join("node_modules").join(paquet);
    let manifeste: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(racine.join("package.json")).ok()?).ok()?;
    let relatif = match manifeste.get("bin")? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(table) => {
            let voulu = install.probe_bin.as_deref().unwrap_or(paquet);
            table
                .get(voulu)
                .or_else(|| table.values().next())?
                .as_str()?
                .to_string()
        }
        _ => return None,
    };
    let script = racine.join(relatif);
    script.is_file().then_some(script)
}

/// Le nom sous lequel lancer un programme.
///
/// Sous Windows, npm et consorts sont des scripts `.cmd` que la bibliothèque
/// standard ne trouve pas sous leur nom nu.
fn programme(nom: &str) -> String {
    if cfg!(windows)
        && Path::new(nom).extension().is_none()
        && matches!(nom, "npm" | "npx" | "pnpm" | "yarn" | "corepack")
    {
        format!("{nom}.cmd")
    } else {
        nom.to_string()
    }
}

/// Une commande du manifeste, prête à lancer.
///
/// Quand elle nomme l'exécutable d'un paquet npm installé dans le dossier de
/// la passerelle, c'est son script qui est lancé par `node` : aucun raccourci
/// `.cmd` à chercher, aucun `PATH` à modifier.
fn resoudre(
    commande: &[String],
    local: &CloudLocalRuntime,
    dir: &Path,
) -> Option<(String, Vec<String>)> {
    let (tete, reste) = commande.split_first()?;
    let args: Vec<String> = reste.iter().map(|a| interpoler(a, dir)).collect();
    if let Some(install) = &local.install {
        let noms = [install.probe_bin.as_deref(), install.package.as_deref()];
        if noms.iter().flatten().any(|n| n == tete) {
            if let Some(script) = script_du_paquet(dir, install) {
                let mut tout = vec![script.display().to_string()];
                tout.extend(args);
                return Some(("node".into(), tout));
            }
        }
    }
    Some((programme(&interpoler(tete, dir)), args))
}

/// Empêcher une console noire d'apparaître derrière chaque commande.
fn quiet(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// Ce programme est-il sur le chemin ?
///
/// `where` sous Windows, `which` ailleurs : les deux répondent par leur code
/// de sortie, ce qui suffit et évite d'analyser une sortie localisée.
fn on_path(bin: &str) -> bool {
    let bin = bin.trim();
    if bin.is_empty() {
        return false;
    }
    // Un chemin explicite se vérifie directement : `which` ne le trouverait pas.
    let p = Path::new(bin);
    if p.is_absolute() || bin.contains('/') || bin.contains('\\') {
        return p.exists();
    }
    let mut command = Command::new(if cfg!(windows) { "where" } else { "which" });
    command.arg(bin);
    quiet(&mut command);
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// La passerelle est-elle installée ?
///
/// Dans son dossier d'abord ; sinon on cherche l'exécutable déclaré
/// (`probe_bin`), ou le premier mot de la commande de démarrage, sur le chemin
/// — une passerelle installée à la main reste reconnue. Sans rien à sonder, on
/// répond « oui » : refuser de démarrer sur une supposition serait pire.
pub fn is_installed(dir: &Path, local: &CloudLocalRuntime) -> bool {
    if local
        .install
        .as_ref()
        .is_some_and(|i| script_du_paquet(dir, i).is_some())
    {
        return true;
    }
    let bin = local
        .install
        .as_ref()
        .and_then(|i| i.probe_bin.clone())
        .or_else(|| local.start.first().cloned());
    match bin {
        Some(b) => on_path(&b),
        None => true,
    }
}

// ============================================================================
// Secrets
// ============================================================================

fn entree_secrete(provider_id: &str, nom: &str) -> String {
    format!("locaryn/cloud/{provider_id}/secret/{nom}")
}

/// Une valeur secrète neuve : deux UUID v4, soit 244 bits tirés du
/// générateur du système, en 64 caractères hexadécimaux.
///
/// La longueur compte autant que l'aléa : une passerelle peut refuser de
/// démarrer avec un secret de moins de 32 caractères — et le dire dans son
/// journal, pas à l'écran de celui qui a cliqué sur « Démarrer ».
fn nouveau_secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Les secrets déclarés, créés au premier démarrage.
///
/// Refuser de démarrer quand le trousseau refuse est volontaire : lancer la
/// passerelle sans eux la ferait tourner avec les valeurs publiques de son
/// paquet, ce qui est exactement ce qu'on veut éviter.
fn secrets(
    host: &Host<'_>,
    p: &DeclaredProvider,
    local: &CloudLocalRuntime,
) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for nom in &local.secrets {
        let entree = entree_secrete(&p.id, nom);
        let valeur = match host.keychain.get(&entree) {
            Ok(v) if !v.trim().is_empty() => v,
            _ => {
                let neuf = nouveau_secret();
                host.keychain.put(&entree, &neuf).map_err(|e| {
                    format!(
                        "Le trousseau du système a refusé le secret {nom} de {} ({e}) : sans \
                         lui, la passerelle démarrerait avec les valeurs publiques de son paquet.",
                        p.label()
                    )
                })?;
                neuf
            }
        };
        out.push((nom.clone(), valeur));
    }
    Ok(out)
}

/// Le mot de passe du tableau de bord, s'il a déjà été généré.
///
/// Réservé à l'écran de l'hôte : c'est le mot de passe de l'utilisateur, pas
/// celui d'une extension.
pub fn dashboard_password(host: &Host<'_>, p: &DeclaredProvider) -> Option<String> {
    let nom = p.manifest.local.as_ref()?.dashboard_password.clone()?;
    host.keychain
        .get(&entree_secrete(&p.id, &nom))
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// L'environnement d'une commande de la passerelle : celui du manifeste, puis
/// les secrets, qui priment.
fn environnement(
    host: &Host<'_>,
    p: &DeclaredProvider,
    local: &CloudLocalRuntime,
    dir: &Path,
) -> Result<Vec<(String, String)>, String> {
    let mut env: Vec<(String, String)> = local
        .env
        .iter()
        .map(|(k, v)| (k.clone(), interpoler(v, dir)))
        .collect();
    env.extend(secrets(host, p, local)?);
    Ok(env)
}

// ============================================================================
// Sonde et état
// ============================================================================

/// Une requête courte : la passerelle est-elle là ?
pub async fn probe(host: &Host<'_>, url: &str, key: Option<&str>) -> bool {
    probe_patient(host, url, key, Duration::from_secs(3)).await
}

/// Le délai d'une sonde au démarrage. Une passerelle Node vient de finir de
/// démarrer qu'elle met encore plus de vingt secondes à servir sa première
/// requête : sondée à 3 s, elle passait pour éteinte, et l'application en
/// lançait une seconde sur le même port. Rien n'écoute : la connexion est
/// refusée aussitôt, ce délai ne coûte rien.
const SONDE_DEMARRAGE: Duration = Duration::from_secs(30);

/// Sonder avec un délai choisi.
async fn probe_patient(host: &Host<'_>, url: &str, key: Option<&str>, delai: Duration) -> bool {
    let mut req = host.http.get(url).timeout(delai);
    if let Some(k) = key {
        req = req.bearer_auth(k);
    }
    match req.send().await {
        // Une passerelle qui exige une clé répond 401 : elle est bien là, et
        // la traiter comme éteinte enverrait l'utilisateur réinstaller ce qui
        // tourne déjà.
        Ok(r) => r.status().is_success() || r.status().as_u16() == 401,
        Err(_) => false,
    }
}

/// L'URL à sonder : celle déclarée, sinon la liste des modèles — si elle
/// répond, tout le reste suit.
fn health_url(p: &DeclaredProvider, local: &CloudLocalRuntime) -> String {
    local
        .health_url
        .clone()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| p.manifest.effective_models_url())
}

/// L'état complet d'un fournisseur.
pub async fn status(host: &Host<'_>, p: &DeclaredProvider) -> CloudProviderStatus {
    let Some(local) = p.manifest.local.clone() else {
        // Un service distant est joignable ou ne l'est pas ; c'est la lecture
        // du catalogue qui le dira, pas une sonde de plus.
        return CloudProviderStatus {
            running: true,
            installed: true,
            detail: format!(
                "{} est un service distant : rien à démarrer ici.",
                p.label()
            ),
            dashboard_url: None,
            starting: false,
        };
    };

    let dir = dossier_de(p);
    let installed = is_installed(&dir, &local);
    let url = health_url(p, &local);
    let running = probe(host, &url, stored_key(host, &p.id).as_deref()).await;
    if running {
        rattraper_cle(host, p, &local, &dir).await;
    }

    let starting = !running && demarrage_en_cours(&p.id);
    let detail = if running {
        format!("{} répond sur {}.", p.label(), p.manifest.api_url)
    } else if starting {
        format!(
            "{} démarre. La première fois après le lancement, elle peut mettre quelques minutes              à répondre : vous pouvez quitter cette page, le démarrage continue.",
            p.label()
        )
    } else if !installed {
        match local.install.as_ref().filter(|i| i.is_runnable()) {
            Some(_) => format!(
                "{} n'est pas installée. Locaryn peut l'installer dans {} — plusieurs centaines \
                 de mégaoctets, et quelques minutes.",
                p.label(),
                dir.display()
            ),
            None => local
                .install_hint
                .clone()
                .unwrap_or_else(|| format!("{} n'est pas installée sur cette machine.", p.label())),
        }
    } else if local.start.is_empty() {
        local.install_hint.clone().unwrap_or_else(|| {
            format!("{} ne répond pas. Démarrez-la, puis actualisez.", p.label())
        })
    } else {
        format!(
            "{} est installée mais ne répond pas sur {}. Démarrez-la depuis ce dossier.",
            p.label(),
            p.manifest.api_url
        )
    };

    CloudProviderStatus {
        running,
        installed,
        detail,
        dashboard_url: local.dashboard_url,
        starting,
    }
}

// ============================================================================
// Installer
// ============================================================================

/// Les dernières lignes d'une sortie : c'est là qu'un gestionnaire de paquets
/// dit ce qui manque.
fn queue(sortie: &[u8]) -> String {
    let texte = String::from_utf8_lossy(sortie);
    let lignes: Vec<&str> = texte.trim().lines().collect();
    lignes[lignes.len().saturating_sub(6)..].join("\n")
}

/// Installer la passerelle avec la commande déclarée par le manifeste.
///
/// Bloquante et longue — une passerelle Node met plusieurs minutes. La sortie est
/// renvoyée en cas d'échec : c'est elle qui dit qu'il manque Node, les droits,
/// ou le réseau.
pub async fn install(host: &Host<'_>, p: &DeclaredProvider) -> Result<String, String> {
    let _ = host;
    let local = p
        .manifest
        .local
        .clone()
        .ok_or_else(|| format!("{} est un service distant : rien à installer.", p.label()))?;
    let dir = dossier_de(p);

    if is_installed(&dir, &local) {
        return Ok(format!("{} est déjà installée.", p.label()));
    }

    let install = local.install.clone().unwrap_or_default();
    let cmd = install.command_line(&dir).ok_or_else(|| {
        local.install_hint.clone().unwrap_or_else(|| {
            format!(
                "{} ne déclare pas comment s'installer. Installez-la à la main.",
                p.label()
            )
        })
    })?;

    // Un paquet npm a besoin de Node. Le dire avant de lancer npm donne un
    // message qu'on comprend, au lieu d'un « program not found » sur npm.
    if install.kind == "npm" && !on_path("node") {
        return Err(format!(
            "{} a besoin de Node.js, qui n'est pas installé sur cette machine. Installez Node 22 \
             ou plus récent depuis nodejs.org, puis recommencez.",
            p.label()
        ));
    }

    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Le dossier {} n'a pas pu être créé : {e}", dir.display()))?;

    // Une installation interrompue laisse un `node_modules` à moitié posé, que
    // npm n'arrive plus à remplacer (« ENOTEMPTY: directory not empty, rmdir »).
    // Il ne contient rien d'autre que ce que npm va reposer : on repart propre.
    if install.kind == "npm" {
        retirer_installation_partielle(&dir);
    }

    tracing::info!(fournisseur = %p.id, commande = %cmd.join(" "), "installation de la passerelle");
    let mut sortie = lancer_installation(&cmd, &dir, &local).await?;
    // Sous Windows, un antivirus ou un indexeur tient parfois un fichier le
    // temps que npm veuille le déplacer : une seconde tentative, sur un dossier
    // remis à plat, passe.
    if !sortie.status.success()
        && install.kind == "npm"
        && queue(&sortie.stderr).contains("ENOTEMPTY")
    {
        tracing::warn!(fournisseur = %p.id, "npm bloqué par un dossier non vide : nouvelle tentative");
        retirer_installation_partielle(&dir);
        sortie = lancer_installation(&cmd, &dir, &local).await?;
    }

    if !sortie.status.success() {
        return Err(format!(
            "L'installation de {} a échoué ({}).\n{}",
            p.label(),
            sortie.status,
            queue(&sortie.stderr)
        ));
    }
    if !is_installed(&dir, &local) {
        // npm peut sortir en succès sans avoir posé l'exécutable attendu —
        // un paquet renommé, un `bin` changé. Le dire vaut mieux qu'un
        // « installée » suivi d'un démarrage impossible.
        return Err(format!(
            "L'installation de {} s'est terminée, mais son exécutable est introuvable dans {}.",
            p.label(),
            dir.display()
        ));
    }
    Ok(format!(
        "{} est installée dans {}.",
        p.label(),
        dir.display()
    ))
}

/// Lancer la commande d'installation dans le dossier de la passerelle.
async fn lancer_installation(
    cmd: &[String],
    dir: &std::path::Path,
    local: &locaryn_extensions::manifest::CloudLocalRuntime,
) -> Result<std::process::Output, String> {
    let programme_ = programme(&cmd[0]);
    let arguments: Vec<String> = cmd[1..].to_vec();
    let dossier = dir.to_path_buf();
    let npm = local.install.as_ref().is_some_and(|i| i.kind == "npm");
    tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&programme_);
        command.args(&arguments).current_dir(&dossier);
        if npm {
            // Sur un réseau lent, un paquet mettait plus de cinq minutes, le
            // délai de npm : l'installation de plusieurs centaines de
            // mégaoctets s'arrêtait au bout de huit minutes, à recommencer.
            command
                .env("npm_config_fetch_timeout", "600000")
                .env("npm_config_fetch_retries", "5")
                .env("npm_config_fetch_retry_maxtimeout", "180000");
        }
        quiet(&mut command);
        command.output()
    })
    .await
    .map_err(|e| format!("l'installation n'a pas pu être lancée : {e}"))?
    .map_err(|e| {
        format!(
            "« {} » n'a pas pu être lancé ({e}). {}",
            cmd.join(" "),
            local
                .install_hint
                .clone()
                .unwrap_or_else(|| "Le gestionnaire de paquets est-il installé ?".into())
        )
    })
}

/// Retirer ce qu'une installation npm interrompue a laissé (le programme, pas
/// les données de la passerelle, qui vivent dans `data`).
fn retirer_installation_partielle(dir: &std::path::Path) {
    for nom in ["node_modules", "package-lock.json"] {
        let chemin = dir.join(nom);
        if !chemin.exists() {
            continue;
        }
        let resultat = if chemin.is_dir() {
            std::fs::remove_dir_all(&chemin)
        } else {
            std::fs::remove_file(&chemin)
        };
        match resultat {
            Ok(()) => tracing::info!(chemin = %chemin.display(), "installation partielle retirée"),
            Err(e) => {
                tracing::warn!(chemin = %chemin.display(), error = %e, "installation partielle non retirée")
            }
        }
    }
}

/// Retirer le programme de la passerelle, en gardant ses données.
///
/// Appelé à la désinstallation du morph : 450 Mo n'ont pas à survivre à
/// l'extension qui les a posés. Le dossier `data` reste — il contient les
/// fournisseurs que l'utilisateur a connectés et leurs clés chiffrées, qu'une
/// réinstallation retrouve.
pub async fn uninstall(host: &Host<'_>, p: &DeclaredProvider) -> Result<(), String> {
    let _ = stop(host, p).await;
    let dir = dossier_de(p);
    for nom in ["node_modules", "package.json", "package-lock.json"] {
        let chemin = dir.join(nom);
        let resultat = if chemin.is_dir() {
            std::fs::remove_dir_all(&chemin)
        } else if chemin.exists() {
            std::fs::remove_file(&chemin)
        } else {
            Ok(())
        };
        resultat.map_err(|e| format!("{} n'a pas pu être retiré : {e}", chemin.display()))?;
    }
    Ok(())
}

// ============================================================================
// Démarrer, arrêter
// ============================================================================

/// Démarrer la passerelle.
///
/// Installe d'abord si le programme manque : demander à l'utilisateur de
/// cliquer deux fois pour un enchaînement qui n'a qu'une issue possible ne
/// rendrait service à personne. Une fois qu'elle répond, sa clé est obtenue si
/// le manifeste sait comment.
pub async fn start(host: &Host<'_>, p: &DeclaredProvider) -> Result<CloudProviderStatus, String> {
    let local = p
        .manifest
        .local
        .clone()
        .ok_or_else(|| format!("{} est un service distant : rien à démarrer.", p.label()))?;
    if local.start.is_empty() {
        return Err(local
            .install_hint
            .clone()
            .unwrap_or_else(|| format!("{} ne déclare aucune commande de démarrage.", p.label())));
    }

    // Un seul démarrage à la fois par passerelle : deux clics, ou le
    // démarrage automatique plus un clic, en lançaient deux sur le même port.
    let verrou = verrou_de_demarrage(&p.id);
    let _demarrage = Demarrage::inscrire(&p.id);
    let _garde = verrou.lock().await;

    let dir = dossier_de(p);
    let url = health_url(p, &local);
    let cle = stored_key(host, &p.id);
    if probe_patient(host, &url, cle.as_deref(), SONDE_DEMARRAGE).await {
        obtenir_cle(host, p, &local, &dir).await;
        return Ok(status(host, p).await);
    }
    if !is_installed(&dir, &local) {
        install(host, p).await?;
    }

    let (programme_, arguments) = resoudre(&local.start, &local, &dir)
        .ok_or_else(|| format!("{} ne déclare aucune commande de démarrage.", p.label()))?;
    let env = environnement(host, p, &local, &dir)?;

    // Le journal plutôt que le néant : une passerelle qui meurt au démarrage
    // n'a que sa sortie pour dire pourquoi.
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Le dossier {} n'a pas pu être créé : {e}", dir.display()))?;
    let journal = dir.join("gateway.log");
    let fichier = std::fs::File::create(&journal).map_err(|e| {
        format!(
            "Le journal {} n'a pas pu être créé : {e}",
            journal.display()
        )
    })?;
    let erreurs = fichier.try_clone().map_err(|e| {
        format!(
            "Le journal {} n'a pas pu être ouvert : {e}",
            journal.display()
        )
    })?;

    let mut command = Command::new(&programme_);
    command.args(&arguments).current_dir(&dir);
    for (k, v) in &env {
        command.env(k, v);
    }
    quiet(&mut command);
    command.stdout(fichier).stderr(erreurs);

    let mut enfant = command.spawn().map_err(|e| {
        format!(
            "« {programme_} » n'a pas pu être lancé ({e}). {}",
            local
                .install_hint
                .clone()
                .unwrap_or_else(|| "La passerelle est-elle installée ?".into())
        )
    })?;

    // Rendre la main avant qu'elle réponde ferait afficher « éteinte » à
    // l'écran qui vient de la démarrer.
    let delai = Duration::from_secs(u64::from(local.start_timeout_seconds.max(1)));
    match attendre_reponse(host, p, &url, &mut enfant, delai).await {
        Attente::Repond => {
            tracing::info!(fournisseur = %p.id, "passerelle locale démarrée");
            obtenir_cle(host, p, &local, &dir).await;
            apres_demarrage(host, p, &local, &dir).await;
            Ok(status(host, p).await)
        }
        Attente::Morte(code) => Err(format!(
            "{} s'est arrêtée au démarrage ({code}).\n{}",
            p.label(),
            queue(&std::fs::read(&journal).unwrap_or_default())
        )),
        Attente::Muette => {
            let mut etat = status(host, p).await;
            etat.detail = format!(
                "{} a été lancée mais ne répond toujours pas après {} min. Son journal dit pourquoi : {}",
                p.label(),
                PATIENCE_MAX.as_secs() / 60,
                journal.display()
            );
            Ok(etat)
        }
    }
}

/// Ce qu'a donné l'attente d'une passerelle qu'on vient de lancer.
enum Attente {
    Repond,
    /// Son processus est sorti : inutile d'attendre la fin du délai.
    Morte(std::process::ExitStatus),
    Muette,
}

/// L'attente au-delà du délai déclaré, tant que le processus lancé vit.
///
/// OmniRoute prépare son catalogue avant de servir quoi que ce soit : 20 s
/// d'habitude, plus de trois minutes au lancement de l'application quand tout
/// démarre en même temps. Abandonner à la fin du délai déclaré laissait une
/// passerelle qui finissait par répondre sans clé ni réglages
/// (Locaryn/Locaryn#89).
const PATIENCE_MAX: Duration = Duration::from_secs(10 * 60);

/// Sonder jusqu'à la réponse, la mort du processus, ou la fin du délai —
/// prolongé tant que le processus vit encore.
async fn attendre_reponse(
    host: &Host<'_>,
    p: &DeclaredProvider,
    url: &str,
    enfant: &mut std::process::Child,
    delai: Duration,
) -> Attente {
    let debut = std::time::Instant::now();
    let mut fin = debut + delai;
    let mut vivant = true;
    loop {
        if std::time::Instant::now() >= fin {
            if vivant && fin < debut + PATIENCE_MAX {
                fin = debut + PATIENCE_MAX;
                tracing::info!(fournisseur = %p.id, "passerelle lente à répondre : l'attente continue tant qu'elle vit");
            } else {
                return Attente::Muette;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        // Le lanceur peut sortir en succès après avoir confié la passerelle à
        // un autre processus : seul un échec dit qu'elle est morte.
        if let Ok(Some(code)) = enfant.try_wait() {
            if !code.success() {
                return Attente::Morte(code);
            }
            vivant = false;
        }
        let restant = fin.saturating_duration_since(std::time::Instant::now());
        let sonde = SONDE_DEMARRAGE.min(restant.max(Duration::from_secs(1)));
        if probe_patient(host, url, stored_key(host, &p.id).as_deref(), sonde).await {
            return Attente::Repond;
        }
    }
}

/// Le verrou de démarrage d'une passerelle, partagé par tous les appelants.
fn verrou_de_demarrage(id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    static VERROUS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
    > = std::sync::OnceLock::new();
    let carte = VERROUS.get_or_init(Default::default);
    let mut carte = match carte.lock() {
        Ok(c) => c,
        Err(empoisonne) => empoisonne.into_inner(),
    };
    carte.entry(id.to_string()).or_default().clone()
}

/// Les identifiants des modèles que la passerelle sert vraiment, d'après la
/// commande `available_models` du manifeste. `None` : pas de commande, ou
/// une sortie illisible — l'appelant ne marque alors rien.
pub async fn modeles_disponibles(
    host: &Host<'_>,
    p: &DeclaredProvider,
) -> Option<std::collections::HashSet<String>> {
    let local = p.manifest.local.clone()?;
    let decl = local.available_models.clone()?;
    let dir = dossier_de(p);
    let (programme_, arguments) = resoudre(&decl.command, &local, &dir)?;
    let env = environnement(host, p, &local, &dir).ok()?;
    let dossier = dir.clone();
    let sortie = tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&programme_);
        command.args(&arguments).current_dir(&dossier);
        for (k, v) in &env {
            command.env(k, v);
        }
        quiet(&mut command);
        command.output()
    })
    .await
    .ok()?
    .ok()?;
    let texte = String::from_utf8_lossy(&sortie.stdout);
    let liste = lire_disponibles(&texte, &decl);
    if liste.is_none() {
        tracing::warn!(fournisseur = %p.id, sortie = %queue(&sortie.stdout), erreurs = %queue(&sortie.stderr), "liste des modèles disponibles illisible");
    }
    liste
}

/// La liste d'une sortie JSON, entourée ou non des messages de la commande.
fn lire_disponibles(
    sortie: &str,
    decl: &locaryn_extensions::manifest::CloudAvailableModels,
) -> Option<std::collections::HashSet<String>> {
    // Une ligne de commande colore ses messages : `\x1b[2m` contient un `[`
    // qui n'ouvre aucun JSON. On essaie chaque début possible, jusqu'au
    // premier document qui se lit.
    let valeur = sortie
        .char_indices()
        .filter(|(_, c)| matches!(c, '{' | '['))
        .find_map(|(i, _)| {
            serde_json::Deserializer::from_str(&sortie[i..])
                .into_iter::<serde_json::Value>()
                .next()?
                .ok()
                .filter(|v| v.is_object() || v.is_array())
        })?;
    let entrees = match decl.list.as_deref().filter(|l| !l.is_empty()) {
        Some(champ) => valeur.get(champ)?.as_array()?.clone(),
        None => valeur.as_array()?.clone(),
    };
    Some(
        entrees
            .iter()
            .filter(|e| {
                decl.flag_field
                    .as_deref()
                    .and_then(|f| e.get(f))
                    .and_then(serde_json::Value::as_bool)
                    != Some(false)
            })
            .filter_map(|e| e.get(&decl.id_field)?.as_str().map(str::to_string))
            .collect(),
    )
}

/// Les commandes `after_start` du manifeste, l'une après l'autre. Un échec
/// est noté et n'empêche ni les suivantes ni la passerelle de servir.
async fn apres_demarrage(
    host: &Host<'_>,
    p: &DeclaredProvider,
    local: &CloudLocalRuntime,
    dir: &Path,
) {
    if local.after_start.is_empty() {
        return;
    }
    let env = match environnement(host, p, local, dir) {
        Ok(env) => env,
        Err(e) => {
            tracing::warn!(fournisseur = %p.id, erreur = %e, "réglages d'après démarrage non lancés");
            return;
        }
    };
    for commande in &local.after_start {
        let Some((programme_, arguments)) = resoudre(commande, local, dir) else {
            continue;
        };
        let (dossier, env) = (dir.to_path_buf(), env.clone());
        let sortie = tokio::task::spawn_blocking(move || {
            let mut command = Command::new(&programme_);
            command.args(&arguments).current_dir(&dossier);
            for (k, v) in &env {
                command.env(k, v);
            }
            quiet(&mut command);
            command.output()
        })
        .await;
        match sortie {
            Ok(Ok(s)) if s.status.success() => {
                tracing::info!(fournisseur = %p.id, commande = %commande.join(" "), "réglage d'après démarrage appliqué")
            }
            Ok(Ok(s)) => {
                tracing::warn!(fournisseur = %p.id, commande = %commande.join(" "), erreurs = %queue(&s.stderr), "réglage d'après démarrage en échec")
            }
            _ => {
                tracing::warn!(fournisseur = %p.id, commande = %commande.join(" "), "réglage d'après démarrage non lancé")
            }
        }
    }
}

/// Lancer la commande d'arrêt que le manifeste déclare.
async fn commande_d_arret(
    host: &Host<'_>,
    p: &DeclaredProvider,
    local: &CloudLocalRuntime,
    dir: &std::path::Path,
) -> Result<(), String> {
    let (programme_, arguments) = resoudre(&local.stop, local, dir)
        .ok_or_else(|| format!("{} ne déclare pas comment s'arrêter.", p.label()))?;
    let env = environnement(host, p, local, dir)?;
    let dossier = dir.to_path_buf();
    let sortie = tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&programme_);
        command.args(&arguments);
        if dossier.is_dir() {
            command.current_dir(&dossier);
        }
        for (k, v) in &env {
            command.env(k, v);
        }
        quiet(&mut command);
        command.output()
    })
    .await
    .map_err(|e| format!("l'arrêt n'a pas pu être lancé : {e}"))?
    .map_err(|e| {
        format!(
            "La commande d'arrêt de {} n'a pas pu être lancée : {e}",
            p.label()
        )
    })?;
    if sortie.status.success() {
        Ok(())
    } else {
        Err(format!(
            "arrêt signalé en échec : {}",
            queue(&sortie.stderr)
        ))
    }
}

/// Arrêter la passerelle avec la commande déclarée.
pub async fn stop(host: &Host<'_>, p: &DeclaredProvider) -> Result<CloudProviderStatus, String> {
    let local = p
        .manifest
        .local
        .clone()
        .ok_or_else(|| format!("{} est un service distant : rien à arrêter.", p.label()))?;
    let dir = dossier_de(p);
    // La commande déclarée d'abord ; sans elle, ou si elle échoue, l'arrêt
    // par le dossier plus bas fait le travail.
    if let Err(e) = commande_d_arret(host, p, &local, &dir).await {
        tracing::warn!(fournisseur = %p.id, erreur = %e, "commande d'arrêt sans effet : arrêt par le dossier");
    }
    let url = health_url(p, &local);
    for _ in 0..15 {
        if !probe(host, &url, stored_key(host, &p.id).as_deref()).await {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    // La commande d'arrêt ne voit pas toujours tout : un serveur lancé par
    // `node` laisse son processus de travail derrière lui, qui garde le
    // dossier ouvert — et la désinstallation échouait ensuite.
    let restants = tokio::task::spawn_blocking(move || locaryn_mcp::arbre::liberer_dossier(&dir))
        .await
        .unwrap_or_default();
    if restants > 0 {
        tracing::info!(fournisseur = %p.id, restants, "processus de la passerelle arrêtés après sa commande d'arrêt");
    }
    Ok(status(host, p).await)
}

// ============================================================================
// La clé
// ============================================================================

/// La clé dans la sortie d'une commande qui imprime un objet JSON.
///
/// Une ligne de commande bavarde écrit volontiers ses propres messages autour
/// de l'objet : on lit de la première accolade ouvrante à la dernière fermante.
fn extraire_cle(sortie: &str, champ: &str) -> Option<String> {
    let debut = sortie.find('{')?;
    let fin = sortie.rfind('}')?;
    if fin <= debut {
        return None;
    }
    let objet: serde_json::Value = serde_json::from_str(&sortie[debut..=fin]).ok()?;
    let cle = objet
        .get(champ)
        .or_else(|| objet.get("data").and_then(|d| d.get(champ)))?
        .as_str()?
        .trim()
        .to_string();
    (!cle.is_empty()).then_some(cle)
}

/// Obtenir la clé de la passerelle quand il n'y en a pas encore.
///
/// Jamais bloquant : une clé qu'on n'a pas pu obtenir ne doit pas faire
/// échouer un démarrage réussi. Le dossier dira qu'elle manque.
async fn obtenir_cle(host: &Host<'_>, p: &DeclaredProvider, local: &CloudLocalRuntime, dir: &Path) {
    if stored_key(host, &p.id).is_some() {
        return;
    }
    let Some(prov) = local.provision_key.clone() else {
        return;
    };
    let Some((programme_, arguments)) = resoudre(&prov.command, local, dir) else {
        return;
    };
    let env = match environnement(host, p, local, dir) {
        Ok(env) => env,
        Err(e) => {
            tracing::warn!(fournisseur = %p.id, erreur = %e, "clé non obtenue");
            return;
        }
    };
    // La passerelle répond (401) avant que son API de clés soit prête : la
    // première demande échouait, et rien ne la refaisait — la clé restait « en
    // attente » pour toujours. On réessaie le temps qu'elle finisse de démarrer.
    for essai in 1..=TENTATIVES_CLE {
        let dossier = dir.to_path_buf();
        let (programme_, arguments, env) = (programme_.clone(), arguments.clone(), env.clone());
        let sortie = tokio::task::spawn_blocking(move || {
            let mut command = Command::new(&programme_);
            command.args(&arguments).current_dir(&dossier);
            for (k, v) in &env {
                command.env(k, v);
            }
            quiet(&mut command);
            command.output()
        })
        .await;
        let Ok(Ok(sortie)) = sortie else {
            tracing::warn!(fournisseur = %p.id, "la commande de clé n'a pas pu être lancée");
            return;
        };
        let texte = String::from_utf8_lossy(&sortie.stdout);
        match extraire_cle(&texte, &prov.field) {
            Some(cle) => {
                match set_key(host, &p.id, &cle) {
                    Ok(()) => {
                        tracing::info!(fournisseur = %p.id, essai, "clé obtenue auprès de la passerelle")
                    }
                    Err(e) => {
                        tracing::warn!(fournisseur = %p.id, erreur = %e, "clé obtenue mais non gardée")
                    }
                }
                return;
            }
            None => tracing::warn!(
                fournisseur = %p.id,
                essai,
                sortie = %queue(&sortie.stdout),
                erreurs = %queue(&sortie.stderr),
                "la passerelle n'a pas rendu de clé"
            ),
        }
        if essai < TENTATIVES_CLE {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

/// Combien de fois demander la clé à une passerelle qui finit de démarrer.
const TENTATIVES_CLE: u32 = 6;

/// La dernière demande de clé faite depuis l'état, par fournisseur : l'écran
/// relit l'état souvent, et chaque demande lance un processus.
static DERNIERE_DEMANDE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
> = std::sync::LazyLock::new(Default::default);

/// La passerelle tourne mais Locaryn n'a pas sa clé (demande ratée au
/// démarrage, ou passerelle lancée à la main) : la redemander, au plus une fois
/// par minute.
async fn rattraper_cle(
    host: &Host<'_>,
    p: &DeclaredProvider,
    local: &CloudLocalRuntime,
    dir: &Path,
) {
    if local.provision_key.is_none() || stored_key(host, &p.id).is_some() {
        return;
    }
    {
        let mut dernieres = DERNIERE_DEMANDE.lock().unwrap_or_else(|e| e.into_inner());
        if dernieres
            .get(&p.id)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            return;
        }
        dernieres.insert(p.id.clone(), std::time::Instant::now());
    }
    obtenir_cle(host, p, local, dir).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use locaryn_extensions::manifest::CloudLocalInstall;

    fn dossier_d_essai(nom: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("locaryn-passerelle-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dossier d'essai");
        d
    }

    fn npm(paquet: &str) -> CloudLocalInstall {
        CloudLocalInstall {
            kind: "npm".into(),
            package: Some(paquet.into()),
            probe_bin: Some(paquet.into()),
            ..Default::default()
        }
    }

    /// Poser un paquet npm factice, comme `npm install --prefix` l'aurait fait.
    fn poser_paquet(dir: &Path, paquet: &str, bin: serde_json::Value) {
        let racine = dir.join("node_modules").join(paquet);
        std::fs::create_dir_all(racine.join("bin")).unwrap();
        std::fs::write(
            racine.join("package.json"),
            serde_json::json!({ "name": paquet, "bin": bin }).to_string(),
        )
        .unwrap();
        std::fs::write(racine.join("bin").join("cli.mjs"), "").unwrap();
    }

    /// Un paquet npm s'installe dans le dossier de la passerelle, jamais
    /// globalement, et la version épinglée s'y retrouve.
    #[test]
    fn un_paquet_npm_s_installe_dans_son_dossier() {
        let dir = Path::new("D:/donnees/gateways/passerelle");
        let mut i = npm("passerelle");
        i.version = Some("3.8.50".into());
        let cmd = i.command_line(dir).unwrap();
        assert!(
            !cmd.contains(&"-g".to_string()),
            "jamais d'installation globale : {cmd:?}"
        );
        assert_eq!(cmd[..3], ["npm", "install", "--prefix"]);
        assert_eq!(cmd[3], dir.display().to_string());
        assert_eq!(cmd.last().unwrap(), "passerelle@3.8.50");
    }

    #[test]
    fn les_autres_gestionnaires_se_deduisent() {
        let docker = CloudLocalInstall {
            kind: "docker".into(),
            package: Some("org/image".into()),
            version: Some("latest".into()),
            ..Default::default()
        };
        assert_eq!(
            docker.command_line(Path::new(".")).unwrap(),
            vec!["docker", "pull", "org/image:latest"]
        );
    }

    /// Un gestionnaire inconnu ne doit pas produire de commande : mieux vaut
    /// renvoyer l'utilisateur à la phrase d'installation qu'exécuter une
    /// approximation en son nom.
    #[test]
    fn un_gestionnaire_inconnu_ne_produit_rien() {
        let inconnu = CloudLocalInstall {
            kind: "sorcellerie".into(),
            package: Some("truc".into()),
            ..Default::default()
        };
        assert!(inconnu.command_line(Path::new(".")).is_none());
        assert!(!inconnu.is_runnable());

        let sans_paquet = CloudLocalInstall {
            kind: "npm".into(),
            ..Default::default()
        };
        assert!(sans_paquet.command_line(Path::new(".")).is_none());
    }

    /// Une commande explicite prime sur toute déduction.
    #[test]
    fn une_commande_explicite_prime() {
        let explicite = CloudLocalInstall {
            kind: "command".into(),
            command: vec!["cargo".into(), "install".into(), "truc".into()],
            package: Some("ignoré".into()),
            ..Default::default()
        };
        assert_eq!(
            explicite.command_line(Path::new(".")).unwrap(),
            vec!["cargo", "install", "truc"]
        );
    }

    /// Sous Windows, npm s'appelle `npm.cmd` : sous son nom nu, la
    /// bibliothèque standard ne le trouve pas et l'installation échouait
    /// toujours.
    #[test]
    fn npm_porte_son_nom_windows() {
        if cfg!(windows) {
            assert_eq!(programme("npm"), "npm.cmd");
            assert_eq!(programme("npm.cmd"), "npm.cmd");
        } else {
            assert_eq!(programme("npm"), "npm");
        }
        assert_eq!(
            programme("node"),
            "node",
            "un vrai exécutable ne change pas"
        );
    }

    /// L'exécutable d'un paquet installé est lancé par `node`, avec ses
    /// arguments et les marqueurs remplacés.
    #[test]
    fn l_executable_du_paquet_est_lance_par_node() {
        let dir = dossier_d_essai("resolution");
        poser_paquet(
            &dir,
            "passerelle",
            serde_json::json!({ "passerelle": "bin/cli.mjs", "autre": "x" }),
        );
        let local = CloudLocalRuntime {
            install: Some(npm("passerelle")),
            ..Default::default()
        };
        assert!(is_installed(&dir, &local), "le script du paquet suffit");

        let commande: Vec<String> = ["passerelle", "serve", "--data", "{{data_dir}}"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (prog, args) = resoudre(&commande, &local, &dir).unwrap();
        assert_eq!(prog, "node");
        assert!(args[0].ends_with("cli.mjs"), "{args:?}");
        assert_eq!(args[1], "serve");
        assert_eq!(args[3], dir.join("data").display().to_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `bin` en chaîne unique : c'est la forme la plus courante.
    #[test]
    fn un_bin_en_chaine_se_lit() {
        let dir = dossier_d_essai("bin-chaine");
        poser_paquet(&dir, "passerelle", serde_json::json!("bin/cli.mjs"));
        assert!(script_du_paquet(&dir, &npm("passerelle")).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sans paquet posé, le programme est lancé sous son nom — une passerelle
    /// installée à la main reste utilisable.
    #[test]
    fn sans_paquet_le_programme_garde_son_nom() {
        let dir = dossier_d_essai("sans-paquet");
        let local = CloudLocalRuntime {
            install: Some(npm("passerelle")),
            ..Default::default()
        };
        let (prog, _) = resoudre(&["passerelle".to_string()], &local, &dir).unwrap();
        assert_eq!(prog, "passerelle");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sans rien à sonder, on suppose l'installation faite : refuser de
    /// démarrer sur une supposition serait pire que d'essayer.
    #[test]
    fn sans_binaire_a_sonder_on_suppose_installe() {
        assert!(is_installed(Path::new("."), &CloudLocalRuntime::default()));
    }

    /// Un exécutable qui n'existe pas ne doit pas passer pour installé.
    #[test]
    fn un_binaire_absent_nest_pas_installe() {
        let local = CloudLocalRuntime {
            start: vec!["ce-programme-nexiste-vraiment-pas-42".into()],
            ..Default::default()
        };
        assert!(!is_installed(Path::new("."), &local));
    }

    /// Un identifiant de manifeste ne sort pas du dossier des passerelles.
    #[test]
    fn un_identifiant_ne_remonte_pas_l_arborescence() {
        let d = gateway_dir("../../windows");
        assert!(d.starts_with(locaryn_config::gateways_dir()));
        assert!(!d.display().to_string().contains(".."), "{}", d.display());
    }

    /// Un secret généré passe la validation usuelle des passerelles (32
    /// caractères au moins), et deux ne se ressemblent jamais.
    #[test]
    fn un_secret_genere_est_long_et_unique() {
        let a = nouveau_secret();
        let b = nouveau_secret();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()), "{a}");
        assert_ne!(a, b);
    }

    /// La clé est trouvée au milieu d'une sortie bavarde, et à plat ou sous
    /// `data`.
    #[test]
    fn la_cle_se_lit_dans_une_sortie_bavarde() {
        let sortie = "  📋 Loaded env\n{\n \"key\": \"sk-123\",\n \"name\": \"Locaryn\"\n}\n";
        assert_eq!(extraire_cle(sortie, "key").as_deref(), Some("sk-123"));
        assert_eq!(
            extraire_cle("{\"data\":{\"key\":\"sk-9\"}}", "key").as_deref(),
            Some("sk-9")
        );
        assert!(extraire_cle("aucun objet", "key").is_none());
        assert!(
            extraire_cle("{\"key\":\"  \"}", "key").is_none(),
            "une clé vide n'en est pas une"
        );
    }

    /// Un démarrage en cours se voit dans l'état, de n'importe quel écran
    /// (Locaryn/Locaryn#89).
    #[test]
    fn un_demarrage_se_voit_tant_qu_il_dure() {
        assert!(!demarrage_en_cours("essai-demarrage"));
        {
            let _d = Demarrage::inscrire("essai-demarrage");
            assert!(demarrage_en_cours("essai-demarrage"));
        }
        assert!(
            !demarrage_en_cours("essai-demarrage"),
            "quitter start, par n'importe quel chemin, retire le démarrage"
        );
    }

    /// La sortie d'une commande bavarde : la liste est lue dans son champ, et
    /// une entrée marquée indisponible est écartée.
    #[test]
    fn la_liste_des_modeles_disponibles_se_lit() {
        let decl = locaryn_extensions::manifest::CloudAvailableModels {
            command: vec![],
            list: Some("models".into()),
            id_field: "fullModel".into(),
            flag_field: Some("available".into()),
        };
        // Les codes couleur ANSI d'une ligne de commande contiennent un `[`.
        let sortie = "\u{1b}[2m📋 chargement\u{1b}[0m\n{\"models\":[\
                      {\"fullModel\":\"f/a\",\"available\":true},\
                      {\"fullModel\":\"f/b\",\"available\":false},\
                      {\"fullModel\":\"f/c\"}]}\n";
        let ids = lire_disponibles(sortie, &decl).expect("liste lisible");
        assert!(ids.contains("f/a") && ids.contains("f/c") && !ids.contains("f/b"));
        assert!(lire_disponibles("pas de json", &decl).is_none());
    }
}
