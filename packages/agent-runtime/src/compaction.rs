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
    for marker in [
        "<!--locaryn-audio:",
        "<!--locaryn-image:",
        "<!--locaryn-joints:",
    ] {
        while let Some(start) = text.find(marker) {
            let Some(end_rel) = text[start..].find("-->") else {
                break;
            };
            text.replace_range(start..start + end_rel + 3, "");
        }
    }
    text.trim().to_string()
}

/// En dessous de ce poids (en jetons estimés), une conversation de quelques
/// messages n'a rien à gagner à être résumée.
const LOURD: usize = 1_500;

/// Ce qu'un message pèse pour le modèle : son texte et les arguments de ses
/// appels d'outils.
fn poids(m: &Message) -> usize {
    let appels: usize = m
        .tool_calls
        .iter()
        .flatten()
        .map(|a| a.args.to_string().chars().count())
        .sum();
    (m.content.chars().count() + appels).div_ceil(4)
}

/// Ce qu'il faut résumer. `Err` : rien à gagner à compresser.
///
/// Une longue conversation garde ses derniers échanges. Mais une tâche menée
/// par outils tient en deux messages — la demande, puis une réponse faite de
/// dizaines d'appels — et remplit la fenêtre quand même : compter les seuls
/// messages répondait « rien à compresser » devant une fenêtre pleine. Elle
/// est alors résumée en entier.
pub fn planifier(messages: &[Message]) -> Result<Plan, String> {
    let convo: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.role, MessageRole::User | MessageRole::Assistant))
        .map(|(i, _)| i)
        .collect();
    let (anciens, cutoff) = if convo.len() >= MINIMUM {
        let premier_garde = convo[convo.len() - GARDER];
        (
            &messages[..premier_garde],
            messages[premier_garde].created_at,
        )
    } else {
        let dernier = messages
            .last()
            .filter(|_| messages.len() >= 2)
            .filter(|_| messages.iter().map(poids).sum::<usize>() >= LOURD)
            .ok_or("La conversation est trop courte pour avoir à être compressée.")?;
        // Tout part dans le résumé : la coupure tombe juste après le dernier.
        (
            messages,
            dernier.created_at + chrono::Duration::milliseconds(1),
        )
    };
    Ok(Plan {
        transcript: transcrire(&historique(anciens)),
        cutoff,
    })
}

