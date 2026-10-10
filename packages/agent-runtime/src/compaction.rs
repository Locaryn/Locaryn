//! Compresser une conversation à la demande : les vieux échanges cèdent la
//! place à un résumé écrit par le modèle lui-même, les derniers restent.
//!
//! Partagé par le bureau (appui long sur la jauge) et le démon (le même geste
//! depuis le téléphone). Ce module dit quoi résumer et obtient le résumé ;
//! l'hôte supprime et écrit dans sa base.

use locaryn_shared_types::{Message, MessageRole};

/// Les derniers échanges gardés tels quels.
pub const GARDER: usize = 4;
/// En dessous, il n'y a rien à compresser.
pub const MINIMUM: usize = 6;
/// Ce que le modèle lit au plus, en caractères.
const TRANSCRIPT_MAX: usize = 12_000;
/// En tête du message qui remplace les vieux tours.
pub const PREFIXE_RESUME: &str = "[Resume des echanges precedents]\n";

/// Ce qui part dans le résumé, et à partir d'où l'on garde.
pub struct Plan {
    pub transcript: String,
    /// Le plus ancien message conservé : tout ce qui le précède part.
    pub cutoff: chrono::DateTime<chrono::Utc>,
}

/// Le texte d'un message tel que le modèle doit le relire : sans sa
/// réflexion ni les marqueurs d'interface (`<!--locaryn-image:…-->`).
pub fn sans_marqueurs(content: &str) -> String {
    let mut text = crate::openai_tool_loop::sans_reflexion(content);
    for marker in ["<!--locaryn-audio:", "<!--locaryn-image:"] {
        while let Some(start) = text.find(marker) {
            let Some(end_rel) = text[start..].find("-->") else {
                break;
            };
            text.replace_range(start..start + end_rel + 3, "");
        }
    }
    text.trim().to_string()
}

/// Ce qu'il faut résumer. `Err` : conversation trop courte.
pub fn planifier(messages: &[Message]) -> Result<Plan, String> {
    let convo: Vec<&Message> = messages
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
        .collect();
    if convo.len() < MINIMUM {
        return Err("La conversation est trop courte pour avoir a compresser".into());
    }
    let split = convo.len() - GARDER;
    let transcript: String = convo[..split]
        .iter()
        .map(|m| {
            format!(
                "{}: {}",
                if m.role == MessageRole::Assistant {
                    "assistant"
                } else {
                    "user"
                },
                sans_marqueurs(&m.content)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(TRANSCRIPT_MAX)
        .collect();
    Ok(Plan {
        transcript,
        cutoff: convo[split].created_at,
    })
}

/// Le résumé, écrit par le modèle actif.
pub async fn resumer(
    http: &reqwest::Client,
    endpoint: &str,
    model: &str,
    transcript: &str,
) -> Result<String, String> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content":
              "Resume la conversation ci-dessous en francais, en moins de 200 mots. Conserve les decisions, contraintes, noms de fichiers et faits techniques. Pas de preambule, uniquement le resume." },
            { "role": "user", "content": transcript }
        ],
        "max_tokens": 320,
        "temperature": 0.2,
        "stream": false,
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let resp = http
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("moteur injoignable : {e}"))?;
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let summary = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if summary.is_empty() {
        return Err("le modele n'a pas produit de resume".into());
    }
    Ok(summary)
}

/// Jetons estimés d'une conversation stockée, pour une jauge : quatre
/// caractères par jeton, la même règle que l'écran du bureau.
pub fn jetons_estimes(messages: &[Message]) -> u64 {
    messages
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
        .map(|m| sans_marqueurs(&m.content).chars().count().div_ceil(4) as u64)
        .sum()
}

/// Ce qu'un résultat d'outil garde, en caractères, quand il est rejoué.
const RESULTAT_MAX: usize = 600;

