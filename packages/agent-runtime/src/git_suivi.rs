//! Le travail de l'agent suivi par Git : un dépôt créé au premier fichier
//! écrit dans un projet qui n'en a pas, et Locaryn co-auteur des commits que
//! l'agent lance. Réglages dans [`locaryn_config::git`].

use std::path::Path;
use std::sync::OnceLock;

/// Le projet est-il déjà dans un dépôt (le sien ou un parent) ?
fn dans_un_depot(racine: &Path) -> bool {
    racine.ancestors().any(|d| d.join(".git").exists())
}

/// Un dossier où un dépôt n'a pas sa place : la racine d'un disque, le
/// dossier personnel ou l'un de ses grands dossiers (Documents, Bureau…).
/// Un projet ouvert sur `C:\Users\moi` ferait sinon suivre tout le profil.
fn dossier_trop_large(racine: &Path) -> bool {
    if racine.parent().is_none() {
        return true;
    }
    let Some(maison) = dirs::home_dir() else {
        return false;
    };
    racine == maison || racine.parent() == Some(maison.as_path())
}

/// Créer le dépôt du projet s'il n'en a pas. Sans Git installé, rien ne se
/// passe : l'écriture du fichier ne doit jamais en dépendre.
pub async fn assurer_depot(racine: &Path) {
    if !locaryn_config::git::init_auto() || dans_un_depot(racine) || dossier_trop_large(racine) {
        return;
    }
    let mut commande = std::process::Command::new("git");
    locaryn_config::hide_console(&mut commande);
    commande.args(["init", "--quiet"]).current_dir(racine);
    match tokio::process::Command::from(commande).output().await {
        Ok(sortie) if sortie.status.success() => {
            tracing::info!(projet = %racine.display(), "dépôt Git créé pour suivre les modifications");
        }
        Ok(sortie) => tracing::debug!(
            erreur = %String::from_utf8_lossy(&sortie.stderr),
            "git init a échoué"
        ),
        Err(e) => tracing::debug!(error = %e, "git absent : pas de dépôt créé"),
    }
}

/// `git commit` dans une commande, mais pas `git commit-tree`.
fn motif_commit() -> &'static regex::Regex {
    static MOTIF: OnceLock<regex::Regex> = OnceLock::new();
    MOTIF
        .get_or_init(|| regex::Regex::new(r"\bgit\s+commit(\s|$)").expect("motif de commit valide"))
}

/// Ajouter Locaryn en co-auteur aux `git commit` d'une commande de l'agent,
/// avec `--trailer` (Git 2.32 et plus) : le message du modèle reste tel quel.
/// Rien n'est ajouté si la ligne y est déjà.
pub fn avec_co_auteur(commande: &str) -> String {
    if !locaryn_config::git::co_auteur() {
        return commande.to_string();
    }
    ajouter_co_auteur(commande)
}

fn ajouter_co_auteur(commande: &str) -> String {
    if commande
        .to_ascii_lowercase()
        .contains(&locaryn_config::git::CO_AUTEUR.to_ascii_lowercase())
    {
        return commande.to_string();
    }
    let ajout = format!(
        "git commit --trailer \"{}\"$1",
        locaryn_config::git::CO_AUTEUR
    );
    motif_commit()
        .replace_all(commande, ajout.as_str())
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locaryn_est_co_auteur_des_commits() {
        let c = ajouter_co_auteur("git add . && git commit -m \"accueil\"");
        assert_eq!(
            c,
            "git add . && git commit --trailer \"Co-Authored-By: Locaryn <noreply@locaryn.dev>\" -m \"accueil\""
        );
        // En fin de commande aussi.
        assert!(ajouter_co_auteur("git commit").contains("--trailer"));
        // Ni `commit-tree`, ni un commit qui le nomme déjà.
        assert_eq!(
            ajouter_co_auteur("git commit-tree abc"),
            "git commit-tree abc"
        );
        let deja = "git commit -m \"x\n\nCo-Authored-By: Locaryn <noreply@locaryn.dev>\"";
        assert_eq!(ajouter_co_auteur(deja), deja);
    }

    #[test]
    fn pas_de_depot_dans_un_dossier_trop_large() {
        assert!(dossier_trop_large(Path::new(if cfg!(windows) {
            "C:\\"
        } else {
            "/"
        })));
        if let Some(maison) = dirs::home_dir() {
            assert!(dossier_trop_large(&maison));
            assert!(dossier_trop_large(&maison.join("Documents")));
            assert!(!dossier_trop_large(&maison.join("Documents").join("site")));
        }
    }
}
