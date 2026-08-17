//! Fake core embarqué : parle les dialectes `responses` et `runs` de façon
//! déterministe, pour tester le pont sans réseau ni vrais noyaux.
//!
//! Si `LOCARYN_FAKE_CORE_URL` est défini (CI de `locaryn-cores`), les tests
//! visent ce serveur externe — `tests/fake-core/fake_core.py` du dépôt
//! d'extensions expose la même surface, y compris `/__probe/state`.
//!
//! Surface : `/health`, `/v1/models`, `/v1/capabilities`, `POST
//! /v1/responses`, `POST /v1/runs`, `GET /v1/runs/{id}/events`,
//! `POST /v1/runs/{id}/stop`, `POST /v1/runs/{id}/approval`,
//! `POST /v1/chat/completions`, et `/__probe/state` (réservé aux tests).
//!
//! Scénarios scriptés par le contenu du message :
//!
//! - « ping »  → réponse texte (`pong from fake core`) ;
//! - « call »  → un `function_call` client (`read_file`), puis, après le
//!   renvoi des `function_call_output`, une réponse texte ;
//! - « approve » → run avec `approval.request`, qui attend la décision
//!   relayée par le pont avant de conclure ;
//! - « stop »  → run qui émet des ticks indéfiniment (le pont doit demander
//!   l'arrêt quand le client abandonne).

use axum::body::Body;
use axum::extract::{Path, State as AxState};
use axum::http::{Response, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::StreamExt as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;

/// État observable du fake core, interrogé par les tests via `/__probe/state`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FakeState {
    /// Champs `user` reçus sur `/v1/responses` (continuité de session).
    pub users: Vec<String>,
    /// Corps reçus sur `/v1/responses`, dans l'ordre.
    pub responses_bodies: Vec<Value>,
    pub responses_count: usize,
    /// Corps reçus sur `/v1/runs`, dans l'ordre.
    pub run_bodies: Vec<Value>,
    pub runs_count: usize,
    /// run_id → message d'entrée (pour le scénario du flux d'événements).
    pub run_inputs: HashMap<String, String>,
    /// run_id des appels `POST /v1/runs/{id}/stop` reçus.
    pub stops: Vec<String>,
    /// Décisions reçues sur `POST /v1/runs/{id}/approval`.
    pub approvals: Vec<Value>,
}

pub struct FakeCore {
    pub base_url: String,
}

impl FakeCore {
    /// Démarre le serveur embarqué, ou vise `LOCARYN_FAKE_CORE_URL`.
    pub async fn spawn() -> FakeCore {
        if let Ok(url) = std::env::var("LOCARYN_FAKE_CORE_URL") {
            if !url.trim().is_empty() {
                return FakeCore {
                    base_url: url.trim_end_matches('/').to_string(),
                };
            }
        }

        let state = Arc::new(Mutex::new(FakeState::default()));
        let app = Router::new()
            .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
            .route(
                "/v1/models",
                get(|| async {
                    Json(json!({"object": "list", "data": [{"id": "fake-core"}]}))
                }),
            )
            .route(
                "/v1/capabilities",
                get(|| async {
                    Json(json!({
                        "object": "hermes.api_server.capabilities",
                        "platform": "fake-core",
                        "model": "fake-core",
                        "features": {
                            "chat_completions": true,
                            "responses_api": true,
                            "run_submission": true,
                            "run_status": true,
                            "run_events_sse": true,
                            "run_stop": true,
                            "run_approval": true
                        }
                    }))
                }),
            )
            .route("/__probe/state", get(probe_state))
            .route("/v1/responses", post(responses))
            .route("/v1/chat/completions", post(chat_completions))
            .route("/v1/runs", post(create_run))
            // Axum 0.7 : les paramètres de chemin s'écrivent `:id`.
            .route("/v1/runs/:run_id/events", get(run_events))
            .route("/v1/runs/:run_id/stop", post(stop_run))
            .route("/v1/runs/:run_id/approval", post(approve_run))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fake core: bind");
        let addr = listener.local_addr().expect("fake core: addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("fake core: serve");
        });
        FakeCore {
            base_url: format!("http://{addr}"),
        }
    }

    /// État courant, lu via la sonde (fonctionne aussi en mode externe).
    pub async fn state(&self) -> FakeState {
        let client = reqwest::Client::new();
        client
            .get(format!("{}/__probe/state", self.base_url))
            .send()
            .await
            .expect("fake core: probe GET")
            .json::<FakeState>()
            .await
            .expect("fake core: probe JSON")
    }
}

// ============================================================================
// Helpers SSE
// ============================================================================

