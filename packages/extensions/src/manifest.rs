//! `plugin.json` manifest schema and validation.
//!
//! Schema URL: https://locaryn.dev/schema/plugin.json/v0.1

use locaryn_shared_types::Permission;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginManifest {
    /// URL du schéma, purement documentaire.
    ///
    /// `$schema` est la convention JSON Schema, et c'est ce qu'écrivent tous
    /// les manifestes publiés — y compris les treize extensions officielles.
    /// L'exiger sous le nom `schema` faisait échouer leur installation avec
    /// « missing field `schema` ». On accepte les deux, et son absence : un
    /// champ qui ne sert qu'à aider les éditeurs de texte n'a pas à bloquer
    /// une installation.
    #[serde(default, alias = "$schema")]
    pub schema: String,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default, rename = "minLocarynVersion")]
    pub min_locaryn_version: Option<String>,
    #[serde(default)]
    pub permissions: PermissionsMap,
    #[serde(default)]
    pub components: Components,
    #[serde(default)]
    pub deps: Vec<Dep>,
    #[serde(default)]
    pub config: Option<ConfigSchema>,
    /// Ce que l'extension apporte comme fonction, en mots que l'interface
    /// comprend : `image-gen`, `voice-tts`, `model-training`…
    ///
    /// C'est ce qui décide de la présence d'un écran : le Studio de génération
    /// n'existe que si une extension installée sait générer quelque chose.
    /// Retirer l'extension retire l'écran.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Ce que l'extension ajoute à l'interface.
    #[serde(default, rename = "ui_contributions", alias = "uiContributions")]
    pub ui: UiContributions,
    /// Quand elle est présente, l'extension est un **noyau** : Locaryn la
    /// pilote (cycle de vie, healthcheck, sessions) au lieu d'utiliser son
    /// propre agent. Le noyau Locaryn n'est jamais remplacé — une session
    /// choisit le sien via `sessions.core_id`.
    #[serde(default)]
    pub core: Option<CoreManifest>,
}

/// Section `core` d'un manifeste de noyau.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(clippy::doc_markdown)]
pub struct CoreManifest {
    /// Dialecte piloté : `responses` (OpenResponses d'OpenClaw), `runs`
    /// (Runs API d'Hermes), `chat_completions` (générique).
    #[serde(default)]
    pub driver: String,
    /// URL de base de l'API du noyau (loopback obligatoire).
    #[serde(default)]
    pub api_url: String,
    /// Port local attendu.
    #[serde(default)]
    pub port: u16,
    /// Modèle annoncé par défaut (ex. `hermes-agent`, `openclaw`).
    #[serde(default)]
    pub model: Option<String>,
    /// Comment démarrer, superviser et sonder le noyau.
    #[serde(default)]
    pub lifecycle: CoreLifecycle,
    /// Skills de l'écosystème du noyau (ClawHub, Hermes…).
    #[serde(default)]
    pub skills: CoreSkills,
    /// Partage des outils entre Locaryn et le noyau.
    #[serde(default)]
    pub tools: CoreTools,
    /// Routage des sessions Locaryn → sessions du noyau.
    #[serde(default)]
    pub session: CoreSession,
}

/// Cycle de vie : commande de lancement, environnement, sonde de santé.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(clippy::doc_markdown)]
pub struct CoreLifecycle {
    /// Liste d'arguments, jamais un shell. `{{port}}` et `{{token}}` y sont
    /// remplacés à l'exécution.
    #[serde(default)]
    pub start: Vec<String>,
    /// Variables d'environnement injectées au lancement (`{{port}}`,
    /// `{{token}}` remplacés).
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Sonde de santé : attendue avant de déclarer le noyau « en marche ».
    #[serde(default)]
    pub health: Option<CoreHealth>,
}

/// Sonde de santé HTTP.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(clippy::doc_markdown)]
pub struct CoreHealth {
    #[serde(default = "methode_par_defaut")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default = "essais_par_defaut")]
    pub retries: u32,
    #[serde(default = "intervalle_par_defaut")]
    pub interval_ms: u64,
}

fn methode_par_defaut() -> String {
    "GET".to_string()
}

fn essais_par_defaut() -> u32 {
    30
}

fn intervalle_par_defaut() -> u64 {
    1000
}

