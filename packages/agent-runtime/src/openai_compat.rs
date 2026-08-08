//! OpenAI-compatible streaming agent — talks to any server that implements
//! the OpenAI `/v1/chat/completions` API with `stream: true`.
//!
//! Compatible with **llama-server** (llama.cpp), LM Studio, vLLM, etc.
//!
//! Wire format: **SSE** (Server-Sent Events)
//! Each event line: `data: {"choices":[{"delta":{"content":"token"}}]}`
//! Final line: `data: [DONE]`
//!
//! We convert this to Locaryn's `StreamEvent` sequence:
//!   `MessageStart` → `Token`* → `MessageEnd`

use crate::{Agent, AgentError, AgentInput, EventStream};
use std::time::Duration;

const DEFAULT_MODEL: &str = "default";
const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// An agent that talks to any OpenAI-compatible server (llama-server, LM Studio…).
pub struct OpenAiCompatAgent {
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiCompatAgent {
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

    pub fn with_defaults(endpoint: Option<&str>, model: Option<&str>) -> Self {
        Self::new(
            endpoint.unwrap_or(DEFAULT_ENDPOINT),
            model.unwrap_or(DEFAULT_MODEL),
        )
    }
}

#[async_trait::async_trait]
impl Agent for OpenAiCompatAgent {
    fn name(&self) -> &str {
        "llama-server"
    }

    async fn run(&self, input: AgentInput) -> Result<EventStream, AgentError> {
        let mut input = input;
        if input.model.is_none() {
            input.model = Some(self.model.clone());
        }
        // One code path for everything: the streamed loop enables tools only
        // when the input carries project context, so a plain chat streams
        // tokens live and a project chat gets the full tool loop — both with
        // sampling params applied.
        crate::openai_tool_loop::run_openai_tool_loop(&self.endpoint, &self.client, &input).await
    }
}
