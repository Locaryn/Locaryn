//! Vérifier son propre travail, plutôt que de le déclarer fini.
//!
//! Un modèle qui écrit du Rust puis annonce « c'est corrigé » sans compiler
//! affirme quelque chose qu'il n'a pas constaté. La machine est là, la commande
//! existe, et elle prend quelques secondes : ne pas la lancer transforme le
//! travail en pari, et l'utilisateur en testeur.
//!
//! Ce module ne devine rien. Il **lit** la racine du projet et ne nomme que ce
//! qu'il y a trouvé : pas de `Cargo.toml`, pas de `cargo check` proposé ; un
//! `package.json` sans script `test`, aucun `test` proposé. Une commande
//! inventée serait pire que rien — le modèle la lancerait, l'échec porterait
//! sur la commande et non sur son travail, et il conclurait de travers.
//!
//! # Ce qui n'est pas ici
//!
//! Rien n'oblige un projet à être du code. Un dossier de dessins ou un mémoire
//! n'a pas de commande de vérification, et [`verifications`] rend alors une
//! liste vide : aucune consigne n'est ajoutée, et le modèle n'est pas envoyé
//! chercher un compilateur qui n'a pas lieu d'être. La vérification d'un
//! travail non technique se fait avec la personne, pas avec un terminal.

use std::path::Path;

/// Une façon de constater que le travail tient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    /// La commande exacte, telle qu'elle sera lancée.
    pub commande: String,
    /// Ce qu'elle prouve, en quelques mots.
    pub objet: &'static str,
    /// Vrai quand elle ne fait que lire — compiler, typer, analyser. Faux
    /// quand elle exécute le projet, ce qui peut avoir des effets.
    pub lecture_seule: bool,
}

impl Verification {
    fn compile(commande: impl Into<String>) -> Self {
        Self {
            commande: commande.into(),
            objet: "que le projet compile encore",
            lecture_seule: true,
        }
    }

    fn teste(commande: impl Into<String>) -> Self {
        Self {
            commande: commande.into(),
            objet: "que les tests passent encore",
            lecture_seule: false,
        }
    }
}

