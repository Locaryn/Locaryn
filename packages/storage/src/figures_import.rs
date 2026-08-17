//! L'import des figures livrées avec une extension.
//!
//! Une extension qui apporte la capacité `figures` transporte, dans son
//! dossier `figures/`, des figures prêtes à l'emploi : des fichiers Markdown
//! au format des Agent Skills — un en-tête pour les réglages, le corps pour
//! les consignes. Le README du dépôt le promet (« Ils sont dans figures/ et
//! s'installent avec l'extension ») ; c'est ici que la promesse est tenue.
//!
//! L'import est versé dans la base à l'installation comme à chaque démarrage
//! (une installation doit survivre à un redémarrage). L'`upsert` par nom fait
//! le reste : réinstaller une extension met à jour les figures du dépôt sans
//! créer de doubles, et ne touche jamais à une figure écrite à la main — le
//! travail de quelqu'un ne se fait pas écraser par une mise à jour.

use crate::figures::{FigureRepo, NouvelleFigure};
use std::path::Path;

/// Verser dans la base les figures du dossier `figures/` de l'extension
/// installée dans `dir`, marquées de `source` (le nom de l'extension).
///
/// Retourne le nombre de figures importées ou mises à jour. Un fichier sans
/// en-tête, sans nom, ou sans consignes est laissé de côté avec un
/// avertissement : un dépôt ne doit pas empêcher l'installation des figures
/// de ses voisines.
pub async fn importer(depot: &FigureRepo, dir: &Path, source: &str) -> usize {
    let dossier = dir.join("figures");
    let Ok(lecture) = std::fs::read_dir(&dossier) else {
        // Pas de dossier figures/ : cette extension n'apporte aucune figure.
        return 0;
    };

    let mut importees = 0usize;
    let mut refusees: Vec<String> = Vec::new();
    for entree in lecture.flatten() {
        let chemin = entree.path();
        if chemin.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(brut) = std::fs::read_to_string(&chemin) else {
            refusees.push(format!("{} : illisible", chemin.display()));
            continue;
        };
        let Some(f) = parse_figure(&brut) else {
            refusees.push(format!("{} : en-tête absent ou incomplet", chemin.display()));
            continue;
        };
        let Some(nom) = f.nom.filter(|n| !n.trim().is_empty()) else {
            refusees.push(format!("{} : sans nom", chemin.display()));
            continue;
        };
        if f.consignes.trim().is_empty() {
            refusees.push(format!("{} : sans consignes", chemin.display()));
            continue;
        }

        let neuve = NouvelleFigure {
            name: &nom,
            description: &f.description,
            instructions: &f.consignes,
            model: f.modele.as_deref().filter(|m| !m.trim().is_empty()),
            opening: f.ouverture.as_deref().filter(|o| !o.trim().is_empty()),
            uses_memory: f.memoire,
            tools: f.outils.as_deref(),
            source,
        };
        match depot.upsert(neuve).await {
            Ok(_) => importees += 1,
            Err(e) => refusees.push(format!("{} : {e}", chemin.display())),
        }
    }

    for refus in &refusees {
        tracing::warn!(fichier = %refus, "figure non importée");
    }
    importees
}

/// Ce qu'un fichier de figure transporte, une fois son en-tête lu.
struct FigureImportee {
    nom: Option<String>,
    description: String,
    consignes: String,
    modele: Option<String>,
    ouverture: Option<String>,
    memoire: bool,
    outils: Option<Vec<String>>,
}

