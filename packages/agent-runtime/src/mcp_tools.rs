//! Bridge between `locaryn-mcp` running clients and the agent tool loop.
//!
//! Two operations:
//! - `collect_mcp_tools`: scans all running MCP servers, calls `discover()`
//!   on each, and returns a `Vec<ToolSpec>` with prefixed names
//!   (`mcp__{server}__{tool}`).
//! - `dispatch_mcp_tool`: parses the prefixed name back to find the right
//!   server and forwards the call via `invoke_tool()`.

use crate::tools::{ToolArtifact, ToolResult, ToolSpec};
use locaryn_mcp::{McpClient, McpState};
use std::collections::HashMap;
use std::sync::Arc;

/// Prefix used to namespace MCP tools in the LLM's tool list.
pub const MCP_PREFIX: &str = "mcp__";

/// Prefix separator between server name and tool name.
const SEP: &str = "__";

/// Build the prefixed tool name that the LLM sees.
pub fn mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("{MCP_PREFIX}{server_name}{SEP}{tool_name}")
}

/// Split a prefixed name back into `(server_name, tool_name)`.
/// Returns `None` if the name doesn't look like an MCP tool.
///
/// Le nom d'un serveur d'extension est `<plugin>__<serveur>` : il contient
/// déjà le séparateur. Découper au premier `__` rendait « plugin-image »
/// comme serveur, introuvable dans la table des serveurs actifs. Le nom d'outil
/// est le dernier segment ; c'est [`resolve_mcp_tool_name`] qui tranche pour de
/// bon, en confrontant le nom aux serveurs réellement démarrés.
pub fn parse_mcp_tool_name(prefixed: &str) -> Option<(String, String)> {
    let rest = prefixed.strip_prefix(MCP_PREFIX)?;
    let (server, tool) = rest.rsplit_once(SEP)?;
    Some((server.to_string(), tool.to_string()))
}

/// Trouver le serveur et l'outil d'un nom préfixé, connaissant les serveurs
/// démarrés.
///
/// Le découpage par position ne peut pas savoir où finit un nom de serveur qui
/// contient le séparateur. La liste des serveurs actifs, elle, le sait : on
/// retient le nom le plus long qui préfixe l'appel. Sans serveur correspondant,
/// on retombe sur le découpage par position, pour produire un message d'erreur
/// qui nomme le serveur attendu.
pub fn resolve_mcp_tool_name(prefixed: &str, running: &[String]) -> Option<(String, String)> {
    let rest = prefixed.strip_prefix(MCP_PREFIX)?;
    let mut best: Option<(String, String)> = None;
    for server in running {
        let Some(tool) = rest
            .strip_prefix(server.as_str())
            .and_then(|tail| tail.strip_prefix(SEP))
        else {
            continue;
        };
        if tool.is_empty() {
            continue;
        }
        let longer = best
            .as_ref()
            .is_none_or(|(current, _)| server.len() > current.len());
        if longer {
            best = Some((server.clone(), tool.to_string()));
        }
    }
    best.or_else(|| parse_mcp_tool_name(prefixed))
}

/// Collect tool specs from all running MCP servers by calling `discover()`
/// on each client. Returns `ToolSpec` items with prefixed names (and clean aliases).
pub async fn collect_mcp_tools(state: &McpState) -> Vec<ToolSpec> {
    // Snapshot running servers under a short-lived read lock.
    let snapshot: HashMap<String, Arc<dyn McpClient>> = {
        let r = state.running.read().await;
        r.clone()
    };

    if snapshot.is_empty() {
        return Vec::new();
    }

    let mut specs = Vec::new();
    let mut clean_names = std::collections::HashSet::new();

    for (server_name, client) in &snapshot {
        match client.discover().await {
            Ok(caps) => {
                for t in &caps.tools {
                    let desc = t
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("MCP tool from server '{server_name}'"));
                    specs.push(ToolSpec {
                        name: mcp_tool_name(server_name, &t.name),
                        description: desc.clone(),
                        input_schema: t.input_schema.clone(),
                        risk: crate::tools::Risk::Medium,
                        required_permissions: Vec::new(),
                    });
                    if clean_names.insert(t.name.clone()) {
                        specs.push(ToolSpec {
                            name: t.name.clone(),
                            description: desc,
                            input_schema: t.input_schema.clone(),
                            risk: crate::tools::Risk::Medium,
                            required_permissions: Vec::new(),
                        });
                    }
                }
            }
            Err(e) => {
                tracing::warn!(server = %server_name, error = %e, "MCP discover failed");
            }
        }
    }

    specs
}

