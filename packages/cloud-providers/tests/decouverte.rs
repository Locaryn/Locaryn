//! Le chemin complet, depuis une extension installée jusqu'au modèle actif.
//!
//! C'est le test qui répond à « est-ce que ça marche vraiment » : une base, un
//! paquet sur le disque avec son manifeste, et l'on vérifie que le fournisseur
//! est découvert, que son modèle est retrouvé par l'API, et que le choisir
//! écrit bien ce que la conversation relira.
//!
//! Rien n'est simulé côté socle : c'est la vraie base SQLite, le vrai
//! manifeste, le vrai cache sur disque. Seuls le trousseau et le réseau sont
//! remplacés — l'un parce qu'un test ne doit pas écrire dans le trousseau de
//! la machine, l'autre parce qu'aucun test ne doit dépendre d'un service
//! payant.

use locaryn_auth::{Keychain, KeychainError};
use locaryn_cloud_providers as cloud;
use locaryn_cloud_providers::{CachedCatalog, CloudModel, Host};
use locaryn_shared_types::{ExtensionEcosystem, ExtensionKind, ExtensionScope};
use locaryn_storage::repos::{NewExtension, ProviderRepo, Storage};
use std::collections::HashMap;
use std::sync::Mutex;

/// Les variables d'environnement sont globales au processus : les tests qui
/// les lisent ou les écrivent partagent ce verrou pour rester déterministes.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Un trousseau en mémoire : le test ne touche pas à celui du système.
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

const MANIFESTE: &str = r#"{
  "apiVersion": "0.1",
  "name": "morph-omniroute",
  "version": "1.0.0",
  "cloud_provider": {
    "id": "omniroute",
    "label": "OmniRoute",
    "api_url": "http://localhost:20128",
    "keys_url": "http://localhost:20128",
    "refresh_hours": 1,
    "local": {
      "start": ["omniroute"],
      "dashboard_url": "http://localhost:20128",
      "install_hint": "npm install -g omniroute",
      "install": { "kind": "npm", "package": "omniroute", "probe_bin": "omniroute" }
    }
  }
}"#;

/// Une machine d'essai : base en mémoire, dossier de données, un paquet
/// installé et actif.
struct Machine {
    storage: Storage,
    data_dir: std::path::PathBuf,
    http: reqwest::Client,
    keychain: TrousseauDEssai,
    _racine: std::path::PathBuf,
}

impl Machine {
    fn host(&self) -> Host<'_> {
        Host {
            storage: &self.storage,
            data_dir: &self.data_dir,
            http: &self.http,
            keychain: &self.keychain,
        }
    }

    async fn nouvelle(nom: &str) -> Self {
        let racine = std::env::temp_dir().join(format!("locaryn-cloud-test-{nom}"));
        let _ = std::fs::remove_dir_all(&racine);
        let paquet = racine.join("plugins/morph-omniroute");
        std::fs::create_dir_all(&paquet).expect("dossier du paquet");
        std::fs::write(paquet.join("morph.json"), MANIFESTE).expect("manifeste");
        let data_dir = racine.join("data");
        std::fs::create_dir_all(&data_dir).expect("dossier de données");

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
                source: Some("github:Locaryn/morph-omniroute".into()),
                manifest_path: paquet.join("morph.json").display().to_string(),
                requested: Vec::new(),
            })
            .await
            .expect("extension enregistrée");
        storage
            .extensions
            .set_enabled(record.id, true)
            .await
            .expect("extension activée");

        Self {
            storage,
            data_dir,
            http: reqwest::Client::new(),
            keychain: TrousseauDEssai::default(),
            _racine: racine,
        }
    }

    /// Poser un catalogue sur le disque, comme l'aurait fait une lecture chez
    /// la passerelle.
    fn poser_catalogue(&self) {
        CachedCatalog {
            updated_at: chrono::Utc::now().to_rfc3339(),
            models: vec![CloudModel {
                id: "anthropic/claude-opus-5".into(),
                name: "Claude Opus 5".into(),
                context_length: 1_000_000,
                supports_tools: true,
                ..Default::default()
            }],
        }
        .save(&self.data_dir, "omniroute")
        .expect("catalogue écrit");
    }
}

/// Une extension active qui déclare un `cloud_provider` doit être découverte
/// telle qu'elle est écrite — sans quoi rien du reste ne peut fonctionner.
#[tokio::test]
async fn une_extension_active_apporte_son_fournisseur() {
    let m = Machine::nouvelle("decouverte").await;
    let h = m.host();

    let trouves = cloud::declared(&h).await;
    assert_eq!(trouves.len(), 1);
    let p = &trouves[0];
    assert_eq!(p.id, "omniroute");
    assert_eq!(p.label(), "OmniRoute");
    assert_eq!(
        p.manifest.effective_models_url(),
        "http://localhost:20128/v1/models"
    );
    assert!(p.manifest.local.is_some(), "une passerelle locale");

    let infos = cloud::list_infos(&h).await;
    assert_eq!(infos.len(), 1);
    assert!(infos[0].is_local);
    assert!(infos[0].can_start);
    assert!(
        infos[0].can_install,
        "npm est déclaré : l'hôte sait installer"
    );
    assert!(!infos[0].has_key, "aucune clé n'a encore été posée");
}

