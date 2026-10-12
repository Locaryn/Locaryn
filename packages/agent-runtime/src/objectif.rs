//! `/objectif` : faire travailler le modèle jusqu'à ce que des conditions
//! écrites soient remplies, sans qu'il rende la main entre-temps.
//!
//! Après chaque tour, un appel discret juge la dernière réponse contre
//! l'objectif. Le juge ne fait rien lui-même : il dit « atteint », ou ce qui
//! reste, et c'est ce reste qui relance le tour suivant. Partagé par le bureau
//! et le démon.

/// Ce que le juge a conclu d'un tour.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Verdict {
    pub atteint: bool,
    /// Ce qui manque encore, en une phrase (vide quand l'objectif est atteint).
    pub reste: String,
}

/// La consigne posée devant l'objectif au premier tour.
pub fn consigne(objectif: &str) -> String {
    format!(
        "{objectif}\n\n[Objectif] Travaille jusqu'à ce que ces conditions soient toutes remplies. \
         Ne t'arrête pas pour demander une confirmation ou proposer la suite : enchaîne. Si une \
         information indispensable te manque, pose la question avec l'outil ask_user puis \
         reprends. Quand tout est rempli et vérifié, termine par la ligne « Objectif atteint »."
    )
}

/// Le message qui relance un tour, d'après ce que le juge dit manquer.
pub fn relance(reste: &str) -> String {
    format!(
        "[Objectif] Pas encore atteint. Il reste : {reste}\nContinue sans demander de \
         confirmation, et termine par « Objectif atteint » quand tout est rempli."
    )
}

/// Juger la dernière réponse contre l'objectif.
///
/// `client` et `cle` : ceux du fournisseur actif — un catalogue distant ou
/// un autre PC Locaryn demande sa clé, et le second son autorité. Le juge
/// parlait au fournisseur sans l'une ni l'autre : refusé, il arrêtait
/// l'objectif sur « juge indisponible ».
pub async fn juger(
    client: Option<&reqwest::Client>,
    endpoint: &str,
    cle: Option<&str>,
    model: &str,
    objectif: &str,
    reponse: &str,
) -> Result<Verdict, String> {
    // La fin de la réponse suffit : c'est là que le modèle fait son bilan.
    let fin: String = {
        let caracteres: Vec<char> = reponse.chars().collect();
        caracteres[caracteres.len().saturating_sub(3000)..]
            .iter()
            .collect()
    };
    let corps = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content":
              "Tu es un juge strict. On te donne un objectif et la dernière réponse d'un \
               assistant qui y travaille. Réponds UNIQUEMENT en JSON : \
               {\"atteint\":bool,\"reste\":\"...\"}. atteint=true seulement si TOUTES les \
               conditions de l'objectif sont remplies et que la réponse le montre (pas une \
               promesse, pas un plan). Sinon, reste dit en une phrase courte ce qui manque, \
               dans la langue de l'objectif." },
            { "role": "user", "content": format!("Objectif :\n{objectif}\n\nDernière réponse :\n{fin}") }
        ],
        "response_format": { "type": "json_object" },
        "max_tokens": 160,
        "temperature": 0.0,
        "stream": false,
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let par_defaut;
    let client = match client {
        Some(c) => c,
        None => {
            par_defaut = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .map_err(|e| e.to_string())?;
            &par_defaut
        }
    };
    let mut req = client
        .post(&url)
        .timeout(std::time::Duration::from_secs(60))
        .json(&corps);
    if let Some(cle) = cle {
        req = req.bearer_auth(cle);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("le juge n'a pas pu être joint : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("le juge a répondu {}", resp.status()));
    }
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let texte = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    Ok(lire_verdict(texte, reponse))
}

/// Le verdict dans la réponse du juge. Un juge illisible ne doit pas boucler
/// pour rien : on se rabat sur la ligne « Objectif atteint » de l'assistant.
fn lire_verdict(texte: &str, reponse: &str) -> Verdict {
    let objet = texte
        .find('{')
        .zip(texte.rfind('}'))
        .and_then(|(a, b)| serde_json::from_str::<serde_json::Value>(&texte[a..=b]).ok());
    if let Some(v) = objet {
        if let Some(atteint) = v.get("atteint").and_then(|a| a.as_bool()) {
            let reste = v
                .get("reste")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            return Verdict {
                atteint,
                reste: if atteint { String::new() } else { reste },
            };
        }
    }
    let annonce = reponse.to_lowercase().contains("objectif atteint");
    Verdict {
        atteint: annonce,
        reste: if annonce {
            String::new()
        } else {
            "terminer et vérifier ce qui est demandé".into()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_verdict_du_juge_prime() {
        let v = lire_verdict(
            r#"{"atteint":false,"reste":"les tests ne passent pas"}"#,
            "Objectif atteint",
        );
        assert!(!v.atteint);
        assert_eq!(v.reste, "les tests ne passent pas");
        let v = lire_verdict(r#"blabla {"atteint":true,"reste":"x"}"#, "");
        assert!(v.atteint);
        assert!(v.reste.is_empty());
    }

    #[test]
    fn sans_juge_lisible_on_lit_l_annonce_de_l_assistant() {
        assert!(lire_verdict("???", "… Objectif atteint").atteint);
        assert!(!lire_verdict("", "j'ai commencé").atteint);
    }

    #[test]
    fn la_consigne_garde_l_objectif_et_interdit_de_rendre_la_main() {
        let c = consigne("Faire passer les tests");
        assert!(c.starts_with("Faire passer les tests"));
        assert!(c.contains("ask_user"));
        assert!(relance("le build casse").contains("le build casse"));
    }
}
