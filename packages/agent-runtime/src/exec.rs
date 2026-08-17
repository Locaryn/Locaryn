//! Exécution d'un appel d'outil avec gating d'approbation, partagée par
//! toutes les boucles d'outils — locale (OpenAI/llama) et ponts de noyaux
//! alternatifs (`core-bridge`).
//!
//! Une seule implémentation du chemin critique : la décision d'approbation,
//! le refus, le dispatch (outil intégré ou MCP) et les événements émis sont
//! identiques où que l'appel vienne. Dupliquer ce code entre boucles
//! reviendrait à maintenir deux politiques de sécurité en parallèle.

use crate::approval::{ApprovalHandle, ApprovalRequest};
use crate::tools::{approval_decision, dispatch_tool, ApprovalInput, Risk, ToolContext, ToolSpec};
use locaryn_events::StreamEvent;
use locaryn_mcp::McpState;

/// Ce qui gouverne le dispatch d'un appel d'outil, identique quelle que soit
/// la boucle appelante — regroupé pour que `execute_tool_call` reste sous
/// le seuil d'arguments de clippy sans perdre en clarté.
#[derive(Clone, Copy)]
pub struct ToolDispatchContext<'a> {
    pub tools: &'a [ToolSpec],
    pub ctx: &'a ToolContext,
    pub mcp: Option<&'a McpState>,
    pub approval: Option<&'a ApprovalHandle>,
}

/// Exécute un appel d'outil du modèle, avec le gating d'approbation, et
/// émet les événements `ToolCall` / `ToolApproval` / `ToolResult` /
/// `Artifact` dans l'ordre attendu par l'UI.
///
/// Retourne le texte à renvoyer au modèle (résultat d'outil, ou motif de
/// refus) — ou `None` si le client a disparu (émission impossible) : dans
/// ce cas l'appelant doit interrompre sa boucle, personne n'écoute plus.
pub async fn execute_tool_call(
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
    call_id: &str,
    tool: &str,
    args: serde_json::Value,
    dispatch: &ToolDispatchContext<'_>,
) -> Option<String> {
    let ToolDispatchContext {
        tools,
        ctx,
        mcp,
        approval,
    } = *dispatch;

    // L'événement ToolCall part d'abord : c'est lui qui fait apparaître la
    // carte d'outil. Sans lui, l'utilisateur verrait le résultat sans savoir
    // ce qui a été appelé.
    if tx
        .send(StreamEvent::ToolCall {
            call_id: call_id.to_string(),
            tool: tool.to_string(),
            args: args.clone(),
        })
        .await
        .is_err()
    {
        return None;
    }

    let tool_spec = tools.iter().find(|t| t.name == tool);

    let decision = match tool_spec {
        Some(spec) => approval_decision(&ApprovalInput {
            spec,
            args: &args,
            ctx,
            agent_reason: None,
        }),
        None => crate::tools::ApprovalDecision {
            call_id: call_id.to_string(),
            tool: tool.to_string(),
            effective_risk: Risk::High,
            declared_risk: Risk::High,
            escalated_to_critical: false,
            needs_user_consent: true,
            reason: "Unknown tool — not in the built-in tool set.".into(),
            diff: None,
            min_scope: locaryn_shared_types::RiskScope::Once,
            hard_blocked: true,
            debug_trace: "tool not found in the declared tool set".into(),
        },
    };

    // Un outil interdit ne se négocie pas : on ne pose pas la question,
    // sinon un « Autoriser » laisserait croire qu'il suffit d'insister.
    let verdict = if decision.hard_blocked {
        Some(format!("Tool '{}' was blocked. {}.", tool, decision.reason))
    } else if decision.needs_user_consent {
        // L'événement part d'abord : c'est lui qui fait apparaître la
        // fenêtre. L'attente vient ensuite, sinon on demanderait à
        // quelqu'un qui n'a encore rien vu.
        let _ = tx
            .send(StreamEvent::ToolApproval {
                call_id: call_id.to_string(),
                tool: tool.to_string(),
                args: args.clone(),
                risk: decision.effective_risk,
                reason: decision.reason.clone(),
                diff: decision.diff.clone(),
                is_remote: ctx.remote_target.is_some(),
            })
            .await;

        let outcome = crate::approval::ask(
            approval,
            ApprovalRequest {
                call_id: call_id.to_string(),
                tool: tool.to_string(),
                args: args.clone(),
                risk: decision.effective_risk,
                reason: decision.reason.clone(),
                diff: decision.diff.clone(),
                is_remote: ctx.remote_target.is_some(),
                project_id: ctx.project_id,
            },
        )
        .await;

        match outcome {
            crate::approval::ApprovalOutcome::Allow => None,
            crate::approval::ApprovalOutcome::Deny { reason } => {
                Some(format!("Tool '{tool}' was denied by the user. {reason}."))
            }
        }
    } else {
        None
    };

    let result_content = if let Some(denial) = verdict {
        if tx
            .send(StreamEvent::ToolResult {
                call_id: call_id.to_string(),
                ok: false,
                output: denial.clone(),
            })
            .await
            .is_err()
        {
            return None;
        }
        denial
    } else {
        let result = if tool.starts_with(crate::mcp_tools::MCP_PREFIX) {
            match mcp {
                Some(mcp) => crate::mcp_tools::dispatch_mcp_tool(mcp, tool, &args).await,
                None => crate::tools::ToolResult {
                    ok: false,
                    output: "MCP state not available".into(),
                    artifact: None,
                },
            }
        } else {
            dispatch_tool(tool, &args, ctx).await
        };
        // Un outil qui produit un fichier le fait savoir ici. Sans cet
        // événement, une image générée restait un chemin dans une phrase :
        // le fichier existait sur le serveur, et aucun client ne pouvait le
        // montrer.
        if let Some(art) = &result.artifact {
            if tx
                .send(StreamEvent::Artifact {
                    artifact_id: uuid::Uuid::new_v4().to_string(),
                    kind: art.kind,
                    path: art.path.clone(),
                })
                .await
                .is_err()
            {
                return None;
            }
        }
        if tx
            .send(StreamEvent::ToolResult {
                call_id: call_id.to_string(),
                ok: result.ok,
                output: result.output.clone(),
            })
            .await
            .is_err()
        {
            return None;
        }
        if result.ok {
            result.output
        } else {
            format!("ERROR: {}", result.output)
        }
    };

    Some(result_content)
}