/// Une extension désactivée n'apporte plus rien : c'est ce qui fait
/// disparaître un catalogue sans le désinstaller.
#[tokio::test]
async fn une_extension_desactivee_retire_son_fournisseur() {
    let m = Machine::nouvelle("desactivee").await;
    let id = m.storage.extensions.list().await.unwrap()[0].id;
    m.storage.extensions.set_enabled(id, false).await.unwrap();
    assert!(cloud::declared(&m.host()).await.is_empty());
}

/// La question que pose l'API compatible OpenAI à chaque requête : qui sert
/// ce modèle ? La réponse vient du catalogue gardé, sans appel réseau.
#[tokio::test]
async fn lapi_retrouve_le_fournisseur_dun_modele() {
    let m = Machine::nouvelle("resolution").await;
    m.poser_catalogue();
    let h = m.host();

    // La forme publiée par la passerelle.
    let p = cloud::provider_of_model(&h, "anthropic/claude-opus-5")
        .await
        .expect("modèle du catalogue");
    assert_eq!(p.id, "omniroute");

    // Et la forme préfixée, que l'API accepte pour lever une ambiguïté entre
    // deux catalogues qui publieraient le même nom.
    let p = cloud::provider_of_model(&h, "omniroute/anthropic/claude-opus-5")
        .await
        .expect("modèle préfixé");
    assert_eq!(p.id, "omniroute");
    assert_eq!(
        cloud::strip_provider_prefix(&p.id, "omniroute/anthropic/claude-opus-5"),
        "anthropic/claude-opus-5",
        "la passerelle ne connaît que son propre identifiant"
    );

    // Un modèle local n'appartient à personne : la requête ira au moteur.
    assert!(cloud::provider_of_model(&h, "qwen3-4b.gguf")
        .await
        .is_none());
}

/// Choisir un modèle sans clé doit être refusé — c'est le seul garde-fou qui
/// évite un appel non authentifié, et il doit dire quoi faire.
#[tokio::test]
async fn choisir_sans_cle_est_refuse_et_avec_cle_ecrit_le_fournisseur() {
    let _env = ENV_LOCK.lock().expect("verrou env");
    let m = Machine::nouvelle("selection").await;
    m.poser_catalogue();
    let h = m.host();

    let refus = cloud::select(&h, "omniroute", "anthropic/claude-opus-5")
        .await
        .expect_err("sans clé, le choix doit être refusé");
    assert!(
        refus.contains("clé"),
        "le message doit parler de la clé : {refus}"
    );

    cloud::set_key(&h, "omniroute", "cle-dessai").expect("clé posée");
    assert!(cloud::stored_key(&h, "omniroute").is_some());

    // Le préfixe est retiré à l'écriture : c'est l'identifiant du fournisseur
    // qui part sur le fil, pas celui de l'API.
    cloud::select(&h, "omniroute", "omniroute/anthropic/claude-opus-5")
        .await
        .expect("le choix doit passer");

    let actif = m
        .storage
        .providers
        .active()
        .await
        .expect("lecture")
        .expect("un fournisseur actif");
    assert_eq!(actif.model.as_deref(), Some("anthropic/claude-opus-5"));
    assert_eq!(
        ProviderRepo::cloud_provider_of(&actif).as_deref(),
        Some("omniroute")
    );
    // Et c'est bien cette clé que la conversation joindra à sa requête.
    assert_eq!(
        cloud::key_for_active_provider(&h, &actif).as_deref(),
        Some("cle-dessai")
    );

    cloud::clear_key(&h, "omniroute").expect("clé effacée");
    assert!(cloud::stored_key(&h, "omniroute").is_none());
}

/// Sur un serveur sans trousseau, la clé vient de l'environnement : sans ce
/// repli, le mode serveur ne pourrait jamais parler à une passerelle.
#[tokio::test]
async fn sans_trousseau_la_cle_vient_de_lenvironnement() {
    let _env = ENV_LOCK.lock().expect("verrou env");
    let m = Machine::nouvelle("environnement").await;
    let nom = cloud::env_key_name("omniroute");
    assert_eq!(nom, "LOCARYN_CLOUD_OMNIROUTE_KEY");

    std::env::set_var(&nom, "cle-de-serveur");
    let trouvee = cloud::stored_key(&m.host(), "omniroute");
    std::env::remove_var(&nom);

    assert_eq!(trouvee.as_deref(), Some("cle-de-serveur"));
}
