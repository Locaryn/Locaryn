//! L'API compatible OpenAI du mode serveur.
//!
//! Locaryn expose son propre dialecte (`/v1/sessions`…) pour son application.
//! Mais ce qui veut s'y brancher — un éditeur, un agent tiers, un script —
//! parle OpenAI. Ces deux routes-là sont donc la porte d'entrée standard :
//! `GET /v1/models` et `POST /v1/chat/completions`.
//!
//! Et surtout : **une seule porte pour tout ce que la machine sait servir**.
//! Le modèle demandé peut être un fichier de poids servi par le moteur local,
//! ou un modèle routé par une passerelle installée par un morph (OmniRoute).
//! Le client n'a pas à savoir lequel : il nomme un modèle, le serveur trouve
//! qui le sert, et joint la clé s'il en faut une. C'est ce qui fait qu'ajouter
//! OmniRoute à l'application l'ajoute du même coup à son API.
//!
//! Le corps de la requête est transmis tel quel, et la réponse renvoyée
//! telle quelle — flux compris. Réécrire l'un ou l'autre ferait perdre les
//! champs que ce serveur ne connaît pas encore : outils, images, `logprobs`,
//! ce que la prochaine version du dialecte ajoutera.

use crate::DaemonState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use locaryn_cloud_providers as cloud;
use serde_json::{json, Value};
use std::sync::Arc;

/// ---- Le dialecte OpenAI ----------------------------------------------------
///
/// Un « dialecte » décrit comment une famille de clients parle : où envoyer le
/// chat, comment se nommer. La structure est prête pour accueillir les
/// prochains (`anthropic` : POST /v1/messages avec x-api-key ; `ollama` :
/// POST /api/chat avec GET /api/tags) sans retoucher la résolution de modèle
/// ni le relais — chaque dialecte ne décrit que ses routes et son auth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    OpenAi,
}

impl Dialect {
    /// Le chemin du chat, tel que le client standard l'appelle.
    pub fn chat_path(self) -> &'static str {
        match self {
            Dialect::OpenAi => "/v1/chat/completions",
        }
    }
}

/// Le préflight CORS de la surface standard : sans lui, un outil web ne peut
/// même pas poser la question avant d'envoyer son Bearer. Aucun cookie n'est
/// accepté — l'auth Bearer reste le seul facteur, donc `*` ne donne rien à
/// un tiers qui n'a pas la clé.
pub async fn cors_preflight() -> Response {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header("access-control-allow-origin", "*")
        .header("access-control-allow-methods", "GET, POST, OPTIONS")
        .header("access-control-allow-headers", "authorization, content-type")
        .header("access-control-max-age", "86400")
        .body(Body::empty())
        .unwrap()
}

/// Les en-têtes CORS d'une réponse ordinaire de la surface standard.
fn cors_headers(resp: &mut Response) {
    let h = resp.headers_mut();
    h.insert(
        "access-control-allow-origin",
        HeaderValue::from_static("*"),
    );
}

/// Les poignées du service, prêtées au socle des fournisseurs.
fn host(state: &DaemonState) -> cloud::Host<'_> {
    cloud::Host {
        storage: &state.storage,
        data_dir: &state.data_dir,
        http: &state.http,
        keychain: state.keychain.as_ref(),
    }
}

/// Une erreur au format que les clients OpenAI savent lire.
fn erreur(code: StatusCode, message: &str, kind: &str) -> Response {
    (
        code,
        Json(json!({ "error": { "message": message, "type": kind } })),
    )
        .into_response()
}

// ============================================================================
// GET /v1/models
// ============================================================================

