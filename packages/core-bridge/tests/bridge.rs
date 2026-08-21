//! Tests d'intégration du pont contre le fake core (embarqué, ou
//! `LOCARYN_FAKE_CORE_URL` en CI de `locaryn-cores`).
//!
//! Couverture : dialecte `responses` (texte, outils client turn-based,
//! continuité de session), dialecte `runs` (tokens, progression d'outils,
//! relais d'approbation, arrêt à l'abandon), délégation `chat_completions`.
//! Les scénarios « vrai noyau » sont `#[ignore]` (voir la fin du fichier).

mod common;

use common::fake_core::FakeCore;
use futures::StreamExt as _;
use locaryn_agent_runtime::approval::{
    ApprovalGate, ApprovalHandle, ApprovalOutcome, ApprovalRequest,
};
use locaryn_agent_runtime::{Agent, AgentInput, EventStream};
use locaryn_core_bridge::CoreAgent;
use locaryn_events::StreamEvent;
use locaryn_extensions::manifest::CoreManifest;
use locaryn_shared_types::{ConnectionMode, TrustLevel};
use serde_json::{json, Value};

// ============================================================================
// Helpers
// ============================================================================

/// Porte d'approbation permissive (les tests d'outil ne veulent pas de modal).
struct YesGate;

#[async_trait::async_trait]
impl ApprovalGate for YesGate {
    async fn request(&self, _r: ApprovalRequest) -> ApprovalOutcome {
        ApprovalOutcome::Allow
    }
}

/// Porte qui refuse, pour vérifier que le refus est relayé au noyau.
struct NoGate;

#[async_trait::async_trait]
impl ApprovalGate for NoGate {
    async fn request(&self, _r: ApprovalRequest) -> ApprovalOutcome {
        ApprovalOutcome::Deny {
            reason: "pas maintenant".into(),
        }
    }
}

/// Construit un `CoreAgent` pointé sur le fake core, avec un manifeste
/// minimal enrichi par `extra` (`tools`, `session`…).
async fn core_agent(fake: &FakeCore, driver: &str, extra: Value) -> CoreAgent {
    let mut v = json!({
        "driver": driver,
        "api_url": fake.base_url,
        "port": 1,
        "model": "fake-core",
    });
    if let Value::Object(e) = extra {
        v.as_object_mut().expect("manifest object").extend(e);
    }
    let manifest: CoreManifest = serde_json::from_value(v).expect("manifest lisible");
    CoreAgent::with_defaults(manifest, &fake.base_url, "test-token")
}

fn input(session: uuid::Uuid, message: &str) -> AgentInput {
    AgentInput {
        session_id: session,
        message: message.to_string(),
        mode: ConnectionMode::Local,
        model: None,
        agent: None,
        project_id: None,
        project_path: None,
        trust: Some(TrustLevel::Trusted),
        images: Vec::new(),
        params: None,
        history: Vec::new(),
        mcp_state: None,
        extra_system: None,
        system_override: None,
        capabilities: Vec::new(),
        tools: None,
        approval: Some(ApprovalHandle::new(YesGate)),
        bearer_token: None,
    }
}

async fn collect(stream: EventStream) -> Vec<StreamEvent> {
    let mut out = Vec::new();
    futures::pin_mut!(stream);
    while let Some(ev) = stream.next().await {
        out.push(ev);
    }
    out
}

