//! Faire tenir les outils offerts au modèle dans son contexte.
//!
//! Chaque outil est décrit au modèle : nom, description, schéma des arguments.
//! Quelques connecteurs MCP suffisent à remplir un contexte local — Roblox Studio
//! en annonce 28, aux descriptions longues, et chaque outil MCP est offert sous
//! deux noms. Mesuré le 04/10/2026 : 41 009 jetons de requête pour un contexte de
//! 8 192, donc une erreur immédiate (« exceeds the available context size ») et
//! un chat qui ne répondait plus du tout.
//!
//! Tant que tout tient, rien ne change. Sinon, par ordre croissant de perte :
//! 1. un seul nom par outil MCP (le court, que le modèle appelle) ;
//! 2. des descriptions raccourcies et des schémas sans leur prose ;
//! 3. seuls les outils qui ressemblent à la demande, les outils intégrés restant
//!    toujours.
//!
//! La réduction est annoncée (journal et `StreamEvent::Log`), jamais silencieuse :
//! un outil absent sans explication ressemble à un modèle qui refuse.

use crate::mcp_tools::MCP_PREFIX;
use crate::tools::ToolSpec;
use std::collections::{HashMap, HashSet};

/// Part du contexte que les définitions d'outils ont le droit d'occuper. Le reste
/// sert à l'historique, à la demande et à la réponse.
const CONTEXT_SHARE: f64 = 0.45;

/// Octets de JSON par jeton, estimation prudente : du JSON et des identifiants
/// se découpent plus finement que de la prose.
const BYTES_PER_TOKEN: f64 = 3.0;

/// Longueur maximale d'une description raccourcie.
const SHORT_DESCRIPTION: usize = 160;

/// Ce qu'il est advenu des outils.
#[derive(Debug, Clone)]
pub struct Fit {
    pub specs: Vec<ToolSpec>,
    /// Outils retirés faute de place, intégrés exceptés.
    pub dropped: usize,
    /// Descriptions raccourcies.
    pub compacted: bool,
    /// Allégement imposé alors que la personne l'a désactivé : les outils
    /// seuls dépassaient ce que la fenêtre peut porter.
    pub forced: bool,
}

/// Jetons estimés d'un outil tel qu'il part dans la requête.
pub fn estimate_tokens(spec: &ToolSpec) -> usize {
    let json = serde_json::json!({
        "name": spec.name,
        "description": spec.description,
        "parameters": spec.input_schema,
    });
    let bytes = serde_json::to_string(&json).map(|s| s.len()).unwrap_or(0);
    (bytes as f64 / BYTES_PER_TOKEN).ceil() as usize
}

fn total_tokens(specs: &[ToolSpec]) -> usize {
    specs.iter().map(estimate_tokens).sum()
}

/// Le budget d'outils pour un contexte de `ctx` jetons.
pub fn budget_for(ctx: usize) -> usize {
    (ctx as f64 * CONTEXT_SHARE) as usize
}

fn is_mcp(spec: &ToolSpec) -> bool {
    spec.name.starts_with(MCP_PREFIX)
}

/// `mcp__serveur__outil` → (`serveur`, `outil`), en coupant au premier `__`.
/// Approximatif quand le nom du serveur contient lui-même `__`
/// (`plugin__serveur`) : voir `split_known`.
fn split_prefixed(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix(MCP_PREFIX)?;
    rest.split_once("__")
}

/// Comme `split_prefixed`, mais en cherchant la coupure qui laisse un nom court
/// que l'on connaît : `mcp__morph-cluster__cluster__cluster_status` se lit
/// (`morph-cluster__cluster`, `cluster_status`) parce que `cluster_status` est
/// offert, et non (`morph-cluster`, `cluster__cluster_status`).
fn split_known<'a>(name: &'a str, clean: &HashSet<String>) -> Option<(&'a str, &'a str)> {
    let rest = name.strip_prefix(MCP_PREFIX)?;
    let mut from = 0;
    while let Some(i) = rest[from..].find("__") {
        let at = from + i;
        let tool = &rest[at + 2..];
        if clean.contains(tool) {
            return Some((&rest[..at], tool));
        }
        from = at + 2;
    }
    None
}

