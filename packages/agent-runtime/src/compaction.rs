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
