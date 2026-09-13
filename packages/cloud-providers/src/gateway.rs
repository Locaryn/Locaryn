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
//! le dossier global de npm. OmniRoute pèse 450 Mo : `-g` le posait sur le
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

/// Remplacer les marqueurs d'une commande ou d'une valeur d'environnement.
fn interpoler(texte: &str, dir: &Path) -> String {
    texte
        .replace("{{gateway_dir}}", &dir.display().to_string())
        .replace("{{data_dir}}", &dir.join("data").display().to_string())
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
/// La longueur compte autant que l'aléa : OmniRoute refuse de démarrer avec un
/// `JWT_SECRET` de moins de 32 caractères — et le dit dans son journal, pas à
/// l'écran de celui qui a cliqué sur « Démarrer ».
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
    let mut req = host.http.get(url).timeout(Duration::from_secs(3));
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
        };
    };

    let dir = gateway_dir(&p.id);
    let installed = is_installed(&dir, &local);
    let url = health_url(p, &local);
    let running = probe(host, &url, stored_key(host, &p.id).as_deref()).await;

    let detail = if running {
        format!("{} répond sur {}.", p.label(), p.manifest.api_url)
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
/// Bloquante et longue — OmniRoute met plusieurs minutes. La sortie est
/// renvoyée en cas d'échec : c'est elle qui dit qu'il manque Node, les droits,
/// ou le réseau.
pub async fn install(host: &Host<'_>, p: &DeclaredProvider) -> Result<String, String> {
    let _ = host;
    let local = p
        .manifest
        .local
        .clone()
        .ok_or_else(|| format!("{} est un service distant : rien à installer.", p.label()))?;
    let dir = gateway_dir(&p.id);

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

    tracing::info!(fournisseur = %p.id, commande = %cmd.join(" "), "installation de la passerelle");
    let programme_ = programme(&cmd[0]);
    let arguments: Vec<String> = cmd[1..].to_vec();
    let dossier = dir.clone();
    let sortie = tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&programme_);
        command.args(&arguments).current_dir(&dossier);
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
    })?;

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

/// Retirer le programme de la passerelle, en gardant ses données.
///
/// Appelé à la désinstallation du morph : 450 Mo n'ont pas à survivre à
/// l'extension qui les a posés. Le dossier `data` reste — il contient les
/// fournisseurs que l'utilisateur a connectés et leurs clés chiffrées, qu'une
/// réinstallation retrouve.
pub async fn uninstall(host: &Host<'_>, p: &DeclaredProvider) -> Result<(), String> {
    let _ = stop(host, p).await;
    let dir = gateway_dir(&p.id);
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

    let dir = gateway_dir(&p.id);
    let url = health_url(p, &local);
    if probe(host, &url, stored_key(host, &p.id).as_deref()).await {
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

    command.spawn().map_err(|e| {
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
    let mut repond = false;
    for _ in 0..local.start_timeout_seconds.max(1) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if probe(host, &url, stored_key(host, &p.id).as_deref()).await {
            repond = true;
            break;
        }
    }
    if repond {
        tracing::info!(fournisseur = %p.id, "passerelle locale démarrée");
        obtenir_cle(host, p, &local, &dir).await;
        return Ok(status(host, p).await);
    }

    let mut etat = status(host, p).await;
    etat.detail = format!(
        "{} a été lancée mais ne répond toujours pas après {} s. Son journal dit pourquoi : {}",
        p.label(),
        local.start_timeout_seconds,
        journal.display()
    );
    Ok(etat)
}

/// Arrêter la passerelle avec la commande déclarée.
pub async fn stop(host: &Host<'_>, p: &DeclaredProvider) -> Result<CloudProviderStatus, String> {
    let local = p
        .manifest
        .local
        .clone()
        .ok_or_else(|| format!("{} est un service distant : rien à arrêter.", p.label()))?;
    let dir = gateway_dir(&p.id);
    let (programme_, arguments) = resoudre(&local.stop, &local, &dir)
        .ok_or_else(|| format!("{} ne déclare pas comment s'arrêter.", p.label()))?;
    let env = environnement(host, p, &local, &dir)?;

    let dossier = dir.clone();
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

    if !sortie.status.success() {
        tracing::warn!(fournisseur = %p.id, sortie = %queue(&sortie.stderr), "arrêt signalé en échec");
    }
    let url = health_url(p, &local);
    for _ in 0..15 {
        if !probe(host, &url, stored_key(host, &p.id).as_deref()).await {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
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
    let dossier = dir.to_path_buf();
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
        Some(cle) => match set_key(host, &p.id, &cle) {
            Ok(()) => tracing::info!(fournisseur = %p.id, "clé obtenue auprès de la passerelle"),
            Err(e) => {
                tracing::warn!(fournisseur = %p.id, erreur = %e, "clé obtenue mais non gardée")
            }
        },
        None => tracing::warn!(
            fournisseur = %p.id,
            sortie = %queue(&sortie.stderr),
            "la passerelle n'a pas rendu de clé"
        ),
    }
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
        let dir = Path::new("D:/donnees/gateways/omniroute");
        let mut i = npm("omniroute");
        i.version = Some("3.8.50".into());
        let cmd = i.command_line(dir).unwrap();
        assert!(
            !cmd.contains(&"-g".to_string()),
            "jamais d'installation globale : {cmd:?}"
        );
        assert_eq!(cmd[..3], ["npm", "install", "--prefix"]);
        assert_eq!(cmd[3], dir.display().to_string());
        assert_eq!(cmd.last().unwrap(), "omniroute@3.8.50");
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
            "omniroute",
            serde_json::json!({ "omniroute": "bin/cli.mjs", "autre": "x" }),
        );
        let local = CloudLocalRuntime {
            install: Some(npm("omniroute")),
            ..Default::default()
        };
        assert!(is_installed(&dir, &local), "le script du paquet suffit");

        let commande: Vec<String> = ["omniroute", "serve", "--data", "{{data_dir}}"]
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
            install: Some(npm("omniroute")),
            ..Default::default()
        };
        let (prog, _) = resoudre(&["omniroute".to_string()], &local, &dir).unwrap();
        assert_eq!(prog, "omniroute");
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

    /// Un secret généré passe la validation d'OmniRoute (32 caractères au
    /// moins), et deux ne se ressemblent jamais.
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
}
