//! Les skills que la personne peut imposer avec `@nom`, comme dans Codex.
//!
//! Deux sources : les skills livrés avec l'application (documents Word, PDF,
//! Excel, PowerPoint) et ceux des extensions activées. Jusqu'ici, seul l'index
//! des skills d'extension atteignait le modèle — leur nom et une ligne de
//! description — sans aucun moyen d'en obtenir le contenu : un skill n'était
//! jamais réellement suivi. Une mention `@pdf` ajoute désormais son texte
//! complet au message envoyé au modèle.

use std::path::PathBuf;

use locaryn_skill_runtime::SkillDef;
use serde::Serialize;
use tauri::State;

use crate::Core;

/// Les skills intégrés, compilés dans l'application : ils marchent sans
/// extension installée et sans réseau.
const INTEGRES: &[(&str, &str)] = &[
    ("docx", include_str!("../skills/docx/SKILL.md")),
    ("pdf", include_str!("../skills/pdf/SKILL.md")),
    ("xlsx", include_str!("../skills/xlsx/SKILL.md")),
    ("pptx", include_str!("../skills/pptx/SKILL.md")),
];

/// Ce qu'affiche la liste « @ » du composeur.
#[derive(Debug, Clone, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: Option<String>,
    /// « Locaryn » pour un skill intégré, sinon le nom de l'extension.
    pub source: String,
}

fn integres() -> Vec<SkillDef> {
    INTEGRES
        .iter()
        .filter_map(|(nom, brut)| {
            locaryn_skill_runtime::parse_str(brut, PathBuf::from(format!("{nom}/SKILL.md"))).ok()
        })
        .collect()
}

/// Tous les skills disponibles, `(source, skill)`. Un skill d'extension qui
/// porte le nom d'un skill intégré le remplace : l'extension a été installée
/// exprès.
pub async fn tous(core: &Core) -> Vec<(String, SkillDef)> {
    let mut v: Vec<(String, SkillDef)> = Vec::new();
    {
        let rt = core.extensions.read().await;
        for p in rt.loaded.values() {
            for s in &p.skills {
                v.push((p.manifest.name.clone(), s.clone()));
            }
        }
    }
    for s in integres() {
        if !v.iter().any(|(_, d)| d.name.eq_ignore_ascii_case(&s.name)) {
            v.push(("Locaryn".to_string(), s));
        }
    }
    v.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    v
}

#[tauri::command]
pub async fn list_skills(core: State<'_, Core>) -> Result<Vec<SkillInfo>, String> {
    Ok(tous(core.inner())
        .await
        .into_iter()
        .map(|(source, s)| SkillInfo {
            name: s.name,
            description: s.description,
            source,
        })
        .collect())
}

/// Le texte d'un skill imposé, tel que le modèle le reçoit.
pub fn consigne(source: &str, s: &SkillDef) -> String {
    format!(
        "[Skill « {} » ({source}) imposé par la personne pour cette demande — suis ces instructions :\n\n{}\n]",
        s.name,
        s.body.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les skills intégrés doivent tous se lire : une faute de frontmatter
    /// les ferait disparaître en silence de la liste « @ ».
    #[test]
    fn les_skills_integres_se_lisent() {
        let noms: Vec<String> = integres().into_iter().map(|s| s.name).collect();
        assert_eq!(noms, ["docx", "pdf", "xlsx", "pptx"]);
        for s in integres() {
            assert!(s.description.is_some(), "{} sans description", s.name);
            assert!(s.body.len() > 200, "{} sans contenu", s.name);
        }
    }

    #[test]
    fn la_consigne_porte_le_contenu_du_skill() {
        let s = integres().into_iter().find(|s| s.name == "pdf").unwrap();
        let c = consigne("Locaryn", &s);
        assert!(c.contains("Skill « pdf »"));
        assert!(c.contains("pypdf"));
    }
}