/// Tout ce que ce serveur sait servir, en une liste.
///
/// Les poids installés localement, et les modèles de chaque passerelle
/// active. Les seconds sont préfixés par l'identifiant de leur fournisseur
/// (`omniroute/anthropic/claude-opus-5`) : deux catalogues peuvent publier le
/// même nom, et un client doit pouvoir désigner celui qu'il veut. La forme
/// non préfixée reste acceptée à l'appel.
pub async fn list_models(State(state): State<Arc<DaemonState>>) -> Response {
    let mut data: Vec<Value> = Vec::new();

    // La meme enumeration que le selecteur de l'application, et pour la meme
    // raison : elle ecarte ce qu'un moteur de chat ne sait pas charger — poids
    // de diffusion, TTS, embeddings, dossiers internes d'Ollama. Une regle
    // « .gguf ou dossier » propre a cette route avait annonce `blobs` et
    // `stable-diffusion` comme modeles de chat, qu'un client proposait ensuite
    // a l'utilisateur.
    for name in crate::media::list_chat_models() {
        data.push(json!({
            "id": name,
            "object": "model",
            "owned_by": "local",
        }));
    }

    let h = host(&state);
    for p in cloud::declared(&h).await {
        // Le catalogue gardé, jamais une lecture réseau : `/v1/models` est
        // appelé à chaque démarrage de client, et faire dépendre sa réponse
        // d'une passerelle éteinte rendrait la liste vide au pire moment.
        let Some(cache) = cloud::CachedCatalog::load(&state.data_dir, &p.id) else {
            continue;
        };
        for m in cache.models {
            data.push(json!({
                "id": format!("{}/{}", p.id, m.id),
                "object": "model",
                "owned_by": p.id,
                "context_length": m.context_length,
            }));
        }
    }

    let mut resp = Json(json!({ "object": "list", "data": data })).into_response();
    cors_headers(&mut resp);
    resp
}

// ============================================================================
// POST /v1/chat/completions
// ============================================================================

