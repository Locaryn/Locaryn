//! Séparer l'en-tête d'un fichier Markdown de son corps.
//!
//! Quatre crates faisaient ce travail, chacune avec sa copie : les profils
//! d'agent, les commandes, les compétences et le chargeur d'extensions. Deux
//! étaient identiques au caractère près, les deux autres avaient divergé — et
//! pas sur un détail.
//!
//! Devant un fichier **sans** en-tête, ou dont l'en-tête n'est pas refermé,
//! `agent-runtime` et `command-runtime` renvoyaient un corps vide : le contenu
//! du fichier était silencieusement perdu. Un profil d'agent écrit sans
//! en-tête n'avait donc aucune instruction, sans que rien ne le dise.
//! `skill-runtime` et le chargeur d'extensions, eux, rendaient le fichier
//! entier comme corps — ce qui est la bonne lecture : un fichier sans en-tête
//! est un fichier qui n'a que du corps.
//!
//! C'est cette lecture-là qui est retenue ici, et une seule copie ne peut plus
//! diverger.

/// L'en-tête et le corps d'un document.
///
/// L'en-tête est rendu sans ses délimiteurs, prêt à être lu ligne à ligne. Le
/// corps est débarrassé des retours à la ligne qui suivaient le délimiteur de
/// fermeture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter<'a> {
    pub header: &'a str,
    pub body: &'a str,
}

/// Sépare l'en-tête du corps.
///
/// Reconnaît un en-tête ouvert par `---` en tête de fichier et fermé par une
/// ligne `---`. Les fins de ligne Windows sont acceptées, ainsi qu'une marque
/// d'ordre des octets en tête.
///
/// Sans en-tête reconnaissable, l'en-tête est vide et **le corps est le fichier
/// entier** : on ne perd jamais le contenu.
pub fn split(raw: &str) -> Frontmatter<'_> {
    let trimmed = raw.trim_start_matches('\u{feff}');

    let Some(apres_ouverture) = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
    else {
        return Frontmatter {
            header: "",
            body: trimmed,
        };
    };

    // La fermeture est une ligne qui ne contient que `---`. On cherche donc le
    // saut de ligne qui la précède, pour ne pas confondre avec un `---` au
    // milieu d'une valeur.
    let fermeture = apres_ouverture
        .find("\n---\n")
        .map(|i| (i, 5))
        .or_else(|| apres_ouverture.find("\r\n---\r\n").map(|i| (i, 7)))
        .or_else(|| {
            // Un en-tête fermé en toute fin de fichier, sans corps derrière.
            apres_ouverture
                .strip_suffix("\n---")
                .map(|h| (h.len(), apres_ouverture.len() - h.len()))
        });

    match fermeture {
        Some((fin, largeur)) => Frontmatter {
            header: &apres_ouverture[..fin],
            body: apres_ouverture[fin + largeur..].trim_start_matches(['\r', '\n']),
        },
        // Ouvert mais jamais refermé : le fichier n'a pas d'en-tête utilisable,
        // et son contenu reste son corps.
        None => Frontmatter {
            header: "",
            body: trimmed,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_en_tete_ferme_se_lit_des_deux_cotes() {
        let f = split("---\nname: essai\n---\nLe corps.\n");
        assert_eq!(f.header, "name: essai");
        assert_eq!(f.body, "Le corps.\n");
    }

    #[test]
    fn les_fins_de_ligne_windows_passent() {
        let f = split("---\r\nname: essai\r\n---\r\nLe corps.\r\n");
        assert_eq!(f.header, "name: essai");
        assert_eq!(f.body, "Le corps.\r\n");
    }

    #[test]
    fn une_marque_dordre_des_octets_ne_gene_pas() {
        let f = split("\u{feff}---\nname: essai\n---\ncorps");
        assert_eq!(f.header, "name: essai");
        assert_eq!(f.body, "corps");
    }

    /// Le cas qui divergeait, et le motif de ce module : deux crates rendaient
    /// ici un corps vide, perdant tout le fichier.
    #[test]
    fn sans_en_tete_le_fichier_entier_est_le_corps() {
        let f = split("Juste des instructions, sans en-tete.\n");
        assert_eq!(f.header, "");
        assert_eq!(f.body, "Juste des instructions, sans en-tete.\n");
    }

    /// L'autre moitie du meme piege : un en-tete ouvert et jamais referme.
    #[test]
    fn un_en_tete_non_ferme_ne_mange_pas_le_corps() {
        let brut = "---\nname: oubli\nLe corps qui suit.\n";
        let f = split(brut);
        assert_eq!(f.header, "");
        assert_eq!(f.body, brut);
    }

    #[test]
    fn un_en_tete_sans_corps_derriere_reste_lisible() {
        let f = split("---\nname: essai\n---");
        assert_eq!(f.header, "name: essai");
        assert_eq!(f.body, "");
    }

    /// Un tiret triple au milieu d'une valeur ne ferme rien.
    #[test]
    fn un_tiret_triple_en_milieu_de_ligne_ne_ferme_pas() {
        let f = split("---\nrule: a --- b\n---\ncorps\n");
        assert_eq!(f.header, "rule: a --- b");
        assert_eq!(f.body, "corps\n");
    }

    #[test]
    fn un_fichier_vide_ne_panique_pas() {
        let f = split("");
        assert_eq!(f.header, "");
        assert_eq!(f.body, "");
    }
}

/// Lit une valeur d'en-tête qui porte une liste.
///
/// Deux écritures circulent dans les fichiers, et les deux doivent passer :
/// la forme entre crochets (`[a, "b", c]`) et la forme séparée par des espaces
/// (`a b c`). Les guillemets et les blancs tombent, les entrées vides aussi.
///
/// Trois crates en portaient une copie identique — profils d'agent, commandes,
/// compétences. Identiques aujourd'hui ne veut pas dire identiques demain.
pub fn parse_list(s: &str) -> Vec<String> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        inner
            .split(',')
            .map(|x| x.trim().trim_matches('"').to_string())
            .filter(|x| !x.is_empty())
            .collect()
    } else {
        s.split_whitespace().map(str::to_string).collect()
    }
}

#[cfg(test)]
mod liste_tests {
    use super::parse_list;

    #[test]
    fn les_deux_ecritures_donnent_la_meme_liste() {
        assert_eq!(parse_list("[a, \"b\", c]"), vec!["a", "b", "c"]);
        assert_eq!(parse_list("a b c"), vec!["a", "b", "c"]);
        assert_eq!(parse_list("  [ a , , b ] "), vec!["a", "b"]);
    }

    #[test]
    fn une_valeur_vide_ne_donne_rien() {
        assert!(parse_list("").is_empty());
        assert!(parse_list("[]").is_empty());
        assert!(parse_list("   ").is_empty());
    }
}
