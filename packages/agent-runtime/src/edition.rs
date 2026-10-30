//! Modifier une partie d'un fichier sans le réécrire.
//!
//! Avec `write_file` seul, un petit modèle qui doit changer trois lignes
//! renvoie le fichier entier : dix minutes de génération à 12 jetons/s, et
//! souvent un fichier qui a perdu une moitié en route (vu sur un 9B : « je
//! retouche la fonction de dessin » → le fichier ne contenait plus qu'elle).
//! `edit_file` remplace un passage exact ; `garde_ecrasement` refuse qu'un
//! fragment écrase un fichier bien plus long sans que le modèle le demande.

use crate::tools::{resolve_path, ToolResult, ToolSpec};
use locaryn_shared_types::{Permission, Risk};
use std::path::Path;

pub const OUTIL: &str = "edit_file";

/// En dessous, un fichier se réécrit sans contrôle : rien à perdre.
const TAILLE_PROTEGEE: usize = 1_500;
/// Part minimale de l'ancien contenu qu'une réécriture doit garder.
const PART_MINIMALE: f64 = 0.4;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: OUTIL.into(),
        description: "Change part of an existing file by replacing an exact passage. Prefer this to write_file for any change to a file that already exists: copy old_string exactly from read_file, with enough surrounding lines to match only once. Set replace_all to true to change every occurrence.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "old_string": { "type": "string", "description": "Exact text to replace, copied from the file." },
                "new_string": { "type": "string", "description": "Text that takes its place." },
                "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of exactly one." }
            },
            "required": ["path", "old_string", "new_string"]
        }),
        risk: Risk::Medium,
        required_permissions: vec![Permission::FilesWrite],
    }
}

/// Le remplacement lui-même, sans disque : la partie testable.
///
/// Les fins de ligne ne comptent pas : sous Windows, `git checkout` rend les
/// fichiers en CRLF (`core.autocrlf`) alors que le modèle recopie le passage
/// avec des `\n` — le passage était « introuvable » alors qu'il était là. Le
/// fichier garde ses fins de ligne d'origine.
pub fn remplacer(
    texte: &str,
    ancien: &str,
    nouveau: &str,
    partout: bool,
) -> Result<(String, usize), String> {
    if !texte.contains("\r\n") {
        return remplacer_exact(
            texte,
            &ancien.replace("\r\n", "\n"),
            &nouveau.replace("\r\n", "\n"),
            partout,
        );
    }
    let lf = |s: &str| s.replace("\r\n", "\n");
    let (modifie, n) = remplacer_exact(&lf(texte), &lf(ancien), &lf(nouveau), partout)?;
    Ok((modifie.replace('\n', "\r\n"), n))
}

fn remplacer_exact(
    texte: &str,
    ancien: &str,
    nouveau: &str,
    partout: bool,
) -> Result<(String, usize), String> {
    if ancien.is_empty() {
        return Err("old_string is empty: copy the exact passage to replace from read_file".into());
    }
    if ancien == nouveau {
        return Err("old_string and new_string are identical: nothing to change".into());
    }
    let n = texte.matches(ancien).count();
    match (n, partout) {
        (0, _) => Err(introuvable(texte, ancien)),
        (1, _) | (_, true) => Ok((texte.replace(ancien, nouveau), n)),
        _ => Err(format!(
            "old_string matches {n} places: add surrounding lines so it matches only once, or set replace_all to true"
        )),
    }
}

/// Longueur maximale du passage réel renvoyé au modèle.
const EXTRAIT_MAX: usize = 1_200;

/// Un passage introuvable vient le plus souvent d'un fichier recopié de
/// mémoire (vu sur un 9B : trois essais avec des lignes inventées). Quand sa
/// première ligne existe, renvoyer le vrai passage évite un essai de plus.
fn introuvable(texte: &str, ancien: &str) -> String {
    let premiere = ancien
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let lignes = ancien.lines().count().max(1);
    let extrait = (!premiere.is_empty())
        .then(|| passage_reel(texte, premiere, lignes))
        .flatten();
    if let Some(extrait) = extrait {
        format!(
            "old_string not found as written, but its first line exists. The file really contains, from that line:\n{extrait}\nCopy old_string from this text exactly (indentation and line breaks included)"
        )
    } else {
        // Vu sur un 9B : le texte à ajouter passé comme old_string.
        "old_string not found in the file: it must be text that is already there, copied with read_file. To add new text, set old_string to the existing lines just before the insertion point and repeat them at the start of new_string".into()
    }
}

/// Les `lignes` lignes du fichier qui commencent à la première ligne
/// contenant `premiere`, coupées à `EXTRAIT_MAX` octets.
fn passage_reel(texte: &str, premiere: &str, lignes: usize) -> Option<String> {
    let tout: Vec<&str> = texte.lines().collect();
    let debut = tout.iter().position(|l| l.contains(premiere))?;
    let mut extrait = tout[debut..(debut + lignes).min(tout.len())].join("\n");
    if extrait.len() > EXTRAIT_MAX {
        let coupe = (0..=EXTRAIT_MAX)
            .rev()
            .find(|&i| extrait.is_char_boundary(i))
            .unwrap_or(0);
        extrait.truncate(coupe);
    }
    Some(extrait)
}

