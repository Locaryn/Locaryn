//! Ollama agent — streams chat completions from a local (or remote)
//! Ollama server via the `/api/chat` endpoint.
//!
//! Wire format: Ollama streams **NDJSON** (one JSON object per line). Each
//! intermediate line has `message.content` (a token delta) and `done: false`.
//! The final line has `done: true` with `prompt_eval_count` / `eval_count`
//! (token counts) and timing fields (in nanoseconds).
//!
//! We convert this to Locaryn's `StreamEvent` sequence:
//!   `MessageStart` → `Token`* → `MessageEnd`
//!
//! No tool-use loop here (that's S4). This is a pure streaming chat pass.

use crate::{tool_loop, Agent, AgentError, AgentInput, EventStream};
use futures::{Stream, StreamExt as _};
use locaryn_events::StreamEvent;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

const DEFAULT_MODEL: &str = "qwen2.5-coder:7b";
const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// An agent that talks to an Ollama server.
pub struct OllamaAgent {
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaAgent {
    /// Create with explicit endpoint and model.
    pub fn new(endpoint: &str, model: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client,
        }
    }

    /// Create from an optional endpoint/model, falling back to defaults.
    pub fn with_defaults(endpoint: Option<&str>, model: Option<&str>) -> Self {
        Self::new(
            endpoint.unwrap_or(DEFAULT_ENDPOINT),
            model.unwrap_or(DEFAULT_MODEL),
        )
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.endpoint)
    }
}

#[async_trait::async_trait]
impl Agent for OllamaAgent {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn run(&self, input: AgentInput) -> Result<EventStream, AgentError> {
        // S4: If we have project context, run the agentic tool-use loop.
        // Otherwise, fall back to simple streaming chat (S3).
        if let (Some(project_path), Some(trust)) = (&input.project_path, input.trust) {
            tracing::info!(
                session = %input.session_id,
                project = ?project_path,
                trust = ?trust,
                "running tool-use loop"
            );
            return tool_loop::run_tool_loop(
                &self.endpoint,
                &self.client,
                &input,
                project_path.clone(),
                trust,
            )
            .await;
        }

        // No project context — simple streaming chat (S3 path).
        let model = input.model.unwrap_or_else(|| self.model.clone());

        let body = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "user", "content": input.message }
            ],
            "stream": true,
        });

        tracing::debug!(url = self.chat_url(), model = %model, "ollama chat request");

        let resp = self
            .client
            .post(self.chat_url())
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "ollama connection failed");
                AgentError::ProviderUnavailable
            })?;

        if !resp.status().is_success() {
            tracing::warn!(status = %resp.status(), "ollama returned non-2xx");
            return Err(AgentError::ProviderUnavailable);
        }

        let byte_stream = resp.bytes_stream();
        let ndjson = NdjsonStream::new(byte_stream);

        let message_id = uuid::Uuid::new_v4().to_string();
        let task_id = uuid::Uuid::new_v4().to_string();
        let start = Instant::now();

        // Prefix with MessageStart, then the NDJSON-derived events.
        // OllamaEventStream guarantees exactly one MessageEnd (either from
        // the `done` chunk or a fallback if the stream drops unexpectedly).
        let events = futures::stream::once({
            let message_id = message_id.clone();
            let task_id = task_id.clone();
            async move {
                StreamEvent::MessageStart {
                    message_id,
                    task_id,
                }
            }
        })
        .chain(OllamaEventStream {
            inner: Box::pin(ndjson),
            message_id,
            task_id,
            start,
            done_sent: false,
            full_text: String::new(),
        });

        Ok(Box::pin(events))
    }
}

// ============================================================================
// NDJSON line parser — converts a byte stream into Vec<serde_json::Value>
// (one per newline-delimited JSON object).
// ============================================================================

struct NdjsonStream<S> {
    inner: Pin<Box<S>>,
    buffer: Vec<u8>,
}

impl<S> NdjsonStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    fn new(inner: S) -> Self {
        Self {
            inner: Box::pin(inner),
            buffer: Vec::with_capacity(4096),
        }
    }
}

