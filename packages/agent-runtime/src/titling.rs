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

// ============================================================================
// Le profil de l'utilisateur
// ============================================================================

/// Ce que le modèle a compris de la personne, à partir d'un échange.
///
/// Une mémoire que l'utilisateur doit remplir lui-même reste vide : personne
/// n'ouvre un formulaire pour déclarer ses préférences. Elles se disent en
/// passant, au fil des conversations — « fais court », « je travaille en
/// Rust », « je suis sur le projet Locaryn » — et c'est là qu'il faut les
/// entendre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fait {
    /// `preference`, `habitude`, `projet` ou `fait`.
    pub category: String,
    pub content: String,
}

/// Lire un échange et en tirer ce qui vaut d'être retenu.
///
/// Rend une liste vide bien plus souvent qu'autre chose : la plupart des
/// échanges n'apprennent rien de durable, et une mémoire qui enfle à chaque
/// message devient un bruit que le modèle traîne dans chaque réponse.
pub async fn ask_for_profile(
    endpoint: &str,
    client: &reqwest::Client,
    model: &str,
    echange: &str,
) -> Vec<Fait> {
    let corps = serde_json::json!({
        "model": model,
        "stream": false,
        "max_tokens": 160,
        "temperature": 0.1,
        "messages": [
            {
                "role": "system",
                "content":
                    "Tu lis un échange et tu notes ce qu'il apprend de DURABLE sur la \
                     personne : ses préférences de travail, ses habitudes, ses projets. \
                     Une ligne par fait, au format « categorie | fait », où categorie vaut \
                     preference, habitude, projet ou fait. Trois lignes au maximum. \
                     N'invente rien, ne note rien de ponctuel ni de trivial. \
                     Si l'échange n'apprend rien de durable, réponds exactement RIEN."
            },
            { "role": "user", "content": tronquer(echange, 3000) }
        ]
    });

    let Ok(resp) = client
        .post(format!(
            "{}/v1/chat/completions",
            endpoint.trim_end_matches('/')
        ))
        .timeout(Duration::from_secs(45))
        .json(&corps)
        .send()
        .await
    else {
        return Vec::new();
    };
    if !resp.status().is_success() {
        return Vec::new();
    }
    let Ok(v) = resp.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    let brut = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .unwrap_or_default();
    lire_faits(brut)
}

/// Transformer la réponse du modèle en faits utilisables.
pub fn lire_faits(brut: &str) -> Vec<Fait> {
    const CATEGORIES: [&str; 4] = ["preference", "habitude", "projet", "fait"];
    let apres_reflexion = brut.rsplit("</think>").next().unwrap_or(brut);
    let mut out = Vec::new();
    for ligne in apres_reflexion.lines() {
        let ligne = ligne.trim().trim_start_matches(['-', '*', '•']).trim();
        if ligne.is_empty() || ligne.eq_ignore_ascii_case("rien") {
            continue;
        }
        let Some((cat, contenu)) = ligne.split_once('|') else {
            continue;
        };
        let cat = cat.trim().to_ascii_lowercase();
        let contenu = contenu.trim().trim_matches('"').trim();
        if !CATEGORIES.contains(&cat.as_str()) || contenu.is_empty() {
            continue;
        }
        // Un « fait » d'une phrase entière n'en est pas un : c'est un résumé,
        // et il polluerait chaque réponse suivante.
        if contenu.chars().count() > 160 {
            continue;
        }
        out.push(Fait {
            category: cat,
            content: contenu.to_string(),
        });
        if out.len() == 3 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod profil_tests {
    use super::{lire_faits, Fait};

    #[test]
    fn les_faits_bien_formes_sont_gardes() {
        let f =
            lire_faits("preference | Préfère les réponses courtes\nprojet | Travaille sur Locaryn");
        assert_eq!(
            f,
            vec![
                Fait {
                    category: "preference".into(),
                    content: "Préfère les réponses courtes".into()
                },
                Fait {
                    category: "projet".into(),
                    content: "Travaille sur Locaryn".into()
                },
            ]
        );
    }

    #[test]
    fn rien_veut_dire_rien() {
        assert!(lire_faits("RIEN").is_empty());
        assert!(lire_faits("rien\n").is_empty());
        assert!(lire_faits("").is_empty());
    }

    #[test]
    fn ce_qui_n_est_pas_un_fait_est_ecarte() {
        // Catégorie inconnue, ligne sans séparateur, phrase-résumé : rien de
        // tout cela n'a sa place dans une mémoire relue à chaque réponse.
        let f = lire_faits(&format!(
            "humeur | content aujourd'hui\nune ligne sans separateur\nfait | {}",
            "a".repeat(200)
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn trois_faits_au_maximum() {
        let f = lire_faits("fait | un\nfait | deux\nfait | trois\nfait | quatre");
        assert_eq!(f.len(), 3);
    }
}
