//! Les réglages d'échantillonnage que les créateurs d'un modèle recommandent.
//!
//! Un morph qui apporte un catalogue de modèles (`marketplace.catalogs`) peut
//! y déclarer, par modèle, `recommendedParams` : température, top-p, top-k,
//! min-p, pénalité de répétition, avec leur source. Les réglages par défaut de
//! l'application (température 0,7, top-k 40, pénalité 1,1) ne conviennent pas
//! à tous : PrismML, par exemple, recommande top-k 20 et aucune pénalité de
//! répétition pour ses modèles 1 bit.
//!
//! On retrouve le modèle actif dans les catalogues des extensions actives
//! d'après son nom de fichier, ce qui vaut aussi pour un modèle téléchargé
//! avant que son catalogue ne déclare quoi que ce soit.

use crate::{extensions, Core};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Recommendation {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub min_p: Option<f32>,
    pub repeat_penalty: Option<f32>,
    /// Qui le recommande, en clair (« PrismML »).
    pub source: String,
    /// Où le vérifier.
    pub source_url: Option<String>,
}

/// Le dernier segment d'un chemin ou d'une adresse : le nom du fichier.
fn nom_de_fichier(chemin: &str) -> &str {
    chemin.rsplit(['/', '\\']).next().unwrap_or(chemin)
}

fn lire(v: &Value) -> Option<Recommendation> {
    let r = v.get("recommendedParams")?;
    let nombre = |cle: &str| r.get(cle).and_then(Value::as_f64).map(|x| x as f32);
    Some(Recommendation {
        temperature: nombre("temperature"),
        top_p: nombre("topP"),
        top_k: r.get("topK").and_then(Value::as_u64).map(|x| x as u32),
        min_p: nombre("minP"),
        repeat_penalty: nombre("repeatPenalty"),
        source: r
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        source_url: r
            .get("sourceUrl")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// La recommandation d'un catalogue pour `fichier`, s'il le connaît. Une
/// variante peut préciser celle de son modèle.
pub fn dans_catalogue(catalogue: &Value, fichier: &str) -> Option<Recommendation> {
    let cible = nom_de_fichier(fichier).to_lowercase();
    let modeles = catalogue.get("models")?.as_array()?;
    for modele in modeles {
        for variante in modele
            .get("variants")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let tag = variante
                .get("tag")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if nom_de_fichier(tag).to_lowercase() == cible {
                return lire(variante).or_else(|| lire(modele));
            }
        }
    }
    None
}

/// La recommandation pour le modèle `fichier`, cherchée dans les catalogues
/// des extensions actives.
pub async fn pour_le_modele(core: State<'_, Core>, fichier: &str) -> Option<Recommendation> {
    let installees = extensions::list_extensions(core.clone()).await.ok()?;
    for ext in installees.iter().filter(|e| e.enabled) {
        for slot in ext
            .ui
            .slots
            .iter()
            .filter(|s| s.slot == "marketplace.catalogs")
        {
            let Some(entree) = &slot.entry else { continue };
            let Ok(texte) =
                extensions::read_extension_asset(core.clone(), ext.id.to_string(), entree.clone())
                    .await
            else {
                continue;
            };
            let Ok(catalogue) = serde_json::from_str::<Value>(&texte) else {
                continue;
            };
            if let Some(r) = dans_catalogue(&catalogue, fichier) {
                return Some(r);
            }
        }
    }
    None
}

/// La recommandation pour le modèle actif, pour le panneau du modèle.
#[tauri::command]
pub async fn model_recommendation(core: State<'_, Core>) -> Result<Option<Recommendation>, String> {
    let actif = core
        .storage
        .providers
        .active()
        .await
        .ok()
        .flatten()
        .and_then(|p| p.model);
    Ok(match actif {
        Some(m) => pour_le_modele(core, &m).await,
        None => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_modele_se_retrouve_par_son_fichier() {
        let catalogue = serde_json::json!({ "models": [{
            "id": "bonsai-27b",
            "recommendedParams": { "temperature": 0.5, "topP": 0.85, "topK": 20, "minP": 0, "repeatPenalty": 1.0, "source": "PrismML" },
            "variants": [
                { "tag": "https://huggingface.co/prism-ml/Bonsai-27B-gguf/resolve/main/Bonsai-27B-Q1_0.gguf" },
                { "tag": "https://exemple/Ternary-Bonsai-27B-Q2_0.gguf", "recommendedParams": { "temperature": 0.7, "source": "PrismML" } }
            ]
        }]});
        let r = dans_catalogue(&catalogue, r"D:\modeles\Bonsai-27B-Q1_0.gguf").unwrap();
        assert_eq!(r.top_k, Some(20));
        assert_eq!(r.repeat_penalty, Some(1.0));
        assert_eq!(r.source, "PrismML");
        // Une variante précise la sienne.
        let t = dans_catalogue(&catalogue, "Ternary-Bonsai-27B-Q2_0.gguf").unwrap();
        assert_eq!(t.temperature, Some(0.7));
        assert!(dans_catalogue(&catalogue, "Qwen3.5-9B-IQ4_XS.gguf").is_none());
    }
}
