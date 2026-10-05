//! Les outils par lesquels le modèle agit sur Locaryn lui-même.
//!
//! « J'ai activé le serveur MCP de DaVinci, connecte-toi », « installe le skill
//! qui cherche des skills » : le modèle ajoute le connecteur ou installe
//! l'extension comme le ferait l'écran des réglages — par les mêmes commandes,
//! donc avec les mêmes contrôles. Chaque appel passe par la porte
//! d'approbation : au niveau « Autonome », installer demande toujours.
//!
//! Les descriptions ne disent que la mécanique des outils (voir la règle
//! « aucun caractère injecté ») : quand s'en servir reste l'affaire du modèle.

use crate::{extensions, mcp_servers, Core};
use locaryn_agent_runtime::host_tools::HostTools;
use locaryn_agent_runtime::tools::{Risk, ToolResult, ToolSpec};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

/// Combien d'extensions du catalogue une recherche rend au modèle : assez pour
/// choisir, trop peu pour noyer sa fenêtre de contexte.
const RESULTATS_CATALOGUE: usize = 10;

pub struct AppTools {
    app: AppHandle,
}

impl AppTools {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

fn spec(name: &str, description: &str, input_schema: Value, risk: Risk) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description: description.into(),
        input_schema,
        risk,
        required_permissions: Vec::new(),
    }
}

fn ok(output: String) -> ToolResult {
    ToolResult {
        ok: true,
        output,
        artifact: None,
    }
}

fn erreur(output: impl Into<String>) -> ToolResult {
    ToolResult {
        ok: false,
        output: output.into(),
        artifact: None,
    }
}

#[async_trait::async_trait]
impl HostTools for AppTools {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            spec(
                "app_list_connectors",
                "Liste les connecteurs MCP configurés dans Locaryn : nom, transport, commande ou adresse, démarré ou non.",
                json!({ "type": "object", "properties": {} }),
                Risk::Low,
            ),
            spec(
                "app_add_connector",
                "Ajoute un connecteur MCP à Locaryn et le démarre. `config` est la configuration JSON telle que la documentation d'un logiciel la donne : un bloc {\"mcpServers\": {...}}, ou un seul serveur {\"command\", \"args\", \"env\"} ou {\"url\"}. Rend les outils que le connecteur expose.",
                json!({
                    "type": "object",
                    "required": ["config"],
                    "properties": {
                        "config": { "description": "Configuration JSON du serveur MCP (texte ou objet)." },
                        "name": { "type": "string", "description": "Nom du connecteur quand `config` décrit un seul serveur sans nom." }
                    }
                }),
                Risk::Medium,
            ),
            spec(
                "app_list_extensions",
                "Liste les extensions installées dans Locaryn (morphs, skills, connecteurs fournis) : nom, version, active ou non, ce qu'elles apportent.",
                json!({ "type": "object", "properties": {} }),
                Risk::Low,
            ),
            spec(
                "app_search_extensions",
                "Cherche des extensions (morphs, skills, plugins) dans les catalogues de Locaryn. Rend nom, description et `install_source` à passer à app_install_extension.",
                json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": { "query": { "type": "string", "description": "Mots recherchés." } }
                }),
                Risk::Low,
            ),
            spec(
                "app_install_extension",
                "Installe et active une extension (morph, skill ou plugin) dans Locaryn, avec les permissions qu'elle demande. `source` : un `install_source` rendu par app_search_extensions, un dépôt « propriétaire/dépôt » ou une adresse git. Ses skills et ses outils sont disponibles au message suivant.",
                json!({
                    "type": "object",
                    "required": ["source"],
                    "properties": { "source": { "type": "string" } }
                }),
                Risk::High,
            ),
        ]
    }

    async fn call(&self, tool: &str, args: &Value) -> ToolResult {
        let core = self.app.state::<Core>();
        match tool {
            "app_list_connectors" => list_connectors(core).await,
            "app_add_connector" => add_connector(core, args).await,
            "app_list_extensions" => list_extensions(core).await,
            "app_search_extensions" => search_extensions(core, args).await,
            "app_install_extension" => install_extension(core, args).await,
            autre => erreur(format!("outil de l'application inconnu : {autre}")),
        }
    }
}

