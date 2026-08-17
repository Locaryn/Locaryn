//! Demander au moteur ce qu'un texte « veut dire », sous forme de vecteur.
//!
//! Les serveurs d'inférence compatibles OpenAI exposent `/v1/embeddings` :
//! llama.cpp, LM Studio, Ollama, vLLM. On leur parle de la même façon qu'on
//! leur demande une réponse, et le modèle qui calcule est celui que la
//! personne a choisi — un modèle de plongement dédié quand il y en a un, celui
//! de conversation sinon.
//!
//! Un modèle de conversation rend des vecteurs utilisables mais médiocres : il
//! a été entraîné à prédire la suite, pas à rapprocher ce qui se ressemble.
//! C'est dit à l'écran plutôt que caché, parce que la différence se voit dans
//! la qualité des réponses et que personne ne devrait avoir à la deviner.
//!
//! Deux familles de modèles de plongement — nomic, e5 — attendent un préfixe
//! qui dit si le texte est un document ou une question. Sans lui, elles rangent
//! à l'envers : mesuré ici, « le chat dort » et « the cat sleeps » tombaient à
//! 0,34 quand « le chat dort » et « la comptabilité trimestrielle » montaient à
//! 0,55. Avec les préfixes, la bonne réponse repasse devant (0,79 contre 0,55).
//! C'est le genre de panne qui ne se voit pas : rien n'échoue, les réponses
//! sont simplement à côté.

use std::time::Duration;

/// Ce qui a empêché le calcul, en français, tel quel à l'écran.
pub type Echec = String;

/// À quoi sert le texte qu'on vectorise.
///
/// Les modèles qui font la différence rangent documents et questions dans deux
/// régions distinctes de l'espace, et le préfixe leur dit laquelle viser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Un morceau de document, qu'on range pour le retrouver plus tard.
    Document,
    /// Une question posée maintenant.
    Question,
}

/// Le préfixe qu'attend ce modèle, s'il en attend un.
///
/// Reconnu au nom du fichier : c'est ce dont on dispose, et les familles
/// concernées le portent lisiblement. Un modèle inconnu ne reçoit rien —
/// ajouter un préfixe à un modèle qui n'en veut pas dégrade ses vecteurs
/// autant que l'omettre chez ceux qui en veulent.
pub fn prefixe(model: &str, role: Role) -> &'static str {
    let m = model.to_ascii_lowercase();
    if m.contains("nomic") {
        return match role {
            Role::Document => "search_document: ",
            Role::Question => "search_query: ",
        };
    }
    // Les E5 et leurs dérivés multilingues.
    if m.contains("e5-") || m.contains("multilingual-e5") {
        return match role {
            Role::Document => "passage: ",
            Role::Question => "query: ",
        };
    }
    ""
}

/// Calculer les vecteurs d'une liste de textes.
///
/// En un seul appel : les serveurs acceptent un tableau, et faire cent
/// requêtes là où une suffit multiplie les allers-retours sans rien gagner.
pub async fn embed(
    endpoint: &str,
    client: &reqwest::Client,
    model: &str,
    textes: &[String],
    role: Role,
) -> Result<Vec<Vec<f32>>, Echec> {
    if textes.is_empty() {
        return Ok(Vec::new());
    }
    let tete = prefixe(model, role);
    let prepares: Vec<String> = if tete.is_empty() {
        textes.to_vec()
    } else {
        textes.iter().map(|t| format!("{tete}{t}")).collect()
    };
    let corps = serde_json::json!({ "model": model, "input": prepares });

    let resp = client
        .post(format!("{}/v1/embeddings", endpoint.trim_end_matches('/')))
        .timeout(Duration::from_secs(300))
        .json(&corps)
        .send()
        .await
        .map_err(|_| {
            "Le moteur d'inférence ne répond pas. Démarrez-le, puis relancez \
             l'indexation."
                .to_string()
        })?;

    if !resp.status().is_success() {
        let statut = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        // 404, c'est le cas courant et il a une cause précise : le serveur
        // tourne, mais pas en mode plongements. Le dire vaut mieux qu'un code.
        if statut == reqwest::StatusCode::NOT_FOUND {
            return Err(
                "Ce moteur n'expose pas le calcul de plongements. Avec llama.cpp, \
                 démarrez le serveur avec `--embeddings`, ou choisissez un modèle \
                 de plongement dédié."
                    .to_string(),
            );
        }
        return Err(format!(
            "Le moteur a refusé le calcul ({statut}){}",
            if detail.trim().is_empty() {
                String::new()
            } else {
                format!(" : {}", tronquer(&detail, 200))
            }
        ));
    }

    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("réponse illisible du moteur : {e}"))?;
    lire_les_vecteurs(&v, textes.len())
}

