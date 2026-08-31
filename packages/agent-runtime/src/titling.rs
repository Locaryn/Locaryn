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

/// Un projet, tel qu'on le propose au modèle.
#[derive(Debug, Clone)]
pub struct ProjetConnu {
    pub id: String,
    pub name: String,
}

/// Demander dans quel projet cette conversation aurait sa place.
///
/// Rend `None` bien plus souvent qu'un projet, et c'est voulu : une
/// proposition qui tombe à côté coûte plus cher qu'une absence de proposition.
/// Le modèle doit répondre par un numéro de la liste, ou par `AUCUN` — c'est
/// la seule façon d'éviter qu'il invente un projet qui n'existe pas.
pub async fn ask_for_project(
    endpoint: &str,
    client: &reqwest::Client,
    model: &str,
    echange: &str,
    projets: &[ProjetConnu],
) -> Option<String> {
    if projets.is_empty() {
        return None;
    }
    let liste = projets
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{}. {}", i + 1, p.name))
        .collect::<Vec<_>>()
        .join("\n");

    let corps = serde_json::json!({
        "model": model,
        "stream": false,
        "max_tokens": 8,
        "temperature": 0.0,
        "messages": [
            {
                "role": "system",
                "content":
                    "On te donne une conversation et une liste numérotée de projets. \
                     Si la conversation relève clairement de l'un d'eux, réponds par son \
                     numéro, et rien d'autre. Au moindre doute, réponds exactement AUCUN. \
                     Mieux vaut AUCUN qu'un rangement à côté."
            },
            {
                "role": "user",
                "content": format!("Projets :\n{liste}\n\nConversation :\n{}", tronquer(echange, 1500))
            }
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
    lire_le_numero(brut, projets.len()).map(|i| projets[i].id.clone())
}

/// Lire le numéro que le modèle a rendu, et refuser tout le reste.
///
/// Un modèle répond rarement par un chiffre seul : il commente, il justifie,
/// il propose deux projets. Un seul nombre, dans l'intervalle, et rien qui
/// ressemble à un refus — sinon on ne propose rien.
pub fn lire_le_numero(brut: &str, combien: usize) -> Option<usize> {
    let apres_reflexion = brut.rsplit("</think>").next().unwrap_or(brut).trim();
    if apres_reflexion.to_uppercase().contains("AUCUN") {
        return None;
    }
    let chiffres: Vec<usize> = apres_reflexion
        .split(|c: char| !c.is_ascii_digit())
        .filter(|m| !m.is_empty())
        .filter_map(|m| m.parse().ok())
        .collect();
    // Deux nombres, c'est une hésitation ou une phrase : on ne tranche pas à
    // la place du modèle.
    match chiffres.as_slice() {
        [n] if *n >= 1 && *n <= combien => Some(n - 1),
        _ => None,
    }
}

/// Écrire ce que deux conversations racontent ensemble.
///
/// Sert quand on dépose une conversation sur une autre : plutôt que de coller
/// deux fils bout à bout, le modèle en fait un récit unique — ce qui a été
/// cherché, ce qui a été trouvé, ce qui reste ouvert. Le texte est versé dans
/// la conversation d'accueil ; rien n'est effacé de l'autre côté tant que la
/// personne n'a pas archivé la conversation absorbée.
pub async fn ask_for_merge(
    endpoint: &str,
    client: &reqwest::Client,
    model: &str,
    accueil: &str,
    absorbee: &str,
) -> Option<String> {
    let corps = serde_json::json!({
        "model": model,
        "stream": false,
        "max_tokens": 700,
        "temperature": 0.2,
        "messages": [
            {
                "role": "system",
                "content":
                    "On te donne deux conversations à réunir. Écris un seul texte qui \
                     reprend ce qui a été cherché, ce qui a été établi, et ce qui reste \
                     ouvert. Garde les détails concrets — noms de fichiers, décisions, \
                     chiffres. N'invente rien, ne commente pas ton travail, ne dis pas \
                     que tu fusionnes : écris le texte, et lui seul."
            },
            {
                "role": "user",
                "content": format!(
                    "Conversation d'accueil :\n{}\n\n---\n\nConversation à y verser :\n{}",
                    tronquer(accueil, 6000),
                    tronquer(absorbee, 6000)
                )
            }
        ]
    });

    let resp = client
        .post(format!(
            "{}/v1/chat/completions",
            endpoint.trim_end_matches('/')
        ))
        .timeout(Duration::from_secs(120))
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
    let texte = brut.rsplit("</think>").next().unwrap_or(brut).trim();
    (!texte.is_empty()).then(|| texte.to_string())
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

/// Ce que le modèle a compris d'un échange, à retenir dans une fiche de
/// mémoire.
///
/// Une mémoire que l'utilisateur doit remplir lui-même reste vide : personne
/// n'ouvre un formulaire pour déclarer ses préférences ou présenter son
/// projet. Elles se disent en passant, au fil des conversations — « fais
/// court », « je travaille sur Bot Bastet avec Paul et Simon » — et c'est là
/// qu'il faut les entendre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fait {
    /// `vous`, `sujets`, `zones` ou `personnes` — voir `storage::memory::GROUPS`.
    /// Un groupe hors de cette liste est de toute façon ramené à `sujets` par
    /// le dépôt au moment d'enregistrer : cette validation-ci n'existe que
    /// pour ne pas transmettre une ligne à moitié comprise.
    pub group: String,
    /// Nom court qui identifie le sujet (« Bot Bastet », « Préférences ») —
    /// c'est la clé qui décide si ce fait rejoint une fiche existante ou en
    /// ouvre une nouvelle.
    pub title: String,
    pub detail: String,
}

const GROUPES_CONNUS: [&str; 4] = ["vous", "sujets", "zones", "personnes"];

/// Lire un échange et en tirer ce qui vaut d'être retenu.
///
/// Rend une liste vide bien plus souvent qu'autre chose : la plupart des
/// échanges n'apprennent rien de durable, et une mémoire qui enfle à chaque
/// message devient un bruit que le modèle traîne dans chaque réponse.
pub async fn ask_for_memory(
    endpoint: &str,
    client: &reqwest::Client,
    model: &str,
    echange: &str,
) -> Vec<Fait> {
    let corps = serde_json::json!({
        "model": model,
        "stream": false,
        "max_tokens": 220,
        "temperature": 0.1,
        "messages": [
            {
                "role": "system",
                "content":
                    "Tu lis un échange et tu notes ce qu'il apprend de DURABLE, classé dans \
                     l'un de ces groupes : vous (une préférence ou un fait sur la personne \
                     elle-même), sujets (un centre d'intérêt, un domaine, une activité \
                     récurrente), zones (un projet ou système nommé : dépôt, robot, \
                     application), personnes (quelqu'un mentionné : coéquipier, collègue). \
                     Une ligne par fait, au format « groupe | titre court | détail », où \
                     titre est un nom de deux à quatre mots qui identifie le sujet (ex. \
                     « Bot Bastet », « Drones FPV », « Préférences ») et détail la phrase \
                     précise apprise. Trois lignes au maximum. N'invente rien, ne note rien \
                     de ponctuel ni de trivial. Si l'échange n'apprend rien de durable, \
                     réponds exactement RIEN."
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
    let apres_reflexion = brut.rsplit("</think>").next().unwrap_or(brut);
    let mut out = Vec::new();
    for ligne in apres_reflexion.lines() {
        let ligne = ligne.trim().trim_start_matches(['-', '*', '•']).trim();
        if ligne.is_empty() || ligne.eq_ignore_ascii_case("rien") {
            continue;
        }
        let mut parts = ligne.splitn(3, '|').map(str::trim);
        let (Some(groupe), Some(titre), Some(detail)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let groupe = groupe.to_ascii_lowercase();
        let titre = titre.trim_matches('"').trim();
        let detail = detail.trim_matches('"').trim();
        if !GROUPES_CONNUS.contains(&groupe.as_str()) || titre.is_empty() || detail.is_empty() {
            continue;
        }
        // Un « détail » d'un paragraphe entier n'en est pas un : c'est un
        // résumé, et il polluerait chaque réponse suivante.
        if detail.chars().count() > 200 || titre.chars().count() > 60 {
            continue;
        }
        out.push(Fait {
            group: groupe,
            title: titre.to_string(),
            detail: detail.to_string(),
        });
        if out.len() == 3 {
            break;
        }
    }
    out
}

// ============================================================================
// Gérer la mémoire en le demandant, plutôt qu'à la main
// ============================================================================

/// Une fiche existante, réduite à ce que le modèle a besoin de voir pour
/// décider quoi en faire — jamais les détails complets : la liste peut
/// compter des dizaines de fiches, et le résumé suffit à les reconnaître.
#[derive(Debug, Clone)]
pub struct FicheResumee {
    pub id: String,
    pub group: String,
    pub title: String,
    pub summary: String,
}

/// Ce que l'utilisateur a demandé de faire à sa mémoire, une fois compris.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryAction {
    /// Oublier la fiche entièrement.
    Forget { id: String },
    /// Corriger son résumé.
    SetSummary { id: String, summary: String },
    /// La renommer.
    Rename { id: String, title: String },
}

/// Demander au modèle de traduire une instruction en actions sur des fiches
/// existantes — « supprime tout ce qui concerne les drones », « renomme
/// Zenbook Tracking en Suivi du Zenbook ».
///
/// Rend une liste vide quand l'instruction ne correspond à aucune fiche
/// connue : mieux vaut ne rien faire que deviner une action sur la mauvaise
/// fiche.
pub async fn ask_memory_command(
    endpoint: &str,
    client: &reqwest::Client,
    model: &str,
    instruction: &str,
    fiches: &[FicheResumee],
) -> Vec<MemoryAction> {
    if fiches.is_empty() || instruction.trim().is_empty() {
        return Vec::new();
    }
    let liste = fiches
        .iter()
        .map(|f| format!("{} | {} | {} | {}", f.id, f.group, f.title, f.summary))
        .collect::<Vec<_>>()
        .join("\n");
    let corps = serde_json::json!({
        "model": model,
        "stream": false,
        "max_tokens": 400,
        "temperature": 0.1,
        "messages": [
            {
                "role": "system",
                "content": format!(
                    "Voici les fiches de mémoire existantes, au format \
                     « id | groupe | titre | résumé » :\n{liste}\n\n\
                     La personne te demande une action sur cette mémoire. Réponds par une \
                     ligne par action, en reprenant l'id exact de la fiche visée :\n\
                     OUBLIE <id>\n\
                     RESUME <id> | nouveau résumé\n\
                     RENOMME <id> | nouveau titre\n\
                     Une fiche par ligne, autant de lignes que nécessaire. N'invente aucun \
                     id : n'agis que sur les fiches listées ci-dessus. Si l'instruction ne \
                     correspond à aucune fiche listée, réponds exactement RIEN."
                )
            },
            { "role": "user", "content": tronquer(instruction, 500) }
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
    // Seules les actions qui visent une fiche réellement listée sont
    // retenues : un id inventé par le modèle ne doit rien pouvoir toucher.
    let connus: std::collections::HashSet<&str> = fiches.iter().map(|f| f.id.as_str()).collect();
    lire_actions(brut)
        .into_iter()
        .filter(|a| connus.contains(a.id()))
        .collect()
}

impl MemoryAction {
    fn id(&self) -> &str {
        match self {
            MemoryAction::Forget { id }
            | MemoryAction::SetSummary { id, .. }
            | MemoryAction::Rename { id, .. } => id,
        }
    }
}

/// Transformer la réponse du modèle en actions.
pub fn lire_actions(brut: &str) -> Vec<MemoryAction> {
    let apres_reflexion = brut.rsplit("</think>").next().unwrap_or(brut);
    let mut out = Vec::new();
    for ligne in apres_reflexion.lines() {
        let ligne = ligne.trim().trim_start_matches(['-', '*', '•']).trim();
        if ligne.is_empty() || ligne.eq_ignore_ascii_case("rien") {
            continue;
        }
        let Some((mot, reste)) = ligne.split_once(char::is_whitespace) else {
            continue;
        };
        let reste = reste.trim();
        match mot.to_ascii_uppercase().as_str() {
            "OUBLIE" if !reste.is_empty() && !reste.contains('|') => {
                out.push(MemoryAction::Forget {
                    id: reste.to_string(),
                });
            }
            "RESUME" => {
                if let Some((id, resume)) = reste.split_once('|') {
                    let (id, resume) = (id.trim(), resume.trim());
                    if !id.is_empty() && !resume.is_empty() {
                        out.push(MemoryAction::SetSummary {
                            id: id.to_string(),
                            summary: resume.to_string(),
                        });
                    }
                }
            }
            "RENOMME" => {
                if let Some((id, titre)) = reste.split_once('|') {
                    let (id, titre) = (id.trim(), titre.trim());
                    if !id.is_empty() && !titre.is_empty() {
                        out.push(MemoryAction::Rename {
                            id: id.to_string(),
                            title: titre.to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
        if out.len() == 20 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod profil_tests {
    use super::{lire_faits, lire_le_numero, Fait};

    #[test]
    fn les_faits_bien_formes_sont_gardes() {
        let f = lire_faits(
            "vous | Préférences | Préfère les réponses courtes\n\
             zones | Bot Bastet | Travaille sur ce robot avec Paul",
        );
        assert_eq!(
            f,
            vec![
                Fait {
                    group: "vous".into(),
                    title: "Préférences".into(),
                    detail: "Préfère les réponses courtes".into()
                },
                Fait {
                    group: "zones".into(),
                    title: "Bot Bastet".into(),
                    detail: "Travaille sur ce robot avec Paul".into()
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
        // Groupe inconnu, ligne sans séparateur, détail-paragraphe, titre
        // démesuré : rien de tout cela n'a sa place dans une mémoire relue à
        // chaque réponse.
        let f = lire_faits(&format!(
            "humeur | Humeur | content aujourd'hui\n\
             une ligne sans separateur\n\
             zones | {} | détail court\n\
             sujets | Titre | {}",
            "a".repeat(70),
            "a".repeat(210)
        ));
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn trois_faits_au_maximum() {
        let f = lire_faits(
            "sujets | Un | un\nsujets | Deux | deux\nsujets | Trois | trois\nsujets | Quatre | quatre",
        );
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn un_numero_seul_et_dans_la_liste() {
        assert_eq!(lire_le_numero("2", 3), Some(1));
        assert_eq!(lire_le_numero(" 1 \n", 3), Some(0));
    }

    #[test]
    fn le_refus_est_respecte() {
        assert_eq!(lire_le_numero("AUCUN", 3), None);
        assert_eq!(
            lire_le_numero(
                "Aucun de ces projets ne correspond, mais le 2 s'en approche",
                3
            ),
            None,
            "un refus l'emporte, même suivi d'un chiffre"
        );
    }

    #[test]
    fn hors_liste_ou_hesitant_ne_range_rien() {
        assert_eq!(lire_le_numero("7", 3), None, "au-delà de la liste");
        assert_eq!(lire_le_numero("0", 3), None, "la liste commence à 1");
        assert_eq!(
            lire_le_numero("1 ou 2", 3),
            None,
            "deux nombres, donc un doute"
        );
        assert_eq!(lire_le_numero("le projet Locaryn", 3), None);
    }

    #[test]
    fn la_reflexion_ne_compte_pas() {
        assert_eq!(
            lire_le_numero("<think>Hmm, 1 ou 3 ?</think>3", 3),
            Some(2),
            "seul ce qui suit la réflexion est lu"
        );
    }
}

#[cfg(test)]
mod memory_command_tests {
    use super::{lire_actions, MemoryAction};

    #[test]
    fn oublie_reconnait_un_identifiant_seul() {
        let a = lire_actions("OUBLIE abc-123");
        assert_eq!(
            a,
            vec![MemoryAction::Forget {
                id: "abc-123".into()
            }]
        );
    }

    #[test]
    fn resume_et_renomme_lisent_les_deux_champs() {
        let a = lire_actions("RESUME id-1 | Nouveau résumé\nRENOMME id-2 | Nouveau titre");
        assert_eq!(
            a,
            vec![
                MemoryAction::SetSummary {
                    id: "id-1".into(),
                    summary: "Nouveau résumé".into()
                },
                MemoryAction::Rename {
                    id: "id-2".into(),
                    title: "Nouveau titre".into()
                },
            ]
        );
    }

    #[test]
    fn rien_ne_produit_aucune_action() {
        assert!(lire_actions("RIEN").is_empty());
        assert!(lire_actions("").is_empty());
    }

    #[test]
    fn une_ligne_mal_formee_est_ignoree_sans_casser_les_autres() {
        let a = lire_actions("QUOI id-1\nOUBLIE id-2");
        assert_eq!(a, vec![MemoryAction::Forget { id: "id-2".into() }]);
    }

    /// « OUBLIE » qui contiendrait un « | » n'est pas un identifiant simple :
    /// mieux vaut l'ignorer que d'oublier une fiche au hasard.
    #[test]
    fn oublie_refuse_un_identifiant_qui_contient_une_barre() {
        assert!(lire_actions("OUBLIE id | autre chose").is_empty());
    }

    #[test]
    fn vingt_actions_au_maximum() {
        let lignes: Vec<String> = (0..30).map(|i| format!("OUBLIE id-{i}")).collect();
        let a = lire_actions(&lignes.join("\n"));
        assert_eq!(a.len(), 20);
    }
}