/// Texte concaténé des événements `Token`.
fn text(events: &[StreamEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Token { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn message_end(events: &[StreamEvent]) -> Option<(u64, u64)> {
    events.iter().find_map(|e| match e {
        StreamEvent::MessageEnd {
            tokens_in,
            tokens_out,
            ..
        } => Some((*tokens_in, *tokens_out)),
        _ => None,
    })
}

// ============================================================================
// Driver `responses` (OpenResponses)
// ============================================================================

#[tokio::test]
async fn responses_texte_streamé() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "responses", json!({})).await;
    let session = uuid::Uuid::new_v4();

    let events = collect(agent.run(input(session, "ping")).await.expect("run")).await;

    assert!(
        text(&events).contains("pong from fake core"),
        "les tokens du noyau doivent arriver en direct"
    );
    assert_eq!(message_end(&events), Some((10, 5)), "usage mappé (D10)");

    // Le routage de session a porté la clé stable `locaryn-{uuid}`.
    let st = fake.state().await;
    assert_eq!(st.users, vec![format!("locaryn-{session}")]);
}

#[tokio::test]
async fn responses_outil_client_turn_based() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "responses", json!({"tools": {"client_tools": true}})).await;

    // Projet temporaire : l'outil `read_file` du pont y lit un vrai fichier.
    let dir = std::env::temp_dir().join(format!("core-bridge-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("Cargo.toml"), "[package]\n").expect("temp file");

    let mut inp = input(uuid::Uuid::new_v4(), "call");
    inp.project_id = Some(uuid::Uuid::new_v4());
    inp.project_path = Some(dir.clone());

    let events = collect(agent.run(inp).await.expect("run")).await;

    // Le noyau a demandé un outil client ; le pont l'a exécuté (read_file est
    // Low + Trusted → exécution silencieuse, sans modal).
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolCall { tool, .. } if tool == "read_file")),
        "un function_call client doit devenir une carte d'outil"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolResult { ok: true, .. })),
        "le résultat de l'outil doit revenir au noyau"
    );
    // Le tour suivant a continué jusqu'à la réponse finale.
    assert!(text(&events).contains("résultat reçu par le noyau"));

    let st = fake.state().await;
    assert!(
        st.responses_count >= 2,
        "premier tour (function_call) + tour de suivi (function_call_output)"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn responses_continuité_de_session() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "responses", json!({})).await;
    let session = uuid::Uuid::new_v4();

    let e1 = collect(agent.run(input(session, "ping")).await.expect("run 1")).await;
    assert!(text(&e1).contains("pong"));

    let e2 = collect(agent.run(input(session, "ping bis")).await.expect("run 2")).await;
    assert!(text(&e2).contains("pong"));

    // La clé est stable et le second run reprend sur le dernier response_id.
    let st = fake.state().await;
    assert_eq!(st.users.len(), 2, "deux requêtes, même session");
    assert!(st.users.iter().all(|u| u == &format!("locaryn-{session}")));
    assert_eq!(
        st.responses_bodies[1]
            .get("previous_response_id")
            .and_then(|v| v.as_str()),
        Some("resp_ping_1"),
        "le run suivant doit chaîner sur le response_id du précédent (D8)"
    );
}

// ============================================================================
// Driver `runs` (Runs API — Hermes)
// ============================================================================

#[tokio::test]
async fn runs_flux_et_relais_d_approbation() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "runs", json!({})).await;

    let events = collect(
        agent
            .run(input(uuid::Uuid::new_v4(), "approve"))
            .await
            .expect("run"),
    )
    .await;

    assert!(text(&events).contains("Préparation"));
    assert!(text(&events).contains("Terminé"));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolCall { tool, .. } if tool == "run_command")),
        "la progression d'outil serveur doit devenir une carte d'outil"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolApproval { .. })),
        "une approbation en attente doit ouvrir le modal Locaryn"
    );
    let tr = events.iter().find_map(|e| match e {
        StreamEvent::ToolResult {
            call_id,
            ok,
            output,
        } if call_id == "tool_1" => Some((*ok, output.clone())),
        _ => None,
    });
    assert_eq!(tr, Some((true, "approuvé: true".into())));

    // La décision est bien partie vers le noyau.
    let st = fake.state().await;
    assert_eq!(st.approvals.len(), 1);
    assert_eq!(st.approvals[0]["approved"], true);
    assert_eq!(st.approvals[0]["request_id"], "req_1");
    assert_eq!(message_end(&events), Some((12, 6)), "usage mappé (D10)");
}

#[tokio::test]
async fn runs_refus_d_approbation_relayé() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "runs", json!({})).await;

    let mut inp = input(uuid::Uuid::new_v4(), "approve");
    inp.approval = Some(ApprovalHandle::new(NoGate));
    let events = collect(agent.run(inp).await.expect("run")).await;

    // Le refus est porté au noyau ; l'UI voit le résultat du tool.complete.
    let st = fake.state().await;
    assert_eq!(st.approvals.len(), 1);
    assert_eq!(st.approvals[0]["approved"], false);
    let tr = events.iter().find_map(|e| match e {
        StreamEvent::ToolResult {
            call_id,
            ok,
            output,
        } if call_id == "tool_1" => Some((*ok, output.clone())),
        _ => None,
    });
    assert_eq!(tr, Some((true, "approuvé: false".into())));
}

#[tokio::test]
async fn runs_arrêt_demandé_quand_le_client_abandonne() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "runs", json!({})).await;

    let mut stream = agent
        .run(input(uuid::Uuid::new_v4(), "stop"))
        .await
        .expect("run");
    let _start = stream.next().await; // MessageStart
    let _tick = stream.next().await; // premier delta
    drop(stream); // l'utilisateur ferme la fenêtre / annule

    // Le pont doit demander l'arrêt coopératif du run (D7) — il ne doit pas
    // continuer à agir en arrière-plan.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let st = fake.state().await;
    let run_id = st.run_inputs.keys().next().cloned().expect("un run soumis");
    assert!(
        st.stops.contains(&run_id),
        "POST /v1/runs/{{id}}/stop doit être appelé à l'abandon"
    );
}

