//! Les mentions `@Nom` d'un message : un connecteur MCP, un morph (extension)
//! ou un skill que la personne impose pour cette réponse.
//!
//! Un petit modèle ne se rend pas toujours compte qu'un connecteur branché
//! sait faire ce qu'on lui demande. `@Roblox_Studio construis une carte` dit
//! explicitement de passer par lui : on ajoute au message envoyé au modèle
//! (pas à celui enregistré) la liste des outils concernés. C'est la demande de
//! la personne rendue explicite, pas une consigne de comportement.

use crate::{extensions, mcp_servers, skills, Core};
use locaryn_shared_types::InstalledExtension;
use tauri::State;

/// `@nom` présent dans `texte`, insensible à la casse, et pas au milieu d'un
/// mot (`courriel@nom.fr` n'est pas une mention).
fn mentionne(texte: &str, nom: &str) -> bool {
    designe(texte, '@', nom)
}

/// `<préfixe>nom` présent dans `texte`, mot entier. Un skill se désigne aussi
/// par `/nom`, comme dans Claude Code ou Codex.
fn designe(texte: &str, prefixe: char, nom: &str) -> bool {
    let bas = texte.to_lowercase();
    let cible = format!("{prefixe}{}", nom.to_lowercase());
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

/// La valeur qui suit `@<nom>:` pour l'un des noms d'une extension
/// (`@wsl:Ubuntu-22.04` → `Ubuntu-22.04`), si la personne en a écrit une.
pub fn valeur_de_mention(texte: &str, noms: &[&str]) -> Option<String> {
    let bas = texte.to_lowercase();
    for nom in noms.iter().filter(|n| !n.is_empty()) {
        let prefixe = format!("@{}:", nom.to_lowercase());
        if let Some(i) = bas.find(&prefixe) {
            let valeur: String = texte[i + prefixe.len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
                .collect();
            if !valeur.is_empty() {
                return Some(valeur);
            }
        }
    }
    None
}

/// Ce que le manifeste d'une extension dit de sa mention.
fn mention_de(e: &InstalledExtension) -> Option<locaryn_extensions::manifest::MentionSpec> {
    locaryn_extensions::manifest::load(std::path::Path::new(&e.install_dir))
        .ok()?
        .mention
}

/// Les noms qui désignent une extension après « @ ».
fn noms_de<'a>(
    e: &'a InstalledExtension,
    spec: Option<&'a locaryn_extensions::manifest::MentionSpec>,
) -> Vec<&'a str> {
    let mut noms = vec![e.name.as_str(), e.display_name.as_str()];
    if let Some(s) = spec {
        noms.extend(s.aliases.iter().map(String::as_str));
    }
    noms
}

/// Ce qu'une mention change à la requête : le texte ajouté au message, et
/// les outils que le modèle doit se voir offrir — et appeler.
#[derive(Debug, Default)]
pub struct Mentions {
    /// Ajouté au message envoyé au modèle (le message enregistré reste tel quel).
    pub ajout: Option<String>,
    /// Noms des outils imposés (`mcp__<serveur>__<outil>`) : toujours offerts,
    /// jamais différés, et le premier tour exige un appel d'outil. Une ligne
    /// dans le message ne suffisait pas : un petit modèle ne comprenait pas
    /// qu'il avait ces outils, et répondait sans s'en servir.
    pub outils: Vec<String>,
    /// Le premier tour doit appeler un de ces outils : vrai seulement quand la
    /// personne les a imposés par « @ ». Un connecteur simplement nommé dans
    /// la demande voit ses outils garantis, pas exigés.
    pub exiger: bool,
}

/// Les mots d'un nom de connecteur assez parlants pour le reconnaître dans
/// une phrase : « Roblox_Studio » → « roblox ». Les mots génériques
/// (« studio », « server »…) désigneraient n'importe quoi.
fn mots_reperes(nom: &str) -> Vec<String> {
    const GENERIQUES: &[&str] = &[
        "studio",
        "server",
        "serveur",
        "tools",
        "outils",
        "local",
        "plugin",
        "connector",
        "connecteur",
        "client",
        "bridge",
        // Tous les morphs s'appellent « morph-… » : « morph » ne désigne
        // aucun d'eux en particulier.
        "morph",
        "morphs",
    ];
    nom.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|m| m.chars().count() >= 4 && !GENERIQUES.contains(&m.as_str()))
        .collect()
}

/// Les mots qui, juste avant un nom, le refusent : « pas le morph desktop »,
/// « sans Roblox », « don't use chrome ».
const NEGATIONS: &[&str] = &[
    "pas", "sans", "jamais", "ni", "not", "without", "never", "t",
];

