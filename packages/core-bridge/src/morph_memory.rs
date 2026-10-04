//! Ce que le noyau sait des morphs installés.
//!
//! Un noyau alternatif (OpenClaw, Hermes) a sa propre mémoire et ses propres
//! skills : il ne lit pas le registre de Locaryn. Sans aide, il ne découvre un
//! morph que le jour où l'un de ses outils lui est offert dans une requête, et
//! l'oublie à la conversation suivante.
//!
//! Locaryn écrit donc, dans le dossier de skills natif du noyau, une skill
//! `locaryn-morphs` qui décrit les morphs actifs : à quoi ils servent, ce
//! qu'ils savent faire, comment les demander. Elle est réécrite à chaque
//! démarrage du noyau et à chaque installation, activation ou retrait — le
//! noyau retrouve ainsi l'état réel de la machine, sans rien avoir à retenir.

use locaryn_extensions::ExtensionEntry;
use std::path::{Path, PathBuf};

/// Nom du dossier de la skill dans celui du noyau.
pub const SKILL_DIR_NAME: &str = "locaryn-morphs";

/// Longueur maximale de l'extrait de skill repris pour chaque morph : assez
/// pour dire comment s'en servir, pas assez pour noyer le contexte du noyau.
const EXTRAIT_MAX: usize = 1200;

/// Un morph actif, tel que le noyau doit le connaître.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphNote {
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    /// Début de la première skill du morph, sans son en-tête.
    pub usage: String,
}

/// Les notes des morphs activés qui ne sont pas des noyaux.
pub fn notes_from_entries(entries: &[ExtensionEntry]) -> Vec<MorphNote> {
    let mut notes: Vec<MorphNote> = entries
        .iter()
        .filter(|e| e.enabled)
        .filter_map(note_of)
        .collect();
    notes.sort_by(|a, b| a.name.cmp(&b.name));
    notes
}

fn note_of(entry: &ExtensionEntry) -> Option<MorphNote> {
    let root = entry.manifest_path.parent()?;
    let manifest = match locaryn_extensions::manifest::load(root) {
        Ok(m) => m,
        Err(e) => {
            tracing::debug!(name = %entry.name, error = %e, "morph illisible pour la mémoire du noyau");
            return None;
        }
    };
    // Un noyau n'a pas à se décrire à lui-même, ni à décrire son voisin.
    if manifest.core.is_some() {
        return None;
    }
    Some(MorphNote {
        name: entry.name.clone(),
        version: entry.version.clone(),
        description: manifest.description.clone().unwrap_or_default(),
        capabilities: entry.capabilities.clone(),
        usage: manifest
            .components
            .skills
            .first()
            .map(|rel| root.join(rel))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| extrait(&text))
            .unwrap_or_default(),
    })
}

/// Le corps d'une skill sans son bloc `---` d'en-tête, coupé proprement.
fn extrait(text: &str) -> String {
    let body = match text.strip_prefix("---") {
        Some(rest) => rest.split_once("\n---").map_or(rest, |(_, after)| after),
        None => text,
    };
    let body = body.trim();
    if body.chars().count() <= EXTRAIT_MAX {
        return body.to_string();
    }
    let cut: String = body.chars().take(EXTRAIT_MAX).collect();
    format!("{cut}…")
}

/// Le contenu de la skill `locaryn-morphs`.
pub fn render(notes: &[MorphNote]) -> String {
    let mut out = String::from(
        "---\n\
         name: locaryn-morphs\n\
         description: Les morphs Locaryn installés sur cette machine, ce qu'ils savent faire \
         et comment les utiliser. À consulter avant de dire qu'une capacité manque.\n\
         ---\n\n\
         # Morphs Locaryn disponibles\n\n\
         Locaryn est l'hôte qui te fait tourner. Ses morphs sont des extensions : leurs outils \
         te sont offerts à chaque requête sous leur nom court. Cette liste est réécrite \
         par Locaryn à chaque changement ; elle dit l'état réel de la machine.\n\n",
    );
    if notes.is_empty() {
        out.push_str("Aucun morph n'est actif pour l'instant.\n");
        return out;
    }
    for note in notes {
        out.push_str(&format!("## {} (v{})\n\n", note.name, note.version));
        if !note.description.is_empty() {
            out.push_str(&format!("{}\n\n", note.description));
        }
        if !note.capabilities.is_empty() {
            out.push_str(&format!("Capacités : {}\n\n", note.capabilities.join(", ")));
        }
        if !note.usage.is_empty() {
            out.push_str(&format!("{}\n\n", note.usage));
        }
    }
    out
}

/// Remplace `~` par le dossier personnel.
fn expand_home(path: &str) -> Option<PathBuf> {
    match path.strip_prefix("~/") {
        Some(rest) => {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            Some(PathBuf::from(home).join(rest))
        }
        None => Some(PathBuf::from(path)),
    }
}

/// Écrit la skill dans le dossier de skills du noyau. Rend le fichier écrit.
pub fn write_skill(install_dir: &str, notes: &[MorphNote]) -> Result<PathBuf, String> {
    let base = expand_home(install_dir)
        .ok_or_else(|| "dossier personnel introuvable pour les skills du noyau".to_string())?;
    write_skill_in(&base, notes)
}

fn write_skill_in(base: &Path, notes: &[MorphNote]) -> Result<PathBuf, String> {
    let dir = base.join(SKILL_DIR_NAME);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let file = dir.join("SKILL.md");
    std::fs::write(&file, render(notes)).map_err(|e| format!("{} : {e}", file.display()))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(name: &str) -> MorphNote {
        MorphNote {
            name: name.into(),
            version: "1.0.0".into(),
            description: "Fait des choses.".into(),
            capabilities: vec!["image-gen".into(), "image-editor".into()],
            usage: "Appelez generate_image avec un prompt anglais.".into(),
        }
    }

    #[test]
    fn la_skill_nomme_chaque_morph_et_ses_capacites() {
        let text = render(&[note("morph-image"), note("morph-ssh")]);
        assert!(text.starts_with("---\nname: locaryn-morphs"));
        assert!(text.contains("## morph-image (v1.0.0)"));
        assert!(text.contains("Capacités : image-gen, image-editor"));
        assert!(text.contains("generate_image"));
    }

    #[test]
    fn sans_morph_la_skill_le_dit_au_lieu_de_laisser_croire_a_une_liste_vide() {
        assert!(render(&[]).contains("Aucun morph n'est actif"));
    }

    #[test]
    fn l_extrait_retire_l_en_tete_et_borne_la_longueur() {
        let long = format!("---\nname: x\n---\n{}", "a".repeat(5000));
        let e = extrait(&long);
        assert!(!e.contains("name: x"));
        assert!(e.chars().count() <= EXTRAIT_MAX + 1);
    }

    #[test]
    fn la_skill_est_reecrite_a_chaque_appel() {
        let base = std::env::temp_dir().join(format!(
            "locaryn_morphs_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let first = write_skill_in(&base, &[note("morph-ssh")]).unwrap();
        assert!(std::fs::read_to_string(&first)
            .unwrap()
            .contains("morph-ssh"));
        write_skill_in(&base, &[note("morph-image")]).unwrap();
        let text = std::fs::read_to_string(&first).unwrap();
        assert!(text.contains("morph-image") && !text.contains("morph-ssh"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