/// Partage des outils entre Locaryn et le noyau.
///
/// Décision D1 (doc 14 §9) : par défaut le pont ne déclare **aucun** outil
/// Locaryn au noyau — OpenClaw et Hermes ont déjà leurs propres outils
/// terminal/fichiers, et déclarer les deux donnerait au modèle deux chemins
/// pour la même action avec deux politiques. `client_tools: true` reste
/// possible en opt-in (ex. exposer un MCP serveur Locaryn au noyau).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::doc_markdown)]
pub struct CoreTools {
    /// Déclarer les outils Locaryn (read_file, write_file, search,
    /// run_command, outils MCP) au noyau comme outils **client**.
    /// `false` par défaut (D1).
    #[serde(default)]
    pub client_tools: bool,
    /// Qui arbitre les appels d'outils en attente : `locaryn` (le gating
    /// existant, modal d'approbation) ou `core` (le noyau décide, Locaryn
    /// n'affiche que la progression).
    #[serde(default = "approbation_par_defaut")]
    pub approval: String,
    /// Noms des événements de progression du noyau à traduire en cartes
    /// d'outil (ex. `hermes.tool.progress`).
    #[serde(default)]
    pub progress_events: Vec<String>,
}

fn approbation_par_defaut() -> String {
    "locaryn".to_string()
}

// `#[derive(Default)]` est piégé ici : quand la clé `tools` est absente du
// JSON, serde construit `CoreTools::default()` — et le derive mettrait
// `approval = ""` au lieu de `"locaryn"`. Le `Default` manuel garde les
// mêmes valeurs que les attributs serde, dans les deux chemins.
impl Default for CoreTools {
    fn default() -> Self {
        Self {
            client_tools: false,
            approval: "locaryn".to_string(),
            progress_events: Vec::new(),
        }
    }
}

/// Routage des sessions Locaryn vers les sessions du noyau.
///
/// Chaque session Locaryn reçoit une clé stable `locaryn-{uuid}` ; le champ
/// du protocole qui la porte dépend du dialecte :
///
/// - `user` : champ `user` (OpenResponses) ou `session_id` (Runs) ;
/// - `conversation` : nom de conversation stable (Runs API d'Hermes) ;
/// - `response` : chaînage par `previous_response_id` (état porté par le
///   pont, pas par le client).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::doc_markdown)]
pub struct CoreSession {
    /// `user` (défaut), `conversation` ou `response`.
    #[serde(default = "routage_par_defaut")]
    pub routing: String,
    /// Nombre maximal de sessions noyau simultanées par noyau.
    #[serde(default)]
    pub max_sessions: u32,
    /// Sessions éphémères par défaut (clé jetable, D9) — les conversations
    /// sans projet ne laissent pas de trace chez le noyau.
    #[serde(default)]
    pub stateless_by_default: bool,
}

fn routage_par_defaut() -> String {
    "user".to_string()
}

// Même piège que `CoreTools` : sans la clé `session` dans le JSON, serde
// appelle `CoreSession::default()` — le derive mettrait `routing = ""` et
// le pont basculerait silencieusement sur `conversation`. Le `Default`
// manuel garantit `user` dans tous les chemins.
impl Default for CoreSession {
    fn default() -> Self {
        Self {
            routing: "user".to_string(),
            max_sessions: 0,
            stateless_by_default: false,
        }
    }
}

/// Skills de l'écosystème du noyau.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(clippy::doc_markdown)]
pub struct CoreSkills {
    /// Registre d'origine (`clawhub`, `hermes`, `folder`).
    #[serde(default)]
    pub registry: String,
    /// Chemin (relatif au dossier de l'extension) d'un index JSON de skills
    /// (`{ "skills": [{ "slug", "name", "description", "verified" }] }`).
    #[serde(default)]
    pub index: Option<String>,
    /// Commande d'installation d'un skill, `{{slug}}` à remplacer. Exécutée
    /// sans shell, avec la permission `shell` de l'extension.
    #[serde(default)]
    pub install: Option<String>,
    /// Dossier où le noyau lit ses skills (affiché dans l'interface).
    #[serde(default)]
    pub install_dir: Option<String>,
}

