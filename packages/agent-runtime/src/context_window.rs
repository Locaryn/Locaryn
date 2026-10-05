//! Faire tenir la conversation dans la fenêtre du modèle, en cours de tâche.
//!
//! Le résumé de l'historique (côté application) ne joue qu'entre deux messages
//! de la personne. Pendant une tâche, chaque appel d'outil ajoute son résultat
//! à la requête suivante : quelques lectures d'un studio Roblox suffisent à
//! dépasser la fenêtre, et le moteur refuse alors la requête — la conversation
//! s'arrêtait net sur « fenêtre pleine ».
//!
//! Avant chaque requête, si elle déborde, on retire par ordre croissant de
//! perte :
//! 1. le milieu des anciens résultats d'outils (début et fin restent) ;
//! 2. le milieu des résultats du dernier tour ;
//! 3. les plus anciens échanges de l'historique, jamais le message en cours ni
//!    ce qui suit (les appels d'outils et leurs réponses vont par paires).
//!
//! Ce qui est retiré est dit dans le texte même : le modèle sait qu'il lit un
//! extrait, et peut relancer l'outil s'il lui faut la suite.

use serde_json::Value;

/// Octets de JSON par jeton, la même estimation prudente que pour les outils.
const BYTES_PER_TOKEN: f64 = 3.0;

/// Longueur gardée d'un ancien résultat d'outil, en caractères.
const ANCIEN_RESULTAT: usize = 1200;

/// Longueur gardée d'un résultat du dernier tour.
const DERNIER_RESULTAT: usize = 6000;

/// Ce qui a été fait pour tenir.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Squeeze {
    /// Résultats d'outils raccourcis.
    pub raccourcis: usize,
    /// Messages d'historique retirés.
    pub retires: usize,
}

impl Squeeze {
    pub fn rien(&self) -> bool {
        self.raccourcis == 0 && self.retires == 0
    }

    /// Une phrase pour le journal de la conversation.
    pub fn annonce(&self) -> String {
        let mut parts = Vec::new();
        if self.raccourcis > 0 {
            parts.push(format!(
                "{} résultat(s) d'outil raccourci(s)",
                self.raccourcis
            ));
        }
        if self.retires > 0 {
            parts.push(format!("{} ancien(s) message(s) retiré(s)", self.retires));
        }
        format!(
            "Fenêtre de contexte pleine : {} pour continuer.",
            parts.join(", ")
        )
    }
}

/// Jetons estimés des messages tels qu'ils partent dans la requête.
pub fn estimate_tokens(messages: &Value) -> usize {
    let bytes = serde_json::to_string(messages).map_or(0, |s| s.len());
    (bytes as f64 / BYTES_PER_TOKEN).ceil() as usize
}

/// Ce que la conversation peut occuper : la fenêtre, moins les outils et la
/// place de la réponse.
pub fn budget(ctx: usize, outils: usize) -> usize {
    let reponse = (ctx / 6).max(1024);
    ctx.saturating_sub(outils).saturating_sub(reponse)
}

/// `texte` ramené à environ `max` caractères : son début, sa fin, et ce qui
/// manque entre les deux.
fn extrait(texte: &str, max: usize) -> Option<String> {
    let total = texte.chars().count();
    if total <= max {
        return None;
    }
    let tete = max * 2 / 3;
    let queue = max - tete;
    let debut: String = texte.chars().take(tete).collect();
    let fin: String = texte.chars().skip(total - queue).collect();
    Some(format!(
        "{debut}\n[… {} caractères retirés pour tenir dans la fenêtre de contexte ; relancez l'outil pour les relire …]\n{fin}",
        total - tete - queue
    ))
}

/// Raccourcir les résultats d'outils d'indice < `jusqua`, du plus ancien au
/// plus récent, jusqu'à tenir.
fn raccourcir(messages: &mut Value, jusqua: usize, max: usize, budget: usize, fait: &mut Squeeze) {
    for i in 0..jusqua {
        if estimate_tokens(messages) <= budget {
            return;
        }
        let Some(msg) = messages.get_mut(i) else {
            return;
        };
        if msg.get("role").and_then(Value::as_str) != Some("tool") {
            continue;
        }
        let nouveau = msg
            .get("content")
            .and_then(Value::as_str)
            .and_then(|c| extrait(c, max));
        if let Some(n) = nouveau {
            msg["content"] = Value::String(n);
            fait.raccourcis += 1;
        }
    }
}

