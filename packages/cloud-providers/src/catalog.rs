//! Le catalogue de modèles d'un fournisseur : le lire, le garder, le relire.
//!
//! Lu chez le fournisseur et non figé dans le paquet — c'est ce qui fait qu'un
//! modèle publié ce matin apparaît sans nouvelle version de l'extension. Gardé
//! sur disque le temps déclaré par le manifeste, et resservi tel quel quand la
//! lecture échoue : une liste d'hier vaut mieux qu'un écran vide.

use crate::{stored_key, DeclaredProvider, Host};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Un modèle du catalogue.
///
/// Les champs sont ceux que tous les catalogues compatibles OpenAI publient,
/// plus ceux qu'une passerelle ajoute et dont l'utilisateur a réellement
/// besoin pour choisir : la fenêtre de contexte et le prix.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CloudModel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub context_length: u64,
    /// Prix pour un million de jetons d'entrée, en dollars. `None` quand le
    /// catalogue n'en publie pas — un modèle gratuit affiche 0.
    #[serde(default)]
    pub prompt_price_per_m: Option<f64>,
    #[serde(default)]
    pub completion_price_per_m: Option<f64>,
    /// `text`, `text+image`… tel que le catalogue le déclare.
    #[serde(default)]
    pub modality: String,
    /// Le modèle sait-il appeler des outils ? Sans cela, la boucle d'outils
    /// tourne à vide, et il vaut mieux le dire avant.
    #[serde(default)]
    pub supports_tools: bool,
}

/// Le catalogue gardé sur disque.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedCatalog {
    pub updated_at: String,
    pub models: Vec<CloudModel>,
}

impl CachedCatalog {
    pub fn path(data_dir: &Path, provider_id: &str) -> PathBuf {
        // Le nom du fichier vient d'un identifiant de manifeste : le réduire à
        // des caractères sûrs évite qu'un « ../ » aille écrire ailleurs.
        let safe: String = provider_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        data_dir.join("cloud").join(format!("{safe}.json"))
    }

    pub fn load(data_dir: &Path, provider_id: &str) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(data_dir, provider_id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self, data_dir: &Path, provider_id: &str) -> std::io::Result<()> {
        let path = Self::path(data_dir, provider_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            serde_json::to_string(self).unwrap_or_else(|_| "{}".into()),
        )
    }

    /// Le catalogue est-il encore frais pour cette fenêtre de fraîcheur ?
    pub fn is_fresh(&self, refresh_hours: u32) -> bool {
        if self.models.is_empty() || refresh_hours == 0 {
            return false;
        }
        chrono::DateTime::parse_from_rfc3339(&self.updated_at)
            .map(|t| {
                let age = chrono::Utc::now().signed_duration_since(t.with_timezone(&chrono::Utc));
                age < chrono::Duration::hours(i64::from(refresh_hours))
            })
            .unwrap_or(false)
    }
}

/// La liste des modèles du fournisseur.
pub async fn models(
    host: &Host<'_>,
    p: &DeclaredProvider,
    refresh: bool,
) -> Result<Vec<CloudModel>, String> {
    let cache = CachedCatalog::load(host.data_dir, &p.id);
    if !refresh {
        if let Some(c) = cache
            .as_ref()
            .filter(|c| c.is_fresh(p.manifest.refresh_hours))
        {
            return Ok(c.models.clone());
        }
    }

    match fetch(host, p).await {
        Ok(models) => {
            let catalog = CachedCatalog {
                updated_at: chrono::Utc::now().to_rfc3339(),
                models,
            };
            if let Err(e) = catalog.save(host.data_dir, &p.id) {
                tracing::warn!(erreur = %e, "catalogue non enregistré");
            }
            Ok(catalog.models)
        }
        Err(e) => match cache {
            Some(c) if !c.models.is_empty() => {
                tracing::warn!(fournisseur = %p.id, erreur = %e, "liste illisible, catalogue gardé");
                Ok(c.models)
            }
            _ => Err(e),
        },
    }
}