/// Lire un fichier de figure : l'en-tête entre `---`, le corps en consignes.
///
/// L'en-tête est le YAML réduit des Agent Skills — des lignes `clé: valeur`
/// à plat, et un bloc `metadata:` indenté pour ce qui est propre aux figures
/// (`model`, `opening`, `memory`). Un fichier sans en-tête n'est pas une
/// figure : le parseur le dit en rendant `None`.
fn parse_figure(brut: &str) -> Option<FigureImportee> {
    let (en_tete, corps) = partager_en_tete(brut)?;
    let mut f = FigureImportee {
        nom: None,
        description: String::new(),
        consignes: corps.trim().to_string(),
        modele: None,
        ouverture: None,
        memoire: false,
        outils: None,
    };

    let mut dans_metadata = false;
    for ligne in en_tete.lines() {
        let indentee = ligne.starts_with(' ') || ligne.starts_with('\t');
        let ligne = ligne.trim();
        if ligne.is_empty() {
            continue;
        }
        // Une clé de niveau haut reprend : on quitte le bloc metadata. Dans
        // les dépôts, `metadata:` est la dernière clé — mais ne pas le
        // supposer rend le parseur indifférent à l'ordre.
        if !indentee && dans_metadata && ligne != "metadata:" {
            dans_metadata = false;
        }
        if !indentee && ligne == "metadata:" {
            dans_metadata = true;
            continue;
        }
        let Some((cle, valeur)) = ligne.split_once(':') else {
            continue;
        };
        match (dans_metadata, cle.trim()) {
            (false, "name") => f.nom = Some(devirer(valeur)),
            (false, "description") => f.description = devirer(valeur),
            // Une valeur vide ne choisit pas de modèle : c'est celui de
            // l'application qui fera tourner la figure.
            (true, "model") => f.modele = Some(devirer(valeur)).filter(|m| !m.is_empty()),
            (true, "opening") => f.ouverture = Some(devirer(valeur)).filter(|o| !o.is_empty()),
            (true, "memory") => f.memoire = devirer(valeur).eq_ignore_ascii_case("true"),
            // Une liste séparée par des virgules, dans l'en-tête du dépôt.
            (true, "tools") => {
                let liste: Vec<String> = devirer(valeur)
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                f.outils = (!liste.is_empty()).then_some(liste);
            }
            _ => {}
        }
    }
    Some(f)
}

/// Séparer l'en-tête (`---` … `---`) du corps d'un fichier de figure.
fn partager_en_tete(brut: &str) -> Option<(String, &str)> {
    let brut = brut.trim_start_matches('\u{feff}');
    let reste = brut.strip_prefix("---")?;
    let reste = reste.trim_start_matches(['\r', '\n']);
    let fin = reste.find("\n---")?;
    let en_tete = &reste[..fin];
    let corps = &reste[fin + 4..];
    Some((en_tete.to_string(), corps.trim_start_matches(['\r', '\n'])))
}

