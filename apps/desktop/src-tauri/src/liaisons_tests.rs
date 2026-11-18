//! Chaque appel `invoke("…", { … })` de `src/lib/core.ts` doit viser une
//! commande enregistrée dans `generate_handler!`, avec les noms d'arguments
//! que Tauri attend (paramètres Rust en camelCase).
//!
//! Une clé mal nommée ne se voit ni à la compilation TypeScript ni à celle de
//! Rust : la commande échoue à l'exécution (« missing required key »), et le
//! réglage qui l'appelait ne s'enregistre jamais. C'est arrivé à la case
//! « Quitter à la fermeture » (`{ prefs }` au lieu de `{ preferences }`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;

fn racine() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn lire(chemin: &Path) -> String {
    std::fs::read_to_string(chemin)
        .unwrap_or_else(|e| panic!("lecture de {} : {e}", chemin.display()))
}

/// Découpe au premier niveau d'imbrication, sur les virgules.
fn decouper(texte: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut profondeur = 0i32;
    let mut courant = String::new();
    for c in texte.chars() {
        match c {
            '(' | '<' | '[' | '{' => profondeur += 1,
            ')' | '>' | ']' | '}' => profondeur -= 1,
            _ => {}
        }
        if c == ',' && profondeur == 0 {
            parts.push(std::mem::take(&mut courant));
        } else {
            courant.push(c);
        }
    }
    parts.push(courant);
    parts
}

fn camel(nom: &str) -> String {
    let mut sortie = String::new();
    let mut majuscule = false;
    for c in nom.trim_start_matches('_').chars() {
        if c == '_' {
            majuscule = true;
        } else if majuscule {
            sortie.extend(c.to_uppercase());
            majuscule = false;
        } else {
            sortie.push(c);
        }
    }
    sortie
}

/// Ce que Tauri injecte lui-même : jamais attendu dans l'objet d'arguments.
fn est_injecte(type_rust: &str) -> bool {
    let t = type_rust.trim().trim_start_matches("tauri::");
    let base = t
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default();
    ["State", "AppHandle", "Window", "WebviewWindow", "Webview"].contains(&base)
}

/// Paramètres de commande : nom camelCase -> optionnel ou non.
type Parametres = BTreeMap<String, bool>;

fn parametres(signature: &str) -> Parametres {
    let sans_commentaires = Regex::new(r"//[^\n]*").unwrap().replace_all(signature, "");
    let mut sortie = Parametres::new();
    for p in decouper(&sans_commentaires) {
        let Some((nom, type_rust)) = p.split_once(':') else {
            continue;
        };
        if est_injecte(type_rust) {
            continue;
        }
        let nom = nom.trim().trim_start_matches("mut ").trim();
        sortie.insert(camel(nom), type_rust.trim().starts_with("Option<"));
    }
    sortie
}

fn commandes_rust() -> BTreeMap<String, Parametres> {
    let motif = Regex::new(
        r"(?s)#\[tauri::command[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*(?:<[^>]*>)?\s*\((.*?)\)\s*(?:->|\{)",
    )
    .unwrap();
    let mut sortie = BTreeMap::new();
    let dossier = racine().join("src");
    for entree in std::fs::read_dir(&dossier).expect("dossier src") {
        let chemin = entree.expect("entrée du dossier src").path();
        if chemin.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let texte = lire(&chemin);
        for m in motif.captures_iter(&texte) {
            sortie.insert(m[1].to_string(), parametres(&m[2]));
        }
    }
    sortie
}

fn commandes_enregistrees() -> BTreeSet<String> {
    let texte = lire(&racine().join("src").join("lib.rs"));
    let debut = texte.find("generate_handler![").expect("generate_handler!");
    let fin = debut + texte[debut..].find("])").expect("fin de generate_handler!");
    let sans_commentaires = Regex::new(r"//[^\n]*")
        .unwrap()
        .replace_all(&texte[debut + "generate_handler![".len()..fin], "")
        .to_string();
    sans_commentaires
        .split(',')
        .map(|s| s.trim().rsplit("::").next().unwrap_or_default().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `(commande, clés envoyées, ligne)` pour chaque `invoke` littéral.
fn appels_ts() -> Vec<(String, Option<BTreeSet<String>>, usize)> {
    let texte = lire(&racine().join("..").join("src").join("lib").join("core.ts"));
    let motif =
        Regex::new(r#"(?s)invoke(?:<[^(]*?>)?\(\s*"(\w+)"\s*(?:,\s*\{(.*?)\}\s*)?\)"#).unwrap();
    motif
        .captures_iter(&texte)
        .map(|m| {
            let ligne = texte[..m.get(0).map_or(0, |g| g.start())]
                .matches('\n')
                .count()
                + 1;
            let cles = m.get(2).map(|corps| cles_objet(corps.as_str()));
            (m[1].to_string(), cles, ligne)
        })
        .collect()
}

fn cles_objet(corps: &str) -> BTreeSet<String> {
    decouper(corps)
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .map(|c| {
            c.trim_start_matches("...")
                .split(|ch: char| ch == ':' || ch.is_whitespace())
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// Un objet étalé (`{ ...x }`) ne se vérifie pas ici : ses clés sont ailleurs.
fn ecarts(attendus: &Parametres, envoyees: &BTreeSet<String>) -> Option<String> {
    if envoyees.iter().any(|c| c.is_empty()) {
        return None;
    }
    let manquantes: Vec<&String> = attendus
        .iter()
        .filter(|(nom, optionnel)| !**optionnel && !envoyees.contains(*nom))
        .map(|(nom, _)| nom)
        .collect();
    let inconnues: Vec<&String> = envoyees
        .iter()
        .filter(|c| !attendus.contains_key(*c))
        .collect();
    (!manquantes.is_empty() || !inconnues.is_empty())
        .then(|| format!("manquantes {manquantes:?}, inconnues {inconnues:?}"))
}

#[test]
fn chaque_invoke_de_core_ts_vise_une_commande_enregistree() {
    let enregistrees = commandes_enregistrees();
    let orphelins: Vec<String> = appels_ts()
        .into_iter()
        .filter(|(cmd, _, _)| !enregistrees.contains(cmd))
        .map(|(cmd, _, ligne)| format!("core.ts:{ligne} {cmd}"))
        .collect();
    assert!(
        orphelins.is_empty(),
        "commandes absentes de generate_handler! : {orphelins:?}"
    );
}

#[test]
fn chaque_invoke_de_core_ts_envoie_les_arguments_attendus() {
    let commandes = commandes_rust();
    let vide = BTreeSet::new();
    let fautes: Vec<String> = appels_ts()
        .into_iter()
        .filter_map(|(cmd, cles, ligne)| {
            let attendus = commandes.get(&cmd)?;
            ecarts(attendus, cles.as_ref().unwrap_or(&vide))
                .map(|e| format!("core.ts:{ligne} {cmd} : {e}"))
        })
        .collect();
    assert!(
        fautes.is_empty(),
        "arguments d'invoke incorrects : {fautes:#?}"
    );
}