/// Une conversation, dans le dialecte que tout le monde parle.
///
/// Le serveur ne fait pas l'agent ici : pas d'outils, pas d'approbation, pas
/// de session — c'est le rôle de `/v1/sessions`. Cette route est une porte
/// d'entrée compatible, et elle se comporte comme telle : ce qu'on lui donne
/// part chez celui qui sert le modèle, et ce qu'il répond revient tel quel.
pub async fn chat_completions(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    body: Json<Value>,
) -> Response {
    let Json(mut corps) = body;
    let modele = corps
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    if modele.is_empty() {
        return erreur(
            StatusCode::BAD_REQUEST,
            "Le champ « model » est obligatoire. Appelez GET /v1/models pour la liste.",
            "invalid_request_error",
        );
    }

    let h = host(&state);
    let (url, cle, modele_cible) = match cloud::provider_of_model(&h, &modele).await {
        // Un modèle de passerelle : on parle à la passerelle, avec sa clé.
        Some(p) => {
            let cle = cloud::stored_key(&h, &p.id);
            if cle.is_none() {
                return erreur(
                    StatusCode::UNAUTHORIZED,
                    &format!(
                        "Aucune clé enregistrée pour {}. Renseignez-la dans son dossier, ou \
                         par la variable d'environnement {}.",
                        p.label(),
                        cloud::env_key_name(&p.id)
                    ),
                    "invalid_request_error",
                );
            }
            (
                format!(
                    "{}/v1/chat/completions",
                    p.manifest.api_url.trim_end_matches('/')
                ),
                cle,
                cloud::strip_provider_prefix(&p.id, &modele),
            )
        }
        // Sinon le moteur local : celui que l'application a rendu actif.
        None => match state.storage.providers.active().await.ok().flatten() {
            Some(p) => (
                format!("{}/v1/chat/completions", p.endpoint.trim_end_matches('/')),
                cloud::key_for_active_provider(&h, &p),
                modele.clone(),
            ),
            None => {
                return erreur(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Aucun moteur actif et aucun fournisseur ne sert ce modèle.",
                    "server_error",
                )
            }
        },
    };

    // Le nom que le serveur d'en face attend, pas celui que le client a écrit.
    corps["model"] = Value::String(modele_cible);

    let mut req = state.http.post(&url).json(&corps);
    if let Some(k) = cle {
        req = req.bearer_auth(k);
    }
    // Deux en-têtes que les clients OpenAI envoient et qui ont un sens en
    // aval. Le reste — dont l'`Authorization` du client, qui authentifie
    // auprès de *ce* serveur — ne doit surtout pas être relayé.
    for nom in ["accept", "content-type"] {
        if let Some(v) = headers.get(nom) {
            req = req.header(nom, v);
        }
    }

    let reponse = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return erreur(
                StatusCode::BAD_GATEWAY,
                &format!("Le fournisseur de ce modèle est injoignable : {e}"),
                "server_error",
            )
        }
    };

    let statut = StatusCode::from_u16(reponse.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let type_contenu = reponse
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();

    // Le flux est relayé octet par octet : le client voit les jetons arriver
    // au même rythme que nous, sans qu'on ait à comprendre le dialecte.
    let flux = reponse.bytes_stream();
    Response::builder()
        .status(statut)
        .header("content-type", type_contenu)
        .header("access-control-allow-origin", "*")
        .body(Body::from_stream(flux))
        .unwrap_or_else(|_| {
            erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                "réponse impossible à relayer",
                "server_error",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Ce que `/v1/models` annonce doit etre chargeable par un moteur de chat.
    ///
    /// Le test porte sur `is_chat_weight`, la regle que la route emprunte
    /// desormais : c'est elle qui ecarte les poids de diffusion, les voix, les
    /// embeddings. L'ancienne version testait une copie de la regle ecrite
    /// dans le test lui-meme, et ne pouvait donc rien attraper.
    #[test]
    fn la_liste_locale_ecarte_ce_qui_nest_pas_un_modele_de_chat() {
        use std::path::Path;
        assert!(crate::media::is_chat_weight(Path::new(
            "UTENA-7B-NSFW-V2-Q4_K_M.gguf"
        )));
        for refuse in [
            "stable-diffusion-v1-5-pruned-emaonly-Q4_0.gguf",
            "nomic-embed-text-v1.5.Q5_K_M.gguf",
            "kokoro-82m.gguf",
            "modele.gguf.part",
            "capture.png",
        ] {
            assert!(
                !crate::media::is_chat_weight(Path::new(refuse)),
                "{refuse} ne doit pas etre propose comme modele de chat"
            );
        }
    }

    /// Une erreur doit être lisible par un client OpenAI : il lit
    /// `error.message`, pas une chaîne nue.
    #[test]
    fn les_erreurs_ont_la_forme_attendue() {
        let r = erreur(
            StatusCode::BAD_REQUEST,
            "modèle manquant",
            "invalid_request_error",
        );
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }
}

// ============================================================================
// Ce que l'API fait réellement, contre une fausse passerelle
// ============================================================================

/// La partie qui compte : un client tiers appelle ce serveur, et sa requête
/// doit atterrir chez celui qui sert le modèle — avec la clé, et sous le nom
/// que ce dernier connaît.
///
/// La passerelle est fausse, mais tout le reste est vrai : la base, le
/// manifeste sur le disque, le catalogue gardé, la résolution du modèle.
#[cfg(test)]
mod api_tests {
    use super::*;
    use axum::http::HeaderMap;
    use locaryn_auth::{Keychain, KeychainError};
    use locaryn_shared_types::{ExtensionEcosystem, ExtensionKind, ExtensionScope};
    use locaryn_storage::repos::{NewExtension, Storage};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Un trousseau en mémoire : un test ne touche pas à celui de la machine.
    #[derive(Default)]
    struct TrousseauDEssai {
        entries: Mutex<HashMap<String, String>>,
    }

    impl Keychain for TrousseauDEssai {
        fn put(&self, key: &str, value: &str) -> Result<(), KeychainError> {
            self.entries
                .lock()
                .expect("verrou")
                .insert(key.into(), value.into());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<String, KeychainError> {
            self.entries
                .lock()
                .expect("verrou")
                .get(key)
                .cloned()
                .ok_or_else(|| KeychainError::NotFound(key.into()))
        }
        fn delete(&self, key: &str) -> Result<(), KeychainError> {
            self.entries.lock().expect("verrou").remove(key);
            Ok(())
        }
    }

    /// Ce que la fausse passerelle a vu passer.
    #[derive(Debug, Default, Clone)]
    struct Vu {
        authorization: Option<String>,
        model: Option<String>,
    }

    /// Une passerelle compatible OpenAI, réduite à ce que le test observe.
    async fn fausse_passerelle() -> (String, Arc<Mutex<Vu>>) {
        let vu = Arc::new(Mutex::new(Vu::default()));
        let app = axum::Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(
                    |axum::extract::State(vu): axum::extract::State<Arc<Mutex<Vu>>>,
                     headers: HeaderMap,
                     Json(body): Json<Value>| async move {
                        {
                            let mut v = vu.lock().expect("verrou");
                            v.authorization = headers
                                .get("authorization")
                                .and_then(|h| h.to_str().ok())
                                .map(str::to_string);
                            v.model = body
                                .get("model")
                                .and_then(|m| m.as_str())
                                .map(str::to_string);
                        }
                        Json(json!({ "choices": [{ "message": { "content": "salut" } }] }))
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

    /// Un service d'essai : base en mémoire, dossier de données, et une
    /// extension active qui déclare la fausse passerelle.
    async fn service(nom: &str, base_passerelle: &str) -> (Arc<DaemonState>, std::path::PathBuf) {
        let racine = std::env::temp_dir().join(format!("locaryn-openai-api-{nom}"));
        let _ = std::fs::remove_dir_all(&racine);
        let paquet = racine.join("plugins/morph-omniroute");
        std::fs::create_dir_all(&paquet).expect("paquet");
        std::fs::write(
            paquet.join("morph.json"),
            format!(
                r#"{{
                  "apiVersion": "0.1",
                  "name": "morph-omniroute",
                  "version": "1.0.0",
                  "cloud_provider": {{
                    "id": "omniroute",
                    "label": "OmniRoute",
                    "api_url": "{base_passerelle}",
                    "local": {{ "start": ["omniroute"] }}
                  }}
                }}"#
            ),
        )
        .expect("manifeste");
        let data_dir = racine.join("data");
        std::fs::create_dir_all(&data_dir).expect("données");

        let pool = locaryn_storage::open(std::path::Path::new(":memory:"))
            .await
            .expect("base");
        let storage = Storage::new(pool);
        let record = storage
            .extensions
            .upsert(NewExtension {
                name: "morph-omniroute".into(),
                version: "1.0.0".into(),
                api_version: "0.1".into(),
                kind: ExtensionKind::Plugin,
                scope: ExtensionScope::User,
                ecosystem: ExtensionEcosystem::Locaryn,
                source: None,
                manifest_path: paquet.join("morph.json").display().to_string(),
                requested: Vec::new(),
            })
            .await
            .expect("extension");
        storage
            .extensions
            .set_enabled(record.id, true)
            .await
            .expect("activée");

        // Le catalogue que la passerelle aurait publié.
        locaryn_cloud_providers::CachedCatalog {
            updated_at: chrono::Utc::now().to_rfc3339(),
            models: vec![locaryn_cloud_providers::CloudModel {
                id: "anthropic/claude-opus-5".into(),
                name: "Claude Opus 5".into(),
                context_length: 1_000_000,
                ..Default::default()
            }],
        }
        .save(&data_dir, "omniroute")
        .expect("catalogue");

        let supervisor = locaryn_provider_supervisor::Supervisor::new(
            locaryn_provider_supervisor::SupervisorConfig::default(),
            storage.clone(),
        );
        let state = Arc::new(DaemonState {
            mode: locaryn_shared_types::ConnectionMode::Local,
            start_time: chrono::Utc::now(),
            data_dir: data_dir.clone(),
            storage,
            supervisor,
            extensions: locaryn_extensions::registry::ExtensionRegistry::new(),
            mcp_state: Arc::new(locaryn_mcp::McpState::new()),
            http: reqwest::Client::new(),
            keychain: Arc::new(TrousseauDEssai::default()),
            cores: locaryn_core_bridge::manager::CoreManager::new(),
            travel: crate::travel::TravelState::new(),
            port: 0,
            auth_required: false,
            local_url: "http://127.0.0.1".into(),
            cancel_map: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        });
        (state, racine)
    }

    async fn corps_json(r: Response) -> Value {
        let bytes = axum::body::to_bytes(r.into_body(), 1 << 20)
            .await
            .expect("corps lu");
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    /// La liste réunit ce que la machine sert : les poids locaux et les
    /// modèles routés, ces derniers préfixés par leur fournisseur.
    #[tokio::test]
    async fn la_liste_des_modeles_reunit_le_local_et_la_passerelle() {
        let (base, _) = fausse_passerelle().await;
        let (state, _racine) = service("liste", &base).await;

        let reponse = list_models(axum::extract::State(state)).await;
        let corps = corps_json(reponse).await;
        let ids: Vec<String> = corps["data"]
            .as_array()
            .expect("une liste")
            .iter()
            .filter_map(|m| m["id"].as_str().map(str::to_string))
            .collect();
        assert!(
            ids.contains(&"omniroute/anthropic/claude-opus-5".to_string()),
            "le modèle routé doit être listé, préfixé : {ids:?}"
        );
    }

    /// Sans clé, la requête ne part pas : un appel non authentifié échouerait
    /// chez la passerelle, et le client ne saurait pas quoi corriger.
    #[tokio::test]
    async fn sans_cle_lapi_refuse_et_dit_ou_la_mettre() {
        let (base, vu) = fausse_passerelle().await;
        let (state, _racine) = service("sans-cle", &base).await;

        let reponse = chat_completions(
            axum::extract::State(state),
            HeaderMap::new(),
            Json(json!({ "model": "anthropic/claude-opus-5", "messages": [] })),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::UNAUTHORIZED);
        let corps = corps_json(reponse).await;
        let message = corps["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("LOCARYN_CLOUD_OMNIROUTE_KEY"),
            "le message doit nommer la variable d'environnement : {message}"
        );
        assert!(
            vu.lock().expect("verrou").model.is_none(),
            "rien n'est parti"
        );
    }

    /// Le cas complet : un client tiers appelle ce serveur, la requête part
    /// chez la passerelle avec sa clé, sous le nom qu'elle connaît.
    #[tokio::test]
    async fn avec_cle_la_requete_part_chez_la_passerelle() {
        let (base, vu) = fausse_passerelle().await;
        let (state, _racine) = service("avec-cle", &base).await;
        state
            .keychain
            .put("locaryn/cloud/omniroute", "cle-dessai")
            .expect("clé posée");

        // La forme préfixée, celle que `GET /v1/models` publie.
        let reponse = chat_completions(
            axum::extract::State(state),
            HeaderMap::new(),
            Json(json!({
                "model": "omniroute/anthropic/claude-opus-5",
                "messages": [{ "role": "user", "content": "bonjour" }]
            })),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::OK);
        let corps = corps_json(reponse).await;
        assert_eq!(corps["choices"][0]["message"]["content"], "salut");

        let v = vu.lock().expect("verrou").clone();
        assert_eq!(
            v.authorization.as_deref(),
            Some("Bearer cle-dessai"),
            "la clé de l'hôte doit accompagner la requête"
        );
        assert_eq!(
            v.model.as_deref(),
            Some("anthropic/claude-opus-5"),
            "le préfixe de l'API ne doit pas partir sur le fil"
        );
    }

    /// Un modèle que personne ne sert, sans moteur actif : le client doit
    /// l'apprendre, pas attendre.
    #[tokio::test]
    async fn un_modele_inconnu_sans_moteur_actif_est_annonce() {
        let (base, _) = fausse_passerelle().await;
        let (state, _racine) = service("inconnu", &base).await;

        let reponse = chat_completions(
            axum::extract::State(state),
            HeaderMap::new(),
            Json(json!({ "model": "un-modele-inexistant.gguf", "messages": [] })),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Le champ `model` est obligatoire, et le dire vaut mieux que de router
    /// au hasard.
    #[tokio::test]
    async fn sans_modele_la_requete_est_refusee() {
        let (base, _) = fausse_passerelle().await;
        let (state, _racine) = service("sans-modele", &base).await;

        let reponse = chat_completions(
            axum::extract::State(state),
            HeaderMap::new(),
            Json(json!({ "messages": [] })),
        )
        .await;
        assert_eq!(reponse.status(), StatusCode::BAD_REQUEST);
    }
}
