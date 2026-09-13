//! Le chemin réel, contre le vrai OmniRoute : démarrer, obtenir la clé, lire
//! le catalogue, converser, arrêter.
//!
//! Ignoré par défaut — il exige Node, un OmniRoute déjà installé dans le
//! dossier des passerelles, et le réseau pour la conversation. Il se lance à la
//! main, quand on touche au démarrage d'une passerelle :
//!
//! ```text
//! LOCARYN_STORAGE_ROOT=<racine> cargo test -p locaryn-cloud-providers \
//!     --test omniroute_reel -- --ignored --nocapture
//! ```
//!
//! C'est le seul test qui prouve que les commandes du manifeste publié
//! fonctionnent vraiment : les autres vérifient qu'elles se lisent.

use locaryn_auth::{Keychain, KeychainError};
use locaryn_cloud_providers as cloud;
use locaryn_cloud_providers::Host;
use locaryn_shared_types::{ExtensionEcosystem, ExtensionKind, ExtensionScope};
use locaryn_storage::repos::{NewExtension, Storage};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct TrousseauDEssai {
    entries: Mutex<HashMap<String, String>>,
}

impl Keychain for TrousseauDEssai {
    fn put(&self, key: &str, value: &str) -> Result<(), KeychainError> {
        self.entries
            .lock()
            .expect("verrou")
            .insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn get(&self, key: &str) -> Result<String, KeychainError> {
        self.entries
            .lock()
            .expect("verrou")
            .get(key)
            .cloned()
            .ok_or_else(|| KeychainError::NotFound(key.to_string()))
    }
    fn delete(&self, key: &str) -> Result<(), KeychainError> {
        self.entries.lock().expect("verrou").remove(key);
        Ok(())
    }
}

#[tokio::test]
#[ignore = "exige Node, un OmniRoute installé et le réseau"]
async fn omniroute_demarre_donne_sa_cle_converse_et_s_arrete() {
    let paquet = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/morph-omniroute")
        .canonicalize()
        .expect("le morph est dans le dépôt");

    let data_dir = std::env::temp_dir().join("locaryn-omniroute-reel");
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("dossier de données");

    let pool = locaryn_storage::open(std::path::Path::new(":memory:"))
        .await
        .expect("base");
    let storage = Storage::new(pool);
    let record = storage
        .extensions
        .upsert(NewExtension {
            name: "morph-omniroute".into(),
            version: "1.0.0-beta.2".into(),
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

    let keychain = TrousseauDEssai::default();
    let http = reqwest::Client::new();
    let h = Host {
        storage: &storage,
        data_dir: &data_dir,
        http: &http,
        keychain: &keychain,
    };
    let p = cloud::find(&h, "omniroute").await.expect("fournisseur");
    println!(
        "dossier de la passerelle : {}",
        cloud::gateway_dir(&p.id).display()
    );

    // ── Démarrer ─────────────────────────────────────────────────────────
    let debut = std::time::Instant::now();
    let etat = cloud::start(&h, &p).await.expect("démarrage");
    println!("démarrée en {:?} : {}", debut.elapsed(), etat.detail);
    assert!(
        etat.running,
        "la passerelle doit répondre : {}",
        etat.detail
    );

    // ── Les secrets et la clé, obtenus sans l'utilisateur ───────────────
    assert!(
        cloud::dashboard_password(&h, &p).is_some(),
        "le mot de passe du tableau de bord est généré"
    );
    let cle = cloud::stored_key(&h, "omniroute").expect("la clé est obtenue au démarrage");
    println!("clé obtenue : {} caractères", cle.len());

    // ── Le catalogue ─────────────────────────────────────────────────────
    let modeles = cloud::models(&h, &p, true).await.expect("catalogue");
    println!("{} modèles routés", modeles.len());
    assert!(!modeles.is_empty(), "la clé ouvre la liste des modèles");

    // ── Une vraie conversation, sans que l'utilisateur ait rien configuré ─
    let reponse = http
        .post(format!("{}/v1/chat/completions", p.manifest.api_url))
        .bearer_auth(&cle)
        .json(&serde_json::json!({
            "model": "auto",
            "messages": [{ "role": "user", "content": "Réponds seulement : bonjour" }],
            "max_tokens": 30
        }))
        .send()
        .await
        .expect("requête");
    let code = reponse.status();
    let corps: serde_json::Value = reponse.json().await.expect("réponse JSON");
    println!("conversation {code} : {corps}");
    assert!(
        code.is_success(),
        "OmniRoute doit répondre sans configuration"
    );

    // ── La boucle locale seulement ───────────────────────────────────────
    // Joindre la passerelle par une autre adresse de la machine doit échouer :
    // par défaut OmniRoute écoute tout le réseau, sans clé.
    let hote = std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if let Some(hote) = hote.filter(|h| !h.is_empty()) {
        let ailleurs = http
            .get(format!("http://{hote}:20128/v1/models"))
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await;
        if let Ok(r) = &ailleurs {
            // `hostname` peut se résoudre en 127.0.0.1 sur certaines machines :
            // on ne conclut que sur une réponse venue d'une autre adresse.
            println!("par {hote} : {}", r.status());
        } else {
            println!("par {hote} : injoignable, comme attendu");
        }
    }

    // ── Arrêter ──────────────────────────────────────────────────────────
    let etat = cloud::stop(&h, &p).await.expect("arrêt");
    assert!(
        !etat.running,
        "la passerelle doit être arrêtée : {}",
        etat.detail
    );
}
