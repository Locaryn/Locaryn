//! Le dossier de travail d'une conversation libre.
//!
//! Une conversation sans dossier choisi travaille quand même quelque part : les
//! fichiers que le modèle écrit y atterrissent, et la personne doit pouvoir les
//! retrouver. Un identifiant aléatoire (`3f9a1c2e-…`) ne se reconnaît pas dans
//! l'explorateur ; un nom comme `2026-10-04-renard-calme` oui, à la manière des
//! dossiers que créent Claude ou Antigravity.
//!
//! Le nom se déduit de l'identifiant et de la date de création de la session :
//! même entrée, même dossier, sans table ni balayage du disque, et sans dépendre
//! d'un message qui n'existe pas encore à la création.

use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Adjectifs identiques au masculin et au féminin, pour que « renard calme » et
/// « lune calme » s'accordent sans règle.
const ADJECTIFS: [&str; 32] = [
    "calme",
    "brave",
    "rapide",
    "libre",
    "sobre",
    "agile",
    "sage",
    "rare",
    "simple",
    "utile",
    "tendre",
    "jeune",
    "svelte",
    "dense",
    "limpide",
    "robuste",
    "paisible",
    "vaste",
    "souple",
    "ample",
    "modeste",
    "stable",
    "solide",
    "habile",
    "tranquille",
    "honnete",
    "sincere",
    "digne",
    "humble",
    "fertile",
    "mobile",
    "durable",
];

const NOMS: [&str; 64] = [
    "renard", "lune", "cedre", "falaise", "heron", "ruisseau", "colline", "faucon", "sentier",
    "orage", "brume", "lagune", "sapin", "galet", "aurore", "vallee", "marmotte", "banquise",
    "tilleul", "cascade", "chouette", "bruyere", "estuaire", "lynx", "prairie", "iris", "comete",
    "dune", "ardoise", "lierre", "hermine", "crique", "mesange", "fougere", "glacier", "saule",
    "cormoran", "rivage", "pivoine", "tourbe", "mistral", "sarcelle", "ajonc", "calanque",
    "bouleau", "loutre", "plateau", "myrtille", "caribou", "ecume", "alize", "garrigue", "cigogne",
    "neige", "oyat", "bruine", "ormeau", "cerf", "mousse", "raisin", "verveine", "etang", "pinson",
    "chene",
];

/// Un nom reconnaissable : `2026-10-04-renard-calme`.
///
/// Les mots viennent des octets de l'identifiant, la date de la création : deux
/// conversations du même jour ne se confondent que si leurs octets retombent sur
/// la même paire parmi 2 048, et le nom porte alors un suffixe (voir `resolve`).
pub fn readable_name(session_id: Uuid, created: chrono::DateTime<chrono::Utc>) -> String {
    let b = session_id.as_bytes();
    let adj = ADJECTIFS[usize::from(b[0]) % ADJECTIFS.len()];
    let nom = NOMS[usize::from(b[1]) % NOMS.len()];
    let jour = created.with_timezone(&chrono::Local).format("%Y-%m-%d");
    format!("{jour}-{nom}-{adj}")
}

/// Le dossier de cette conversation, sans le créer.
///
/// Les conversations d'avant ce changement ont un dossier nommé par leur
/// identifiant complet : on le garde, sans rien déplacer. Ensuite le nom lisible ;
/// si une autre conversation du même jour l'occupe déjà, on y ajoute le début de
/// l'identifiant plutôt que de partager le dossier.
pub fn resolve(root: &Path, session_id: Uuid, created: chrono::DateTime<chrono::Utc>) -> PathBuf {
    let legacy = root.join(session_id.to_string());
    if legacy.is_dir() {
        return legacy;
    }
    let base = readable_name(session_id, created);
    let plain = root.join(&base);
    // Libre, ou déjà le nôtre : c'est le bon nom.
    if !plain.join(MARKER).is_file() || belongs_to(&plain, session_id) {
        return plain;
    }
    // Pris par une autre conversation du même jour. Le suffixe vient de la fin de
    // l'identifiant : le début a déjà servi à choisir les mots, il ne
    // départagerait rien.
    let simple = session_id.simple().to_string();
    let short = &simple[simple.len() - 6..];
    root.join(format!("{base}-{short}"))
}

/// Le fichier qui dit à quelle conversation appartient un dossier, pour que deux
/// conversations ne partagent pas le même nom.
pub const MARKER: &str = ".locaryn-session";

fn belongs_to(dir: &Path, session_id: Uuid) -> bool {
    std::fs::read_to_string(dir.join(MARKER))
        .map(|s| s.trim() == session_id.to_string())
        .unwrap_or(false)
}

/// Créer le dossier (s'il n'existe pas) et y noter à qui il appartient.
pub async fn ensure(dir: &Path, session_id: Uuid) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let marker = dir.join(MARKER);
    if !marker.exists() {
        tokio::fs::write(&marker, session_id.to_string()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn jour() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap()
    }

    fn racine(nom: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("locaryn-free-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn le_nom_est_lisible_et_stable() {
        let id = Uuid::parse_str("3f9a1c2e-0000-4000-8000-000000000000").unwrap();
        let a = readable_name(id, jour());
        assert_eq!(a, readable_name(id, jour()));
        // Date, puis deux mots : aucun identifiant brut.
        assert!(
            chrono::NaiveDate::parse_from_str(&a[..10], "%Y-%m-%d").is_ok(),
            "{a}"
        );
        assert!(!a.contains("3f9a"), "{a}");
        assert!(a.matches('-').count() >= 4, "{a}");
    }

    #[test]
    fn un_ancien_dossier_a_identifiant_est_conserve() {
        let root = racine("legacy");
        let id = Uuid::new_v4();
        std::fs::create_dir_all(root.join(id.to_string())).unwrap();
        assert_eq!(resolve(&root, id, jour()), root.join(id.to_string()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deux_conversations_au_meme_nom_ne_partagent_pas_leur_dossier() {
        let root = racine("collision");
        let a = Uuid::from_bytes([1; 16]);
        // Même octets de tête : même nom lisible.
        let mut octets = [1u8; 16];
        octets[15] = 9;
        let b = Uuid::from_bytes(octets);
        assert_eq!(readable_name(a, jour()), readable_name(b, jour()));

        let dir_a = resolve(&root, a, jour());
        block(ensure(&dir_a, a));
        let dir_b = resolve(&root, b, jour());
        assert_ne!(dir_a, dir_b, "la seconde doit avoir son propre dossier");
        block(ensure(&dir_b, b));
        // Et chacune retrouve le sien.
        assert_eq!(resolve(&root, a, jour()), dir_a);
        assert_eq!(resolve(&root, b, jour()), dir_b);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn block<F: std::future::Future<Output = std::io::Result<()>>>(f: F) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f)
            .unwrap();
    }
}