/// Le connecteur est-il nommé en clair dans la demande (sans « @ ») ?
///
/// Pas quand la phrase le refuse : « utilise le navigateur intégré, pas le
/// morph desktop » imposait justement les outils de contrôle du bureau, et le
/// modèle prenait la main sur le PC au moment où on le lui interdisait.
fn nomme_en_clair(texte: &str, nom: &str) -> bool {
    let bas = texte.to_lowercase();
    let mots: Vec<&str> = bas
        .split(|c: char| !c.is_alphanumeric())
        .filter(|m| !m.is_empty())
        .collect();
    let reperes = mots_reperes(nom);
    mots.iter().enumerate().any(|(i, mot)| {
        reperes.iter().any(|r| r == mot)
            && !mots[i.saturating_sub(4)..i]
                .iter()
                .any(|avant| NEGATIONS.contains(avant))
    })
}

/// Ce que `texte` désigne : connecteurs MCP, morphs (extensions) et skills.
pub async fn precisions(core: State<'_, Core>, texte: &str) -> Mentions {
    if !texte.contains('@') && !texte.contains('/') {
        // Sans « @ », seul un connecteur nommé en clair compte : on le vérifie
        // sur les noms configurés avant d'interroger quoi que ce soit, pour ne
        // rien coûter aux messages qui n'en parlent pas.
        let noms: Vec<String> = core
            .mcp
            .config
            .lock()
            .map(|c| c.mcp_servers.keys().cloned().collect())
            .unwrap_or_default();
        if !noms.iter().any(|n| nomme_en_clair(texte, n)) {
            return Mentions::default();
        }
    }
    let mut lignes = Vec::new();
    let mut outils_imposes: Vec<String> = Vec::new();
    let mut outils_garantis: Vec<String> = Vec::new();
    let serveurs = mcp_servers::list_mcp_servers(core.clone())
        .await
        .unwrap_or_default();
    let exts = extensions::liste_installees(core.clone())
        .await
        .unwrap_or_default();
    let morphs: Vec<(
        &InstalledExtension,
        Option<locaryn_extensions::manifest::MentionSpec>,
    )> = exts
        .iter()
        .filter(|e| e.enabled)
        .map(|e| (e, mention_de(e)))
        .filter(|(e, spec)| {
            noms_de(e, spec.as_ref())
                .iter()
                .any(|n| mentionne(texte, n))
        })
        .collect();

    for s in &serveurs {
        // Un connecteur est imposé s'il est désigné par « @ », ou s'il
        // appartient à un morph désigné (`<morph>__<serveur>`).
        let impose = mentionne(texte, &s.name)
            || morphs
                .iter()
                .any(|(e, _)| s.name.starts_with(&format!("{}__", e.name)));
        if !impose {
            // Nommé en clair (« code dans mon Roblox Studio ») : ses outils
            // restent offerts même dans un petit contexte, sans rien exiger
            // ni démarrer.
            let disponible = s.running && s.companion.as_ref().is_none_or(|c| c.running);
            if disponible && nomme_en_clair(texte, &s.name) {
                outils_garantis.extend(
                    s.tools
                        .iter()
                        .filter(|t| !s.disabled_tools.contains(t))
                        .map(|t| locaryn_agent_runtime::mcp_tools::mcp_tool_name(&s.name, t)),
                );
            }
            continue;
        }
        // Le logiciel piloté est fermé : ses outils échoueront tous. Le dire
        // évite au modèle de les essayer un par un pour rien.
        if let Some(c) = s.companion.as_ref().filter(|c| !c.running) {
            lignes.push(format!(
                "- le connecteur MCP « {} » : {} n'est PAS ouvert sur cette machine, ses outils échoueront. N'appelle aucun de ses outils : demande d'abord à la personne d'ouvrir {}.",
                s.name, c.name, c.name
            ));
            continue;
        }
        // Désigné mais arrêté : on le démarre, plutôt que d'annoncer au modèle
        // des outils qu'il ne verra pas.
        let outils: Vec<String> = if s.running {
            s.tools.clone()
        } else {
            match mcp_servers::start_mcp_server(core.clone(), s.name.clone()).await {
                Ok(t) => t,
                Err(e) => {
                    lignes.push(format!(
                        "- le connecteur MCP « {} » n'a pas pu démarrer ({e}) : dis-le à la personne au lieu de deviner ses outils.",
                        s.name
                    ));
                    continue;
                }
            }
        };
        let outils: Vec<String> = outils
            .into_iter()
            .filter(|t| !s.disabled_tools.contains(t))
            .filter(|t| !locaryn_agent_runtime::mcp_tools::is_ui_only_tool(t))
            .collect();
        if outils.is_empty() {
            lignes.push(format!(
                "- le connecteur MCP « {} » (il n'annonce aucun outil utilisable)",
                s.name
            ));
            continue;
        }
        outils_imposes.extend(
            outils
                .iter()
                .map(|t| locaryn_agent_runtime::mcp_tools::mcp_tool_name(&s.name, t)),
        );
        lignes.push(format!(
            "- le connecteur MCP « {} », outils : {}",
            s.name,
            outils.join(", ")
        ));
    }

    let tous_les_skills = skills::tous(core.inner()).await;
    for (e, spec) in &morphs {
        // Un morph désigné : ses skills et ses connecteurs, nommés, pour que
        // le modèle sache ce qu'il apporte réellement.
        let siens: Vec<&str> = tous_les_skills
            .iter()
            .filter(|(source, _)| source == &e.name)
            .map(|(_, s)| s.name.as_str())
            .collect();
        // `@nom:valeur` : ce que la valeur veut dire, l'extension l'a écrit
        // dans son manifeste — le socle ne le devine pas.
        if let Some(phrase) = spec.as_ref().and_then(|s| s.argument.as_deref()) {
            if let Some(v) = valeur_de_mention(texte, &noms_de(e, spec.as_ref())) {
                lignes.push(format!("- {}", phrase.replace("{valeur}", &v)));
            }
        }
        lignes.push(if siens.is_empty() {
            format!("- le morph « {} »", e.display_name)
        } else {
            format!(
                "- le morph « {} » (skills : {} — mentionne-les avec @ pour les suivre)",
                e.display_name,
                siens.join(", ")
            )
        });
    }

    // Les skills désignés : leur contenu complet, pas seulement leur nom.
    let imposes: Vec<String> = tous_les_skills
        .iter()
        .filter(|(_, s)| mentionne(texte, &s.name) || designe(texte, '/', &s.name))
        .map(|(source, s)| skills::consigne(source, s))
        .collect();

    let exiger = !outils_imposes.is_empty();
    outils_imposes.extend(outils_garantis);
    if lignes.is_empty() && imposes.is_empty() {
        outils_imposes.sort();
        outils_imposes.dedup();
        return Mentions {
            ajout: None,
            outils: outils_imposes,
            exiger: false,
        };
    }
    let mut out = String::new();
    if !lignes.is_empty() {
        let consigne = if !exiger {
            "la personne a désigné ces outils — utilise-les pour répondre"
        } else {
            "la personne IMPOSE ces outils — appelle-les pour répondre, ne réponds pas de mémoire et ne dis pas que tu n'y as pas accès"
        };
        out.push_str(&format!(
            "\n\n[Pour cette demande, {consigne} :\n{}]",
            lignes.join("\n")
        ));
    }
    for c in imposes {
        out.push_str("\n\n");
        out.push_str(&c);
    }
    outils_imposes.sort();
    outils_imposes.dedup();
    Mentions {
        ajout: Some(out),
        outils: outils_imposes,
        exiger,
    }
}