#[tokio::test]
async fn runs_session_id_porte_la_clé_stable() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "runs", json!({})).await;
    let session = uuid::Uuid::new_v4();

    collect(agent.run(input(session, "bonjour")).await.expect("run")).await;

    let st = fake.state().await;
    assert_eq!(st.run_bodies.len(), 1);
    // Routage par défaut (`user`) → `session_id` corrélé par Hermes.
    let expected = format!("locaryn-{session}");
    assert_eq!(
        st.run_bodies[0].get("session_id").and_then(|v| v.as_str()),
        Some(expected.as_str())
    );
    assert!(
        text(&collect(agent.run(input(session, "bonjour")).await.expect("run")).await)
            .contains("hello from runs fake core")
    );
}

// ============================================================================
// Driver `chat_completions` (générique)
// ============================================================================

#[tokio::test]
async fn chat_completions_délègue_à_la_boucle_existante() {
    let fake = FakeCore::spawn().await;
    let agent = core_agent(&fake, "chat_completions", json!({})).await;

    let events = collect(
        agent
            .run(input(uuid::Uuid::new_v4(), "salut"))
            .await
            .expect("run"),
    )
    .await;
    assert!(text(&events).contains("chat completions fake"));
}

// ============================================================================
// Scénarios « vrai noyau » (manuels, hors CI) — voir docs/architecture/14 §8
// ============================================================================

/// Scénario e2e contre un vrai OpenClaw en marche (gateway + OpenResponses
/// activé) : streaming d'un message, puis continuité sur la même session
/// (clé `user` stable).
/// Usage : `OPENCLAW_URL=http://127.0.0.1:18789 OPENCLAW_TOKEN=… cargo test
/// -p locaryn-core-bridge real_openclaw -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "nécessite un vrai noyau OpenClaw en marche"]
async fn real_openclaw_e2e() {
    let url = std::env::var("OPENCLAW_URL").expect("OPENCLAW_URL");
    let token = std::env::var("OPENCLAW_TOKEN").unwrap_or_default();
    let manifest: CoreManifest = serde_json::from_value(json!({
        "driver": "responses", "api_url": url, "port": 0, "model": "openclaw"
    }))
    .unwrap();
    let agent = CoreAgent::with_defaults(manifest, &url, &token);
    let session = uuid::Uuid::new_v4();

    // Tour 1 : un message simple doit produire des tokens en streaming.
    let e1 = collect(
        agent
            .run(input(session, "Reply with exactly: pong"))
            .await
            .expect("run 1"),
    )
    .await;
    let t1 = text(&e1);
    eprintln!("tour 1 → {t1:?}");
    assert!(!t1.is_empty(), "le noyau doit répondre en streaming");

    // Tour 2 : même session — le noyau doit reprendre le contexte (user
    // stable `locaryn-{uuid}`) et répondre à nouveau.
    let e2 = collect(
        agent
            .run(input(session, "Now reply with exactly: pong bis"))
            .await
            .expect("run 2"),
    )
    .await;
    let t2 = text(&e2);
    eprintln!("tour 2 → {t2:?}");
    assert!(
        !t2.is_empty(),
        "la continuité de session doit répondre aussi"
    );

    let end = message_end(&e2);
    eprintln!("usage mappé (tokens_in, tokens_out) = {end:?}");
}

/// Contre un vrai Hermes Agent en marche (API server).
/// Usage : `HERMES_URL=http://127.0.0.1:8642 HERMES_TOKEN=… cargo test
/// -p locaryn-core-bridge real_hermes -- --ignored`.
#[tokio::test]
#[ignore = "nécessite un vrai noyau Hermes en marche"]
async fn real_hermes_ping() {
    let url = std::env::var("HERMES_URL").expect("HERMES_URL");
    let token = std::env::var("HERMES_TOKEN").unwrap_or_default();
    let manifest: CoreManifest = serde_json::from_value(json!({
        "driver": "runs", "api_url": url, "port": 0, "model": "hermes-agent"
    }))
    .unwrap();
    let agent = CoreAgent::with_defaults(manifest, &url, &token);
    let events = collect(
        agent
            .run(input(uuid::Uuid::new_v4(), "ping"))
            .await
            .expect("run"),
    )
    .await;
    assert!(!text(&events).is_empty());
}