/// Bloc SSE complet : `event:` + `data:` (les deux formes sont lues par le
/// pont — l'une ou l'autre suffit).
fn se_ev(name: &str, v: Value) -> String {
    format!("event: {name}\ndata: {v}\n\n")
}

/// Bloc SSE `data:` seul (forme minimaliste OpenResponses).
fn se(v: Value) -> String {
    format!("data: {v}\n\n")
}

fn sse_response(blocks: Vec<String>) -> Response<Body> {
    let stream = futures::stream::iter(blocks).map(Ok::<_, Infallible>);
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(stream))
        .expect("fake core: body")
}

// ============================================================================
// Sondes
// ============================================================================

async fn probe_state(AxState(state): AxState<Arc<Mutex<FakeState>>>) -> Json<FakeState> {
    Json(state.lock().await.clone())
}

// ============================================================================
// OpenResponses (driver `responses`)
// ============================================================================

async fn responses(
    AxState(state): AxState<Arc<Mutex<FakeState>>>,
    body: String,
) -> Response<Body> {
    let val: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let (is_follow_up, input_str) = {
        let mut st = state.lock().await;
        st.responses_count += 1;
        st.responses_bodies.push(val.clone());
        if let Some(u) = val.get("user").and_then(|u| u.as_str()) {
            st.users.push(u.to_string());
        }
        drop(st);
        let is_follow_up = val.get("input").and_then(|i| i.as_array()).is_some();
        let input_str = val
            .get("input")
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string();
        (is_follow_up, input_str)
    };

    let n = state.lock().await.responses_count;
    let resp_id = if input_str.contains("ping") {
        format!("resp_ping_{n}")
    } else if input_str.contains("call") {
        format!("resp_call_{n}")
    } else {
        format!("resp_hi_{n}")
    };

    let mut blocks = vec![
        se_ev("response.created", json!({"type": "response.created", "response": {"id": resp_id}})),
        se_ev("response.in_progress", json!({"type": "response.in_progress"})),
    ];

    if is_follow_up {
        // Le pont a renvoyé des `function_call_output` : le tour continue.
        blocks.push(se_ev(
            "response.output_item.added",
            json!({"type": "response.output_item.added", "item": {"type": "message", "role": "assistant"}}),
        ));
        blocks.push(se_ev(
            "response.output_text.delta",
            json!({"type": "response.output_text.delta", "delta": "résultat reçu par le noyau"}),
        ));
        blocks.push(se_ev(
            "response.output_text.done",
            json!({"type": "response.output_text.done", "text": "résultat reçu par le noyau"}),
        ));
    } else if input_str.contains("call") {
        // Le noyau demande un outil client.
        blocks.push(se_ev(
            "response.output_item.added",
            json!({
                "type": "response.output_item.added",
                "item": {"type": "function_call", "call_id": "call_fake_1", "name": "read_file", "arguments": "{\"path\":\"Cargo.toml\"}"}
            }),
        ));
        blocks.push(se_ev(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "item": {"type": "function_call", "call_id": "call_fake_1", "name": "read_file", "arguments": "{\"path\":\"Cargo.toml\"}"}
            }),
        ));
    } else if input_str.contains("ping") {
        blocks.push(se_ev(
            "response.output_text.delta",
            json!({"type": "response.output_text.delta", "delta": "pong from fake core"}),
        ));
        blocks.push(se_ev(
            "response.output_text.done",
            json!({"type": "response.output_text.done", "text": "pong from fake core"}),
        ));
    } else {
        blocks.push(se_ev(
            "response.output_text.delta",
            json!({"type": "response.output_text.delta", "delta": "bonjour depuis le fake core"}),
        ));
    }

    blocks.push(se_ev(
        "response.completed",
        json!({
            "type": "response.completed",
            "response": {"id": resp_id, "status": "completed", "usage": {"input_tokens": 10, "output_tokens": 5}}
        }),
    ));
    blocks.push("data: [DONE]".to_string());
    sse_response(blocks)
}

// ============================================================================
// Chat Completions (driver `chat_completions`)
// ============================================================================

