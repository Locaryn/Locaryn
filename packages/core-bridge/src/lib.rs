//! Pont hôte ↔ noyaux alternatifs (OpenClaw, Hermes Agent…).
//!
//! Un noyau est une extension dont le `plugin.json` porte une section `core`.
//! Locaryn ne réimplémente pas le noyau : il le **pilote** par HTTP loopback,
//! via un driver par dialecte :
//!
//! - `responses` : OpenResponses (OpenClaw) — outils client turn-based,
//!   SSE, continuité de session (`user` / `previous_response_id`) ;
//! - `runs` : Runs API (Hermes) — `POST /v1/runs`, SSE d'événements,
//!   relais d'approbation (`POST /v1/runs/{id}/approval`), arrêt
//!   (`POST /v1/runs/{id}/stop`) ;
//! - `chat_completions` : OpenAI-compatible générique (délègue à la boucle
//!   d'outils existante du runtime agent).
//!
//! [`CoreAgent`] implémente le même trait `Agent` que `OllamaAgent` :
//! l'aval — streaming, tool cards, persistance, métriques, annulation — ne
//! change pas. Les sessions confiées à un noyau sont sérialisées par session
//! ([`session::SessionStore`], décision D3) et mappées vers une clé stable
//! du noyau (D8).

pub mod drivers;
pub mod manager;
pub mod session;

use locaryn_agent_runtime::{Agent, AgentError, AgentInput, EventStream};
use locaryn_extensions::manifest::CoreManifest;
use std::sync::Arc;

/// Configuration figée d'un noyau : manifeste + jeton + client HTTP.
///
/// Construit par l'hôte (desktop ou daemon) à partir du manifeste installé ;
/// les drivers ne font que lire cette configuration.
pub struct CoreAgentConfig {
    pub manifest: CoreManifest,
    /// URL de base loopback, sans chemin (`http://127.0.0.1:18789`).
    /// Les chemins (`/v1/responses`, `/v1/runs`…) sont ajoutés par le driver.
    pub base_url: String,
    /// Jeton Bearer généré par Locaryn (CSPRNG), jamais dans le manifeste.
    pub bearer: String,
    pub client: reqwest::Client,
    /// Sérialisation par session + mappage session Locaryn → session noyau.
    pub sessions: Arc<session::SessionStore>,
}

/// Agent pilote d'un noyau alternatif. Le driver est choisi par
/// `manifest.driver` ; tout le reste (événements, gating, files) est commun.
pub struct CoreAgent {
    inner: Arc<CoreAgentConfig>,
}

impl CoreAgent {
    pub fn new(config: CoreAgentConfig) -> Self {
        Self {
            inner: Arc::new(config),
        }
    }

    /// Construit l'agent avec un client HTTP par défaut (timeout long :
    /// un run de noyau peut durer plusieurs minutes).
    pub fn with_defaults(manifest: CoreManifest, base_url: &str, bearer: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self::new(CoreAgentConfig {
            manifest,
            base_url: base_url.trim_end_matches('/').to_string(),
            bearer: bearer.to_string(),
            client,
            sessions: session::SessionStore::new(),
        })
    }

    pub fn config(&self) -> &Arc<CoreAgentConfig> {
        &self.inner
    }
}

/// Ramène une `api_url` de manifeste à sa base loopback, que le manifeste
/// ait écrit `http://127.0.0.1:18789` ou `…/v1/responses` (les deux formes
/// se rencontrent). Défensif : les drivers reconstruisent leurs chemins.
pub fn base_url_of(api_url: &str) -> String {
    for suffix in [
        "/v1/responses",
        "/v1/chat/completions",
        "/v1/runs",
        "/v1",
    ] {
        if let Some(base) = api_url.trim_end_matches('/').strip_suffix(suffix) {
            return base.to_string();
        }
    }
    api_url.trim_end_matches('/').to_string()
}

#[async_trait::async_trait]
impl Agent for CoreAgent {
    fn name(&self) -> &str {
        match self.inner.manifest.driver.as_str() {
            "responses" => "openclaw",
            "runs" => "hermes",
            "chat_completions" => "external-core",
            other => other,
        }
    }

    async fn run(&self, input: AgentInput) -> Result<EventStream, AgentError> {
        match self.inner.manifest.driver.as_str() {
            "responses" => drivers::responses::run(self.inner.clone(), input).await,
            "runs" => drivers::runs::run(self.inner.clone(), input).await,
            "chat_completions" => {
                // Dialecte générique : la boucle existante du runtime agent
                // (outils, approbation, streaming) parle déjà ce dialecte.
                let mut input = input;
                if input.bearer_token.is_none() {
                    input.bearer_token = Some(self.inner.bearer.clone());
                }
                if input.model.is_none() {
                    input.model = self.inner.manifest.model.clone();
                }
                locaryn_agent_runtime::openai_tool_loop::run_openai_tool_loop(
                    &self.inner.base_url,
                    &self.inner.client,
                    &input,
                )
                .await
            }
            other => {
                tracing::warn!(driver = %other, "driver de noyau inconnu");
                Err(AgentError::UnknownDriver(other.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::base_url_of;

    /// L'`api_url` d'un manifeste peut porter le chemin (`…/v1/responses`)
    /// ou pas ; les drivers reconstruisent leurs routes depuis la base.
    #[test]
    fn api_url_avec_ou_sans_chemin_donne_la_base() {
        assert_eq!(
            base_url_of("http://127.0.0.1:18789/v1/responses"),
            "http://127.0.0.1:18789"
        );
        assert_eq!(
            base_url_of("http://127.0.0.1:18789/v1/chat/completions"),
            "http://127.0.0.1:18789"
        );
        assert_eq!(base_url_of("http://127.0.0.1:18789"), "http://127.0.0.1:18789");
        assert_eq!(base_url_of("http://127.0.0.1:8642/v1"), "http://127.0.0.1:8642");
        assert_eq!(base_url_of("http://127.0.0.1:8642/"), "http://127.0.0.1:8642");
    }
}
