//! S4: Agentic tool-use loop for OllamaAgent.
//!
//! When the agent has project context (project_path + trust level), instead
//! of a simple streaming chat pass, we run an **iterative tool-use loop**:
//!
//!   1. Send the conversation to Ollama `/api/chat` with `stream: false` and
//!      the `tools` array (JSON-schema function definitions).
//!   2. If the model returns `message.tool_calls`, for each call:
//!      a. Emit a `ToolCall` event.
//!      b. Check approval gating (trust level → auto-approve or emit
//!      `ToolApproval` and auto-deny in MVP — interactive approval lands
//!      in a later step with the desktop/CLI approval UI).
//!      c. Dispatch the tool via `dispatch_tool`.
//!      d. Emit a `ToolResult` event.
//!      e. Append the assistant message + tool result to the conversation.
//!   3. Loop back to step 1 with the updated conversation.
//!   4. When the model returns plain content (no tool_calls), emit it as
//!      `Token` events and a final `MessageEnd`.
//!
//! We use `stream: false` for the tool-use loop because Ollama's tool-call
//! responses are atomic (the `tool_calls` array arrives in one JSON object,
//! not streamed token-by-token). The final text response is also non-streamed
//! for simplicity — we emit it as a single `Token` event. True token streaming
//! for the final turn is a V1.1 enhancement.

use crate::tools::{
    builtin_tools, dispatch_tool, ollama_tools_json, requires_approval, ToolContext, ToolSpec,
};
use crate::{AgentError, AgentInput, EventStream};
use locaryn_events::{LogLevel, StreamEvent};
use locaryn_shared_types::TrustLevel;
use std::sync::Arc;
use std::time::Instant;

const MAX_TOOL_ROUNDS: u32 = 10;

/// Outcome of processing a single Ollama `/api/chat` response chunk.
enum ChunkOutcome {
    /// Tool calls were emitted; the loop should continue with the updated
    /// conversation.
    Continue,
    /// The model returned a final answer (no tool calls).
    Final(String),
    /// A fatal error occurred (already logged); the loop should stop.
    Error,
}