/// Un appel passé en une ligne : son outil et ses arguments courts (chemin,
/// commande, adresse). Le contenu d'un fichier écrit n'aide pas un résumé ;
/// son chemin, si.
fn appel_en_bref(a: &crate::AppelRejoue) -> String {
    const VALEUR_MAX: usize = 100;
    let arguments = match serde_json::from_str::<serde_json::Value>(&a.arguments) {
        Ok(serde_json::Value::Object(champs)) => champs
            .iter()
            .filter_map(|(cle, v)| match v {
                serde_json::Value::String(s) if s.chars().count() <= VALEUR_MAX => {
                    Some(format!("{cle}={s}"))
                }
                serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
                    Some(format!("{cle}={v}"))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => abreger(&a.arguments, VALEUR_MAX),
    };
    format!("{}({arguments})", a.name)
}

/// Les tours à résumer, tels que le modèle les lit : qui parle, ce qu'il dit,
/// et les outils qu'il a appelés — c'est là que sont les noms de fichiers.
///
/// Trop long, le texte garde son début et surtout sa fin : couper la fin
/// faisait oublier au résumé le travail le plus récent.
pub fn transcrire(tours: &[crate::ChatTurn]) -> String {
    let texte = tours
        .iter()
        .filter(|t| t.role != "tool")
        .map(|t| {
            let appels: Vec<String> = t.tool_calls.iter().map(appel_en_bref).collect();
            let outils = if appels.is_empty() {
                String::new()
            } else {
                format!(" [outils : {}]", appels.join(", "))
            };
            format!("{}: {}{outils}", t.role, t.content)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let caracteres: Vec<char> = texte.chars().collect();
    if caracteres.len() <= TRANSCRIPT_MAX {
        return texte;
    }
    let tete = TRANSCRIPT_MAX / 4;
    let queue = TRANSCRIPT_MAX - tete;
    format!(
        "{}\n[…]\n{}",
        caracteres[..tete].iter().collect::<String>(),
        caracteres[caracteres.len() - queue..]
            .iter()
            .collect::<String>()
    )
}

/// Le résumé, écrit par le modèle actif. `cle` : celle du fournisseur distant
/// qui le sert — sans elle, il refusait la requête et la compression
/// répondait « rien à compresser ».
pub async fn resumer(
    http: &reqwest::Client,
    endpoint: &str,
    cle: Option<&str>,
    model: &str,
    transcript: &str,
) -> Result<String, String> {
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content":
              "Résume la conversation ci-dessous en français, en moins de 250 mots. Conserve les décisions, contraintes, noms de fichiers, faits techniques, et ce qui reste à faire. Pas de préambule, uniquement le résumé." },
            { "role": "user", "content": transcript }
        ],
        "max_tokens": 600,
        "temperature": 0.2,
        "stream": false,
        "reasoning_budget": 0,
        "chat_template_kwargs": { "enable_thinking": false }
    });
    let mut requete = http.post(&url).json(&body);
    if let Some(cle) = cle.filter(|c| !c.is_empty()) {
        requete = requete.bearer_auth(cle);
    }
    let resp = requete
        .send()
        .await
        .map_err(|e| format!("le modèle est injoignable : {e}"))?;
    let statut = resp.status();
    if !statut.is_success() {
        return Err(format!("le modèle a refusé de résumer (HTTP {statut})"));
    }
    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    // Un modèle qui réfléchit quand même : sa réflexion n'est pas le résumé.
    let summary = crate::openai_tool_loop::sans_reflexion(
        val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or(""),
    )
    .trim()
    .to_string();
    if summary.is_empty() {
        return Err("le modèle n'a pas produit de résumé".into());
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
    for m in messages {
        match m.role {
            MessageRole::User => tours.push(crate::ChatTurn {
                role: "user".into(),
                content: sans_marqueurs(&m.content),
                ..Default::default()
            }),
            MessageRole::Assistant => {
                let appels: Vec<crate::AppelRejoue> = m
                    .tool_calls
                    .iter()
                    .flatten()
                    .map(|a| crate::AppelRejoue {
                        id: a.call_id.clone(),
                        name: a.tool.clone(),
                        arguments: arguments_abreges(&a.args).to_string(),
                    })
                    .collect();
                tours.push(crate::ChatTurn {
                    role: "assistant".into(),
                    content: sans_appels_en_texte(&sans_marqueurs(&m.content)),
                    tool_calls: appels,
                    tool_call_id: None,
                });
            }
            MessageRole::Tool => tours.push(crate::ChatTurn {
                role: "tool".into(),
                content: abreger(m.content.trim(), RESULTAT_MAX),
                tool_calls: Vec::new(),
                tool_call_id: m.tool_call_id.clone(),
            }),
            _ => {}
        }
    }
    tours.retain(|t| !t.content.trim().is_empty() || !t.tool_calls.is_empty());
    tours
}

/// Une réponse où le modèle avait écrit ses appels en texte
/// (« [Outil appelé : …] », ancien format de l'historique) : ces lignes sont
/// retirées, sinon il les relit comme un exemple à suivre.
fn sans_appels_en_texte(texte: &str) -> String {
    texte
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.starts_with("[Outil appelé") || t.starts_with("[Résultat de "))
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        )
        .trim()
        .to_string()
}

