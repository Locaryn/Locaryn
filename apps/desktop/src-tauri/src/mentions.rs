//! Les mentions `@Nom` d'un message : un connecteur MCP, une extension ou un
//! skill que la personne demande d'utiliser pour cette réponse.
//!
//! Un petit modèle ne se rend pas toujours compte qu'un connecteur branché
//! sait faire ce qu'on lui demande. `@Roblox_Studio construis une carte` dit
//! explicitement de passer par lui : on ajoute au message envoyé au modèle
//! (pas à celui enregistré) la liste des outils concernés. C'est la demande de
//! la personne rendue explicite, pas une consigne de comportement.

use crate::{extensions, mcp_servers, skills, Core};
use tauri::State;

/// `@nom` présent dans `texte`, insensible à la casse, et pas au milieu d'un
/// mot (`courriel@nom.fr` n'est pas une mention).
fn mentionne(texte: &str, nom: &str) -> bool {
    let bas = texte.to_lowercase();
    let cible = format!("@{}", nom.to_lowercase());
    let mut depart = 0;
    while let Some(i) = bas[depart..].find(&cible) {
        let debut = depart + i;
        let fin = debut + cible.len();
        let avant_ok = bas[..debut]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let apres_ok = bas[fin..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'));
        if avant_ok && apres_ok {
            return true;
        }
        depart = fin;
    }
    false
}

/// Ce qu'on ajoute au message pour le modèle, si `texte` mentionne un
/// connecteur ou une extension active.
pub async fn precisions(core: State<'_, Core>, texte: &str) -> Option<String> {
    if !texte.contains('@') {
        return None;
    }
    let mut lignes = Vec::new();
    if let Ok(serveurs) = mcp_servers::list_mcp_servers(core.clone()).await {
        for s in serveurs.iter().filter(|s| mentionne(texte, &s.name)) {
            let outils: Vec<&str> = s
                .tools
                .iter()
                .filter(|t| !s.disabled_tools.contains(t))
                .map(String::as_str)
                .collect();
            lignes.push(if outils.is_empty() {
                format!(
                    "- le connecteur MCP « {} » (il n'est pas démarré : ses outils n'apparaissent pas encore)",
                    s.name
                )
            } else {
                format!(
                    "- le connecteur MCP « {} », outils : {}",
                    s.name,
                    outils.join(", ")
                )
            });
        }
    }
    let tous_les_skills = skills::tous(core.inner()).await;
    if let Ok(exts) = extensions::list_extensions(core.clone()).await {
        for e in exts.iter().filter(|e| {
            e.enabled && (mentionne(texte, &e.name) || mentionne(texte, &e.display_name))
        }) {
            // Une extension désignée : ses skills et ses connecteurs, nommés,
            // pour que le modèle sache ce qu'elle apporte réellement.
            let siens: Vec<&str> = tous_les_skills
                .iter()
                .filter(|(source, _)| source == &e.name)
                .map(|(_, s)| s.name.as_str())
                .collect();
            lignes.push(if siens.is_empty() {
                format!("- l'extension « {} »", e.display_name)
            } else {
                format!(
                    "- l'extension « {} » (skills : {} — mentionne-les avec @ pour les suivre)",
                    e.display_name,
                    siens.join(", ")
                )
            });
        }
    }

    // Les skills désignés : leur contenu complet, pas seulement leur nom.
    let imposes: Vec<String> = tous_les_skills
        .iter()
        .filter(|(_, s)| mentionne(texte, &s.name))
        .map(|(source, s)| skills::consigne(source, s))
        .collect();

    if lignes.is_empty() && imposes.is_empty() {
        return None;
    }
    let mut out = String::new();
    if !lignes.is_empty() {
        out.push_str(&format!(
            "\n\n[Pour cette demande, la personne a désigné ces outils — utilise-les pour répondre :\n{}]",
            lignes.join("\n")
        ));
    }
    for c in imposes {
        out.push_str("\n\n");
        out.push_str(&c);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::mentionne;

    #[test]
    fn une_mention_est_un_mot_entier() {
        assert!(mentionne(
            "@Roblox_Studio construis une carte",
            "Roblox_Studio"
        ));
        assert!(mentionne("avec @roblox_studio stp", "Roblox_Studio"));
        assert!(mentionne("(@fetch)", "fetch"));
        assert!(!mentionne("écris à moi@fetch.fr", "fetch"));
        assert!(!mentionne("@fetcher la page", "fetch"));
        assert!(!mentionne("sans mention", "fetch"));
    }
}