/// Entrées d'interface apportées par une extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiContributions {
    /// Points d'extension et composants dynamiques (Slots universels).
    #[serde(default)]
    pub slots: Vec<UiSlotContribution>,
    /// Entrées de navigation de premier niveau.
    #[serde(default, rename = "nav_items", alias = "navItems")]
    pub nav_items: Vec<UiEntry>,
    /// Onglets à l'intérieur du Studio de génération.
    #[serde(default, rename = "studio_tabs", alias = "studioTabs")]
    pub studio_tabs: Vec<UiEntry>,
    /// Boutons posés à côté du champ de saisie — un micro pour dicter, un
    /// modèle de demande à insérer. Ils suivent l'extension : sur
    /// l'ordinateur comme sur le téléphone.
    #[serde(default, rename = "composer_actions", alias = "composerActions")]
    pub composer_actions: Vec<ComposerAction>,
    /// Sections ajoutées à l'écran des réglages, avec leurs champs. C'est là
    /// qu'une extension fait choisir son modèle, sa langue, sa voix.
    #[serde(default, rename = "settings_sections", alias = "settingsSections")]
    pub settings_sections: Vec<SettingsSection>,
}

/// Contribution d'un composant ou d'une action à un point d'extension (Slot) de l'interface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiSlotContribution {
    pub id: String,
    /// Nom du slot cible (ex: `composer.before_send`, `composer.toolbar`, `topbar.actions`,
    /// `studio.tabs`, `marketplace.categories`, `engines.runtimes`, `nav.drawer`).
    pub slot: String,
    /// Priorité d'ordre d'affichage (ex: 10, 50, 100).
    #[serde(default = "ordre_par_defaut")]
    pub order: i32,
    /// Type de rendu : `button`, `action`, `custom-element`, `iframe`, `modal`.
    #[serde(default = "type_slot_par_defaut", rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
    /// Action : `insert`, `tool`, `event`, `view`, `script`.
    #[serde(default)]
    pub action: Option<String>,
    /// Valeur associée à l'action.
    #[serde(default)]
    pub value: Option<String>,
    /// Fichier d'entrée pour les scripts ou interfaces personnalisées (ex: `dist/ui.js`).
    #[serde(default)]
    pub entry: Option<String>,
    /// Nom de balise custom-element (ex: `locaryn-dictaphone-btn`).
    #[serde(default)]
    pub tag: Option<String>,
    /// Catégorie ou domaine (ex: `image`, `audio`, `video`).
    #[serde(default)]
    pub category: Option<String>,
    /// Surfaces visées : `desktop`, `mobile`, `web`. Vide = toutes.
    ///
    /// Deux contributions au même slot, chacune avec ses plateformes, donnent
    /// deux formes du même écran — un panneau large sur l'ordinateur, autre
    /// chose sur le téléphone.
    #[serde(default)]
    pub platforms: Vec<String>,
}

fn ordre_par_defaut() -> i32 {
    100
}

fn type_slot_par_defaut() -> String {
    "button".to_string()
}

/// Un bouton à côté du champ de saisie.
///
/// Deux comportements, pas plus. `insert` écrit un texte dans le champ : un
/// modèle de demande, une consigne récurrente. `tool` appelle un outil de
/// l'extension avec ce que contient le champ, et met la réponse à la place.
/// Tout le reste demanderait de faire tourner du code de l'extension dans
/// l'interface, ce qu'aucune extension ne devrait pouvoir faire.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposerAction {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// `insert` ou `tool`.
    #[serde(default = "action_par_defaut")]
    pub action: String,
    /// Le texte à insérer, ou le nom de l'outil à appeler.
    #[serde(default)]
    pub value: String,
    /// Ce que le bouton fait, dit à la personne au survol.
    #[serde(default)]
    pub hint: Option<String>,
}

fn action_par_defaut() -> String {
    "insert".to_string()
}

/// Une section de réglages, telle que l'extension la décrit.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsSection {
    pub id: String,
    /// `label` est la forme documentée ; `title` est acceptée aussi, parce que
    /// c'est le mot qu'écrivent la moitié des manifestes existants.
    #[serde(alias = "label")]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<SettingsField>,
}