/// La conversation telle que le modèle doit la relire au tour suivant.
///
/// Les appels d'outils et leurs résultats y restent, résumés dans la réponse
/// qui les a faits. Sans eux, une réponse faite seulement de réflexion et d'un
/// appel d'outil devenait vide : au « Réessaye » suivant, le modèle ne savait
/// plus ni ce qu'on lui avait demandé d'essayer, ni pourquoi ça avait échoué.
/// Partagé par le bureau et le démon, qui rejouaient chacun la leur.
pub fn historique(messages: &[Message]) -> Vec<crate::ChatTurn> {
    let mut tours: Vec<crate::ChatTurn> = Vec::new();
    // L'outil de chaque appel, pour nommer son résultat.
    let mut outils: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for m in messages {
        match m.role {
            MessageRole::User => tours.push(crate::ChatTurn {
                role: "user".into(),
                content: sans_marqueurs(&m.content),
            }),
            MessageRole::Assistant => {
                let mut texte = sans_marqueurs(&m.content);
                for appel in m.tool_calls.iter().flatten() {
                    outils.insert(appel.call_id.as_str(), appel.tool.as_str());
                    let args = abreger(&appel.args.to_string(), 200);
                    texte.push_str(&format!(
                        "
[Outil appelé : {}({args})]",
                        appel.tool
                    ));
                }
                tours.push(crate::ChatTurn {
                    role: "assistant".into(),
                    content: texte.trim().to_string(),
                });
            }
            MessageRole::Tool => {
                let outil = m
                    .tool_call_id
                    .as_deref()
                    .and_then(|id| outils.get(id).copied())
                    .unwrap_or("outil");
                let note = format!(
                    "
[Résultat de {outil} : {}]",
                    abreger(m.content.trim(), RESULTAT_MAX)
                );
                // Le résultat suit la réponse qui l'a demandé.
                match tours.last_mut() {
                    Some(t) if t.role == "assistant" => t.content.push_str(&note),
                    _ => tours.push(crate::ChatTurn {
                        role: "assistant".into(),
                        content: note.trim().to_string(),
                    }),
                }
            }
            _ => {}
        }
    }
    tours.retain(|t| !t.content.trim().is_empty());
    tours
}

fn abreger(texte: &str, max: usize) -> String {
    if texte.chars().count() <= max {
        return texte.to_string();
    }
    let debut: String = texte.chars().take(max).collect();
    format!("{debut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: MessageRole, content: &str, minute: i64) -> Message {
        Message {
            id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::nil(),
            role,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            tokens_in: 0,
            tokens_out: 0,
            parent_id: None,
            created_at: chrono::DateTime::from_timestamp(minute * 60, 0).unwrap(),
        }
    }

    #[test]
    fn les_outils_et_leurs_resultats_restent_dans_l_historique() {
        let mut appel = message(MessageRole::Assistant, "<think>je regarde</think>", 1);
        appel.tool_calls = Some(vec![locaryn_shared_types::ToolCall {
            call_id: "c1".into(),
            tool: "desktop_status".into(),
            args: serde_json::json!({}),
        }]);
        let mut resultat = message(MessageRole::Tool, "Tool 'desktop_status' was blocked.", 2);
        resultat.tool_call_id = Some("c1".into());
        let msgs = vec![
            message(MessageRole::User, "Mes 3 applis les plus gourmandes ?", 0),
            appel,
            resultat,
            message(MessageRole::User, "Réessaye", 3),
        ];
        let tours = historique(&msgs);
        assert_eq!(tours.len(), 3);
        assert_eq!(tours[1].role, "assistant");
        assert!(tours[1]
            .content
            .contains("[Outil appelé : desktop_status({})]"));
        assert!(tours[1]
            .content
            .contains("[Résultat de desktop_status : Tool 'desktop_status' was blocked.]"));
        assert!(!tours[1].content.contains("je regarde"));
        assert_eq!(tours[2].content, "Réessaye");
    }

    #[test]
    fn les_derniers_echanges_restent_et_les_marqueurs_partent() {
        let msgs: Vec<Message> = (0..8)
            .map(|i| {
                message(
                    if i % 2 == 0 {
                        MessageRole::User
                    } else {
                        MessageRole::Assistant
                    },
                    &format!("tour {i}<!--locaryn-image:/x.png-->"),
                    i,
                )
            })
            .collect();
        let plan = planifier(&msgs).unwrap();
        assert!(plan.transcript.contains("tour 3"));
        assert!(!plan.transcript.contains("tour 4"));
        assert!(!plan.transcript.contains("locaryn-image"));
        assert_eq!(plan.cutoff, msgs[4].created_at);
        assert!(planifier(&msgs[..5]).is_err());
    }
}