/// L'indice du message de la personne qui a lancé la tâche : le dernier
/// message `user` suivi d'aucun autre message `user` avant les outils. Tout ce
/// qui le précède est de l'historique.
fn message_en_cours(arr: &[Value]) -> usize {
    let premier_outil = arr
        .iter()
        .position(|m| {
            m.get("tool_calls").is_some() || m.get("role").and_then(Value::as_str) == Some("tool")
        })
        .unwrap_or(arr.len());
    arr[..premier_outil]
        .iter()
        .rposition(|m| m.get("role").and_then(Value::as_str) == Some("user"))
        .unwrap_or(arr.len().saturating_sub(1))
}

/// Faire tenir `messages` (un tableau au format OpenAI) dans `budget` jetons.
pub fn fit(messages: &mut Value, budget: usize) -> Squeeze {
    let mut fait = Squeeze::default();
    if estimate_tokens(messages) <= budget {
        return fait;
    }
    let Some(len) = messages.as_array().map(Vec::len) else {
        return fait;
    };

    // 1. Les anciens résultats : tout ce qui précède le dernier message de
    // l'assistant qui appelle des outils.
    let dernier_appel = messages
        .as_array()
        .and_then(|a| a.iter().rposition(|m| m.get("tool_calls").is_some()))
        .unwrap_or(len);
    raccourcir(messages, dernier_appel, ANCIEN_RESULTAT, budget, &mut fait);

    // 2. Ceux du dernier tour.
    raccourcir(messages, len, DERNIER_RESULTAT, budget, &mut fait);

    // 3. Les plus anciens échanges de l'historique, après le message système.
    while estimate_tokens(messages) > budget {
        let Some(arr) = messages.as_array_mut() else {
            break;
        };
        let debut = usize::from(
            arr.first()
                .and_then(|m| m.get("role"))
                .and_then(Value::as_str)
                == Some("system"),
        );
        if message_en_cours(arr) <= debut {
            break;
        }
        arr.remove(debut);
        fait.retires += 1;
    }
    fait
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn conversation(resultat: &str) -> Value {
        json!([
            { "role": "system", "content": "outils" },
            { "role": "user", "content": "ancien message ".repeat(200) },
            { "role": "assistant", "content": "ancienne réponse ".repeat(200) },
            { "role": "user", "content": "construis la carte" },
            { "role": "assistant", "content": "", "tool_calls": [{ "id": "a" }] },
            { "role": "tool", "tool_call_id": "a", "content": resultat },
            { "role": "assistant", "content": "", "tool_calls": [{ "id": "b" }] },
            { "role": "tool", "tool_call_id": "b", "content": resultat },
        ])
    }

    #[test]
    fn rien_ne_change_quand_tout_tient() {
        let mut m = conversation("court");
        let avant = m.clone();
        assert!(fit(&mut m, 100_000).rien());
        assert_eq!(m, avant);
    }

    #[test]
    fn les_anciens_resultats_partent_en_premier() {
        let gros = "x".repeat(30_000);
        let mut m = conversation(&gros);
        let budget = estimate_tokens(&m) - 8_000;
        let fait = fit(&mut m, budget);
        assert_eq!(fait.raccourcis, 1, "l'ancien suffit");
        assert_eq!(fait.retires, 0);
        let ancien = m[5]["content"].as_str().unwrap();
        assert!(ancien.contains("caractères retirés"));
        assert_eq!(m[7]["content"].as_str().unwrap().len(), 30_000);
        assert!(estimate_tokens(&m) <= budget);
    }

    #[test]
    fn l_historique_part_en_dernier_et_jamais_la_tache() {
        let gros = "x".repeat(30_000);
        let mut m = conversation(&gros);
        let fait = fit(&mut m, 1_000);
        assert_eq!(fait.raccourcis, 2);
        assert_eq!(fait.retires, 2, "les deux anciens messages");
        let arr = m.as_array().unwrap();
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[1]["content"], "construis la carte");
        // Les paires appel / résultat sont intactes.
        assert_eq!(arr.len(), 6);
    }
}
