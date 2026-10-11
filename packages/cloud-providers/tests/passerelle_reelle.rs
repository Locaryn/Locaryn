//! Le chemin réel, contre une vraie passerelle apportée par un morph :
//! démarrer, obtenir la clé, lire le catalogue, converser, arrêter.
//!
//! Ignoré par défaut — il exige le morph sur le disque, sa passerelle déjà
//! installée, et souvent le réseau. Il se lance à la main, quand on touche au
//! démarrage d'une passerelle. Aucun morph n'est nommé ici : on lui donne le
//! dossier de celui qu'on veut éprouver.
//!
//! ```text
//! LOCARYN_PASSERELLE_MORPH=<dossier du morph> LOCARYN_STORAGE_ROOT=<racine> \
//!     cargo test -p locaryn-cloud-providers --test passerelle_reelle -- --ignored --nocapture
//! ```
//!
//! C'est le seul test qui prouve que les commandes d'un manifeste publié
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
#[ignore = "exige un morph de passerelle installé (LOCARYN_PASSERELLE_MORPH)"]
async fn une_passerelle_demarre_donne_sa_cle_et_s_arrete() {
    let paquet = std::path::PathBuf::from(
        std::env::var("LOCARYN_PASSERELLE_MORPH")
            .expect("LOCARYN_PASSERELLE_MORPH : le dossier du morph à éprouver"),
    )
    .canonicalize()
    .expect("le dossier du morph existe");
    let manifeste: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(paquet.join("morph.json")).expect("morph.json lisible"),
    )
    .expect("morph.json valide");
    let nom = manifeste["name"].as_str().expect("un nom").to_string();
    let version = manifeste["version"].as_str().unwrap_or("0.0.0").to_string();

    let data_dir = std::env::temp_dir().join("locaryn-passerelle-reelle");
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("dossier de données");

    let pool = locaryn_storage::open(std::path::Path::new(":memory:"))
        .await
        .expect("base");
    let storage = Storage::new(pool);
    let record = storage
        .extensions
        .upsert(NewExtension {
            name: nom.clone(),
            version,
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
    let p = cloud::declared(&h)
        .await
        .into_iter()
        .next()
        .expect("le morph déclare un fournisseur");
    println!(
        "{} — dossier de la passerelle : {}",
        p.label(),
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

    // ── La clé, obtenue sans l'utilisateur, si le manifeste sait comment ──
    let declare_une_cle = p
        .manifest
        .local
        .as_ref()
        .is_some_and(|l| l.provision_key.is_some());
    let cle = cloud::stored_key(&h, &p.id);
    if declare_une_cle {
        let cle = cle.as_ref().expect("la clé est obtenue au démarrage");
        println!("clé obtenue : {} caractères", cle.len());
    }

    // ── Le catalogue ─────────────────────────────────────────────────────
    let modeles = cloud::models(&h, &p, true).await.expect("catalogue");
    println!("{} modèles routés", modeles.len());
    assert!(!modeles.is_empty(), "la liste des modèles s'ouvre");

    // ── Une conversation : son succès dépend des comptes connectés à la
    //    passerelle, on le montre sans l'exiger.
    let mut requete = http
        .post(format!("{}/v1/chat/completions", p.manifest.api_url))
        .json(&serde_json::json!({
            "model": "auto",
            "messages": [{ "role": "user", "content": "Réponds seulement : bonjour" }],
            "max_tokens": 30
        }));
    if let Some(cle) = &cle {
        requete = requete.bearer_auth(cle);
    }
    match requete.send().await {
        Ok(r) => println!("conversation : {}", r.status()),
        Err(e) => println!("conversation impossible : {e}"),
    }

    // ── Arrêter ──────────────────────────────────────────────────────────
    let etat = cloud::stop(&h, &p).await.expect("arrêt");
    assert!(
        !etat.running,
        "la passerelle doit être arrêtée : {}",
        etat.detail
    );
}
