//! Le Marketplace des modèles, servi au téléphone et au web.
//!
//! Deux sources, et rien d'autre : les modèles de conversation du socle
//! (`chat-models.json`), et ce que chaque morph **actif** déclare dans le
//! fichier de son slot `marketplace.catalogs` — ses catégories et ses modèles.
//! Aucune catégorie spécialisée n'est tenue ici : sans le morph qui l'apporte,
//! elle n'existe pas ; installé et activé, elle apparaît.

use crate::DaemonState;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

/// Les catégories du socle : ce que sait faire le moteur de conversation.
fn categories_du_socle() -> Vec<Value> {
    vec![
        json!({ "id": "chat", "label": "Conversation", "icon": "chat", "matches": ["chat"] }),
        json!({ "id": "code", "label": "Code", "icon": "code", "matches": ["code"] }),
        json!({ "id": "vision", "label": "Vision", "icon": "image", "matches": ["vision"] }),
        json!({ "id": "reasoning", "label": "Raisonnement", "icon": "brain", "matches": ["reasoning"] }),
    ]
}

/// Ce qu'une famille du socle sait faire, au vocabulaire des catégories.
fn capacites_du_socle(famille: &Value) -> Vec<&'static str> {
    let mut c = Vec::new();
    for (drapeau, capacite) in [
        ("instruct", "chat"),
        ("code", "code"),
        ("vision", "vision"),
        ("reasoning", "reasoning"),
    ] {
        if famille.get(drapeau).and_then(Value::as_bool) == Some(true) {
            c.push(capacite);
        }
    }
    c
}

/// GET /v1/marketplace — `{ categories, families }`.
pub async fn catalogue(State(s): State<Arc<DaemonState>>) -> Response {
    let extensions: Vec<_> = s.extensions.list().into_iter().filter(|e| e.enabled).collect();
    let capacites_actives: std::collections::HashSet<String> = extensions
        .iter()
        .flat_map(|e| e.capabilities.iter().cloned())
        .collect();

    let mut categories = categories_du_socle();
    let mut familles: Vec<Value> = locaryn_shared_types::chat_models::familles()
        .into_iter()
        .map(|mut f| {
            let caps = capacites_du_socle(&f);
            f["marketplaceCapabilities"] = json!(caps);
            f
        })
        .collect();

    for e in &extensions {
        let Some(racine) = e.manifest_path.parent() else {
            continue;
        };
        for slot in e.ui.slots.iter().filter(|s| s.slot == "marketplace.catalogs") {
            let Some(entree) = slot.entry.as_deref() else {
                continue;
            };
            // Le fichier reste dans le dossier de l'extension.
            if entree.contains("..") {
                continue;
            }
            let chemin = racine.join(entree);
            let catalogue: Value = match std::fs::read_to_string(&chemin)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
            {
                Some(v) => v,
                None => {
                    tracing::warn!(extension = %e.name, chemin = %chemin.display(), "catalogue du Marketplace illisible");
                    continue;
                }
            };
            for c in catalogue["categories"].as_array().into_iter().flatten() {
                let requises: Vec<String> = c["requires"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.as_str().map(str::to_string))
                    .collect();
                let ouverte =
                    requises.is_empty() || requises.iter().any(|r| capacites_actives.contains(r));
                let deja = categories.iter().any(|x| x["id"] == c["id"]);
                if ouverte && !deja {
                    categories.push(c.clone());
                }
            }
            for m in catalogue["models"].as_array().into_iter().flatten() {
                let mut m = m.clone();
                if m.get("marketplaceCapabilities").is_none() {
                    m["marketplaceCapabilities"] = m["capabilities"].clone();
                }
                m["marketplaceOwner"] = json!(e.name);
                m["source"] = json!("extension");
                familles.push(m);
            }
        }
    }

    Json(json!({ "categories": categories, "families": familles })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_famille_du_socle_dit_ce_qu_elle_sait_faire() {
        let f = json!({ "id": "x", "instruct": true, "vision": true, "variants": [] });
        assert_eq!(capacites_du_socle(&f), vec!["chat", "vision"]);
    }

    #[test]
    fn le_socle_ne_tient_aucune_categorie_specialisee() {
        let ids: Vec<String> = categories_du_socle()
            .iter()
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, vec!["chat", "code", "vision", "reasoning"]);
    }
}