/// Les arguments d'un appel passé, ses longues chaînes abrégées (le contenu
/// entier d'un `write_file` n'a pas à repasser à chaque tour) : le JSON reste
/// valide, contrairement à un texte coupé au milieu.
fn arguments_abreges(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => serde_json::Value::String(abreger(s, 300)),
        serde_json::Value::Array(a) => a.iter().map(arguments_abreges).collect(),
        serde_json::Value::Object(o) => o
            .iter()
            .map(|(k, x)| (k.clone(), arguments_abreges(x)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
        autre => autre.clone(),
    }
}

/// Les tours d'historique dans le format de l'API du modèle : un appel passé
/// reste un vrai `tool_calls`, son résultat un vrai message `tool`. Un
/// résultat dont l'appel n'est plus là (coupé par la compression) est
/// écarté, plutôt qu'un message `tool` orphelin que le gabarit refuserait ;
/// un appel sans résultat en reçoit un vide.
pub fn messages_api(tours: &[crate::ChatTurn]) -> Vec<serde_json::Value> {
    let mut sortie: Vec<serde_json::Value> = Vec::new();
    let mut ouverts: Vec<String> = Vec::new();
    let fermer = |ouverts: &mut Vec<String>, sortie: &mut Vec<serde_json::Value>| {
        for id in ouverts.drain(..) {
            sortie.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": id,
                "content": "(pas de résultat enregistré)",
            }));
        }
    };
    for t in tours {
        if t.role == "tool" {
            let id = t.tool_call_id.clone().unwrap_or_default();
            if let Some(pos) = ouverts.iter().position(|o| *o == id) {
                ouverts.remove(pos);
                sortie.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": t.content,
                }));
            }
            continue;
        }
        fermer(&mut ouverts, &mut sortie);
        if t.content.trim().is_empty() && t.tool_calls.is_empty() {
            continue;
        }
        let mut m = serde_json::json!({ "role": t.role, "content": t.content });
        if t.role == "assistant" && !t.tool_calls.is_empty() {
            m["tool_calls"] = t
                .tool_calls
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "id": a.id,
                        "type": "function",
                        "function": { "name": a.name, "arguments": a.arguments },
                    })
                })
                .collect();
            ouverts.extend(t.tool_calls.iter().map(|a| a.id.clone()));
        }
        sortie.push(m);
    }
    fermer(&mut ouverts, &mut sortie);
    sortie
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
            args: serde_json::json!({ "contenu": "x".repeat(1000) }),
        }]);
        let mut resultat = message(MessageRole::Tool, "Tool 'desktop_status' was blocked.", 2);
        resultat.tool_call_id = Some("c1".into());
        let msgs = vec![
            message(MessageRole::User, "Mes 3 applis les plus gourmandes ?", 0),
            appel,
            resultat,
            message(MessageRole::User, "Réessaye", 3),
        ];
        let api = messages_api(&historique(&msgs));
        // Un vrai appel, pas un texte que le modèle imiterait.
        assert_eq!(api.len(), 4);
        assert_eq!(api[1]["role"], "assistant");
        assert!(!api[1]["content"].as_str().unwrap().contains("Outil appelé"));
        assert!(!api[1]["content"].as_str().unwrap().contains("je regarde"));
        assert_eq!(
            api[1]["tool_calls"][0]["function"]["name"],
            "desktop_status"
        );
        let args: serde_json::Value = serde_json::from_str(
            api[1]["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert!(args["contenu"].as_str().unwrap().chars().count() <= 301);
        assert_eq!(api[2]["role"], "tool");
        assert_eq!(api[2]["tool_call_id"], "c1");
        assert_eq!(api[2]["content"], "Tool 'desktop_status' was blocked.");
        assert_eq!(api[3]["content"], "Réessaye");
    }

    #[test]
    fn un_appel_ecrit_en_texte_ne_sert_plus_d_exemple() {
        let msgs = vec![message(
            MessageRole::Assistant,
            "Je regarde.
[Outil appelé : wsl_run({\"command\":\"ls\"})]",
            0,
        )];
        assert_eq!(historique(&msgs)[0].content, "Je regarde.");
    }

    #[test]
    fn un_resultat_orphelin_ou_un_appel_sans_resultat_ne_casse_pas_la_conversation() {
        let tours = vec![
            crate::ChatTurn {
                role: "tool".into(),
                content: "résultat d'un appel coupé par la compression".into(),
                tool_call_id: Some("perdu".into()),
                ..Default::default()
            },
            crate::ChatTurn {
                role: "assistant".into(),
                tool_calls: vec![crate::AppelRejoue {
                    id: "c2".into(),
                    name: "wsl_run".into(),
                    arguments: "{}".into(),
                }],
                ..Default::default()
            },
            crate::ChatTurn {
                role: "user".into(),
                content: "continue".into(),
                ..Default::default()
            },
        ];
        let api = messages_api(&tours);
        assert_eq!(api.len(), 3);
        assert_eq!(api[0]["role"], "assistant");
        assert_eq!(api[1]["role"], "tool");
        assert_eq!(api[1]["tool_call_id"], "c2");
        assert_eq!(api[2]["role"], "user");
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

    #[test]
    fn une_tache_d_outils_en_deux_messages_se_compresse_en_entier() {
        let mut reponse = message(MessageRole::Assistant, "J'ai écrit le site.", 1);
        reponse.tool_calls = Some(
            (0..40)
                .map(|i| locaryn_shared_types::ToolCall {
                    call_id: format!("c{i}"),
                    tool: "write_file".into(),
                    args: serde_json::json!({
                        "path": format!("src/page{i}.tsx"),
                        "content": "x".repeat(400),
                    }),
                })
                .collect(),
        );
        let msgs = vec![message(MessageRole::User, "Construis le site", 0), reponse];
        let plan = planifier(&msgs).expect("une fenêtre pleine d'outils se compresse");
        assert!(plan.transcript.contains("write_file"));
        assert!(plan.transcript.contains("src/page39.tsx"));
        assert!(plan.cutoff > msgs[1].created_at);
    }

    #[test]
    fn un_long_transcript_garde_sa_fin() {
        let tours: Vec<crate::ChatTurn> = (0..400)
            .map(|i| crate::ChatTurn {
                role: "user".into(),
                content: format!("message numéro {i} avec un peu de texte pour peser"),
                ..Default::default()
            })
            .collect();
        let texte = transcrire(&tours);
        assert!(texte.chars().count() <= TRANSCRIPT_MAX + 10);
        assert!(texte.contains("message numéro 0 "));
        assert!(texte.contains("message numéro 399 "));
    }
}
