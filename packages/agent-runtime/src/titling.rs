//! Nommer une conversation.
//!
//! Une liste où tout s'appelle « Conversation » ne se lit pas, et un titre
//! fabriqué en coupant la première phrase se lit à peine mieux : « Explique-moi
//! comment fonctionne la génér… » ne dit pas de quoi il s'agit, il le répète.
//!
//! C'est donc le modèle qui nomme, à partir de ce qui a été demandé et du
//! projet dans lequel ça se passe. Il tourne déjà sur la machine, la question
//! est courte, et la réponse tient en cinq mots.

use std::time::Duration;

/// Ce qu'on donne au modèle pour qu'il trouve un titre.
#[derive(Debug, Clone)]
pub struct TitleRequest {
    /// La première demande de l'utilisateur.
    pub first_message: String,
    /// La réponse, si elle est déjà écrite : elle dit souvent mieux le sujet.
    pub first_reply: Option<String>,
    /// Le projet ouvert, s'il y en a un.
    pub project: Option<String>,
}

/// Demander un titre au modèle. `None` si le modèle n'a rien donné
/// d'utilisable — l'appelant garde alors le titre qu'il avait.
///
/// Volontairement sans flux et avec un budget serré : c'est une question de
/// service, elle ne doit ni faire attendre ni monopoliser le moteur.
pub async fn ask_for_title(
    endpoint: &str,
    client: &reqwest::Client,
    model: &str,
    req: &TitleRequest,
) -> Option<String> {
    let mut contexte = String::new();
    if let Some(p) = &req.project {
        contexte.push_str(&format!("Projet ouvert : {p}\n"));
    }
    contexte.push_str(&format!(
        "Demande : {}\n",
        tronquer(&req.first_message, 800)
    ));
    if let Some(r) = &req.first_reply {
        contexte.push_str(&format!("Réponse : {}\n", tronquer(r, 500)));
    }

    let corps = serde_json::json!({
        "model": model,
        "stream": false,
        "max_tokens": 24,
        "temperature": 0.2,
        "messages": [
            {
                "role": "system",
                "content":
                    "Tu nommes des conversations. Réponds par un titre de 2 à 5 mots, \
                     dans la langue de la demande, qui dit le sujet. Pas de guillemets, \
                     pas de ponctuation finale, pas de préfixe du genre « Titre : ». \
                     Rien d'autre que le titre."
            },
            { "role": "user", "content": contexte }
        ]
    });

    let resp = client
        .post(format!(
            "{}/v1/chat/completions",
            endpoint.trim_end_matches('/')
        ))
        .timeout(Duration::from_secs(30))
        .json(&corps)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    let brut = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())?;
    nettoyer(brut)
}

/// Ce que rend un modèle n'est pas toujours un titre : il arrive qu'il
/// commente, qu'il cite, ou qu'il raisonne à voix haute. On garde la
/// substance et on refuse le reste.
pub fn nettoyer(brut: &str) -> Option<String> {
    // Un modèle « à raisonnement » peut préfixer sa réponse d'un bloc de
    // réflexion ; seul ce qui suit compte.
    let apres_reflexion = brut.rsplit("</think>").next().unwrap_or(brut);
    let ligne = apres_reflexion
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())?;
    let ligne = ligne
        .trim_start_matches("Titre :")
        .trim_start_matches("Titre:")
        .trim_start_matches("Title:")
        .trim();
    // Guillemets et ponctuation s'emboîtent — `"Un titre".` — donc on répète
    // jusqu'à ce que plus rien ne parte, plutôt que de deviner l'ordre.
    let mut ligne = ligne;
    loop {
        let avant = ligne;
        ligne = ligne
            .trim_matches(|c| c == '"' || c == '«' || c == '»' || c == '\'')
            .trim_end_matches(['.', '!', '?', ':', ';'])
            .trim();
        if ligne == avant {
            break;
        }
    }

    if ligne.is_empty() || ligne.chars().count() > 70 {
        return None;
    }
    // Plus de douze mots, ce n'est plus un titre mais une phrase.
    if ligne.split_whitespace().count() > 12 {
        return None;
    }
    Some(ligne.to_string())
}

fn tronquer(texte: &str, max: usize) -> String {
    if texte.chars().count() <= max {
        return texte.to_string();
    }
    texte.chars().take(max).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::{nettoyer, tronquer};

    #[test]
    fn un_titre_propre_passe_tel_quel() {
        assert_eq!(
            nettoyer("Génération d'images").unwrap(),
            "Génération d'images"
        );
    }

    #[test]
    fn les_habillages_du_modele_sont_retires() {
        assert_eq!(
            nettoyer("Titre : \"Phare au couchant\".").unwrap(),
            "Phare au couchant"
        );
        assert_eq!(
            nettoyer("« Réglage du serveur »").unwrap(),
            "Réglage du serveur"
        );
    }

    #[test]
    fn le_bloc_de_reflexion_est_ignore() {
        assert_eq!(
            nettoyer("<think>l'utilisateur veut…</think>\nSauvegarde des photos").unwrap(),
            "Sauvegarde des photos"
        );
    }

    #[test]
    fn une_phrase_entiere_est_refusee() {
        // Mieux vaut garder le titre existant que d'en poser un qui déborde.
        assert!(nettoyer(
            "Voici un titre possible pour cette conversation qui parle de beaucoup de choses à la fois"
        )
        .is_none());
        assert!(nettoyer("   ").is_none());
    }

    #[test]
    fn le_contexte_donne_au_modele_est_borne() {
        assert!(tronquer(&"a".repeat(2000), 800).chars().count() <= 801);
        assert_eq!(tronquer("court", 800), "court");
    }
}