#[cfg(test)]
mod tests {
    use super::{mentionne, valeur_de_mention};

    /// La valeur suit l'un des noms de l'extension, alias compris.
    #[test]
    fn la_valeur_d_une_mention_se_lit() {
        let noms = ["morph-exemple", "Exemple", "ex"];
        assert_eq!(
            valeur_de_mention("verifie @ex:Serveur-2.1 le docker", &noms).as_deref(),
            Some("Serveur-2.1")
        );
        assert_eq!(
            valeur_de_mention("@morph-exemple:Debian go", &noms).as_deref(),
            Some("Debian")
        );
        assert_eq!(valeur_de_mention("@ex fais ça", &noms), None);
        assert!(mentionne("@ex:Serveur fais", "ex"));
    }

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
        // Nommé en clair : « roblox » suffit, « studio » seul est trop vague.
        assert!(super::nomme_en_clair(
            "code dans mon Roblox Studio",
            "Roblox_Studio"
        ));
        assert!(!super::nomme_en_clair(
            "ouvre le studio photo",
            "Roblox_Studio"
        ));
        assert!(!super::nomme_en_clair(
            "parle-moi de robloxien",
            "Roblox_Studio"
        ));
        // Refusé dans la phrase : pas désigné.
        assert!(!super::nomme_en_clair(
            "Je t'ai dit d'utiliser le moteur intégré, pas le morph pilote qui permet de contrôler le PC.",
            "morph-pilote__pilote"
        ));
        assert!(!super::nomme_en_clair(
            "fais-le sans Roblox",
            "Roblox_Studio"
        ));
        assert!(!super::nomme_en_clair(
            "don't use the pilote",
            "morph-pilote__pilote"
        ));
        assert!(super::nomme_en_clair(
            "prends le contrôle avec pilote",
            "morph-pilote__pilote"
        ));
        // « morph » seul ne désigne aucun morph.
        assert!(!super::nomme_en_clair(
            "installe un morph",
            "morph-exemple__exemple"
        ));
        // Un skill se désigne aussi par « /nom », jamais au milieu d'un chemin.
        assert!(super::designe("fais un rapport /docx stp", '/', "docx"));
        assert!(!super::designe("ouvre docs/docx/a.md", '/', "docx"));
    }
}