/// Dispatch a tool call to the correct MCP server.
/// Supports both `mcp__{server}__{tool}` and direct tool names like `generate_image`.
pub async fn dispatch_mcp_tool(
    state: &McpState,
    prefixed_or_clean_name: &str,
    args: &serde_json::Value,
) -> ToolResult {
    let running_names: Vec<String> = {
        let r = state.running.read().await;
        r.keys().cloned().collect()
    };
    let (server_name, tool_name, client) = if let Some((server, tool)) =
        resolve_mcp_tool_name(prefixed_or_clean_name, &running_names)
    {
        let client = {
            let r = state.running.read().await;
            r.get(&server).cloned()
        };
        let Some(c) = client else {
            return ToolResult {
                ok: false,
                output: format!("MCP server '{server}' is not running. Call /v1/mcp/servers/{server}/start first."),
                artifact: None,
            };
        };
        (server, tool, c)
    } else {
        // Direct tool name lookup across running servers
        let snapshot: HashMap<String, Arc<dyn McpClient>> = {
            let r = state.running.read().await;
            r.clone()
        };
        let mut matched = None;
        for (s_name, c) in snapshot {
            if let Ok(caps) = c.discover().await {
                if caps.tools.iter().any(|t| t.name == prefixed_or_clean_name) {
                    matched = Some((s_name, prefixed_or_clean_name.to_string(), c));
                    break;
                }
            }
        }
        let Some((s_name, t_name, c)) = matched else {
            return ToolResult {
                ok: false,
                output: format!("not an MCP tool or server not running: {prefixed_or_clean_name}"),
                artifact: None,
            };
        };
        (s_name, t_name, c)
    };

    match client.invoke_tool(&tool_name, args).await {
        Ok(val) => {
            // Convert the JSON-RPC result to a display string.
            let output = serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
            ToolResult {
                ok: true,
                output,
                // MCP transports return a text content item for most tools.
                // The host stays capability-agnostic: an extension may opt in
                // to the generic artifact envelope without teaching Locaryn
                // what the file means.
                artifact: artifact_from_mcp_value(&val),
            }
        }
        Err(e) => ToolResult {
            ok: false,
            output: format!("MCP tool '{tool_name}' on '{server_name}' failed: {e}"),
            artifact: None,
        },
    }
}

/// Read the generic artifact envelope emitted by an MCP extension.
///
/// Both the direct JSON shape (`{\"artifacts\":[...]}`) and the MCP text
/// shape (a JSON string containing that object) are accepted. This keeps the
/// transport layer independent from image/audio/video plugins while still
/// allowing every client to render the produced file.
fn artifact_from_mcp_value(value: &serde_json::Value) -> Option<ToolArtifact> {
    let payload = value
        .as_str()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .unwrap_or_else(|| value.clone());
    let candidate = payload
        .get("artifacts")
        .and_then(|items| items.as_array())
        .and_then(|items| items.first())
        .or_else(|| payload.get("artifact"));
    let object = candidate?.as_object()?;
    let kind = object.get("kind")?.as_str()?;
    let path = object.get("path")?.as_str()?;
    let kind = match kind {
        "html" => locaryn_shared_types::ArtifactKind::Html,
        "markdown" => locaryn_shared_types::ArtifactKind::Markdown,
        "python_text" => locaryn_shared_types::ArtifactKind::PythonText,
        "image_png" => locaryn_shared_types::ArtifactKind::ImagePng,
        "plotly_html" => locaryn_shared_types::ArtifactKind::PlotlyHtml,
        "audio_wav" => locaryn_shared_types::ArtifactKind::AudioWav,
        _ => return None,
    };
    Some(ToolArtifact {
        kind,
        path: path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ce que le serveur MCP de plugin-image renvoie réellement, relevé sur
    /// une génération. Le transport enveloppe l'objet dans une chaîne de texte,
    /// et c'est cette chaîne que reçoit `artifact_from_mcp_value`. Si ce
    /// contrat casse, l'image générée redevient un chemin dans une phrase.
    #[test]
    fn image_plugin_result_yields_a_displayable_artifact() {
        let brut = serde_json::Value::String(
            r#"{"artifacts":[{"kind":"image_png","path":"D:/media/img_1787192753769.png"}],"model":"stable-diffusion-v1-5-pruned-emaonly-Q4_0.gguf","paths":["D:/media/img_1787192753769.png"]}"#
                .to_string(),
        );
        let artefact = artifact_from_mcp_value(&brut).expect("un artefact affichable");
        assert_eq!(artefact.kind, locaryn_shared_types::ArtifactKind::ImagePng);
        assert_eq!(artefact.path, "D:/media/img_1787192753769.png");
    }

    /// Un outil qui ne produit pas de fichier ne doit pas fabriquer d'artefact.
    #[test]
    fn a_plain_result_produces_no_artifact() {
        let brut = serde_json::Value::String(r#"{"models":["a.gguf","b.gguf"]}"#.to_string());
        assert!(artifact_from_mcp_value(&brut).is_none());
    }

    /// Le nom court est celui que le modèle emploie : il doit rester
    /// reconnaissable comme non préfixé, pour que le routage l'envoie vers MCP
    /// au lieu de la table des outils natifs.
    #[test]
    fn clean_tool_names_are_not_mcp_prefixed() {
        assert!(parse_mcp_tool_name("generate_image").is_none());
        assert!(resolve_mcp_tool_name("generate_image", &[]).is_none());
    }

    /// Le serveur d'une extension s'appelle `<plugin>__<serveur>` : son nom
    /// contient le séparateur, et seul le registre des serveurs démarrés dit où
    /// il s'arrête.
    #[test]
    fn a_server_name_containing_the_separator_is_resolved() {
        let running = vec!["plugin-image__image-gen".to_string()];
        assert_eq!(
            resolve_mcp_tool_name("mcp__plugin-image__image-gen__generate_image", &running),
            Some((
                "plugin-image__image-gen".to_string(),
                "generate_image".to_string()
            ))
        );
    }

    /// Entre deux serveurs dont l'un préfixe l'autre, c'est le plus long qui
    /// correspond réellement à l'appel.
    #[test]
    fn the_longest_matching_server_wins() {
        let running = vec!["image".to_string(), "image__gen".to_string()];
        assert_eq!(
            resolve_mcp_tool_name("mcp__image__gen__generate_image", &running),
            Some(("image__gen".to_string(), "generate_image".to_string()))
        );
    }
}