/// `Some(message)` quand `write_file` s'apprête à remplacer un fichier long par
/// un contenu qui n'en garde qu'une petite partie, sans `overwrite: true`.
pub fn garde_ecrasement(ancien: usize, nouveau: usize, force: bool) -> Option<String> {
    if force || ancien < TAILLE_PROTEGEE || nouveau as f64 >= ancien as f64 * PART_MINIMALE {
        return None;
    }
    Some(format!(
        "refused: this would replace a {ancien}-byte file with {nouveau} bytes and delete most of it. To change part of the file, use edit_file. To really replace the whole file, call write_file again with the complete content and overwrite: true"
    ))
}

pub async fn executer(args: &serde_json::Value, project_root: &Path) -> ToolResult {
    let texte = |cle: &str| args.get(cle).and_then(|v| v.as_str());
    let (Some(chemin), Some(ancien), Some(nouveau)) =
        (texte("path"), texte("old_string"), texte("new_string"))
    else {
        return echec(
            "missing required argument: path, old_string and new_string are all required",
        );
    };
    let partout = args
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let complet = match resolve_path(project_root, chemin) {
        Ok(p) => p,
        Err(e) => return echec(&format!("edit_file error: {e}")),
    };
    let actuel = match tokio::fs::read_to_string(&complet).await {
        Ok(t) => t,
        Err(e) => {
            return echec(&format!(
                "edit_file error: cannot read {chemin}: {e}. To create a file, use write_file"
            ))
        }
    };
    let (modifie, n) = match remplacer(&actuel, ancien, nouveau, partout) {
        Ok(r) => r,
        Err(e) => return echec(&e),
    };
    match tokio::fs::write(&complet, &modifie).await {
        Ok(()) => ToolResult {
            ok: true,
            output: format!(
                "edited {chemin}: {n} replacement(s), {} → {} bytes",
                actuel.len(),
                modifie.len()
            ),
            artifact: None,
        },
        Err(e) => echec(&format!("edit_file error: {e}")),
    }
}

fn echec(message: &str) -> ToolResult {
    ToolResult {
        ok: false,
        output: message.to_string(),
        artifact: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_passage_unique_est_remplace() {
        let (t, n) = remplacer("a\nb\nc\n", "b\n", "B\n", false).unwrap();
        assert_eq!(t, "a\nB\nc\n");
        assert_eq!(n, 1);
    }

    #[test]
    fn un_passage_ambigu_est_refuse_sauf_partout() {
        let e = remplacer("x x", "x", "y", false).unwrap_err();
        assert!(e.contains("matches 2 places"), "{e}");
        assert_eq!(
            remplacer("x x", "x", "y", true).unwrap(),
            ("y y".to_string(), 2)
        );
    }

    #[test]
    fn un_passage_absent_dit_pourquoi() {
        let proche =
            remplacer("    let a = 1;\n", "let a = 1;\n  let b = 2;", "z", false).unwrap_err();
        assert!(proche.contains("first line exists"), "{proche}");
        assert!(
            proche.contains("    let a = 1;"),
            "le vrai passage est renvoyé : {proche}"
        );
        let absent = remplacer("abc", "zzz", "y", false).unwrap_err();
        assert!(absent.contains("not found in the file"), "{absent}");
    }

    /// Le cas observé : app.js restauré par git en CRLF, passage recopié en LF.
    #[test]
    fn les_fins_de_ligne_ne_comptent_pas() {
        let (t, n) = remplacer("a\r\nb\r\nc\r\n", "a\nb\n", "a\nB\n", false).unwrap();
        assert_eq!(t, "a\r\nB\r\nc\r\n", "le fichier garde ses CRLF");
        assert_eq!(n, 1);
        let (t, _) = remplacer("a\nb\n", "b\r\n", "B\r\n", false).unwrap();
        assert_eq!(t, "a\nB\n", "un fichier LF reste en LF");
    }

    #[test]
    fn les_demandes_vides_sont_refusees() {
        assert!(remplacer("abc", "", "y", false).is_err());
        assert!(remplacer("abc", "b", "b", false).is_err());
    }

    /// Le cas observé : un fichier de 6 Ko remplacé par une fonction de 2 Ko.
    #[test]
    fn un_fragment_n_ecrase_pas_un_long_fichier() {
        assert!(garde_ecrasement(6_000, 2_000, false).is_some());
        assert!(
            garde_ecrasement(6_000, 2_000, true).is_none(),
            "overwrite: true passe"
        );
        assert!(
            garde_ecrasement(6_000, 5_000, false).is_none(),
            "une vraie réécriture passe"
        );
        assert!(
            garde_ecrasement(800, 10, false).is_none(),
            "un petit fichier n'est pas protégé"
        );
    }

    #[tokio::test]
    async fn l_outil_modifie_le_fichier_sur_le_disque() {
        let dir = std::env::temp_dir().join(format!("locaryn-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.js"), "const a = 1;\nconst b = 2;\n").unwrap();
        let args = serde_json::json!({ "path": "app.js", "old_string": "const b = 2;", "new_string": "const b = 3;" });
        let r = executer(&args, &dir).await;
        assert!(r.ok, "{}", r.output);
        assert_eq!(
            std::fs::read_to_string(dir.join("app.js")).unwrap(),
            "const a = 1;\nconst b = 3;\n"
        );
        let manque = executer(
            &serde_json::json!({ "path": "absent.js", "old_string": "a", "new_string": "b" }),
            &dir,
        )
        .await;
        assert!(
            !manque.ok && manque.output.contains("use write_file"),
            "{}",
            manque.output
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
