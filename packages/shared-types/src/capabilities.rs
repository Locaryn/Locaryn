//! La liste canonique des capacités d'extension.
//!
//! Un seul fichier fait foi — `../capabilities.json` : le daemon le lit ici
//! (`include_str!`), l'ordinateur et le téléphone lisent le même fichier via
//! `@locaryn/ui-core`, et la documentation y renvoie au lieu d'en tenir une
//! copie. Ajouter une capacité se fait donc à un seul endroit, et la doc ne
//! peut plus diverger du code.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Une capacité reconnue : le mot que l'interface comprend, son nom lisible
/// et ce qu'elle apporte.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// Le contenu brut de la liste canonique, embarqué à la compilation.
pub const CAPABILITIES_JSON: &str = include_str!("../capabilities.json");

/// Toutes les capacités reconnues, dans l'ordre du fichier canonique.
pub fn all() -> &'static [Capability] {
    static ALL: OnceLock<Vec<Capability>> = OnceLock::new();
    ALL.get_or_init(|| {
        serde_json::from_str(CAPABILITIES_JSON)
            .expect("packages/shared-types/capabilities.json doit être un tableau valide")
    })
    .as_slice()
}

/// `true` si `id` est une capacité reconnue.
pub fn is_known(id: &str) -> bool {
    all().iter().any(|c| c.id == id)
}

/// La capacité `id`, si elle est reconnue.
pub fn get(id: &str) -> Option<&'static Capability> {
    all().iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_liste_canonique_se_parse_et_contient_figures() {
        let caps = all();
        assert!(!caps.is_empty(), "la liste canonique ne doit pas être vide");
        // `figures` est une capacité reconnue : plugin-figures la déclare, et
        // l'écran Figures ne doit pas dépendre d'une liste documentaire.
        assert!(is_known("figures"));
        assert!(is_known("image-gen"));
        assert!(!is_known("n-importe-quoi"));
    }

    #[test]
    fn pas_de_doublon_dans_la_liste_canonique() {
        let caps = all();
        let mut ids: Vec<&str> = caps.iter().map(|c| c.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            caps.len(),
            "un id ne doit apparaître qu'une fois"
        );
    }
}
