//! Ce qui part réellement sur le fil quand le modèle actif vient d'une
//! passerelle installée par un morph (OmniRoute et consorts).
//!
//! Trois choses doivent tenir, et aucune ne se voit à la lecture du code
//! appelant : l'adresse (`{endpoint}/v1/chat/completions`), l'en-tête
//! d'autorisation porteur de la clé gardée par l'hôte, et l'identifiant du
//! modèle transmis tel quel — `fournisseur/modèle`, jamais raccourci.
//!
//! Le serveur ici est un faux : il enregistre ce qu'il reçoit et répond un
//! flux minimal. C'est le seul moyen d'affirmer ces trois points sans appeler
//! un service payant.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use futures::StreamExt as _;
use locaryn_agent_runtime::openai_tool_loop::run_openai_tool_loop;
use locaryn_agent_runtime::AgentInput;
use locaryn_events::StreamEvent;
use locaryn_shared_types::{ConnectionMode, TrustLevel};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// Ce que la fausse passerelle a vu passer.
#[derive(Debug, Default, Clone)]
struct Vu {
    authorization: Option<String>,
    body: Option<Value>,
    path: Option<String>,
}

async fn fausse_passerelle() -> (String, Arc<Mutex<Vu>>) {
    let vu = Arc::new(Mutex::new(Vu::default()));
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(
                |State(vu): State<Arc<Mutex<Vu>>>, headers: HeaderMap, Json(body): Json<Value>| async move {
                    {
                        let mut v = vu.lock().expect("verrou");
                        v.authorization = headers
                            .get("authorization")
                            .and_then(|h| h.to_str().ok())
                            .map(str::to_string);
                        v.body = Some(body);
                        v.path = Some("/v1/chat/completions".to_string());
                    }
                    // Un flux minimal : un fragment de texte, puis la fin.
                    let corps = "data: {\"choices\":[{\"delta\":{\"content\":\"salut\"},\
                                 \"finish_reason\":null}]}\n\n\
                                 data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                                 data: [DONE]\n\n";
                    axum::response::Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(axum::body::Body::from(corps))
                        .expect("réponse")
                },
            ),
        )
        .with_state(vu.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port libre");
    let addr = listener.local_addr().expect("adresse");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), vu)
}

fn entree(model: &str) -> AgentInput {
    AgentInput {
        session_id: uuid::Uuid::new_v4(),
        message: "bonjour".into(),
        mode: ConnectionMode::Local,
        model: Some(model.to_string()),
        agent: None,
        project_id: None,
        project_path: None,
        trust: Some(TrustLevel::Trusted),
        images: Vec::new(),
        params: None,
        history: Vec::new(),
        mcp_state: None,
        extra_system: None,
        project_context: None,
        system_override: None,
        capabilities: Vec::new(),
        tools: None,
        approval: None,
        question: None,
        bearer_token: Some("cle-de-passerelle".into()),
        native_chat_api: false,
    }
}

async fn vider(mut flux: locaryn_agent_runtime::EventStream) -> String {
    let mut texte = String::new();
    while let Some(ev) = flux.next().await {
        if let StreamEvent::Token { text } = ev {
            texte.push_str(&text);
        }
    }
    texte
}

