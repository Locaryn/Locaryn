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
}

/// Entrées d'interface apportées par une extension.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiContributions {
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
}