async fn list_connectors(core: tauri::State<'_, Core>) -> ToolResult {
    match mcp_servers::list_mcp_servers(core).await {
        // Les valeurs des variables d'environnement (des clés d'API, souvent)
        // ne partent jamais vers le modèle : seulement leurs noms.
        Ok(servers) => ok(json!(servers
            .iter()
            .map(|s| json!({
                "name": s.name,
                "transport": s.transport,
                "target": s.target,
                "running": s.running,
                "env": s.env.keys().collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>())
        .to_string()),
        Err(e) => erreur(e),
    }
}

async fn add_connector(core: tauri::State<'_, Core>, args: &Value) -> ToolResult {
    let texte = match &args["config"] {
        Value::String(s) => s.clone(),
        Value::Null => return erreur("`config` manque : la configuration JSON du serveur MCP."),
        objet => objet.to_string(),
    };
    let nom = args["name"].as_str().map(str::to_string);
    let resultat = match mcp_servers::import_mcp_json(core.clone(), texte, nom, true).await {
        Ok(r) => r,
        Err(e) => return erreur(e),
    };
    let ajoutes: Vec<_> = resultat
        .servers
        .iter()
        .filter(|s| resultat.imported.contains(&s.name))
        .collect();
    let mut lignes: Vec<String> = Vec::new();
    for s in &ajoutes {
        let outils = match mcp_servers::list_mcp_tools(core.clone(), s.name.clone()).await {
            Ok(t) => t.into_iter().map(|t| t.name).collect::<Vec<_>>().join(", "),
            Err(_) => String::new(),
        };
        lignes.push(if s.running {
            format!("« {} » ajouté et démarré. Outils : {}", s.name, outils)
        } else {
            format!("« {} » ajouté, mais pas démarré.", s.name)
        });
    }
    lignes.extend(resultat.errors.iter().map(|e| format!("Erreur : {e}")));
    let tout_va_bien = resultat.errors.is_empty() && ajoutes.iter().all(|s| s.running);
    ToolResult {
        ok: tout_va_bien && !ajoutes.is_empty(),
        output: lignes.join("\n"),
        artifact: None,
    }
}

async fn list_extensions(core: tauri::State<'_, Core>) -> ToolResult {
    match extensions::list_extensions(core).await {
        Ok(list) => ok(json!(list
            .iter()
            .map(|e| json!({
                "name": e.name,
                "display_name": e.display_name,
                "version": e.version,
                "enabled": e.enabled,
                "skills": e.components.skills,
                "mcp_servers": e.components.mcp_servers,
                "source": e.source,
            }))
            .collect::<Vec<_>>())
        .to_string()),
        Err(e) => erreur(e),
    }
}

async fn search_extensions(core: tauri::State<'_, Core>, args: &Value) -> ToolResult {
    let Some(query) = args["query"].as_str() else {
        return erreur("`query` manque : les mots recherchés.");
    };
    // Le catalogue n'est en cache qu'après une première visite de l'écran ;
    // le modèle ne doit pas trouver une liste vide pour cette seule raison.
    let mut trouve = extensions::browse_extension_catalog(
        core.clone(),
        Some(query.into()),
        None,
        Some(RESULTATS_CATALOGUE as u32),
    )
    .await;
    if matches!(&trouve, Ok(s) if s.entries.is_empty() && s.fetched_at.is_none()) {
        if let Err(e) = extensions::refresh_extension_catalog(core.clone()).await {
            return erreur(e);
        }
        trouve = extensions::browse_extension_catalog(
            core,
            Some(query.into()),
            None,
            Some(RESULTATS_CATALOGUE as u32),
        )
        .await;
    }
    match trouve {
        Ok(s) => ok(json!(s
            .entries
            .iter()
            .take(RESULTATS_CATALOGUE)
            .map(|e| json!({
                "name": e.name,
                "description": e.description,
                "install_source": e.install_source,
                "installed": e.installed,
                "beta": e.is_beta,
                "catalog": e.catalog_label,
            }))
            .collect::<Vec<_>>())
        .to_string()),
        Err(e) => erreur(e),
    }
}

async fn install_extension(core: tauri::State<'_, Core>, args: &Value) -> ToolResult {
    let Some(source) = args["source"].as_str().filter(|s| !s.trim().is_empty()) else {
        return erreur("`source` manque : ce qu'il faut installer.");
    };
    let installee =
        match extensions::install_extension(core.clone(), source.into(), None, None).await {
            Ok(e) => e,
            Err(e) => return erreur(e),
        };
    let id = installee.id.to_string();
    // L'accord donné à cet appel vaut pour les permissions que l'extension
    // demande : sans elles, ses serveurs MCP ne démarreraient pas.
    let demandees: Vec<_> = installee
        .permissions
        .iter()
        .map(|p| p.permission.clone())
        .collect();
    if let Err(e) = extensions::set_extension_permissions(core.clone(), id.clone(), demandees).await
    {
        return erreur(format!("installée, mais permissions non accordées : {e}"));
    }
    if let Err(e) = extensions::set_extension_enabled(core, id, true).await {
        return erreur(format!("installée, mais non activée : {e}"));
    }
    let c = &installee.components;
    ok(format!(
        "« {} » {} installée et activée : {} skill(s), {} serveur(s) MCP, {} commande(s). Disponible au message suivant.",
        installee.display_name, installee.version, c.skills, c.mcp_servers, c.commands
    ))
}