/// Dé-guillemeter une valeur : `"mot"` ou `'mot'` deviennent `mot`.
fn devirer(v: &str) -> String {
    v.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn depot() -> FigureRepo {
        let pool = crate::open_in_memory().await.unwrap();
        FigureRepo::new(pool)
    }

    #[test]
    fn lit_un_fichier_de_figure_du_depot() {
        // Le contenu réel de figures/relecteur.md : ce que le dépôt transporte
        // doit se lire tel quel.
        let brut = "---\r\nname: relecteur\r\ndescription: Relit un diff et signale les défauts de correction, pas de style. À ouvrir avant de proposer une fusion.\r\nlicense: Apache-2.0\r\nmetadata:\r\n  model: \"\"\r\n  opening: \"Colle le diff à relire.\"\r\n  memory: false\r\n---\r\n\r\nTu relis du code. Tu ne signales que ce qui peut casser.\r\n";
        let f = parse_figure(brut).expect("la figure du dépôt se lit");
        assert_eq!(f.nom.as_deref(), Some("relecteur"));
        assert!(f.description.contains("Relit un diff"));
        assert_eq!(f.consignes, "Tu relis du code. Tu ne signales que ce qui peut casser.");
        assert_eq!(f.modele, None, "model vide vaut « celui de l'application »");
        assert_eq!(f.ouverture.as_deref(), Some("Colle le diff à relire."));
        assert!(!f.memoire);
    }

    #[test]
    fn les_outils_de_l_en_tete_se_lisent() {
        let brut = "---
name: jarvis
metadata:
  tools: \"generate_speech, generate_image\"
---
Tu réponds en vocal.
";
        let f = parse_figure(brut).unwrap();
        assert_eq!(
            f.outils.as_deref(),
            Some(&["generate_speech".to_string(), "generate_image".to_string()][..])
        );
    }

    #[test]
    fn la_memoire_vraie_se_lit() {
        let brut = "---\nname: secretaire\nmetadata:\n  memory: true\n---\nNotes.\n";
        let f = parse_figure(brut).unwrap();
        assert!(f.memoire);
    }

    #[test]
    fn sans_en_tete_ce_n_est_pas_une_figure() {
        assert!(parse_figure("Juste du texte, sans en-tête.").is_none());
    }

    #[tokio::test]
    async fn importe_les_figures_d_un_depot_et_les_met_a_jour() {
        let d = depot().await;
        let dossier = std::env::temp_dir().join("locaryn-figures-import-test");
        let figures = dossier.join("figures");
        std::fs::create_dir_all(&figures).unwrap();

        std::fs::write(
            figures.join("relecteur.md"),
            "---\nname: Relecteur\nmetadata:\n  memory: false\n---\nVersion une.\n",
        )
        .unwrap();

        let n = importer(&d, &dossier, "Locaryn/plugin-figures").await;
        assert_eq!(n, 1);
        let liste = d.list().await.unwrap();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].name, "Relecteur");
        assert_eq!(liste[0].instructions, "Version une.");
        assert_eq!(liste[0].source, "Locaryn/plugin-figures");

        // Réinstaller le dépôt met à jour la figure sans la doubler.
        std::fs::write(
            figures.join("relecteur.md"),
            "---\nname: Relecteur\nmetadata:\n  memory: true\n---\nVersion deux.\n",
        )
        .unwrap();
        let n = importer(&d, &dossier, "Locaryn/plugin-figures").await;
        assert_eq!(n, 1);
        let liste = d.list().await.unwrap();
        assert_eq!(liste.len(), 1, "le nom identifie la figure, pas l'installation");
        assert_eq!(liste[0].instructions, "Version deux.");
        assert!(liste[0].uses_memory);

        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[tokio::test]
    async fn une_extension_sans_dossier_figures_n_importe_rien() {
        let d = depot().await;
        let dossier = std::env::temp_dir().join("locaryn-figures-import-vide");
        std::fs::create_dir_all(&dossier).unwrap();
        assert_eq!(importer(&d, &dossier, "plugin-figures").await, 0);
        assert!(d.list().await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[tokio::test]
    async fn un_fichier_sans_consignes_ne_rompt_pas_ses_voisines() {
        let d = depot().await;
        let dossier = std::env::temp_dir().join("locaryn-figures-import-tolérant");
        let figures = dossier.join("figures");
        std::fs::create_dir_all(&figures).unwrap();
        std::fs::write(figures.join("vide.md"), "---\nname: vide\n---\n").unwrap();
        std::fs::write(
            figures.join("bonne.md"),
            "---\nname: bonne\n---\nDes consignes.\n",
        )
        .unwrap();

        let n = importer(&d, &dossier, "plugin-figures").await;
        assert_eq!(n, 1, "la figure invalide est ignorée, la bonne est importée");
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[tokio::test]
    async fn importe_les_trois_figures_du_depot() {
        // Les trois fichiers réels de Locaryn/plugin-figures, copiés tels
        // quels : c'est la promesse du dépôt qui est vérifiée ici.
        let d = depot().await;
        let dossier = std::env::temp_dir().join("locaryn-figures-import-depot");
        let figures = dossier.join("figures");
        std::fs::create_dir_all(&figures).unwrap();

        std::fs::write(
            figures.join("relecteur.md"),
            "---\r\nname: relecteur\r\ndescription: Relit un diff et signale les défauts de correction, pas de style. À ouvrir avant de proposer une fusion.\r\nlicense: Apache-2.0\r\nmetadata:\r\n  model: \"\"\r\n  opening: \"Colle le diff à relire.\"\r\n  memory: false\r\n---\r\n\r\nTu relis du code. Tu ne signales que ce qui peut casser.\r\n",
        )
        .unwrap();
        std::fs::write(
            figures.join("traducteur.md"),
            "---\r\nname: traducteur\r\ndescription: Traduit un texte sans le commenter, sans l'expliquer et sans rien y ajouter.\r\nlicense: Apache-2.0\r\nmetadata:\r\n  model: \"\"\r\n  opening: \"Colle le texte, et dis vers quelle langue.\"\r\n  memory: false\r\n---\r\n\r\nTu traduis. Tu rends la traduction, et rien d'autre.\r\n",
        )
        .unwrap();
        std::fs::write(
            figures.join("secretaire.md"),
            "---\r\nname: secretaire\r\ndescription: Met au propre des notes prises à la volée : structure, corrige, et n'ajoute rien.\r\nlicense: Apache-2.0\r\nmetadata:\r\n  model: \"\"\r\n  opening: \"Colle tes notes.\"\r\n  memory: true\r\n---\r\n\r\nTu reçois des notes prises vite. Tu les rends lisibles.\r\n",
        )
        .unwrap();

        let n = importer(&d, &dossier, "Locaryn/plugin-figures").await;
        assert_eq!(n, 3);
        let liste = d.list().await.unwrap();
        assert_eq!(liste.len(), 3);
        let secretaire = liste.iter().find(|f| f.name == "secretaire").unwrap();
        assert!(secretaire.uses_memory, "le Secrétaire lit la mémoire");
        assert_eq!(secretaire.opening.as_deref(), Some("Colle tes notes."));
        let relecteur = liste.iter().find(|f| f.name == "relecteur").unwrap();
        assert_eq!(relecteur.model, None, "model vide vaut celui de l'application");
        let _ = std::fs::remove_dir_all(&dossier);
    }
}
