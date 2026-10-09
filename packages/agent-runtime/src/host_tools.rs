//! Les outils que l'hôte prête au modèle pour agir sur l'application elle-même.
//!
//! Ajouter un connecteur MCP, installer un morph ou un skill : ces gestes
//! touchent à l'état de l'application (sa base, ses extensions, ses
//! connecteurs), que la boucle d'outils ne connaît pas. L'hôte les décrit et
//! les exécute ; la boucle les propose au modèle et les fait passer par la
//! même porte d'approbation que tout le reste. Sans eux, « j'ai activé le
//! serveur MCP de DaVinci, connecte-toi » restait une demande que le modèle ne
//! pouvait que commenter.

use crate::tools::{ToolResult, ToolSpec};
use std::sync::Arc;

/// Ce que l'hôte sait faire sur lui-même. Implémenté par l'application.
#[async_trait::async_trait]
pub trait HostTools: Send + Sync {
    /// Les outils offerts au modèle, avec leur risque déclaré.
    fn specs(&self) -> Vec<ToolSpec>;

    /// Exécute `tool`. Appelé seulement après la porte d'approbation.
    async fn call(&self, tool: &str, args: &serde_json::Value) -> ToolResult;
}

/// Enveloppe les outils de l'hôte pour qu'ils traversent une structure
/// `Debug`, comme la porte d'approbation.
#[derive(Clone)]
pub struct HostToolsHandle(pub Arc<dyn HostTools>);

impl std::fmt::Debug for HostToolsHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostTools(présents)")
    }
}

impl HostToolsHandle {
    pub fn new(tools: impl HostTools + 'static) -> Self {
        Self(Arc::new(tools))
    }

    /// L'hôte sert-il cet outil ?
    pub fn serves(&self, tool: &str) -> bool {
        self.0.specs().iter().any(|s| s.name == tool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{execute_tool_call, ToolDispatchContext};
    use crate::tools::{Risk, ToolContext};
    use locaryn_shared_types::TrustLevel;

    struct Faux;

    #[async_trait::async_trait]
    impl HostTools for Faux {
        fn specs(&self) -> Vec<ToolSpec> {
            vec![ToolSpec {
                name: "app_install_extension".into(),
                description: "installe".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                risk: Risk::High,
                required_permissions: Vec::new(),
            }]
        }
        async fn call(&self, tool: &str, _args: &serde_json::Value) -> ToolResult {
            ToolResult {
                ok: true,
                output: format!("{tool} exécuté par l'hôte"),
                artifact: None,
            }
        }
    }

    async fn appeler(trust: TrustLevel) -> String {
        let hote = HostToolsHandle::new(Faux);
        let specs = hote.0.specs();
        let ctx = ToolContext {
            project_id: uuid::Uuid::nil(),
            project_path: std::path::PathBuf::new(),
            trust,
            session_id: uuid::Uuid::nil(),
            remote_target: None,
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move { while rx.recv().await.is_some() {} });
        execute_tool_call(
            &tx,
            "c1",
            "app_install_extension",
            serde_json::json!({ "source": "Locaryn/morph-image" }),
            &ToolDispatchContext {
                tools: &specs,
                ctx: &ctx,
                mcp: None,
                // Personne pour répondre : une demande d'accord vaut refus.
                approval: None,
                question: None,
                host: Some(&hote),
                trust: None,
                gpu: None,
            },
        )
        .await
        .unwrap_or_default()
    }

    /// « Tout autoriser » exécute l'installation chez l'hôte ; « Autonome »
    /// la soumet à un accord — refusé ici, faute d'interlocuteur.
    #[tokio::test]
    async fn une_installation_passe_par_la_porte_puis_par_l_hote() {
        assert_eq!(
            appeler(TrustLevel::Unrestricted).await,
            "app_install_extension exécuté par l'hôte"
        );
        let autonome = appeler(TrustLevel::Autonomous).await;
        assert!(autonome.contains("denied"), "{autonome}");
    }
}
