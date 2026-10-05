//! Les fichiers et les modifications du dossier d'une conversation, pour les
//! onglets « Fichiers » et « Modifications » de l'espace de travail.
//!
//! Lecture seule : écrire reste l'affaire du modèle (outils `write_file`,
//! `run_command`) ou de l'éditeur de la personne. Tout chemin est résolu dans
//! le dossier de la conversation, comme pour les outils du modèle.

use crate::Core;
use locaryn_agent_runtime::tools::resolve_path;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::State;
use uuid::Uuid;

/// Au-delà, un fichier ne s'affiche qu'en partie.
const LECTURE_MAX: usize = 512 * 1024;

/// Au-delà, un dossier ne montre que ses premières entrées.
const ENTREES_MAX: usize = 2000;

/// Un diff plus long est coupé.
const DIFF_MAX: usize = 400 * 1024;

/// Dossiers qu'aucun arbre de projet n'a besoin de déplier.
const IGNORES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".venv",
    "__pycache__",
    ".next",
    ".turbo",
];

const DELAI_GIT: Duration = Duration::from_secs(15);

#[derive(Debug, Serialize)]
pub struct Entree {
    pub nom: String,
    /// Relatif au dossier de la conversation, avec des `/`.
    pub chemin: String,
    pub dossier: bool,
    pub taille: u64,
}

#[derive(Debug, Serialize)]
pub struct Fichier {
    pub chemin: String,
    /// Absent pour un fichier binaire.
    pub contenu: Option<String>,
    pub taille: u64,
    pub tronque: bool,
}

#[derive(Debug, Serialize)]
pub struct Modification {
    pub chemin: String,
    /// Le code de `git status` : « M », « A », « D », « R », « ?? »…
    pub etat: String,
    pub ajouts: Option<u32>,
    pub retraits: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct Modifications {
    pub racine: String,
    /// Faux quand le dossier n'est pas un dépôt git.
    pub depot: bool,
    pub branche: Option<String>,
    pub fichiers: Vec<Modification>,
}

async fn racine(core: &State<'_, Core>, session_id: Uuid) -> Result<PathBuf, String> {
    let dossier = crate::session_workspace(core.clone(), session_id).await?;
    Ok(PathBuf::from(dossier))
}

fn relatif(racine: &Path, chemin: &Path) -> String {
    chemin
        .strip_prefix(racine)
        .unwrap_or(chemin)
        .to_string_lossy()
        .replace('\\', "/")
}

#[tauri::command]
pub async fn workspace_list(
    core: State<'_, Core>,
    session_id: Uuid,
    dossier: Option<String>,
) -> Result<Vec<Entree>, String> {
    let racine = racine(&core, session_id).await?;
    let cible =
        resolve_path(&racine, dossier.as_deref().unwrap_or(".")).map_err(|e| e.to_string())?;
    let mut lecture = tokio::fs::read_dir(&cible)
        .await
        .map_err(|e| format!("{} : {e}", cible.display()))?;
    let mut entrees = Vec::new();
    while let Some(e) = lecture.next_entry().await.map_err(|e| e.to_string())? {
        let nom = e.file_name().to_string_lossy().to_string();
        let meta = match e.metadata().await {
            Ok(m) => m,
            Err(err) => {
                tracing::debug!(fichier = %nom, erreur = %err, "entrée illisible, ignorée");
                continue;
            }
        };
        if meta.is_dir() && IGNORES.contains(&nom.as_str()) {
            continue;
        }
        entrees.push(Entree {
            chemin: relatif(&racine, &e.path()),
            nom,
            dossier: meta.is_dir(),
            taille: meta.len(),
        });
        if entrees.len() >= ENTREES_MAX {
            break;
        }
    }
    entrees.sort_by(|a, b| {
        b.dossier
            .cmp(&a.dossier)
            .then_with(|| a.nom.to_lowercase().cmp(&b.nom.to_lowercase()))
    });
    Ok(entrees)
}

/// Le contenu d'un fichier texte, ou rien s'il est binaire.
fn decoder(octets: &[u8]) -> Option<String> {
    let debut = &octets[..octets.len().min(8192)];
    if debut.contains(&0) {
        return None;
    }
    match std::str::from_utf8(octets) {
        Ok(t) => Some(t.to_string()),
        // Coupé au milieu d'un caractère : on garde ce qui précède.
        Err(e) if e.error_len().is_none() => {
            Some(String::from_utf8_lossy(&octets[..e.valid_up_to()]).to_string())
        }
        Err(_) => None,
    }
}

#[tauri::command]
pub async fn workspace_read(
    core: State<'_, Core>,
    session_id: Uuid,
    chemin: String,
) -> Result<Fichier, String> {
    let racine = racine(&core, session_id).await?;
    let cible = resolve_path(&racine, &chemin).map_err(|e| e.to_string())?;
    let taille = tokio::fs::metadata(&cible)
        .await
        .map_err(|e| format!("{chemin} : {e}"))?
        .len();
    // Jamais plus que ce qui s'affiche : un journal de plusieurs gigaoctets ne
    // doit pas passer en mémoire pour en montrer le début.
    let fichier = tokio::fs::File::open(&cible)
        .await
        .map_err(|e| format!("{chemin} : {e}"))?;
    let mut octets = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(
        &mut tokio::io::AsyncReadExt::take(fichier, LECTURE_MAX as u64 + 1),
        &mut octets,
    )
    .await
    .map_err(|e| format!("{chemin} : {e}"))?;
    let tronque = octets.len() > LECTURE_MAX;
    octets.truncate(LECTURE_MAX);
    Ok(Fichier {
        chemin: relatif(&racine, &cible),
        contenu: decoder(&octets),
        taille,
        tronque,
    })
}

/// Lancer git dans `racine`, sans fenêtre de console.
async fn git(racine: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    let mut commande = std::process::Command::new("git");
    locaryn_config::hide_console(&mut commande);
    commande.arg("-C").arg(racine).args(args);
    let sortie = tokio::time::timeout(DELAI_GIT, tokio::process::Command::from(commande).output())
        .await
        .map_err(|_| "git n'a pas répondu à temps".to_string())?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "git n'est pas installé sur cette machine".to_string()
            } else {
                format!("git : {e}")
            }
        })?;
    Ok(sortie)
}

