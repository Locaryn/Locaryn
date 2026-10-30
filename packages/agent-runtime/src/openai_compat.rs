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
/// Silence maximale entre deux morceaux de réponse. Pas de plafond sur la
/// durée totale : à 12 jetons/s, écrire un gros fichier prend plus de dix
/// minutes, et l'appel était coupé en pleine génération.
const IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// An agent that talks to any OpenAI-compatible server (llama-server, LM Studio…).
pub struct OpenAiCompatAgent {
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiCompatAgent {
    pub fn new(endpoint: &str, model: &str) -> Self {
        // `read_timeout` borne le silence, pas la durée : une réponse qui coule
        // lentement continue tant qu'elle coule. La connexion a son propre
        // plafond : sur certaines machines, se connecter à un port loopback
        // mort traîne bien au-delà de l'instant attendu (pare-feu, VPN,
        // antivirus qui inspecte le trafic local), et `connect_timeout`
        // referme cette phase seule à une durée raisonnable.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(IDLE_TIMEOUT)
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

    /// Parler au serveur avec ce client : celui d'un autre PC Locaryn croit
    /// l'autorité qui a émis son certificat.
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Un moteur injoignable ne se signalait qu'au bout de dix minutes : le
    /// client n'avait qu'un `timeout` global, qui couvre aussi la connexion.
    /// Sur une machine où se connecter à un port loopback mort peut traîner
    /// (pare-feu, VPN, antivirus), l'utilisateur voyait un chargement sans
    /// fin plutôt qu'une erreur. `connect_timeout` doit refermer cette phase
    /// seule, bien avant le budget de dix minutes réservé à la génération.
    ///
    /// Ignoré par défaut (réseau, dépend d'une adresse non routée qui
    /// n'existe pas toujours de la même façon en CI) : `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn une_connexion_qui_ne_repond_jamais_echoue_avant_le_budget_de_generation() {
        let agent = OpenAiCompatAgent::new("http://10.255.255.1:8080", "test");
        let debut = std::time::Instant::now();
        let resultat = agent
            .client
            .get("http://10.255.255.1:8080/v1/models")
            .send()
            .await;
        let ecoule = debut.elapsed();

        assert!(resultat.is_err(), "une adresse non routée doit échouer");
        assert!(
            ecoule < Duration::from_secs(30),
            "la connexion a pris {ecoule:?} — connect_timeout ne borne pas la phase de connexion"
        );
    }
}
