//! Ce qui part réellement sur le fil quand le modèle actif vient d'une
//! passerelle installée par un morph.
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
        git_contributeur: None,
        project_context: None,
        system_override: None,
        capabilities: Vec::new(),
        tools: None,
        approval: None,
        question: None,
        host_tools: None,
        trust_source: None,
        mailbox: None,
        gpu: None,
        trim_tools: false,
        forced_tools: Vec::new(),
        require_tool_call: false,
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

/// « @connecteur » impose ses outils : le premier tour part avec
/// `tool_choice: "required"`, et l'outil imposé est dans la liste offerte.
/// Sans outil imposé, rien ne change — le modèle reste libre.
#[tokio::test]
async fn un_outil_impose_exige_un_appel_au_premier_tour() {
    let client = reqwest::Client::new();

    let (base, vu) = fausse_passerelle().await;
    let libre = entree("qwen3-4b.gguf");
    let _ = vider(
        run_openai_tool_loop(&base, &client, &libre)
            .await
            .expect("le flux doit s'ouvrir"),
    )
    .await;
    let corps = vu
        .lock()
        .expect("verrou")
        .body
        .clone()
        .expect("un corps JSON");
    assert!(
        corps.get("tool_choice").is_none(),
        "sans mention, le modèle choisit librement : {corps}"
    );
    let premier_outil = corps
        .get("tools")
        .and_then(|t| t.as_array())
        .and_then(|t| t.first())
        .and_then(|t| t.pointer("/function/name"))
        .and_then(|n| n.as_str())
        .expect("la boucle offre au moins un outil sans projet")
        .to_string();

    let (base, vu) = fausse_passerelle().await;
    let mut impose = entree("qwen3-4b.gguf");
    impose.forced_tools = vec![premier_outil.clone()];
    impose.require_tool_call = true;
    let _ = vider(
        run_openai_tool_loop(&base, &client, &impose)
            .await
            .expect("le flux doit s'ouvrir"),
    )
    .await;
    let corps = vu
        .lock()
        .expect("verrou")
        .body
        .clone()
        .expect("un corps JSON");
    assert_eq!(
        corps.get("tool_choice").and_then(|c| c.as_str()),
        Some("required"),
        "un outil imposé doit être appelé"
    );
    let offerts: Vec<&str> = corps["tools"]
        .as_array()
        .expect("des outils")
        .iter()
        .filter_map(|t| t.pointer("/function/name").and_then(|n| n.as_str()))
        .collect();
    assert!(offerts.contains(&premier_outil.as_str()), "{offerts:?}");

    // Un outil imposé qui n'existe pas (connecteur arrêté entre-temps)
    // n'exige rien : forcer un appel sans outil correspondant bloquerait.
    let (base, vu) = fausse_passerelle().await;
    let mut fantome = entree("qwen3-4b.gguf");
    fantome.forced_tools = vec!["mcp__absent__outil".into()];
    fantome.require_tool_call = true;
    let _ = vider(
        run_openai_tool_loop(&base, &client, &fantome)
            .await
            .expect("le flux doit s'ouvrir"),
    )
    .await;
    let corps = vu
        .lock()
        .expect("verrou")
        .body
        .clone()
        .expect("un corps JSON");
    assert!(corps.get("tool_choice").is_none(), "{corps}");

    // Garanti sans être exigé (connecteur nommé sans « @ ») : offert, mais
    // le modèle reste libre de ne pas l'appeler.
    let (base, vu) = fausse_passerelle().await;
    let mut garanti = entree("qwen3-4b.gguf");
    garanti.forced_tools = vec![premier_outil.clone()];
    let _ = vider(
        run_openai_tool_loop(&base, &client, &garanti)
            .await
            .expect("le flux doit s'ouvrir"),
    )
    .await;
    let corps = vu
        .lock()
        .expect("verrou")
        .body
        .clone()
        .expect("un corps JSON");
    assert!(corps.get("tool_choice").is_none(), "{corps}");
}

/// Un moteur qui refuse la grammaire des outils au premier envoi (llama-server :
/// « failed to parse grammar ») : la boucle renvoie une fois, et la réponse
/// arrive au lieu d'une erreur affichée à la personne.
#[tokio::test]
async fn une_grammaire_refusee_est_retentee_une_fois() {
    let appels = Arc::new(Mutex::new(0u32));
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(|State(appels): State<Arc<Mutex<u32>>>| async move {
                let n = {
                    let mut a = appels.lock().expect("verrou");
                    *a += 1;
                    *a
                };
                if n == 1 {
                    return axum::response::Response::builder()
                        .status(400)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from(
                            r#"{"error":{"code":400,"message":"Failed to initialize samplers: failed to parse grammar"}}"#,
                        ))
                        .expect("réponse");
                }
                axum::response::Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(axum::body::Body::from(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"salut\"},\"finish_reason\":null}]}\n\n\
                         data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                         data: [DONE]\n\n",
                    ))
                    .expect("réponse")
            }),
        )
        .with_state(appels.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port libre");
    let addr = listener.local_addr().expect("adresse");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let client = reqwest::Client::new();
    let flux = run_openai_tool_loop(&format!("http://{addr}"), &client, &entree("qwen3-4b.gguf"))
        .await
        .expect("le second envoi doit aboutir");
    assert_eq!(vider(flux).await, "salut");
    assert_eq!(*appels.lock().expect("verrou"), 2, "un seul nouvel essai");
}