/// `git status --porcelain=v1 -z --branch` : la branche, puis une entrée par
/// fichier (un renommage porte un second chemin, l'ancien).
fn lire_statut(sortie: &str) -> (Option<String>, Vec<Modification>) {
    let mut branche = None;
    let mut fichiers = Vec::new();
    let mut morceaux = sortie.split('\0').filter(|m| !m.is_empty());
    while let Some(m) = morceaux.next() {
        if let Some(b) = m.strip_prefix("## ") {
            branche = Some(b.split("...").next().unwrap_or(b).to_string());
            continue;
        }
        if m.len() < 4 {
            continue;
        }
        let code = m[..2].trim().to_string();
        let chemin = m[3..].to_string();
        if code.starts_with('R') || code.starts_with('C') {
            // L'ancien chemin suit : il ne fait pas une entrée de plus.
            morceaux.next();
        }
        fichiers.push(Modification {
            chemin,
            etat: if code.is_empty() { "M".into() } else { code },
            ajouts: None,
            retraits: None,
        });
    }
    (branche, fichiers)
}

#[tauri::command]
pub async fn workspace_changes(
    core: State<'_, Core>,
    session_id: Uuid,
) -> Result<Modifications, String> {
    let racine = racine(&core, session_id).await?;
    let statut = git(&racine, &["status", "--porcelain=v1", "-z", "--branch"]).await?;
    if !statut.status.success() {
        // Pas un dépôt : ce n'est pas une erreur, l'onglet le dit.
        return Ok(Modifications {
            racine: racine.display().to_string(),
            depot: false,
            branche: None,
            fichiers: Vec::new(),
        });
    }
    let (branche, mut fichiers) = lire_statut(&String::from_utf8_lossy(&statut.stdout));

    // Lignes ajoutées et retirées, quand le dépôt a déjà un commit.
    if let Ok(chiffres) = git(&racine, &["diff", "HEAD", "--numstat", "-z"]).await {
        let texte = String::from_utf8_lossy(&chiffres.stdout).to_string();
        for ligne in texte.split('\0') {
            let mut parts = ligne.splitn(3, '\t');
            let (Some(a), Some(r), Some(p)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            if let Some(f) = fichiers.iter_mut().find(|f| f.chemin == p) {
                f.ajouts = a.parse().ok();
                f.retraits = r.parse().ok();
            }
        }
    }
    Ok(Modifications {
        racine: racine.display().to_string(),
        depot: true,
        branche,
        fichiers,
    })
}

/// Le diff d'un fichier ; pour un fichier nouveau, tout son contenu en ajout.
#[tauri::command]
pub async fn workspace_diff(
    core: State<'_, Core>,
    session_id: Uuid,
    chemin: String,
) -> Result<String, String> {
    let racine = racine(&core, session_id).await?;
    // Le chemin doit rester dans le dossier, même passé à git.
    let cible = resolve_path(&racine, &chemin).map_err(|e| e.to_string())?;
    let relatif = relatif(&racine, &cible);
    let suivi = git(&racine, &["diff", "HEAD", "--", &relatif]).await?;
    let mut texte = String::from_utf8_lossy(&suivi.stdout).to_string();
    if texte.trim().is_empty() {
        // Non suivi : comparé à rien. Code de sortie 1 = « il y a une différence ».
        let neuf = git(
            &racine,
            &["diff", "--no-index", "--", "/dev/null", &relatif],
        )
        .await?;
        texte = String::from_utf8_lossy(&neuf.stdout).to_string();
    }
    if texte.len() > DIFF_MAX {
        let mut coupe = DIFF_MAX;
        while !texte.is_char_boundary(coupe) {
            coupe -= 1;
        }
        texte.truncate(coupe);
        texte.push_str("\n[… diff coupé …]");
    }
    Ok(texte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_statut_git_se_lit_renommages_compris() {
        let sortie =
            "## main...origin/main [ahead 1]\0 M src/a.rs\0R  neuf.rs\0vieux.rs\0?? note.md\0";
        let (branche, f) = lire_statut(sortie);
        assert_eq!(branche.as_deref(), Some("main"));
        assert_eq!(f.len(), 3);
        assert_eq!(
            (f[0].etat.as_str(), f[0].chemin.as_str()),
            ("M", "src/a.rs")
        );
        assert_eq!((f[1].etat.as_str(), f[1].chemin.as_str()), ("R", "neuf.rs"));
        assert_eq!(
            (f[2].etat.as_str(), f[2].chemin.as_str()),
            ("??", "note.md")
        );
    }

    #[test]
    fn un_fichier_binaire_ne_s_affiche_pas() {
        assert_eq!(decoder(b"bonjour").as_deref(), Some("bonjour"));
        assert!(decoder(&[0x89, b'P', b'N', b'G', 0, 0]).is_none());
        // Coupé au milieu de « é » : le début reste lisible.
        assert_eq!(decoder(&[b'c', b'a', b'f', 0xC3]).as_deref(), Some("caf"));
    }
}
