//! La passerelle qui tourne sur la machine : l'installer, la sonder, la
//! démarrer.
//!
//! Rien n'est deviné. La commande d'installation et celle de démarrage
//! viennent du manifeste de l'extension, jamais de l'interface ni du panneau :
//! une commande choisie ailleurs qu'au manifeste ferait de l'application un
//! exécuteur de commandes arbitraires.
//!
//! Une passerelle absente est le premier cas d'échec du dossier — catalogue
//! vide, choix de modèle refusé, sans qu'un écran dise pourquoi. D'où trois
//! réponses tenues ici : est-elle installée, répond-elle, et sinon comment y
//! remédier.

use crate::{stored_key, DeclaredProvider, Host};
use locaryn_extensions::manifest::CloudLocalRuntime;
use serde::Serialize;
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
    let p = std::path::Path::new(bin);
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
/// On cherche l'exécutable déclaré (`probe_bin`), sinon le premier mot de la
/// commande de démarrage. Sans aucun des deux, on répond « oui » : rien ne
/// permet d'affirmer le contraire, et refuser de démarrer sur une supposition
/// serait pire.
pub fn is_installed(local: &CloudLocalRuntime) -> bool {
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

    let installed = is_installed(&local);
    let url = health_url(p, &local);
    let running = probe(host, &url, stored_key(host, &p.id).as_deref()).await;

    let detail = if running {
        format!("{} répond sur {}.", p.label(), p.manifest.api_url)
    } else if !installed {
        match local.install.as_ref().and_then(|i| i.command_line()) {
            Some(cmd) => format!(
                "{} n'est pas installée. Locaryn peut le faire : {}.",
                p.label(),
                cmd.join(" ")
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

/// Installer la passerelle avec la commande déclarée par le manifeste.
///
/// Bloquante et longue — un `npm install -g` prend des dizaines de secondes.
/// La sortie est renvoyée telle quelle en cas d'échec : c'est elle qui dit
/// qu'il manque Node, les droits, ou le réseau.
pub async fn install(host: &Host<'_>, p: &DeclaredProvider) -> Result<String, String> {
    let local = p
        .manifest
        .local
        .clone()
        .ok_or_else(|| format!("{} est un service distant : rien à installer.", p.label()))?;

    if is_installed(&local) {
        return Ok(format!("{} est déjà installée.", p.label()));
    }

    let cmd = local
        .install
        .as_ref()
        .and_then(|i| i.command_line())
        .ok_or_else(|| {
            local.install_hint.clone().unwrap_or_else(|| {
                format!(
                    "{} ne déclare pas comment s'installer. Installez-la à la main.",
                    p.label()
                )
            })
        })?;

    tracing::info!(fournisseur = %p.id, commande = %cmd.join(" "), "installation de la passerelle");
    let programme = cmd[0].clone();
    let arguments: Vec<String> = cmd[1..].to_vec();
    let sortie = tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&programme);
        command.args(&arguments);
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
        let détail = String::from_utf8_lossy(&sortie.stderr);
        let détail = détail.trim();
        let queue: String = détail.lines().rev().take(6).collect::<Vec<_>>().join("\n");
        return Err(format!(
            "L'installation de {} a échoué ({}).\n{}",
            p.label(),
            sortie.status,
            queue
        ));
    }
    let _ = host;
    Ok(format!("{} est installée.", p.label()))
}

/// Démarrer la passerelle.
///
/// Installe d'abord si le programme manque : demander à l'utilisateur de
/// cliquer deux fois pour un enchaînement qui n'a qu'une issue possible ne
/// rendrait service à personne.
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

    let url = health_url(p, &local);
    if probe(host, &url, stored_key(host, &p.id).as_deref()).await {
        return Ok(status(host, p).await);
    }
    if !is_installed(&local) {
        install(host, p).await?;
    }

    let mut command = Command::new(&local.start[0]);
    command.args(&local.start[1..]);
    for (k, v) in &local.env {
        command.env(k, v);
    }
    quiet(&mut command);
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    command.spawn().map_err(|e| {
        format!(
            "« {} » n'a pas pu être lancé ({e}). {}",
            local.start.join(" "),
            local
                .install_hint
                .clone()
                .unwrap_or_else(|| "La passerelle est-elle installée ?".into())
        )
    })?;

    // Une passerelle met quelques secondes à ouvrir son port. Rendre la main
    // avant qu'elle réponde ferait afficher « éteinte » à l'écran qui vient de
    // la démarrer.
    for _ in 0..24 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if probe(host, &url, stored_key(host, &p.id).as_deref()).await {
            break;
        }
    }
    tracing::info!(fournisseur = %p.id, "passerelle locale démarrée");
    Ok(status(host, p).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use locaryn_extensions::manifest::CloudLocalInstall;

    /// Les commandes d'installation sont déduites du gestionnaire déclaré, et
    /// la version épinglée s'y retrouve : sans elle, deux machines
    /// n'installent pas la même chose.
    #[test]
    fn les_commandes_dinstallation_se_deduisent() {
        let npm = CloudLocalInstall {
            kind: "npm".into(),
            package: Some("omniroute".into()),
            version: Some("1.2.3".into()),
            ..Default::default()
        };
        assert_eq!(
            npm.command_line().unwrap(),
            vec!["npm", "install", "-g", "omniroute@1.2.3"]
        );

        let sans_version = CloudLocalInstall {
            kind: "npm".into(),
            package: Some("omniroute".into()),
            ..Default::default()
        };
        assert_eq!(
            sans_version.command_line().unwrap(),
            vec!["npm", "install", "-g", "omniroute"]
        );

        let docker = CloudLocalInstall {
            kind: "docker".into(),
            package: Some("org/image".into()),
            version: Some("latest".into()),
            ..Default::default()
        };
        assert_eq!(
            docker.command_line().unwrap(),
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
        assert!(inconnu.command_line().is_none());
        assert!(!inconnu.is_runnable());

        let sans_paquet = CloudLocalInstall {
            kind: "npm".into(),
            ..Default::default()
        };
        assert!(sans_paquet.command_line().is_none());
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
            explicite.command_line().unwrap(),
            vec!["cargo", "install", "truc"]
        );
    }

    /// Sans rien à sonder, on suppose l'installation faite : refuser de
    /// démarrer sur une supposition serait pire que d'essayer.
    #[test]
    fn sans_binaire_a_sonder_on_suppose_installe() {
        let vide = CloudLocalRuntime::default();
        assert!(is_installed(&vide));
    }

    /// Un exécutable qui n'existe pas ne doit pas passer pour installé.
    #[test]
    fn un_binaire_absent_nest_pas_installe() {
        let local = CloudLocalRuntime {
            start: vec!["ce-programme-nexiste-vraiment-pas-42".into()],
            ..Default::default()
        };
        assert!(!is_installed(&local));
    }
}
