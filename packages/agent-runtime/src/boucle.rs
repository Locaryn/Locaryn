//! Reconnaître un modèle qui tourne en rond.
//!
//! Il n'y a pas de plafond au nombre d'actions d'une tâche : un long travail
//! en demande des centaines, et l'arrêter à un compte fixe obligeait à écrire
//! « continue ». Ce qui s'arrête, c'est une vraie boucle — le même passage
//! écrit sans fin dans une réponse, ou le même enchaînement d'appels rejoué.

/// Longueur de texte examinée en fin de réponse.
const FENETRE: usize = 12_000;

/// Longueur du motif cherché à la toute fin du texte.
const MOTIF: usize = 48;

/// Longueur totale répétée avant de conclure à une boucle. En dessous, un
/// tableau ou une liste aux lignes voisines reste un texte ordinaire.
const REPETE_MIN: usize = 1_200;

/// Un passage très court (« ha ha ha », un même jeton) doit s'étendre plus
/// longtemps : des tirets ou des espaces alignés ne sont pas une boucle.
const REPETE_MIN_COURT: usize = 2_400;
const PERIODE_COURTE: usize = 20;

/// Nombre minimal de copies d'affilée.
const COPIES_MIN: usize = 4;

/// Le texte finit-il par le même passage recopié à l'identique, encore et
/// encore ?
pub fn texte_en_boucle(texte: &str) -> bool {
    let octets = texte.as_bytes();
    let queue = &octets[octets.len().saturating_sub(FENETRE)..];
    if queue.len() < REPETE_MIN {
        return false;
    }
    let motif = &queue[queue.len() - MOTIF..];
    // L'occurrence précédente du motif donne la période du passage répété.
    let avant = &queue[..queue.len() - 1];
    let Some(debut) = avant.windows(MOTIF).rposition(|f| f == motif) else {
        return false;
    };
    let periode = queue.len() - MOTIF - debut;
    let a_couvrir = if periode < PERIODE_COURTE {
        REPETE_MIN_COURT
    } else {
        REPETE_MIN
    };
    let copies = COPIES_MIN.max(a_couvrir.div_ceil(periode));
    if queue.len() < copies * periode {
        return false;
    }
    let derniere = &queue[queue.len() - periode..];
    (1..copies).all(|k| {
        let fin = queue.len() - k * periode;
        &queue[fin - periode..fin] == derniere
    })
}

/// Période la plus longue examinée dans la suite des tours.
const CYCLE_MAX: usize = 8;

/// Nombre de fois qu'un enchaînement doit revenir à l'identique.
const CYCLES_MIN: usize = 3;

/// Tours gardés en mémoire : de quoi voir `CYCLES_MIN` cycles de `CYCLE_MAX`.
pub const TOURS_GARDES: usize = CYCLE_MAX * CYCLES_MIN;

/// Les derniers tours rejouent-ils le même enchaînement d'appels (A, B, A, B,
/// A, B) ? Chaque élément est la signature d'un tour : outils et arguments.
/// Un même appel répété seul (période 1) est compté ailleurs.
pub fn tours_en_cycle(signatures: &[String]) -> bool {
    (2..=CYCLE_MAX).any(|periode| {
        let longueur = periode * CYCLES_MIN;
        if signatures.len() < longueur {
            return false;
        }
        let queue = &signatures[signatures.len() - longueur..];
        let dernier = &queue[longueur - periode..];
        // Un cycle fait d'un seul appel répété relève de l'autre garde.
        let varie = dernier.iter().any(|s| s != &dernier[0]);
        varie && queue.chunks(periode).all(|bloc| bloc == dernier)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_paragraphe_recopie_sans_fin_est_une_boucle() {
        let paragraphe = "Je dois vérifier la structure du fichier avant d'écrire le composant. \
                          Regardons d'abord ce que contient le dossier src.\n\n";
        let texte = format!("Voici mon plan.\n\n{}", paragraphe.repeat(12));
        assert!(texte_en_boucle(&texte));
    }

    #[test]
    fn un_texte_ordinaire_n_est_pas_une_boucle() {
        let texte: String = (0..200)
            .map(|i| format!("Ligne {i} : la valeur calculée vaut {}.\n", i * 37))
            .collect();
        assert!(!texte_en_boucle(&texte));
    }

    #[test]
    fn quelques_lignes_identiques_ne_suffisent_pas() {
        let texte = format!(
            "{}{}",
            "x".repeat(2_000).replace("xx", "ab"),
            "  <li class=\"item\">élément</li>\n".repeat(6)
        );
        assert!(!texte_en_boucle(&texte));
    }

    #[test]
    fn un_meme_mot_repete_longtemps_est_une_boucle() {
        let texte = format!("Début de la réponse. {}", "oui ".repeat(900));
        assert!(texte_en_boucle(&texte));
    }

    #[test]
    fn une_coupure_au_milieu_d_un_caractere_ne_plante_pas() {
        let texte = "é".repeat(9_000);
        assert!(texte_en_boucle(&texte));
    }

    fn tours(suite: &[&str]) -> Vec<String> {
        suite.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn un_enchainement_rejoue_trois_fois_est_un_cycle() {
        assert!(tours_en_cycle(&tours(&["x", "a", "b", "a", "b", "a", "b"])));
        assert!(tours_en_cycle(&tours(&[
            "a", "b", "c", "a", "b", "c", "a", "b", "c"
        ])));
    }

    #[test]
    fn des_tours_differents_ne_sont_pas_un_cycle() {
        assert!(!tours_en_cycle(&tours(&["a", "b", "a", "b", "a", "c"])));
        assert!(!tours_en_cycle(&tours(&["a", "b", "c", "d", "e", "f"])));
        // Le même appel seul : l'autre garde s'en charge.
        assert!(!tours_en_cycle(&tours(&["a", "a", "a", "a", "a", "a"])));
    }
}