/// Un réglage. `model` fait choisir parmi les modèles installés — c'est le cas
/// le plus fréquent, et celui qu'une extension ne peut pas remplir seule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsField {
    /// `id` dans la documentation ; `key` est acceptée de la même façon.
    #[serde(alias = "id")]
    pub key: String,
    pub label: String,
    /// `boolean`, `string`, `number`, `select`, `model` ou `prompt`.
    ///
    /// Ces mots sont ceux de la documentation. Ils sont ramenés à quatre
    /// rendus au moment de l'affichage : un interrupteur, une liste, un choix
    /// de modèle, ou du texte — un champ numérique et une zone multiligne
    /// restent du texte, et prétendre le contraire ferait une promesse que
    /// l'écran ne tiendrait pas.
    #[serde(default = "champ_par_defaut", alias = "type")]
    pub kind: String,
    #[serde(default)]
    pub hint: Option<String>,
    /// Les valeurs offertes, pour `choice`.
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
}

fn champ_par_defaut() -> String {
    "text".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiEntry {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<String>,
}

/// Permissions can be `false` (explicit no), `true` (request with defaults),
/// or an object with reason/scope/requireApproval.
pub type PermissionsMap = std::collections::HashMap<String, PermissionValue>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionValue {
    Bool(bool),
    Object(PermissionRequest),
    /// List of env var names (for `env`).
    EnvList(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub reason: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default, rename = "requireApproval")]
    pub require_approval: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Components {
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub hooks: Option<String>,
    #[serde(default)]
    pub mcp: Option<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub lsp: Option<String>,
}

impl Components {
    /// True when the bundle contributes nothing Locaryn can load.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
            && self.commands.is_empty()
            && self.agents.is_empty()
            && self.rules.is_empty()
            && self.hooks.is_none()
            && self.mcp.is_none()
            && self.lsp.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dep {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub schema: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

/// Load and validate a `plugin.json` from `dir/plugin.json`.
pub fn load(dir: &Path) -> Result<PluginManifest, ManifestError> {
    let path = dir.join("plugin.json");
    let raw = std::fs::read_to_string(&path)?;
    let m: PluginManifest = serde_json::from_str(&raw)?;
    validate(&m)?;
    Ok(m)
}

/// Validate the manifest invariants.
pub fn validate(m: &PluginManifest) -> Result<(), ManifestError> {
    if m.name.is_empty() {
        return Err(ManifestError::Invalid("name is required".into()));
    }
    if m.version.is_empty() {
        return Err(ManifestError::Invalid("version is required".into()));
    }
    if m.api_version.is_empty() {
        return Err(ManifestError::Invalid("apiVersion is required".into()));
    }
    // apiVersion must be a known Locaryn extension API version.
    if !is_supported_api_version(&m.api_version) {
        return Err(ManifestError::Invalid(format!(
            "unsupported apiVersion {} (supported: 0.1)",
            m.api_version
        )));
    }
    // Names must be lowercase kebab-ish.
    if !m
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(ManifestError::Invalid(
            "name must be lowercase ascii, digits, '-' or '_'".into(),
        ));
    }
    Ok(())
}

/// Locaryn extension API versions this build supports.
pub fn supported_api_versions() -> &'static [&'static str] {
    &["0.1"]
}

pub fn is_supported_api_version(v: &str) -> bool {
    supported_api_versions().contains(&v)
}

/// Enumerate the permissions requested by the manifest, paired with the
/// `Permission` variant each request corresponds to (so the registry can
/// match a request to a granted permission end-to-end).
pub fn requested_permissions(m: &PluginManifest) -> Vec<(Permission, PermissionRequest)> {
    let mut out = Vec::new();
    for (name, val) in &m.permissions {
        let perm = match name.as_str() {
            "shell" => Some(Permission::Shell),
            "files.read" => Some(Permission::FilesRead),
            "files.write" => Some(Permission::FilesWrite),
            "network" => Some(Permission::Network),
            "extensions" => Some(Permission::Extensions),
            "mcp" => Some(Permission::Mcp),
            "preview" => Some(Permission::Preview),
            "lsp" => Some(Permission::Lsp),
            "env" => Some(Permission::Env),
            _ => None,
        };
        let Some(p) = perm else { continue };
        let req = match val {
            PermissionValue::Bool(false) => continue,
            PermissionValue::Bool(true) => PermissionRequest {
                reason: None,
                scope: None,
                require_approval: None,
            },
            PermissionValue::Object(r) => r.clone(),
            PermissionValue::EnvList(_) => PermissionRequest {
                reason: Some("env access".into()),
                scope: None,
                require_approval: Some(true),
            },
        };
        out.push((p, req));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_manifest_parses() {
        let json = r#"{
            "schema":"x","apiVersion":"0.1","name":"my-plugin","version":"1.0.0",
            "permissions": { "shell": { "reason": "run tests" } }
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert!(validate(&m).is_ok());
    }

    #[test]
    fn rejects_bad_name() {
        let json = r#"{"schema":"x","apiVersion":"0.1","name":"Bad Name","version":"1"}"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert!(validate(&m).is_err());
    }

    #[test]
    fn rejects_unsupported_api() {
        let json = r#"{"schema":"x","apiVersion":"9.9","name":"ok","version":"1"}"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert!(validate(&m).is_err());
    }

    /// La documentation écrit `label`, `id` et `type` ; le code parle de
    /// `title`, `key` et `kind`. Un manifeste écrit d'après la documentation
    /// doit se lire — sinon la documentation ment.
    #[test]
    fn la_forme_documentee_se_lit() {
        let json = r#"{
            "schema": "x", "apiVersion": "0.1", "name": "dictee", "version": "1",
            "ui_contributions": {
                "composer_actions": [
                    { "id": "dictate", "label": "Dicter", "icon": "mic",
                      "action": "tool", "value": "transcribe_audio" }
                ],
                "settings_sections": [{
                    "id": "dictee", "label": "Dictée",
                    "fields": [
                        { "id": "model", "type": "model", "label": "Modèle d'écoute" },
                        { "id": "auto_send", "type": "boolean", "label": "Envoyer après" }
                    ]
                }]
            }
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert!(validate(&m).is_ok());

        let bouton = &m.ui.composer_actions[0];
        assert_eq!(bouton.id, "dictate");
        assert_eq!(bouton.action, "tool");
        assert_eq!(bouton.value, "transcribe_audio");

        let section = &m.ui.settings_sections[0];
        assert_eq!(section.title, "Dictée", "`label` vaut `title`");
        assert_eq!(section.fields[0].key, "model", "`id` vaut `key`");
        assert_eq!(section.fields[0].kind, "model", "`type` vaut `kind`");
        assert_eq!(section.fields[1].kind, "boolean");
    }

    /// Un bouton sans `action` ne doit pas appeler un outil par surprise.
    #[test]
    fn un_bouton_sans_action_se_contente_d_inserer() {
        let json = r#"{
            "schema": "x", "apiVersion": "0.1", "name": "modeles", "version": "1",
            "ui_contributions": {
                "composer_actions": [{ "id": "revue", "label": "Revue", "value": "Relis ce code" }]
            }
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.ui.composer_actions[0].action, "insert");
    }

    /// La section `core` d'un noyau : tools et session ont des défauts
    /// sûrs — pas d'outils client déclarés (D1), approbation Locaryn,
    /// routage `user`.
    #[test]
    fn une_section_core_avec_tools_et_session_se_lit() {
        let json = r#"{
            "schema": "x", "apiVersion": "0.1", "name": "noyau", "version": "1",
            "core": {
                "driver": "runs",
                "api_url": "http://127.0.0.1:8642",
                "port": 8642,
                "tools": { "client_tools": false, "approval": "core" },
                "session": { "routing": "conversation", "max_sessions": 20 }
            }
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        let c = m.core.expect("la section core doit être lue");
        assert_eq!(c.driver, "runs");
        assert!(!c.tools.client_tools);
        assert_eq!(c.tools.approval, "core");
        assert_eq!(c.session.routing, "conversation");
        assert_eq!(c.session.max_sessions, 20);
    }

    #[test]
    fn les_defauts_d_une_section_core_sont_surs() {
        let json = r#"{
            "schema": "x", "apiVersion": "0.1", "name": "noyau", "version": "1",
            "core": { "driver": "responses", "api_url": "http://127.0.0.1:1", "port": 1 }
        }"#;
        let m: PluginManifest = serde_json::from_str(json).unwrap();
        let c = m.core.unwrap();
        assert!(
            !c.tools.client_tools,
            "D1 : pas d'outils Locaryn par défaut"
        );
        assert_eq!(c.tools.approval, "locaryn");
        assert_eq!(c.session.routing, "user");
        assert_eq!(c.session.max_sessions, 0);
    }
}