/// Run the tool-use loop and return the full event stream.
///
/// `endpoint` is the Ollama base URL (e.g. `http://127.0.0.1:11434`).
/// `client` is reused from the `OllamaAgent` for connection pooling.
pub async fn run_tool_loop(
    endpoint: &str,
    client: &reqwest::Client,
    input: &AgentInput,
    project_path: std::path::PathBuf,
    trust: TrustLevel,
) -> Result<EventStream, AgentError> {
    let model = input
        .model
        .clone()
        .unwrap_or_else(|| "qwen2.5-coder:7b".into());
    let chat_url = format!("{}/api/chat", endpoint.trim_end_matches('/'));
    let tools = builtin_tools();
    // Discover MCP tools and merge into the tool list.
    let mcp_tools = if let Some(ref mcp) = input.mcp_state {
        crate::mcp_tools::collect_mcp_tools(mcp).await
    } else {
        Vec::new()
    };
    let all_tools: Vec<_> = tools.into_iter().chain(mcp_tools.clone()).collect();
    let tools_json = ollama_tools_json(&all_tools);

    // Build the initial conversation: system prompt + user message. Attached
    // images (base64) ride on the user message via Ollama's `images` field —
    // vision-capable models use them; text models ignore them.
    let mut user_msg = serde_json::json!({
        "role": "user",
        "content": input.message
    });
    if !input.images.is_empty() {
        user_msg["images"] = serde_json::json!(input.images);
    }
    let mut messages = serde_json::json!([
        {
            "role": "system",
            "content": crate::compose_system_prompt(
                &{
                    // Même règle que l'autre boucle : la consigne de la
                    // personne d'abord, la mécanique des outils ensuite, et
                    // rien d'autre.
                    let mut morceaux: Vec<String> = Vec::new();
                    if let Some(consigne) = input
                        .system_override
                        .as_deref()
                        .map(str::trim)
                        .filter(|texte| !texte.is_empty())
                    {
                        morceaux.push(consigne.to_string());
                    }
                    morceaux.push(crate::tool_discipline_prompt());
                    morceaux.join("\n\n")
                },
                input.extra_system.as_ref(),
            )
        },
        user_msg
    ]);

    let message_id = uuid::Uuid::new_v4().to_string();
    let task_id = uuid::Uuid::new_v4().to_string();
    let start = Instant::now();

    // Use an mpsc channel to collect events from the async loop and expose
    // them as a Stream. The channel is bounded so the loop naturally
    // backpressures if the consumer is slow.
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);

    // Make the first request synchronously so that model/endpoint errors are
    // propagated to the caller (the daemon can then fall back to StubAgent).
    // Once the first request succeeds, we know the runtime is usable and we
    // can process the rest in a spawned task.
    let first_body = serde_json::json!({
        "model": &model,
        "messages": &messages,
        "tools": &tools_json,
        "stream": false,
    });

    tracing::debug!(url = %chat_url, "tool loop first request");

    let first_resp = client
        .post(&chat_url)
        .json(&first_body)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "ollama connection failed on first tool loop request");
            AgentError::ProviderUnavailable
        })?;

    if !first_resp.status().is_success() {
        tracing::warn!(status = %first_resp.status(), "ollama returned non-2xx on first tool loop request");
        return Err(AgentError::ProviderUnavailable);
    }

    let first_chunk: serde_json::Value = first_resp.json().await.map_err(|e| {
        tracing::warn!(error = %e, "failed to parse first ollama response");
        AgentError::ProviderUnavailable
    })?;

    // Emit MessageStart (before spawn — this is on the calling task).
    let _ = tx
        .send(StreamEvent::MessageStart {
            message_id: message_id.clone(),
            task_id: task_id.clone(),
        })
        .await;

    // Clone all borrowed data before spawning — tokio::spawn requires 'static.
    let input = input.clone();
    let client = client.clone();
    let message_id_loop = message_id.clone();
    let chat_url_loop = chat_url.clone();
    let model_loop = model.clone();
    let tools_json_loop = tools_json.clone();
    let tools_for_dispatch = all_tools.clone();
    let mcp_state_for_dispatch: Option<Arc<locaryn_mcp::McpState>> = input.mcp_state.clone();
    tokio::spawn(async move {
        let ctx = ToolContext {
            project_id: input.project_id.unwrap_or_default(),
            project_path,
            trust,
            session_id: input.session_id,
            // TODO doc 11 section 5.1: the agent loop must populate
            // remote_target for any call that targets an SSH/MCP-HTTP
            // endpoint so approval_decision() can escalate to Critical.
            // The dispatch site (process_chunk) does not know yet —
            // wire it in V1.1.
            remote_target: None,
        };

        let mut total_tokens_in = 0u64;
        let mut total_tokens_out = 0u64;
        let mut final_text = String::new();
        let mut outcome = process_chunk(
            first_chunk,
            &tx,
            &mut messages,
            &ctx,
            &tools_for_dispatch,
            trust,
            &mut total_tokens_in,
            &mut total_tokens_out,
            &mcp_state_for_dispatch,
        )
        .await;

        for _round in 1..MAX_TOOL_ROUNDS {
            match outcome {
                ChunkOutcome::Continue => {}
                ChunkOutcome::Final(text) => {
                    final_text = text;
                    break;
                }
                ChunkOutcome::Error => break,
            }

            // Build the request body.
            let body = serde_json::json!({
                "model": model_loop,
                "messages": messages,
                "tools": tools_json_loop,
                "stream": false,
            });

            tracing::debug!(round = _round, url = %chat_url_loop, "tool loop request");

            let resp = match client.post(&chat_url_loop).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Log {
                            level: LogLevel::Warn,
                            msg: format!("ollama connection failed: {e}"),
                            source: "tool_loop".into(),
                        })
                        .await;
                    break;
                }
            };

            if !resp.status().is_success() {
                let _ = tx
                    .send(StreamEvent::Log {
                        level: LogLevel::Warn,
                        msg: format!("ollama returned {}", resp.status()),
                        source: "tool_loop".into(),
                    })
                    .await;
                break;
            }

            let chunk: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Log {
                            level: LogLevel::Warn,
                            msg: format!("failed to parse ollama response: {e}"),
                            source: "tool_loop".into(),
                        })
                        .await;
                    break;
                }
            };

            outcome = process_chunk(
                chunk,
                &tx,
                &mut messages,
                &ctx,
                &tools_for_dispatch,
                trust,
                &mut total_tokens_in,
                &mut total_tokens_out,
                &mcp_state_for_dispatch,
            )
            .await;
        }

        // If we never got a final answer, log it. This can happen when the
        // model returns empty content or the loop hits an error before a
        // final answer.
        if final_text.is_empty() {
            let _ = tx
                .send(StreamEvent::Log {
                    level: LogLevel::Warn,
                    msg: "tool loop finished without a final response".into(),
                    source: "tool_loop".into(),
                })
                .await;
        }

        // Emit MessageEnd.
        let duration_ms = start.elapsed().as_millis() as u64;
        let _ = tx
            .send(StreamEvent::MessageEnd {
                message_id: message_id_loop,
                tokens_in: total_tokens_in,
                tokens_out: total_tokens_out,
                duration_ms,
            })
            .await;
    });

    // Convert the mpsc receiver into a Stream.
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Box::pin(stream))
}

