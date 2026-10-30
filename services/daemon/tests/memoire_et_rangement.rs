//! La mémoire apprend-elle ce que la personne dit d'elle — son métier, ses
//! goûts, ses projets — et propose-t-elle de ranger une conversation libre
//! dans le projet dont elle parle ?
//!
//! Le vrai chemin est joué de bout en bout, sauf le modèle : un faux moteur
//! compatible OpenAI répond selon ce que le prompt lui montre. Il ne devine
//! rien — il ne retient que ce qui est réellement dans la demande, donc un
//! indice absent du prompt est un indice perdu.

use axum::{routing::post, Json, Router};
use locaryn_agent_runtime::titling::{
    ask_for_memory, ask_for_project, indices_du_projet, ProjetConnu,
};
use locaryn_shared_types::{MessageRole, TrustLevel};
use locaryn_storage::{open_in_memory, Storage};
use serde_json::{json, Value};

/// Les mots d'une phrase assez longs pour désigner quelque chose.
fn mots(texte: &str) -> Vec<String> {
    texte
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|m| m.chars().count() >= 5)
        .map(str::to_string)
        .collect()
}

/// Ce que dirait un petit modèle raisonnable, d'après le seul prompt reçu.
fn repondre(corps: &Value) -> String {
    let systeme = corps
        .pointer("/messages/0/content")
        .and_then(Value::as_str)
        .unwrap_or("");
    let user = corps
        .pointer("/messages/1/content")
        .and_then(Value::as_str)
        .unwrap_or("");

    if systeme.contains("DURABLE") {
        let mut lignes = Vec::new();
        if user.contains("développeur") {
            lignes.push("vous | Métier | Développeur logiciel indépendant");
        }
        if user.contains("drones") {
            lignes.push("sujets | Drones FPV | Passionné de drones FPV, en construit le week-end");
        }
        if user.contains("Bastet") {
            lignes.push("zones | Bot Bastet | Robot compagnon du projet Campus");
        }
        return if lignes.is_empty() {
            "RIEN".into()
        } else {
            lignes.join("\n")
        };
    }

    // Choix d'un projet : une liste numérotée, puis la conversation.
    let (liste, conversation) = user.split_once("Conversation :").unwrap_or((user, ""));
    let vocabulaire = mots(conversation);
    for ligne in liste.lines().filter(|l| l.contains(". ")) {
        let (numero, reste) = ligne.split_once(". ").unwrap_or(("", ligne));
        if mots(reste).iter().any(|m| vocabulaire.contains(m)) {
            return numero.trim().to_string();
        }
    }
    "AUCUN".into()
}

/// Un faux moteur sur un port libre ; rend son adresse.
async fn faux_moteur() -> String {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|Json(corps): Json<Value>| async move {
            Json(json!({ "choices": [{ "message": { "content": repondre(&corps) } }] }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("port libre");
    let adresse = listener.local_addr().expect("adresse locale");
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("faux moteur arrêté : {e}");
        }
    });
    format!("http://{adresse}")
}

async fn stockage() -> Storage {
    Storage::new(open_in_memory().await.expect("base en mémoire"))
}

/// Une conversation apprise : lire l'échange, retenir, comme le fait le service.
async fn apprendre(s: &Storage, endpoint: &str, echange: &str) -> usize {
    let client = reqwest::Client::new();
    let faits = ask_for_memory(endpoint, &client, "micro", echange).await;
    let n = faits.len();
    for f in faits {
        s.memory
            .remember(None, &f.group, &f.title, &f.detail, "assistant")
            .await
            .expect("le fait s'enregistre");
    }
    n
}

const ECHANGE_BASTET: &str = "Personne : Mon robot Bastet n'avance plus.\nAssistant : Voyons.";

#[tokio::test]
async fn le_metier_les_gouts_et_les_projets_se_retiennent() {
    let endpoint = faux_moteur().await;
    let s = stockage().await;

    let n = apprendre(
        &s,
        &endpoint,
        "Personne : Je suis développeur indépendant, et le week-end je construis des drones.\n\
         Assistant : Super ! Que veux-tu savoir ?",
    )
    .await;
    assert_eq!(n, 2, "métier et goût entendus");
    apprendre(&s, &endpoint, ECHANGE_BASTET).await;

    let fiches = s.memory.list(None).await.expect("liste");
    let titres: Vec<(&str, &str)> = fiches
        .iter()
        .map(|f| (f.group.as_str(), f.title.as_str()))
        .collect();
    assert_eq!(
        titres,
        vec![
            ("vous", "Métier"),
            ("sujets", "Drones FPV"),
            ("zones", "Bot Bastet")
        ],
        "chaque fait va dans le bon groupe, dans l'ordre de l'écran"
    );

    // Et le modèle de la prochaine conversation le sait, sans qu'on le redise.
    let bloc = s
        .memory
        .as_system_block(None)
        .await
        .expect("bloc")
        .expect("un bloc est ajouté au prompt");
    for attendu in [
        "Développeur logiciel indépendant",
        "drones FPV",
        "projet Campus",
    ] {
        assert!(
            bloc.contains(attendu),
            "« {attendu} » absent du prompt : {bloc}"
        );
    }
}