impl<S> Stream for NdjsonStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    type Item = Result<serde_json::Value, AgentError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // Try to extract a complete line from the buffer.
            if let Some(idx) = self.buffer.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=idx).collect();
                let line = String::from_utf8_lossy(&line).trim().to_string();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(v) => return Poll::Ready(Some(Ok(v))),
                    Err(e) => {
                        tracing::warn!(error = %e, line = %line, "bad NDJSON line from ollama");
                        continue;
                    }
                }
            }
            // Need more bytes.
            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.extend_from_slice(&chunk);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(AgentError::Io(std::io::Error::other(e)))));
                }
                Poll::Ready(None) => {
                    // Flush any remaining buffer (no trailing newline).
                    if !self.buffer.is_empty() {
                        let line = String::from_utf8_lossy(&self.buffer).trim().to_string();
                        self.buffer.clear();
                        if !line.is_empty() {
                            match serde_json::from_str::<serde_json::Value>(&line) {
                                Ok(v) => return Poll::Ready(Some(Ok(v))),
                                Err(e) => {
                                    tracing::warn!(error = %e, "bad final NDJSON line");
                                    return Poll::Ready(None);
                                }
                            }
                        }
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

// ============================================================================
// OllamaEventStream — converts NDJSON Value objects into StreamEvent.
// ============================================================================

struct OllamaEventStream {
    inner: Pin<Box<dyn Stream<Item = Result<serde_json::Value, AgentError>> + Send>>,
    message_id: String,
    // Kept for parity with MessageStart (emitted before this stream is built);
    // no event after MessageStart carries the task id yet.
    #[allow(dead_code)]
    task_id: String,
    start: Instant,
    done_sent: bool,
    full_text: String,
}

impl Stream for OllamaEventStream {
    type Item = StreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // If we already sent MessageEnd, we're done.
            if self.done_sent {
                return Poll::Ready(None);
            }

            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    let done = chunk.get("done").and_then(|d| d.as_bool()).unwrap_or(false);

                    // Extract content delta from message.content
                    let delta = chunk
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("");

                    if done {
                        // Final chunk — extract token counts and emit MessageEnd.
                        self.done_sent = true;
                        let tokens_in = chunk
                            .get("prompt_eval_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let tokens_out = chunk
                            .get("eval_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or_else(|| {
                                // Fallback: estimate from accumulated text.
                                self.full_text.split_whitespace().count() as u64
                            });
                        let duration_ms = self.start.elapsed().as_millis() as u64;
                        return Poll::Ready(Some(StreamEvent::MessageEnd {
                            message_id: self.message_id.clone(),
                            tokens_in,
                            tokens_out,
                            duration_ms,
                        }));
                    } else if !delta.is_empty() {
                        // Intermediate chunk — emit a Token event.
                        self.full_text.push_str(delta);
                        return Poll::Ready(Some(StreamEvent::Token {
                            text: delta.to_string(),
                        }));
                    }
                    // Empty delta and not done — skip (keepalive or loading).
                    continue;
                }
                Poll::Ready(Some(Err(e))) => {
                    tracing::warn!(error = %e, "ollama stream error");
                    self.done_sent = true;
                    // Emit a Log event then let the chain's fallback MessageEnd fire.
                    return Poll::Ready(Some(StreamEvent::Log {
                        level: locaryn_events::LogLevel::Warn,
                        msg: format!("ollama stream error: {e}"),
                        source: "ollama".into(),
                    }));
                }
                Poll::Ready(None) => {
                    // Stream ended without a `done` chunk (e.g. connection
                    // dropped). Emit a fallback MessageEnd so the consumer
                    // always sees exactly one end event.
                    if !self.done_sent {
                        self.done_sent = true;
                        return Poll::Ready(Some(StreamEvent::MessageEnd {
                            message_id: self.message_id.clone(),
                            tokens_in: 0,
                            tokens_out: self.full_text.split_whitespace().count() as u64,
                            duration_ms: self.start.elapsed().as_millis() as u64,
                        }));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