/// Les scripts déclarés dans un `package.json`, s'il y en a un.
///
/// Séparé pour être testable sans écrire de fichier, et parce que l'erreur à
/// éviter est précise : proposer `npm test` à un projet qui n'a pas de script
/// `test`. La commande échouerait, et le modèle croirait avoir cassé quelque
/// chose.
fn scripts_declares(contenu: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(contenu) else {
        return Vec::new();
    };
    json.get("scripts")
        .and_then(|s| s.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// Le lanceur de paquets à employer, d'après le verrou présent.
///
/// Lancer `npm run build` dans un dépôt géré par pnpm marche parfois, et
/// parfois réécrit un `package-lock.json` que personne n'a demandé. Le verrou
/// dit lequel est le bon.
fn lanceur(racine: &Path) -> &'static str {
    if racine.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if racine.join("yarn.lock").exists() {
        "yarn"
    } else if racine.join("bun.lockb").exists() {
        "bun"
    } else {
        "npm"
    }
}

/// Ce qui permet de constater que le travail tient, dans ce projet-ci.
///
/// Vide quand rien n'a été reconnu — un dossier de dessins, un mémoire, ou un
/// langage dont on ne sait pas encore lire l'outillage. Une liste vide est une
/// réponse honnête : mieux vaut ne rien proposer que d'envoyer le modèle
/// lancer une commande absente.
#[must_use]
pub fn verifications(racine: &Path) -> Vec<Verification> {
    let mut out = Vec::new();

    if racine.join("Cargo.toml").exists() {
        // `check` et non `build` : on cherche à savoir si ça compile, pas à
        // produire un binaire. C'est le même verdict, en bien moins de temps.
        out.push(Verification::compile("cargo check"));
        out.push(Verification::teste("cargo test"));
    }

    if let Ok(contenu) = std::fs::read_to_string(racine.join("package.json")) {
        let scripts = scripts_declares(&contenu);
        let outil = lanceur(racine);
        // L'ordre suit le coût : on type avant de tester, on teste avant de
        // construire. Un modèle qui suit la liste s'arrête au premier échec.
        for nom in ["typecheck", "lint", "test", "build"] {
            if scripts.iter().any(|s| s == nom) {
                let commande = format!("{outil} run {nom}");
                out.push(match nom {
                    "test" => Verification::teste(commande),
                    "typecheck" | "lint" => Verification {
                        commande,
                        objet: "que le code passe le contrôle de types et de style",
                        lecture_seule: true,
                    },
                    _ => Verification::compile(commande),
                });
            }
        }
        // `tsconfig.json` sans script : `tsc --noEmit` reste vrai, et c'est la
        // vérification la moins chère qui existe pour du TypeScript.
        if racine.join("tsconfig.json").exists()
            && !scripts.iter().any(|s| s == "typecheck" || s == "build")
        {
            out.push(Verification {
                commande: "npx tsc --noEmit".into(),
                objet: "que les types tiennent",
                lecture_seule: true,
            });
        }
    }

    if racine.join("go.mod").exists() {
        out.push(Verification::compile("go build ./..."));
        out.push(Verification::teste("go test ./..."));
    }

    if racine.join("pyproject.toml").exists() || racine.join("setup.py").exists() {
        // Python ne compile pas : le plus proche d'un « ça tient » est la
        // batterie de tests, quand elle existe.
        if racine.join("tests").is_dir() || racine.join("test").is_dir() {
            out.push(Verification::teste("python -m pytest -q"));
        }
    }

    if racine.join("Makefile").exists() {
        // On ne lit pas le Makefile : une cible `test` peut déployer. On se
        // contente de dire qu'il existe, le modèle regardera.
        out.push(Verification {
            commande: "make -n".into(),
            objet: "quelles cibles ce projet déclare, avant d'en lancer une",
            lecture_seule: true,
        });
    }

    out
}

/// La consigne à ajouter au message système, ou `None` s'il n'y a rien à dire.
///
/// Formulée comme une attente, pas comme une option : « lancez », pas
/// « vous pouvez lancer ». Et bornée à ce qui a été modifié — relancer toute
/// la batterie de tests après avoir corrigé une faute de frappe dans un
/// commentaire ferait perdre plus de temps que la faute.
#[must_use]
pub fn consigne(racine: &Path) -> Option<String> {
    let liste = verifications(racine);
    if liste.is_empty() {
        return None;
    }
    let mut texte = String::from(
        "After you change files in this project, verify your own work instead of \
         declaring it done. This project can be checked with:\n",
    );
    for v in &liste {
        texte.push_str(&format!("- `{}` — {}\n", v.commande, v.objet));
    }
    texte.push_str(
        "\nRun the cheapest relevant one with `run_command`, read its output, and fix what \
         it reports before you answer. If it fails for a reason you cannot fix, say so and \
         quote the error — do not describe the work as finished. Skip this only when you \
         changed nothing that these commands cover, such as a comment or a document.",
    );
    Some(texte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Un dossier neuf par appel : deux tests en parallele ne doivent pas se
    /// relire l'un l'autre. `tempfile` n'est pas une dependance de l'espace de
    /// travail, et en ajouter une pour six tests serait cher.
    struct Dossier(PathBuf);

    impl Dossier {
        fn neuf() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let d = std::env::temp_dir().join(format!(
                "locaryn_verif_{}_{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).expect("dossier de test");
            Self(d)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn ecrire(&self, nom: &str, contenu: &str) {
            std::fs::write(self.0.join(nom), contenu).expect("ecriture");
        }
    }

    impl Drop for Dossier {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Un dossier quelconque ne propose rien. Un projet de dessins n'a pas de
    /// compilateur, et l'y envoyer chercher serait absurde.
    #[test]
    fn un_dossier_sans_outillage_ne_propose_rien() {
        let d = Dossier::neuf();
        d.ecrire("croquis.txt", "A2, grain torchon");
        assert!(verifications(d.path()).is_empty());
        assert!(consigne(d.path()).is_none(), "aucune consigne à donner");
    }

    #[test]
    fn un_projet_rust_se_compile_et_se_teste() {
        let d = Dossier::neuf();
        d.ecrire("Cargo.toml", "[package]\nname = \"x\"");
        let v = verifications(d.path());
        assert_eq!(v[0].commande, "cargo check");
        assert!(v[0].lecture_seule, "compiler ne change rien");
        assert!(v.iter().any(|x| x.commande == "cargo test"));
        let c = consigne(d.path()).unwrap();
        assert!(c.contains("cargo check"));
    }

    /// L'erreur à ne pas commettre : proposer `npm test` là où aucun script
    /// `test` n'existe. La commande échouerait, et le modèle croirait avoir
    /// cassé quelque chose.
    #[test]
    fn un_script_absent_n_est_pas_propose() {
        let d = Dossier::neuf();
        d.ecrire("package.json", r#"{"scripts": {"build": "vite build"}}"#);
        let v = verifications(d.path());
        assert!(v.iter().any(|x| x.commande == "npm run build"));
        assert!(
            !v.iter().any(|x| x.commande.contains("test")),
            "aucun script test declare, donc rien a proposer"
        );
    }

    /// Le verrou dit quel lanceur employer : `npm` dans un dépôt pnpm réécrit
    /// un fichier de verrou que personne n'a demandé.
    #[test]
    fn le_verrou_choisit_le_lanceur() {
        let d = Dossier::neuf();
        d.ecrire("package.json", r#"{"scripts":{"test":"v"}}"#);
        assert!(verifications(d.path())[0].commande.starts_with("npm "));
        d.ecrire("pnpm-lock.yaml", "lockfileVersion: 9");
        assert_eq!(verifications(d.path())[0].commande, "pnpm run test");
    }

    /// L'ordre suit le coût : typer avant de tester, tester avant de
    /// construire. Un modèle qui suit la liste s'arrête au premier échec.
    #[test]
    fn l_ordre_suit_le_cout() {
        let d = Dossier::neuf();
        d.ecrire(
            "package.json",
            r#"{"scripts":{"build":"b","test":"t","typecheck":"tc"}}"#,
        );
        let noms: Vec<String> = verifications(d.path())
            .into_iter()
            .map(|v| v.commande)
            .collect();
        assert_eq!(
            noms,
            vec!["npm run typecheck", "npm run test", "npm run build"]
        );
    }

    /// Sans script mais avec un `tsconfig.json`, `tsc --noEmit` reste vrai.
    #[test]
    fn typescript_sans_script_garde_tsc() {
        let d = Dossier::neuf();
        d.ecrire("package.json", "{}");
        d.ecrire("tsconfig.json", "{}");
        assert!(verifications(d.path())
            .iter()
            .any(|v| v.commande == "npx tsc --noEmit"));
    }

    /// ... mais pas en double quand un script le fait déjà.
    #[test]
    fn tsc_ne_double_pas_un_script_existant() {
        let d = Dossier::neuf();
        d.ecrire(
            "package.json",
            r#"{"scripts":{"typecheck":"tsc --noEmit"}}"#,
        );
        d.ecrire("tsconfig.json", "{}");
        let v = verifications(d.path());
        assert_eq!(v.len(), 1, "une seule verification de types");
    }

    /// Un `package.json` illisible ne fait pas tomber la détection : on n'en
    /// tire simplement aucun script.
    #[test]
    fn un_package_json_casse_ne_propose_rien() {
        let d = Dossier::neuf();
        d.ecrire("package.json", "{ pas du json");
        assert!(verifications(d.path()).is_empty());
    }

    /// Python sans dossier de tests ne propose pas pytest : il n'y aurait rien
    /// à exécuter, et l'échec porterait sur l'absence de tests.
    #[test]
    fn python_sans_tests_ne_propose_pas_pytest() {
        let d = Dossier::neuf();
        d.ecrire("pyproject.toml", "[project]");
        assert!(verifications(d.path()).is_empty());
        std::fs::create_dir(d.path().join("tests")).expect("dossier tests");
        assert_eq!(verifications(d.path()).len(), 1);
    }

    /// La consigne dit quoi faire de l'échec : le citer, et ne pas annoncer
    /// que c'est fini. Sans cette phrase, un modèle lance la commande, la voit
    /// échouer, et répond quand même « corrigé ».
    #[test]
    fn la_consigne_interdit_d_annoncer_fini_apres_un_echec() {
        let d = Dossier::neuf();
        d.ecrire("go.mod", "module x");
        let c = consigne(d.path()).unwrap();
        assert!(c.contains("quote the error"));
        assert!(c.contains("do not describe the work as finished"));
    }
}