/// Lire la liste chez le fournisseur.
async fn fetch(host: &Host<'_>, p: &DeclaredProvider) -> Result<Vec<CloudModel>, String> {
    let url = p.manifest.effective_models_url();
    let mut req = host.http.get(&url);
    // La clé n'est pas toujours nécessaire pour lire un catalogue ; quand elle
    // l'est, l'absence se voit sur un 401 explicite plutôt que sur une liste
    // vide.
    if let Some(key) = stored_key(host, &p.id) {
        req = req.bearer_auth(key);
    }
    for (name, value) in &p.manifest.headers {
        req = req.header(name.as_str(), value.as_str());
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("{} est injoignable : {e}", p.label()))?;
    if !resp.status().is_success() {
        let code = resp.status();
        return Err(match code.as_u16() {
            401 | 403 => format!(
                "{} a refusé la clé ({code}). Vérifiez-la dans le panneau du fournisseur.",
                p.label()
            ),
            _ => format!("{} a répondu {code}.", p.label()),
        });
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Réponse illisible de {} : {e}", p.label()))?;
    Ok(parse_models(&body))
}

/// Extraire les modèles d'une réponse au format OpenAI.
///
/// Isolée du réseau pour être testable : c'est la partie qui casse quand un
/// fournisseur change la forme de sa réponse.
pub fn parse_models(body: &serde_json::Value) -> Vec<CloudModel> {
    let list = body
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| body.get("models").and_then(|d| d.as_array()))
        .or_else(|| body.as_array());
    let Some(list) = list else {
        return Vec::new();
    };

    list.iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(|v| v.as_str())?.trim().to_string();
            if id.is_empty() {
                return None;
            }
            let pricing = m.get("pricing");
            // Les prix sont publiés par jeton, en chaîne. Par million, ils
            // deviennent lisibles — et comparables à ce qu'affichent les
            // fournisseurs eux-mêmes.
            let per_million = |champ: &str| -> Option<f64> {
                pricing?
                    .get(champ)
                    .and_then(|v| {
                        v.as_str()
                            .and_then(|s| s.parse::<f64>().ok())
                            .or_else(|| v.as_f64())
                    })
                    .map(|prix| prix * 1_000_000.0)
            };
            let architecture = m.get("architecture");
            Some(CloudModel {
                name: m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&id)
                    .to_string(),
                description: m
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                context_length: m
                    .get("context_length")
                    .and_then(serde_json::Value::as_u64)
                    .or_else(|| m.get("top_provider")?.get("context_length")?.as_u64())
                    .unwrap_or(0),
                prompt_price_per_m: per_million("prompt"),
                completion_price_per_m: per_million("completion"),
                modality: architecture
                    .and_then(|a| a.get("modality"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                supports_tools: m
                    .get("supported_parameters")
                    .and_then(|v| v.as_array())
                    .map(|params| {
                        params
                            .iter()
                            .filter_map(|p| p.as_str())
                            .any(|p| p == "tools" || p == "tool_choice")
                    })
                    .unwrap_or(false),
                id,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La forme publiée par une passerelle, réduite à ce que l'écran montre.
    #[test]
    fn la_reponse_dun_catalogue_se_lit() {
        let body = serde_json::json!({
            "data": [{
                "id": "anthropic/claude-opus-5",
                "name": "Claude Opus 5",
                "context_length": 1_000_000,
                "architecture": { "modality": "text+image->text" },
                "pricing": { "prompt": "0.000005", "completion": "0.000025" },
                "supported_parameters": ["tools", "max_tokens"]
            }]
        });
        let m = &parse_models(&body)[0];
        assert_eq!(m.id, "anthropic/claude-opus-5");
        assert_eq!(m.context_length, 1_000_000);
        // Publié par jeton, montré par million : 0,000005 → 5 $.
        assert_eq!(m.prompt_price_per_m, Some(5.0));
        assert!(m.supports_tools);
    }

    /// Un modèle sans identifiant n'est pas sélectionnable : le garder
    /// mettrait dans la liste une ligne qu'aucun clic ne peut activer.
    #[test]
    fn un_modele_sans_identifiant_est_ecarte() {
        let body = serde_json::json!({ "data": [{ "name": "sans id" }, { "id": "  " }] });
        assert!(parse_models(&body).is_empty());
    }

    /// Tous les catalogues ne publient pas `data` : certains renvoient un
    /// tableau nu, d'autres `models`. Les trois formes se lisent.
    #[test]
    fn les_trois_formes_de_liste_se_lisent() {
        for body in [
            serde_json::json!([{ "id": "a/b" }]),
            serde_json::json!({ "models": [{ "id": "a/b" }] }),
            serde_json::json!({ "data": [{ "id": "a/b" }] }),
        ] {
            assert_eq!(parse_models(&body).len(), 1, "forme non reconnue : {body}");
        }
    }

    /// Sans `name`, l'identifiant sert de nom : une ligne vide dans la liste
    /// serait pire qu'un identifiant technique.
    #[test]
    fn sans_nom_lidentifiant_fait_office() {
        let body = serde_json::json!({ "data": [{ "id": "meta/llama-4" }] });
        assert_eq!(parse_models(&body)[0].name, "meta/llama-4");
    }

    /// Un catalogue vieilli doit être relu ; un catalogue vide n'est jamais
    /// « frais », sinon une panne du premier jour figerait un écran vide.
    #[test]
    fn la_fraicheur_du_catalogue() {
        let frais = CachedCatalog {
            updated_at: chrono::Utc::now().to_rfc3339(),
            models: vec![CloudModel::default()],
        };
        assert!(frais.is_fresh(12));
        assert!(
            !frais.is_fresh(0),
            "0 heure veut dire « relis à chaque fois »"
        );

        let vieux = CachedCatalog {
            updated_at: (chrono::Utc::now() - chrono::Duration::hours(30)).to_rfc3339(),
            models: vec![CloudModel::default()],
        };
        assert!(!vieux.is_fresh(12));

        let vide = CachedCatalog {
            updated_at: chrono::Utc::now().to_rfc3339(),
            models: Vec::new(),
        };
        assert!(!vide.is_fresh(12));
    }

    /// Le nom de fichier du cache vient d'un manifeste : un identifiant tordu
    /// ne doit pas pouvoir écrire hors du dossier de données.
    #[test]
    fn un_identifiant_tordu_ne_sort_pas_du_dossier() {
        let base = Path::new("/donnees");
        let chemin = CachedCatalog::path(base, "../../etc/passwd");
        assert!(chemin.starts_with(base.join("cloud")));
        assert!(!chemin.to_string_lossy().contains(".."));
    }
}