async fn chat_completions(body: String) -> Response<Body> {
    let val: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let streamed = val
        .get("stream")
        .and_then(|s| s.as_bool())
        .unwrap_or(false);
    if !streamed {
        return Response::builder()
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "id": "chatcmpl-fake",
                    "object": "chat.completion",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": "chat completions fake"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
                })
                .to_string(),
            ))
            .expect("fake core: body");
    }
    sse_response(vec![
        se(json!({"id": "chatcmpl-fake", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": {"role": "assistant", "content": "chat completions fake"}, "finish_reason": null}]})),
        se(json!({"id": "chatcmpl-fake", "object": "chat.completion.chunk", "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}})),
        "data: [DONE]".to_string(),
    ])
}

// ============================================================================
// Runs API (driver `runs`)
// ============================================================================

async fn create_run(
    AxState(state): AxState<Arc<Mutex<FakeState>>>,
    body: String,
) -> (StatusCode, Json<Value>) {
    let val: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let mut st = state.lock().await;
    st.runs_count += 1;
    let run_id = format!("run_fake_{}", st.runs_count);
    st.run_bodies.push(val.clone());
    if let Some(s) = val.get("input").and_then(|i| i.as_str()) {
        st.run_inputs.insert(run_id.clone(), s.to_string());
    }
    (
        StatusCode::CREATED,
        Json(json!({"run_id": run_id, "status": "started"})),
    )
}

async fn run_events(
    AxState(state): AxState<Arc<Mutex<FakeState>>>,
    Path(run_id): Path<String>,
) -> Response<Body> {
    let input = state
        .lock()
        .await
        .run_inputs
        .get(&run_id)
        .cloned()
        .unwrap_or_default();

    let (tx, rx) = tokio::sync::mpsc::channel::<String>(16);
    tokio::spawn(async move {
        let _ = tx
            .send(se_ev("run.started", json!({"type": "run.started", "run_id": run_id})))
            .await;

        if input.contains("approve") {
            let _ = tx
                .send(se_ev("message.delta", json!({"type": "message.delta", "delta": "Préparation…"})))
                .await;
            let _ = tx
                .send(se_ev(
                    "tool.start",
                    json!({"type": "tool.start", "call_id": "tool_1", "tool": "run_command", "args": {"command": "echo hi"}}),
                ))
                .await;
            let _ = tx
                .send(se_ev(
                    "approval.request",
                    json!({
                        "type": "approval.request",
                        "request_id": "req_1",
                        "tool": "run_command",
                        "args": {"command": "echo hi"},
                        "message": "Exécuter la commande ?"
                    }),
                ))
                .await;

            // Attendre la décision relayée par le pont (au plus 20 s).
            let mut decision: Option<Value> = None;
            for _ in 0..200 {
                {
                    let st = state.lock().await;
                    decision = st
                        .approvals
                        .iter()
                        .rev()
                        .find(|a| {
                            a.get("request_id").and_then(|r| r.as_str()) == Some("req_1")
                        })
                        .cloned();
                }
                if decision.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            if let Some(d) = decision {
                let approved = d.get("approved").and_then(|a| a.as_bool()).unwrap_or(false);
                let _ = tx
                    .send(se_ev(
                        "tool.complete",
                        json!({
                            "type": "tool.complete",
                            "call_id": "tool_1",
                            "tool": "run_command",
                            "output": format!("approuvé: {approved}"),
                            "ok": true
                        }),
                    ))
                    .await;
                let _ = tx
                    .send(se_ev("message.delta", json!({"type": "message.delta", "delta": "Terminé"})))
                    .await;
            }
            let _ = tx
                .send(se_ev(
                    "run.completed",
                    json!({"type": "run.completed", "run_id": run_id, "usage": {"input_tokens": 12, "output_tokens": 6}}),
                ))
                .await;
        } else if input.contains("stop") {
            // Run qui ne finit pas : le pont doit demander l'arrêt quand le
            // client abandonne le flux.
            for i in 0..200 {
                if tx
                    .send(se_ev(
                        "message.delta",
                        json!({"type": "message.delta", "delta": format!("tick {i}")}),
                    ))
                    .await
                    .is_err()
                {
                    break; // client parti — on ferme le flux
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            let _ = tx
                .send(se_ev("run.completed", json!({"type": "run.completed", "run_id": run_id})))
                .await;
        } else {
            let _ = tx
                .send(se_ev(
                    "message.delta",
                    json!({"type": "message.delta", "delta": "hello from runs fake core"}),
                ))
                .await;
            let _ = tx
                .send(se_ev(
                    "run.completed",
                    json!({"type": "run.completed", "run_id": run_id, "usage": {"input_tokens": 8, "output_tokens": 4}}),
                ))
                .await;
        }
    });

    let stream = ReceiverStream::new(rx).map(Ok::<_, Infallible>);
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(stream))
        .expect("fake core: body")
}

async fn stop_run(
    AxState(state): AxState<Arc<Mutex<FakeState>>>,
    Path(run_id): Path<String>,
) -> Json<Value> {
    state.lock().await.stops.push(run_id);
    Json(json!({"status": "stopping"}))
}

async fn approve_run(
    AxState(state): AxState<Arc<Mutex<FakeState>>>,
    Path(_run_id): Path<String>,
    body: String,
) -> Json<Value> {
    let val: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    state.lock().await.approvals.push(val);
    Json(json!({"status": "recorded"}))
}
