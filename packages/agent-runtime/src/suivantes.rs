//! Ce que la personne peut faire juste après une réponse : trois suggestions
//! courtes, affichées en puces sous la réponse. Un appel discret au modèle de
//! conversation, jamais enregistré dans la conversation.
//!
//! Partagé par le bureau et le démon (que le téléphone appelle).

/// Demande au modèle servi par `endpoint` trois suites possibles à `answer`.
pub async fn proposer(
    endpoint: &str,
    model: &str,
    question: Option<&str>,
    answer: &str,
) -> Result<Vec<String>, String> {
    // Keep the context small: only the tail of the answer matters for "what next".
    let tail: String = answer
        .chars()
        .rev()
        .take(1200)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    // La demande de la personne, pas seulement la réponse : sans elle, le
    // modèle ne sait pas de quoi on parle et retombe sur des formules
    // générales — « ajuster le style », « vérifier la précision » — qui
    // reviennent à chaque tour quel que soit le sujet.
    let demande: String = question
        .map(str::trim)
        .filter(|texte| !texte.is_empty())
        .map(|texte| texte.chars().take(400).collect())
        .unwrap_or_default();
    let echange = if demande.is_empty() {
        format!("Réponse de l'assistant :\n\n{tail}")
    } else {
        format!("Message de la personne :\n\n{demande}\n\nRéponse de l'assistant :\n\n{tail}")
    };

    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content":
              "Tu proposes ce que la personne peut faire juste après, dans la conversation \
               en cours. Réponds UNIQUEMENT en JSON : {\"suggestions\":[\"...\"]}. \
               Trois au maximum, chacune une action courte (moins de 60 caractères), à \
               l'impératif, dans la MÊME LANGUE que la réponse.\n\
               Règles :\n\
               - Si la réponse pose une question ou propose quelque chose, la première \
                 suggestion y répond directement : « Génère cette image », « Applique ce \
                 correctif ». C'est le prolongement naturel de l'échange.\n\
               - Reprends les mots de la conversation. « Ajoute une lumière de fin de \
                 journée » vaut mieux que « Ajuster le style » : la seconde conviendrait \
                 à n'importe quelle réponse, donc à aucune.\n\
               - Ne propose jamais ce qui vient d'être fait, ni une variante de la même \
                 idée trois fois.\n\
               - Écris des phrases ordinaires. N'invente pas de commande de \
                 l'application : ce que la personne peut dire suffit." },
            { "role": "user", "content": format!("{echange}\n\nQue proposer ensuite ?") }
        ],
        "response_format": { "type": "json_object" },
        "max_tokens": 220,
        "temperature": 0.4,
        "stream": false,
        // Small models burn their budget "thinking" — keep this side call terse.
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("model returned {}", resp.status()));
    }
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(content).unwrap_or(serde_json::json!({}));
    let list = parsed["suggestions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && s.chars().count() <= 90)
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(list)
}
