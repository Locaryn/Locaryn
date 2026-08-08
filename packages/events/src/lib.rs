//! Locaryn streaming events. These are serialized as SSE
//! (`event: <type>\ndata: <json>\n\n`) on the wire.

use bytes::Bytes;
use futures::Stream;
use locaryn_shared_types::{ArtifactKind, ProviderEngine, ProviderKind};
pub use locaryn_shared_types::Risk;
use std::pin::Pin;
use std::task::{Context, Poll};

// ============================================================================
// Event types
// ============================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart {
        message_id: String,
        task_id: String,
    },
    Token {
        text: String,
    },
    ToolCall {
        call_id: String,
        tool: String,
        args: serde_json::Value,
    },
    ToolApproval {
        call_id: String,
        tool: String,
        args: serde_json::Value,
        risk: Risk,
        /// Human-readable explanation shown in the approval modal. Falls back
        /// to the tool description if the agent didn't supply its own.
        reason: String,
        /// Rendered impact the modal must show before the user clicks a button.
        /// `write_file`: unified diff (after->before). `run_command`: the
        /// resolved shell command. `ssh_run_command`: `host:cmd`. `None`
        /// for read-only tools where nothing can change.
        diff: Option<String>,
        /// True when this call crosses the machine boundary (SSH, MCP HTTP,
        /// network-aware extensions). Drives an extra banner in the modal.
        is_remote: bool,
    },
    ToolResult {
        call_id: String,
        ok: bool,
        output: String,
    },
    Artifact {
        artifact_id: String,
        kind: ArtifactKind,
        path: String,
    },
    TaskUpdate {
        task_id: String,
        status: String,
        progress: f32,
    },
    PreviewUpdate {
        artifact_id: String,
        url: String,
    },
    ProviderChanged {
        provider: ProviderKind,
        engine: ProviderEngine,
        model: Option<String>,
        reason: String,
    },
    Log {
        level: LogLevel,
        msg: String,
        source: String,
    },
    MessageEnd {
        message_id: String,
        tokens_in: u64,
        tokens_out: u64,
        duration_ms: u64,
    },
}

// Risk is re-exported from locaryn_shared_types so there is a single
// canonical definition across the workspace. No need to define it here.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// ============================================================================
// SSE parsing
// ============================================================================

/// Convert an HTTP body byte stream into a stream of parsed `StreamEvent`s.
/// The input stream must be `Send` so the returned stream is `Send` too
/// (required to use it across await points in the CLI/daemon).
pub fn sse_stream<S>(byte_stream: S) -> impl Stream<Item = Result<StreamEvent, SseError>> + Send
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    SseStream {
        inner: Box::pin(byte_stream),
        buffer: String::new(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SseError {
    #[error("http transport: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid SSE payload: {0}")]
    Parse(String),
}

// `Send` is auto-derived: both fields (`Pin<Box<dyn Stream + Send>>` and
// `String`) are `Send`, so the compiler derives `Send` for `SseStream`.
// No manual `impl Send` is needed (and a manual one would be either
// redundant or require `unsafe impl`).
struct SseStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: String,
}

impl Stream for SseStream {
    type Item = Result<StreamEvent, SseError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // Try to parse a complete event from the buffer first.
            if let Some(idx) = self.buffer.find("\n\n") {
                let raw = self.buffer.split_off(idx + 2);
                let event_block = std::mem::replace(&mut self.buffer, raw);
                if let Some(event) = parse_sse_block(&event_block)? {
                    return Poll::Ready(Some(Ok(event)));
                }
                continue; // event block had no data; loop
            }
            // Need more bytes.
            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.push_str(
                        std::str::from_utf8(&chunk)
                            .map_err(|e| SseError::Parse(format!("non-utf8 chunk: {e}")))?,
                    );
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(SseError::Http(e)))),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

fn parse_sse_block(block: &str) -> Result<Option<StreamEvent>, SseError> {
    let mut event_name: Option<&str> = None;
    let mut data_lines: Vec<&str> = Vec::new();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = Some(rest.trim());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim());
        }
    }
    let data = data_lines.join("\n");
    if data.is_empty() {
        return Ok(None);
    }
    // We ignore `event_name` for deserialization because `StreamEvent` is
    // a `#[serde(tag = "type")]` enum — the discriminant is in the JSON.
    // The SSE `event:` line is kept for humans/debugging only.
    let _ = event_name;
    serde_json::from_str::<StreamEvent>(&data)
        .map(Some)
        .map_err(|e| SseError::Parse(format!("invalid JSON: {e}")))
}

// ============================================================================
// SSE encoding (server side)
// ============================================================================

/// The SSE `event:` line tag for a `StreamEvent`. Useful for frameworks
/// (axum's `Sse`) that do their own SSE framing and only need the tag +
/// the JSON payload, not a pre-formatted SSE block.
pub fn sse_event_tag(event: &StreamEvent) -> &'static str {
    match event {
        StreamEvent::MessageStart { .. } => "message.start",
        StreamEvent::Token { .. } => "token",
        StreamEvent::ToolCall { .. } => "tool_call",
        StreamEvent::ToolApproval { .. } => "tool_approval",
        StreamEvent::ToolResult { .. } => "tool_result",
        StreamEvent::Artifact { .. } => "artifact",
        StreamEvent::TaskUpdate { .. } => "task.update",
        StreamEvent::PreviewUpdate { .. } => "preview.update",
        StreamEvent::ProviderChanged { .. } => "provider.changed",
        StreamEvent::Log { .. } => "log",
        StreamEvent::MessageEnd { .. } => "message.end",
    }
}

/// Encode an event as a raw SSE text block (`event: <tag>\ndata: <json>\n\n`).
/// Use this when you produce SSE directly (e.g. a raw `text/event-stream`
/// response). When using a framework that does its own SSE framing (axum's
/// `Sse` + `Event`), use `sse_event_tag` + `serde_json::to_string` instead
/// to avoid double-encoding.
pub fn encode_sse(event: &StreamEvent) -> String {
    let tag = sse_event_tag(event);
    let data = serde_json::to_string(event).unwrap_or_else(|_| "{}".into());
    format!("event: {tag}\ndata: {data}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_token() {
        let ev = StreamEvent::Token { text: "Hi".into() };
        let s = encode_sse(&ev);
        assert!(s.starts_with("event: token\n"));
        let parsed = parse_sse_block(&s).unwrap().unwrap();
        match parsed {
            StreamEvent::Token { text } => assert_eq!(text, "Hi"),
            _ => panic!("wrong variant"),
        }
    }
}
