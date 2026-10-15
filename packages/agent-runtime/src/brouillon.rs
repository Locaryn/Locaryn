//! Lire un appel d'outil pendant qu'il s'écrit.
//!
//! Les arguments d'un appel arrivent par fragments d'un JSON incomplet :
//! `{"path": "index.html", "content": "<!doctype html>\n<ht`. Pour montrer à
//! l'utilisateur ce que le modèle est en train d'écrire — un fichier de 5 000
//! jetons prend des minutes —, on en tire le chemin visé et la fin du contenu
//! déjà produit, sans attendre que le JSON soit complet.

/// Les clés qui portent le fichier visé.
const CLES_CHEMIN: &[&str] = &["path", "file_path", "filepath", "filename", "file"];
/// Les clés qui portent le gros du texte écrit.
const CLES_CONTENU: &[&str] = &[
    "content",
    "contents",
    "text",
    "code",
    "new_string",
    "command",
    "body",
];
/// Longueur de la fin de contenu envoyée à l'interface.
pub const LONGUEUR_FIN: usize = 1_500;

/// Ce qu'on sait d'un appel en cours d'écriture.
#[derive(Debug, Default, PartialEq)]
pub struct Brouillon {
    pub chemin: Option<String>,
    /// Caractères du contenu décodés jusqu'ici.
    pub caracteres: u64,
    /// La fin du contenu, décodée.
    pub fin: String,
}

/// Décode une chaîne JSON à partir de `debut` (juste après le guillemet
/// ouvrant). Rend le texte et `true` si la chaîne est fermée.
fn chaine(src: &str, debut: usize) -> (String, bool) {
    let mut out = String::new();
    let mut it = src[debut..].chars();
    while let Some(c) = it.next() {
        match c {
            '"' => return (out, true),
            '\\' => match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => {}
                Some('b') | Some('f') => {}
                Some('u') => {
                    let hex: String = it.by_ref().take(4).collect();
                    if hex.len() < 4 {
                        return (out, false);
                    }
                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(ch);
                    }
                }
                Some(autre) => out.push(autre),
                None => return (out, false),
            },
            _ => out.push(c),
        }
    }
    (out, false)
}

/// Position juste après `"cle"\s*:\s*"`, si la clé porte une chaîne.
fn valeur_de(src: &str, cle: &str) -> Option<usize> {
    let motif = format!("\"{cle}\"");
    let mut depart = 0;
    while let Some(i) = src[depart..].find(&motif) {
        let apres = depart + i + motif.len();
        let reste = src[apres..].trim_start();
        if let Some(r) = reste.strip_prefix(':') {
            let r2 = r.trim_start();
            if let Some(stripped) = r2.strip_prefix('"') {
                return Some(src.len() - stripped.len());
            }
        }
        depart = apres;
    }
    None
}

/// Le chemin et la fin du contenu d'arguments encore incomplets.
pub fn lire(arguments: &str) -> Brouillon {
    let chemin = CLES_CHEMIN.iter().find_map(|cle| {
        let debut = valeur_de(arguments, cle)?;
        let (texte, ferme) = chaine(arguments, debut);
        (ferme && !texte.is_empty()).then_some(texte)
    });
    let contenu = CLES_CONTENU
        .iter()
        .find_map(|cle| valeur_de(arguments, cle).map(|d| chaine(arguments, d).0));
    match contenu {
        Some(texte) => {
            let n = texte.chars().count();
            let fin: String = texte.chars().skip(n.saturating_sub(LONGUEUR_FIN)).collect();
            Brouillon {
                chemin,
                caracteres: n as u64,
                fin,
            }
        }
        None => Brouillon {
            chemin,
            caracteres: arguments.chars().count() as u64,
            fin: String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_un_fichier_en_cours_d_ecriture() {
        let b = lire(
            r#"{"path": "site/index.html", "content": "<!doctype html>\n<h1 class=\"t\">Bonjour é"#,
        );
        assert_eq!(b.chemin.as_deref(), Some("site/index.html"));
        assert_eq!(b.fin, "<!doctype html>\n<h1 class=\"t\">Bonjour é");
        assert_eq!(b.caracteres, 39);
    }

    #[test]
    fn un_chemin_incomplet_n_est_pas_un_chemin() {
        let b = lire(r#"{"path": "site/ind"#);
        assert_eq!(b.chemin, None);
    }

    #[test]
    fn un_echappement_coupe_ne_casse_rien() {
        let b = lire(r#"{"content": "ligne\"#);
        assert_eq!(b.fin, "ligne");
        let b = lire(r#"{"content": "a\u00"#);
        assert_eq!(b.fin, "a");
    }

    #[test]
    fn sans_contenu_on_compte_les_arguments() {
        let b = lire(r#"{"query": "météo"#);
        assert_eq!(b.fin, "");
        assert!(b.caracteres > 0);
    }

    #[test]
    fn la_fin_est_bornee() {
        let long = "x".repeat(LONGUEUR_FIN * 2);
        let b = lire(&format!(r#"{{"content": "{long}"#));
        assert_eq!(b.fin.chars().count(), LONGUEUR_FIN);
        assert_eq!(b.caracteres, (LONGUEUR_FIN * 2) as u64);
    }
}