#[tokio::test]
async fn reentendre_la_meme_chose_ne_double_rien() {
    let endpoint = faux_moteur().await;
    let s = stockage().await;
    let echange = "Personne : Je suis développeur.\nAssistant : Bien noté.";
    apprendre(&s, &endpoint, echange).await;
    apprendre(&s, &endpoint, echange).await;
    let fiches = s.memory.list(None).await.expect("liste");
    assert_eq!(fiches.len(), 1);
    assert_eq!(fiches[0].details.len(), 1, "le même détail ne s'empile pas");
}

#[tokio::test]
async fn un_echange_banal_n_apprend_rien() {
    let endpoint = faux_moteur().await;
    let s = stockage().await;
    let n = apprendre(
        &s,
        &endpoint,
        "Personne : Quelle heure est-il ?\nAssistant : 14 h.",
    )
    .await;
    assert_eq!(n, 0);
    assert!(s
        .memory
        .as_system_block(None)
        .await
        .expect("bloc")
        .is_none());
}

/// Les projets tels que le service les présente au modèle, avec leurs indices.
async fn candidats(s: &Storage, hors: uuid::Uuid) -> Vec<ProjetConnu> {
    let mut out = Vec::new();
    for p in s.projects.list().await.expect("projets") {
        if p.id == hors || p.name == "Conversations libres" {
            continue;
        }
        let titres: Vec<String> = s
            .sessions
            .list_for_project(p.id)
            .await
            .expect("conversations")
            .into_iter()
            .filter_map(|c| c.title)
            .collect();
        let notes = s.memory.notes_sur(None, &p.name).await.expect("notes");
        out.push(ProjetConnu {
            id: p.id.to_string(),
            name: p.name.clone(),
            indices: indices_du_projet(&p.path, &titres, &notes),
        });
    }
    out
}

#[tokio::test]
async fn une_conversation_libre_est_proposee_pour_le_projet_dont_elle_parle() {
    let endpoint = faux_moteur().await;
    let s = stockage().await;
    let libre = s
        .projects
        .create("C:\\libre", "Conversations libres", TrustLevel::Untrusted)
        .await
        .expect("conteneur");
    let campus = s
        .projects
        .create(
            "D:\\Projets\\robots-campus",
            "Campus",
            TrustLevel::Untrusted,
        )
        .await
        .expect("projet");
    s.projects
        .create("D:\\Projets\\compta", "Compta", TrustLevel::Untrusted)
        .await
        .expect("projet");

    // Ce que la mémoire sait déjà du robot, appris dans une autre conversation.
    apprendre(&s, &endpoint, ECHANGE_BASTET).await;

    // Une conversation lancée à la volée, hors de tout projet.
    let conv = s
        .sessions
        .create(libre.id, Some("Calibrage".into()))
        .await
        .expect("conversation");
    s.messages
        .append(
            conv.id,
            MessageRole::User,
            "Comment calibrer le lidar de Bastet ?",
        )
        .await
        .expect("message");
    s.messages
        .append(
            conv.id,
            MessageRole::Assistant,
            "Place le robot face à un mur plat.",
        )
        .await
        .expect("message");
    let echange = "Comment calibrer le lidar de Bastet ?\nPlace le robot face à un mur plat.";

    let client = reqwest::Client::new();
    let liste = candidats(&s, libre.id).await;
    let choix = ask_for_project(&endpoint, &client, "micro", echange, &liste).await;
    assert_eq!(
        choix,
        Some(campus.id.to_string()),
        "la mémoire relie Bastet au projet Campus"
    );

    // Sans ce que la mémoire sait, rien ne relie « Bastet » à « Campus » :
    // l'indice est ce qui fait la proposition, pas le hasard.
    let nus: Vec<ProjetConnu> = liste
        .iter()
        .cloned()
        .map(|p| ProjetConnu {
            indices: vec![],
            ..p
        })
        .collect();
    assert_eq!(
        ask_for_project(&endpoint, &client, "micro", echange, &nus).await,
        None
    );

    // La personne accepte : la conversation rejoint le projet.
    s.sessions
        .move_to_project(conv.id, campus.id)
        .await
        .expect("déplacement");
    let dans_campus = s.sessions.list_for_project(campus.id).await.expect("liste");
    assert!(dans_campus.iter().any(|c| c.id == conv.id));
    assert!(s
        .sessions
        .list_for_project(libre.id)
        .await
        .expect("liste")
        .is_empty());
}

#[tokio::test]
async fn une_conversation_sans_rapport_ne_propose_rien() {
    let endpoint = faux_moteur().await;
    let s = stockage().await;
    let libre = s
        .projects
        .create("C:\\libre", "Conversations libres", TrustLevel::Untrusted)
        .await
        .expect("conteneur");
    s.projects
        .create(
            "D:\\Projets\\robots-campus",
            "Campus",
            TrustLevel::Untrusted,
        )
        .await
        .expect("projet");
    apprendre(&s, &endpoint, ECHANGE_BASTET).await;

    let liste = candidats(&s, libre.id).await;
    let choix = ask_for_project(
        &endpoint,
        &reqwest::Client::new(),
        "micro",
        "Donne-moi une recette de crêpes.\nMélange farine, œufs et lait.",
        &liste,
    )
    .await;
    assert_eq!(
        choix, None,
        "mieux vaut ne rien proposer qu'un rangement à côté"
    );
}