/// Un seul nom par outil MCP : le court quand il est offert, le préfixé sinon
/// (deux connecteurs qui annoncent le même nom, ou un nom pris par un outil
/// intégré). Rend aussi, pour chaque nom court, le serveur d'où il vient : il
/// aide à reconnaître ce que la demande désigne.
fn one_name_per_tool(specs: Vec<ToolSpec>) -> (Vec<ToolSpec>, HashMap<String, String>) {
    let clean: HashSet<String> = specs
        .iter()
        .filter(|s| !is_mcp(s))
        .map(|s| s.name.clone())
        .collect();
    let mut server_of: HashMap<String, String> = HashMap::new();
    let mut kept = Vec::with_capacity(specs.len());
    for spec in specs {
        if !is_mcp(&spec) {
            kept.push(spec);
            continue;
        }
        // Le court existe : il suffit, et on retient son serveur. Sinon on garde
        // le préfixé, seul moyen de distinguer deux connecteurs au même outil.
        match split_known(&spec.name, &clean) {
            Some((server, tool)) => {
                server_of.insert(tool.to_string(), server.to_string());
            }
            None => kept.push(spec),
        }
    }
    (kept, server_of)
}

/// Longueur maximale de la description d'un argument obligatoire.
const REQUIRED_ARGUMENT_DESCRIPTION: usize = 220;

/// Raccourcir une description à sa première phrase.
fn short_description(text: &str) -> String {
    short_text(text, SHORT_DESCRIPTION)
}

/// Les `max` premiers caractères du premier paragraphe, coupés à la fin d'une
/// phrase quand il y en a une assez loin.
fn short_text(text: &str, max: usize) -> String {
    let first = text
        .split(['\n', '\r'])
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let sentence_end = first
        .match_indices(". ")
        .map(|(i, _)| i + 1)
        .find(|&i| i >= 20)
        .unwrap_or(first.len());
    let cut = first[..sentence_end].trim();
    if cut.chars().count() <= max {
        return cut.to_string();
    }
    let truncated: String = cut.chars().take(max - 1).collect();
    format!("{}…", truncated.trim_end())
}