/// Le cas complet : clé jointe, modèle transmis tel quel, bonne route.
#[tokio::test]
async fn la_cle_et_le_modele_de_la_passerelle_partent_sur_le_fil() {
    let (base, vu) = fausse_passerelle().await;
    let client = reqwest::Client::new();

    let flux = run_openai_tool_loop(&base, &client, &entree("anthropic/claude-opus-5"))
        .await
        .expect("le flux doit s'ouvrir");
    assert_eq!(vider(flux).await, "salut");

    let v = vu.lock().expect("verrou").clone();
    assert_eq!(
        v.path.as_deref(),
        Some("/v1/chat/completions"),
        "la base du manifeste ne porte pas /v1 : c'est la boucle qui l'ajoute"
    );
    assert_eq!(
        v.authorization.as_deref(),
        Some("Bearer cle-de-passerelle"),
        "sans cet en-tête, la passerelle répond 401 et l'utilisateur ne sait pas pourquoi"
    );
    let body = v.body.expect("un corps JSON");
    assert_eq!(
        body.get("model").and_then(|m| m.as_str()),
        Some("anthropic/claude-opus-5"),
        "l'identifiant « fournisseur/modèle » doit arriver entier"
    );
    assert_eq!(
        body.get("stream").and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

/// Un moteur local n'a pas de clé : l'en-tête doit rester **absent**, pas vide.
/// Un « Bearer » sans jeton fait échouer certains serveurs qui accepteraient
/// une requête anonyme.
#[tokio::test]
async fn sans_cle_aucun_en_tete_dautorisation() {
    let (base, vu) = fausse_passerelle().await;
    let client = reqwest::Client::new();

    let mut sans_cle = entree("qwen3-4b.gguf");
    sans_cle.bearer_token = None;
    let flux = run_openai_tool_loop(&base, &client, &sans_cle)
        .await
        .expect("le flux doit s'ouvrir");
    let _ = vider(flux).await;

    assert_eq!(vu.lock().expect("verrou").authorization, None);
}

/// La forme d'un identifiant de passerelle contient une barre oblique. Rien
/// dans la chaîne ne doit la traiter comme un chemin : le corps JSON la porte,
/// pas l'URL.
#[tokio::test]
async fn un_identifiant_a_barre_oblique_ne_change_pas_la_route() {
    let (base, vu) = fausse_passerelle().await;
    let client = reqwest::Client::new();

    let flux = run_openai_tool_loop(&base, &client, &entree("meta-llama/llama-4-maverick:free"))
        .await
        .expect("le flux doit s'ouvrir");
    let _ = vider(flux).await;

    let v = vu.lock().expect("verrou").clone();
    assert_eq!(v.path.as_deref(), Some("/v1/chat/completions"));
    assert_eq!(
        v.body
            .and_then(|b| b.get("model").and_then(|m| m.as_str()).map(str::to_string))
            .as_deref(),
        Some("meta-llama/llama-4-maverick:free")
    );
}

/// Un serveur qui répond ce flux, tel quel : de quoi tester ce que la boucle
/// tire des cadres finals d'un vrai llama-server, dont `timings`.
async fn serveur_avec_flux(corps: &'static str) -> String {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || async move {
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(corps))
                .expect("réponse")
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port libre");
    let addr = listener.local_addr().expect("adresse");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

async fn evenements(base: &str) -> Vec<StreamEvent> {
    let client = reqwest::Client::new();
    let mut flux = run_openai_tool_loop(base, &client, &entree("local"))
        .await
        .expect("le flux doit s'ouvrir");
    let mut tous = Vec::new();
    while let Some(ev) = flux.next().await {
        tous.push(ev);
    }
    tous
}

/// La vitesse affichée sous une réponse est celle que le moteur a mesurée :
/// llama-server la donne dans `timings` (jetons et millisecondes du prompt,
/// puis de la génération). Elle part juste avant la fin du message.
#[tokio::test]
async fn la_vitesse_mesuree_par_le_moteur_est_remontee() {
    let base = serveur_avec_flux(
        "data: {\"choices\":[{\"delta\":{\"content\":\"salut\"},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\
         \"usage\":{\"prompt_tokens\":120,\"completion_tokens\":60},\
         \"timings\":{\"prompt_n\":120,\"prompt_ms\":300.0,\"predicted_n\":60,\"predicted_ms\":1500.0}}\n\n\
         data: [DONE]\n\n",
    )
    .await;

    let evts = evenements(&base).await;
    let position = evts
        .iter()
        .position(|e| matches!(e, StreamEvent::Timings { .. }))
        .expect("un événement Timings doit partir");
    assert!(
        matches!(evts.get(position + 1), Some(StreamEvent::MessageEnd { .. })),
        "Timings précède immédiatement MessageEnd : {evts:?}"
    );
    let StreamEvent::Timings {
        prompt_tokens,
        generated_tokens,
        prompt_tokens_per_sec,
        generation_tokens_per_sec,
    } = &evts[position]
    else {
        unreachable!()
    };
    assert_eq!((*prompt_tokens, *generated_tokens), (120, 60));
    // 60 jetons en 1,5 s = 40 par seconde ; 120 en 0,3 s = 400.
    assert!((generation_tokens_per_sec - 40.0).abs() < 0.01);
    assert!((prompt_tokens_per_sec - 400.0).abs() < 0.01);
}

/// Un moteur qui ne mesure rien ne se voit pas attribuer une vitesse : on ne
/// la déduit pas de la durée totale, qui compte aussi le chargement du modèle
/// et les outils.
#[tokio::test]
async fn sans_mesure_du_moteur_aucune_vitesse_n_est_inventee() {
    let base = serveur_avec_flux(
        "data: {\"choices\":[{\"delta\":{\"content\":\"salut\"},\"finish_reason\":null}]}\n\n\
         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
         data: [DONE]\n\n",
    )
    .await;

    let evts = evenements(&base).await;
    assert!(
        !evts
            .iter()
            .any(|e| matches!(e, StreamEvent::Timings { .. })),
        "aucun `timings` reçu : aucun événement de vitesse : {evts:?}"
    );
    assert!(evts
        .iter()
        .any(|e| matches!(e, StreamEvent::MessageEnd { .. })));
}