/// Extraire les vecteurs de la réponse, et refuser ce qui n'en est pas.
///
/// Une réponse mal formée doit s'arrêter ici : un vecteur manquant décalerait
/// tous les suivants, et les morceaux se retrouveraient rangés sous le sens
/// de leur voisin — une panne invisible, qui ne se verrait que dans des
/// réponses subtilement fausses.
pub fn lire_les_vecteurs(v: &serde_json::Value, attendus: usize) -> Result<Vec<Vec<f32>>, Echec> {
    let items = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "le moteur n'a pas rendu de plongements".to_string())?;

    let mut out: Vec<Vec<f32>> = Vec::with_capacity(items.len());
    for item in items {
        let brut = item
            .get("embedding")
            .ok_or_else(|| "un plongement manque dans la réponse".to_string())?;
        // llama.cpp rend parfois `[[...]]` — un vecteur par jeton, groupé.
        // On prend le premier, qui est celui de la séquence entière.
        let plat = match brut.as_array() {
            Some(a) if a.first().and_then(|x| x.as_array()).is_some() => a
                .first()
                .and_then(|x| x.as_array())
                .ok_or_else(|| "plongement vide".to_string())?,
            Some(a) => a,
            None => return Err("un plongement n'est pas une liste".to_string()),
        };
        let vecteur: Vec<f32> = plat
            .iter()
            .filter_map(|x| x.as_f64().map(|f| f as f32))
            .collect();
        if vecteur.is_empty() {
            return Err("le moteur a rendu un plongement vide".to_string());
        }
        out.push(vecteur);
    }

    if out.len() != attendus {
        return Err(format!(
            "le moteur a rendu {} plongements pour {attendus} textes",
            out.len()
        ));
    }
    Ok(out)
}

fn tronquer(texte: &str, max: usize) -> String {
    if texte.chars().count() <= max {
        return texte.to_string();
    }
    texte.chars().take(max).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::lire_les_vecteurs;

    #[test]
    fn la_forme_courante_se_lit() {
        let v = serde_json::json!({
            "data": [
                { "embedding": [0.1, 0.2, 0.3] },
                { "embedding": [0.4, 0.5, 0.6] }
            ]
        });
        let out = lire_les_vecteurs(&v, 2).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), 3);
    }

    #[test]
    fn la_forme_groupee_de_llama_cpp_se_lit_aussi() {
        // Un vecteur par jeton : on prend celui de la séquence.
        let v = serde_json::json!({ "data": [ { "embedding": [[1.0, 2.0], [3.0, 4.0]] } ] });
        let out = lire_les_vecteurs(&v, 1).unwrap();
        assert_eq!(out, vec![vec![1.0f32, 2.0]]);
    }

    #[test]
    fn un_compte_qui_ne_tombe_pas_juste_est_refuse() {
        let v = serde_json::json!({ "data": [ { "embedding": [1.0] } ] });
        let e = lire_les_vecteurs(&v, 3).unwrap_err();
        assert!(e.contains("1 plongements pour 3"), "{e}");
    }

    #[test]
    fn un_plongement_vide_est_refuse() {
        let v = serde_json::json!({ "data": [ { "embedding": [] } ] });
        assert!(lire_les_vecteurs(&v, 1).is_err());
    }

    #[test]
    fn les_familles_a_prefixe_sont_reconnues() {
        use super::{prefixe, Role};
        assert_eq!(
            prefixe("nomic-embed-text-v1.5.Q5_K_M.gguf", Role::Document),
            "search_document: "
        );
        assert_eq!(
            prefixe("nomic-embed-text-v1.5.Q5_K_M.gguf", Role::Question),
            "search_query: "
        );
        assert_eq!(
            prefixe("multilingual-e5-large", Role::Document),
            "passage: "
        );
        assert_eq!(prefixe("multilingual-e5-large", Role::Question), "query: ");
    }

    #[test]
    fn un_modele_inconnu_ne_recoit_rien() {
        use super::{prefixe, Role};
        // Ajouter un préfixe à un modèle qui n'en veut pas dégrade ses
        // vecteurs autant que l'omettre chez ceux qui en veulent.
        assert_eq!(prefixe("Qwen3-4B-Instruct", Role::Document), "");
        assert_eq!(prefixe("bge-m3", Role::Question), "");
    }

    #[test]
    fn une_reponse_sans_donnees_est_refusee() {
        let v = serde_json::json!({ "error": "nope" });
        assert!(lire_les_vecteurs(&v, 1).is_err());
    }
}