/// Process a single Ollama response chunk: accumulate token counts, emit
/// tool events, dispatch tools, and feed results back to the conversation.
/// Returns `Continue` if the loop should request another round, `Final`
/// if the model gave a final answer, or `Error` if a fatal error occurred.
#[allow(clippy::too_many_arguments)]
async fn process_chunk(
    chunk: serde_json::Value,
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
    messages: &mut serde_json::Value,
    ctx: &ToolContext,
    tools: &[ToolSpec],
    trust: TrustLevel,
    total_tokens_in: &mut u64,
    total_tokens_out: &mut u64,
    mcp_state: &Option<Arc<locaryn_mcp::McpState>>,
) -> ChunkOutcome {
    // Accumulate token counts.
    if let Some(pi) = chunk.get("prompt_eval_count").and_then(|v| v.as_u64()) {
        *total_tokens_in += pi;
    }
    if let Some(eo) = chunk.get("eval_count").and_then(|v| v.as_u64()) {
        *total_tokens_out += eo;
    }

    let msg = chunk.get("message");
    let content = msg
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let tool_calls_raw = msg.and_then(|m| m.get("tool_calls"));

    if let Some(tc_arr) = tool_calls_raw.and_then(|t| t.as_array()) {
        if !tc_arr.is_empty() {
            // The model wants to call tools. Append the assistant message
            // (with tool_calls) to the conversation.
            messages.as_array_mut().unwrap().push(serde_json::json!({
                "role": "assistant",
                "content": content,
                "tool_calls": tc_arr.clone(),
            }));

            for tc in tc_arr {
                let fn_obj = tc.get("function").or_else(|| tc.get("Function"));
                let tool_name = fn_obj
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                // Ollama may return arguments as an object or a JSON-encoded
                // string (depending on version).
                let args = match fn_obj.and_then(|f| f.get("arguments")) {
                    Some(serde_json::Value::String(s)) => {
                        serde_json::from_str(s).unwrap_or(serde_json::json!({}))
                    }
                    Some(v) => v.clone(),
                    None => serde_json::json!({}),
                };

                let call_id = uuid::Uuid::new_v4().to_string();

                // Emit ToolCall event.
                if tx
                    .send(StreamEvent::ToolCall {
                        call_id: call_id.clone(),
                        tool: tool_name.to_string(),
                        args: args.clone(),
                    })
                    .await
                    .is_err()
                {
                    return ChunkOutcome::Error; // consumer gone
                }

                // Approval gating — the `tools` slice includes MCP tools.
                let tool_spec = tools.iter().find(|t| t.name == tool_name);
                let needs_approval = tool_spec
                    .map(|s| requires_approval(s, trust))
                    .unwrap_or(true);

                let approved = if needs_approval {
                    match trust {
                        TrustLevel::Trusted => {
                            matches!(
                                tool_spec.map(|s| s.risk),
                                Some(crate::tools::Risk::Low)
                                    | Some(crate::tools::Risk::Medium)
                                    | Some(crate::tools::Risk::Critical)
                            )
                        }
                        TrustLevel::Untrusted => {
                            matches!(
                                tool_spec.map(|s| s.risk),
                                Some(crate::tools::Risk::Low) | Some(crate::tools::Risk::Medium)
                            )
                        }
                        TrustLevel::Sandbox => false,
                    }
                } else {
                    true
                };

                if !approved {
                    let _ = tx
                        .send(StreamEvent::ToolApproval {
                            call_id: call_id.clone(),
                            tool: tool_name.to_string(),
                            args: args.clone(),
                            risk: risk_to_event_risk(tool_spec.map(|s| s.risk)),
                            // MVP auto-gate: no agent_reason, no diff, no remote.
                            reason: String::new(),
                            diff: None,
                            is_remote: false,
                        })
                        .await;
                    if tx
                        .send(StreamEvent::ToolResult {
                            call_id: call_id.clone(),
                            ok: false,
                            output: "tool call denied: approval required (MVP auto-gate)".into(),
                        })
                        .await
                        .is_err()
                    {
                        return ChunkOutcome::Error;
                    }
                    // Feed the denial back to the model.
                    messages.as_array_mut().unwrap().push(serde_json::json!({
                        "role": "tool",
                        "name": tool_name,
                        "tool_call_id": call_id,
                        "content": format!("Tool '{tool_name}' was denied. Reason: approval required."),
                    }));
                    continue;
                }

                // Dispatch the tool — route MCP calls to the MCP provider.
                let result = if tool_name.starts_with(crate::mcp_tools::MCP_PREFIX)
                    || !crate::tools::is_native_tool(tool_name)
                {
                    if let Some(ref mcp) = mcp_state {
                        crate::mcp_tools::dispatch_mcp_tool(mcp, tool_name, &args).await
                    } else {
                        crate::tools::ToolResult {
                            ok: false,
                            output: format!(
                                "outil « {tool_name} » inconnu : aucune extension active ne l'expose."
                            ),
                            artifact: None,
                        }
                    }
                } else {
                    dispatch_tool(tool_name, &args, ctx).await
                };

                // Emit ToolResult event.
                if tx
                    .send(StreamEvent::ToolResult {
                        call_id: call_id.clone(),
                        ok: result.ok,
                        output: result.output.clone(),
                    })
                    .await
                    .is_err()
                {
                    return ChunkOutcome::Error;
                }

                // Feed the result back to the model.
                let result_content = if result.ok {
                    result.output
                } else {
                    format!("ERROR: {}", result.output)
                };
                messages.as_array_mut().unwrap().push(serde_json::json!({
                    "role": "tool",
                    "name": tool_name,
                    "tool_call_id": call_id,
                    "content": result_content,
                }));
            }

            return ChunkOutcome::Continue;
        }
    }

    // No tool calls — this is the final text response.
    if !content.is_empty() {
        let _ = tx
            .send(StreamEvent::Token {
                text: content.to_string(),
            })
            .await;
        return ChunkOutcome::Final(content.to_string());
    }

    ChunkOutcome::Final(String::new())
}

fn risk_to_event_risk(risk: Option<crate::tools::Risk>) -> locaryn_events::Risk {
    match risk {
        Some(crate::tools::Risk::Low) => locaryn_events::Risk::Low,
        Some(crate::tools::Risk::Medium) => locaryn_events::Risk::Medium,
        Some(crate::tools::Risk::High) => locaryn_events::Risk::High,
        Some(crate::tools::Risk::Critical) => locaryn_events::Risk::Critical,
        None => locaryn_events::Risk::High,
    }
}