/// Retirer d'un schéma la prose (`description`, `title`, `examples`) : le modèle
/// garde les noms, les types, les valeurs permises et ce qui est obligatoire.
///
/// Les arguments **obligatoires** gardent une description courte : c'est elle qui
/// dit comment les remplir (Roblox Studio : « datamodel_type … This is a required
/// argument »), et un modèle qui les remplit mal échoue à tous les appels.
///
/// `in_properties` : l'objet courant est la table `properties`, dont les clés sont
/// des noms d'arguments — un argument peut s'appeler « description » et ne doit
/// pas être retiré.
fn strip_schema_prose(value: &mut serde_json::Value, in_properties: bool) {
    match value {
        serde_json::Value::Object(map) => {
            let required: HashSet<String> = map
                .get("required")
                .and_then(|r| r.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if !in_properties {
                map.remove("description");
                map.remove("title");
                map.remove("examples");
                map.remove("$schema");
            }
            for (key, child) in map.iter_mut() {
                let is_properties = !in_properties && key == "properties";
                if is_properties {
                    strip_properties(child, &required);
                } else {
                    strip_schema_prose(child, false);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                strip_schema_prose(child, false);
            }
        }
        _ => {}
    }
}

/// La table `properties` d'un schéma : chaque argument est allégé, et les
/// obligatoires gardent leur description raccourcie.
fn strip_properties(properties: &mut serde_json::Value, required: &HashSet<String>) {
    let Some(map) = properties.as_object_mut() else {
        return;
    };
    for (name, schema) in map.iter_mut() {
        let kept = if required.contains(name) {
            schema
                .get("description")
                .and_then(|d| d.as_str())
                .map(|d| short_text(d, REQUIRED_ARGUMENT_DESCRIPTION))
        } else {
            None
        };
        strip_schema_prose(schema, false);
        if let (Some(text), Some(obj)) = (kept, schema.as_object_mut()) {
            obj.insert("description".into(), serde_json::Value::String(text));
        }
    }
}

fn compact(spec: &ToolSpec) -> ToolSpec {
    let mut out = spec.clone();
    out.description = short_description(&spec.description);
    strip_schema_prose(&mut out.input_schema, false);
    out
}

fn words(text: &str) -> HashSet<String> {
    const STOP: [&str; 24] = [
        "le", "la", "les", "de", "des", "du", "un", "une", "et", "ou", "en", "pour", "avec",
        "dans", "sur", "the", "and", "for", "with", "use", "utilise", "outil", "peux", "tu",
    ];
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(w))
        .map(str::to_string)
        .collect()
}

/// Ressemblance entre la demande et un outil : le nom du serveur et de l'outil
/// pèsent plus que la description, parce que c'est ce que la personne nomme.
fn relevance(request: &HashSet<String>, spec: &ToolSpec, server: Option<&str>) -> usize {
    let mut score = 0;
    let name_words = words(&spec.name.replace('_', " "));
    score += 4 * request.intersection(&name_words).count();
    if let Some(server) = server {
        let server_words = words(&server.replace('_', " "));
        score += 6 * request.intersection(&server_words).count();
    }
    score + request.intersection(&words(&spec.description)).count()
}

/// Part de la fenêtre au-delà de laquelle les outils sont allégés même quand
/// la personne a désactivé l'allègement : au-dessus, il ne reste plus la place
/// d'une question et d'une réponse.
const FORCED_SHARE: f64 = 0.8;

/// Choisir les outils offerts au modèle selon le réglage de la personne.
///
/// `alleger` faux (le défaut) : tous les outils partent — un seul nom par
/// outil MCP, ce qui ne retire rien — tant qu'ils laissent la place de
/// converser ; au-delà de [`FORCED_SHARE`], l'allègement s'impose et le dit.
/// `alleger` vrai : l'allègement s'applique dès [`CONTEXT_SHARE`] (petits
/// modèles, petits contextes).
pub fn fit_selon(specs: Vec<ToolSpec>, ctx: usize, request: &str, alleger: bool) -> Fit {
    if alleger {
        return fit(specs, ctx, request);
    }
    let (specs, _) = one_name_per_tool(specs);
    if total_tokens(&specs) <= (ctx as f64 * FORCED_SHARE) as usize {
        return Fit {
            specs,
            dropped: 0,
            compacted: false,
            forced: false,
        };
    }
    Fit {
        forced: true,
        ..fit(specs, ctx, request)
    }
}

/// Faire tenir `specs` dans `ctx` jetons de contexte. `request` est la demande de
/// la personne : elle départage les outils quand il faut en retirer.
pub fn fit(specs: Vec<ToolSpec>, ctx: usize, request: &str) -> Fit {
    let budget = budget_for(ctx);
    if total_tokens(&specs) <= budget {
        return Fit {
            specs,
            dropped: 0,
            compacted: false,
            forced: false,
        };
    }

    // 1. un seul nom par outil MCP
    let (specs, server_of) = one_name_per_tool(specs);
    if total_tokens(&specs) <= budget {
        return Fit {
            specs,
            dropped: 0,
            compacted: false,
            forced: false,
        };
    }

    // 2. descriptions et schémas allégés (les outils intégrés gardent les leurs :
    // le modèle en dépend, et ils sont peu nombreux)
    let specs: Vec<ToolSpec> = specs
        .into_iter()
        .map(|s| {
            if is_builtin_like(&s, &server_of) {
                s
            } else {
                compact(&s)
            }
        })
        .collect();
    if total_tokens(&specs) <= budget {
        return Fit {
            specs,
            dropped: 0,
            compacted: true,
            forced: false,
        };
    }

    // 3. seuls les outils qui ressemblent à la demande, intégrés toujours gardés
    let request_words = words(request);
    let (fixed, mut optional): (Vec<ToolSpec>, Vec<ToolSpec>) = specs
        .into_iter()
        .partition(|s| is_builtin_like(s, &server_of));
    let fixed_len = fixed.len();
    let mut used = total_tokens(&fixed);
    optional.sort_by_key(|s| {
        let server = server_of
            .get(&s.name)
            .map(String::as_str)
            .or_else(|| split_prefixed(&s.name).map(|(srv, _)| srv));
        std::cmp::Reverse(relevance(&request_words, s, server))
    });
    let total_optional = optional.len();
    let mut kept = fixed;
    for spec in optional {
        let cost = estimate_tokens(&spec);
        if used + cost > budget {
            continue;
        }
        used += cost;
        kept.push(spec);
    }
    let dropped = total_optional - (kept.len() - fixed_len);
    Fit {
        specs: kept,
        dropped,
        compacted: true,
        forced: false,
    }
}

/// Outil que l'on ne retire ni n'allège : tout ce qui ne vient pas d'un
/// connecteur MCP.
fn is_builtin_like(spec: &ToolSpec, server_of: &HashMap<String, String>) -> bool {
    !is_mcp(spec) && !server_of.contains_key(&spec.name)
}

/// Le contexte du serveur d'inférence, en jetons, quand il le dit.
///
/// llama.cpp l'annonce dans `/props`. La valeur de la configuration de la
/// personne n'est pas la bonne source : le serveur a pu être lancé avec un autre
/// `-c`, et c'est lui qui refuse la requête.
pub async fn server_context(client: &reqwest::Client, endpoint: &str) -> Option<usize> {
    let url = format!("{}/props", endpoint.trim_end_matches('/'));
    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    v.pointer("/default_generation_settings/n_ctx")
        .or_else(|| v.get("n_ctx"))
        .and_then(|n| n.as_u64())
        .map(|n| n as usize)
        .filter(|n| *n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Risk;

    fn spec(name: &str, description: &str) -> ToolSpec {
        ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Le chemin du fichier, avec beaucoup de détails inutiles au modèle." },
                    "description": { "type": "string", "description": "Un argument qui s'appelle description." }
                },
                "required": ["path"],
                "description": "Prose du schéma."
            }),
            risk: Risk::Medium,
            required_permissions: Vec::new(),
        }
    }

    /// Les 28 outils d'un connecteur « Roblox_Studio », sous leurs deux noms, plus
    /// ceux d'un autre connecteur et un outil intégré.
    fn beaucoup_d_outils() -> Vec<ToolSpec> {
        let long = "Fait quelque chose de très utile dans le logiciel. ".repeat(12);
        let mut v = vec![spec("lire_fichier", "Lit un fichier.")];
        for i in 0..28 {
            let nom = format!("studio_outil_{i}");
            v.push(spec(&format!("mcp__Roblox_Studio__{nom}"), &long));
            v.push(spec(&nom, &long));
        }
        for i in 0..30 {
            let nom = format!("cluster_outil_{i}");
            v.push(spec(&format!("mcp__morph-cluster__{nom}"), &long));
            v.push(spec(&nom, &long));
        }
        v
    }

    #[test]
    fn tout_ce_qui_tient_reste_tel_quel() {
        let specs = vec![
            spec("a", "Court."),
            spec("mcp__s__b", "Court."),
            spec("b", "Court."),
        ];
        let r = fit(specs.clone(), 32_768, "peu importe");
        assert_eq!(r.specs.len(), 3);
        assert_eq!(r.dropped, 0);
        assert!(!r.compacted);
    }

    #[test]
    fn un_contexte_de_8k_recoit_des_outils_qui_tiennent() {
        let avant = total_tokens(&beaucoup_d_outils());
        assert!(avant > 8192, "le cas de départ doit déborder : {avant}");
        let r = fit(
            beaucoup_d_outils(),
            8192,
            "utilise studio_outil_3 de Roblox Studio",
        );
        assert!(
            total_tokens(&r.specs) <= budget_for(8192),
            "{}",
            total_tokens(&r.specs)
        );
        assert!(r.compacted);
        assert!(r.dropped > 0);
        // L'outil intégré reste, et celui que la demande nomme aussi.
        assert!(r.specs.iter().any(|s| s.name == "lire_fichier"));
        assert!(r.specs.iter().any(|s| s.name == "studio_outil_3"));
    }

    #[test]
    fn la_demande_departage_les_connecteurs() {
        let r = fit(beaucoup_d_outils(), 8192, "montre l'état du cluster");
        let cluster = r
            .specs
            .iter()
            .filter(|s| s.name.contains("cluster"))
            .count();
        let studio = r.specs.iter().filter(|s| s.name.contains("studio")).count();
        assert!(cluster > studio, "cluster={cluster} studio={studio}");
    }

    #[test]
    fn sans_allegement_tous_les_outils_partent_tant_qu_ils_laissent_la_place() {
        // 8 192 jetons : l'allègement retirerait des outils, le réglage par
        // défaut les garde tous tant qu'ils restent sous 80 % de la fenêtre.
        let outils = beaucoup_d_outils();
        let tous = outils.len();
        let leger = fit_selon(outils.clone(), 8192, "x", true);
        assert!(leger.dropped > 0 || leger.compacted);
        let large = fit_selon(outils.clone(), 200_000, "x", false);
        assert_eq!(large.dropped, 0);
        assert!(!large.compacted && !large.forced);
        assert!(
            large.specs.len() <= tous,
            "seuls les doublons de noms partent"
        );
        // Des outils plus gros que la fenêtre : l'allègement s'impose et le dit.
        let force = fit_selon(outils, 1024, "x", false);
        assert!(force.forced);
    }

    #[test]
    fn un_seul_nom_par_outil_quand_cela_suffit() {
        // Trop gros avec deux noms, assez petit avec un seul.
        let mut specs = Vec::new();
        for i in 0..10 {
            specs.push(spec(&format!("mcp__s__t{i}"), "Court."));
            specs.push(spec(&format!("t{i}"), "Court."));
        }
        let un_nom = total_tokens(&specs) / 2;
        let ctx = (un_nom as f64 / CONTEXT_SHARE).ceil() as usize + 8;
        let r = fit(specs, ctx, "x");
        assert_eq!(r.specs.len(), 10);
        assert!(r.specs.iter().all(|s| !s.name.starts_with("mcp__")));
        assert!(!r.compacted);
    }

    #[test]
    fn le_nom_court_suffit_quand_il_est_offert() {
        let specs = vec![spec("mcp__a__ouvrir", "x"), spec("ouvrir", "x")];
        let (kept, server_of) = one_name_per_tool(specs);
        let noms: Vec<&str> = kept.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(noms, ["ouvrir"]);
        assert_eq!(server_of["ouvrir"], "a");
    }

    #[test]
    fn deux_connecteurs_au_meme_nom_sans_nom_court_gardent_leur_nom_prefixe() {
        let specs = vec![spec("mcp__a__ouvrir", "x"), spec("mcp__b__ouvrir", "x")];
        let (kept, _) = one_name_per_tool(specs);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn un_serveur_d_extension_contenant_deux_soulignes_est_lu_correctement() {
        let specs = vec![
            spec("mcp__morph-cluster__cluster__cluster_status", "x"),
            spec("cluster_status", "x"),
        ];
        let (kept, server_of) = one_name_per_tool(specs);
        assert_eq!(kept.len(), 1);
        assert_eq!(server_of["cluster_status"], "morph-cluster__cluster");
    }

    #[test]
    fn un_argument_nomme_description_survit_a_l_allegement() {
        let c = compact(&spec(
            "t",
            "Lit le contenu du fichier demandé. Seconde phrase.",
        ));
        let props = &c.input_schema["properties"];
        assert!(props.get("description").is_some(), "{props}");
        // `path` est obligatoire : sa description reste, raccourcie. L'argument
        // facultatif qui s'appelle « description » existe toujours, sans prose.
        assert!(props["path"].get("description").is_some());
        assert!(props["description"].get("description").is_none());
        assert!(c.input_schema.get("description").is_none());
        assert_eq!(c.description, "Lit le contenu du fichier demandé.");
        assert_eq!(c.input_schema["required"][0], "path");
    }

    #[test]
    fn un_argument_obligatoire_garde_sa_description_courte() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "datamodel_type": { "type": "string", "enum": ["Edit"], "description": "Le datamodel visé. C'est un argument obligatoire. Plus de détails inutiles ici." },
                "note": { "type": "string", "description": "Facultatif, avec beaucoup de prose." }
            },
            "required": ["datamodel_type"]
        });
        strip_schema_prose(&mut schema, false);
        let props = &schema["properties"];
        assert_eq!(
            props["datamodel_type"]["description"],
            "Le datamodel visé. C'est un argument obligatoire."
        );
        assert_eq!(props["datamodel_type"]["enum"][0], "Edit");
        assert!(props["note"].get("description").is_none());
    }

    #[test]
    fn une_description_trop_longue_est_coupee() {
        let d = short_description(&"mot ".repeat(200));
        assert!(d.chars().count() <= SHORT_DESCRIPTION);
        assert!(d.ends_with('…'));
    }
}
