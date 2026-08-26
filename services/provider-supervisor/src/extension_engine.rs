//! Moteurs d'inférence apportés par une extension.
//!
//! Le superviseur sait déjà lancer, sonder et arrêter un runtime local ; ce
//! module lui apprend à le faire pour un runtime qu'il ne connaît pas — celui
//! qu'une extension décrit dans la section `engine` de son manifeste.
//!
//! Rien ici ne nomme un moteur en particulier. L'application lit une liste
//! d'arguments, y substitue des chemins qu'elle seule connaît (les poids de
//! l'utilisateur, le dossier privé de l'extension, le port), lance le
//! processus et attend la sonde de santé. Ce qui est propre à un moteur — un
//! passage par WSL, une conversion de checkpoint, un choix de backend —
//! appartient au programme que l'extension livre dans son `bin/`.

use locaryn_extensions::hostpaths;
use locaryn_extensions::manifest::{EngineManifest, PluginManifest};
use locaryn_shared_types::ProviderEngine;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

use crate::SupervisorError;

/// Un moteur d'extension tel que l'hôte l'a installé : ce que le manifeste
/// déclare, plus les chemins réels de cette machine.
///
/// L'hôte (bureau ou daemon) construit cette description à partir du registre
/// d'extensions ; le superviseur ne lit jamais le disque des extensions
/// lui-même.
#[derive(Debug, Clone)]
pub struct ExtensionEngineSpec {
    /// Identifiant du moteur — la partie après `ext:` dans le jeton.
    pub id: String,
    /// Nom affiché. Jamais utilisé pour décider quoi que ce soit.
    pub label: String,
    /// Version de l'extension qui l'apporte, pour les journaux.
    pub extension_version: String,
    /// La section `engine` du manifeste, telle qu'écrite par l'auteur.
    pub manifest: EngineManifest,
    /// Racine de l'extension installée (`LOCARYN_PLUGIN_ROOT`).
    pub plugin_root: PathBuf,
    /// Dossier privé de l'extension (`LOCARYN_EXTENSION_DATA_DIR`).
    pub extension_data_dir: PathBuf,
    /// Nom de l'extension qui l'apporte — ce que l'interface montre quand un
    /// moteur pose problème, et la clé de son dossier privé.
    pub extension_name: String,
}

impl ExtensionEngineSpec {
    /// Construit la description d'un moteur depuis un manifeste installé.
    ///
    /// `None` quand l'extension n'apporte pas de moteur, ou quand sa section
    /// `engine` ne passe pas la validation — un manifeste refusé est signalé
    /// dans le journal plutôt que lancé à moitié.
    pub fn from_manifest(manifest: &PluginManifest, plugin_root: &Path) -> Option<Self> {
        let engine = manifest.engine.as_ref()?;
        if let Err(e) = locaryn_extensions::manifest::validate_engine(engine, &manifest.name) {
            tracing::warn!(
                extension = %manifest.name,
                erreur = %e,
                "section engine invalide — le moteur n'est pas proposé"
            );
            return None;
        }
        let id = if engine.id.is_empty() {
            manifest.name.clone()
        } else {
            engine.id.clone()
        };
        let label = engine
            .label
            .clone()
            .unwrap_or_else(|| id.clone());
        Some(Self {
            id,
            label,
            extension_version: manifest.version.clone(),
            manifest: engine.clone(),
            plugin_root: plugin_root.to_path_buf(),
            extension_data_dir: hostpaths::extension_data_dir(&manifest.name),
            extension_name: manifest.name.clone(),
        })
    }

    /// Le moteur, sous la forme que le reste de l'application manipule.
    pub fn engine(&self) -> ProviderEngine {
        ProviderEngine::Extension(self.id.clone())
    }

    /// URL de base du serveur, sans chemin.
    pub fn endpoint(&self) -> String {
        self.manifest
            .api_url
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_string()
    }

    /// URL de la sonde de santé. À défaut de sonde déclarée, `/v1/models` —
    /// ce que sert tout dialecte compatible OpenAI.
    pub fn health_url(&self) -> String {
        match &self.manifest.lifecycle.health {
            Some(h) if !h.url.is_empty() => substitute(&h.url, &self.placeholders(None)),
            _ => format!("{}/v1/models", self.endpoint()),
        }
    }

    /// Combien de temps laisser au premier chargement.
    pub fn startup_timeout(&self) -> std::time::Duration {
        let secs = if self.manifest.startup_timeout_secs == 0 {
            300
        } else {
            self.manifest.startup_timeout_secs
        };
        std::time::Duration::from_secs(secs)
    }

    /// Ce moteur sait-il servir ce modèle ?
    ///
    /// C'est la question que pose l'écran des modèles : un répertoire de
    /// shards safetensors n'est un modèle de conversation *que* si un moteur
    /// installé sait le charger.
    ///
    /// `model` est ce que la base enregistre — un nom relatif à la
    /// bibliothèque de poids, un chemin absolu, ou un identifiant de dépôt
    /// Hugging Face. La résolution se fait ici, une fois, plutôt que chez
    /// chaque appelant.
    pub fn serves_model(&self, model: &str) -> bool {
        let formats = &self.manifest.model_formats;
        let m = model.trim();
        if m.is_empty() {
            return false;
        }
        let lower = m.to_ascii_lowercase();
        if formats.files.iter().any(|ext| {
            lower.ends_with(&format!(
                ".{}",
                ext.trim_start_matches('.').to_ascii_lowercase()
            ))
        }) {
            return true;
        }
        if formats.directories {
            if let Some(dir) = self.resolve_model_path(m) {
                if dir.is_dir() && self.directory_looks_servable(&dir) {
                    return true;
                }
            }
        }
        if formats.hf_repo_ids && is_hf_repo_id(m) {
            return true;
        }
        false
    }

    /// Où se trouve ce modèle sur le disque, si c'est un chemin ou un nom de
    /// la bibliothèque de poids. `None` pour un identifiant de dépôt distant.
    pub fn resolve_model_path(&self, model: &str) -> Option<PathBuf> {
        let m = model.trim();
        if m.is_empty() {
            return None;
        }
        let direct = Path::new(m);
        if direct.is_absolute() {
            return Some(direct.to_path_buf());
        }
        let dans_la_bibliotheque = locaryn_config::models_dir().join(m);
        if dans_la_bibliotheque.exists() {
            return Some(dans_la_bibliotheque);
        }
        let prive = self.extension_data_dir.join("models").join(m);
        if prive.exists() {
            return Some(prive);
        }
        None
    }

    /// Un répertoire de checkpoint porte des fichiers qui le signent. Sans
    /// marqueur déclaré, tout répertoire compte — l'auteur du moteur a choisi
    /// de ne pas trier.
    fn directory_looks_servable(&self, dir: &Path) -> bool {
        let markers = &self.manifest.model_formats.directory_markers;
        if markers.is_empty() {
            return true;
        }
        markers.iter().any(|m| dir.join(m).exists())
    }

    /// Les substitutions disponibles dans `lifecycle.start` et
    /// `lifecycle.env`.
    fn placeholders(&self, model: Option<&str>) -> HashMap<&'static str, String> {
        let mut map = HashMap::new();
        map.insert("port", self.manifest.port.to_string());
        map.insert("model", model.unwrap_or_default().to_string());
        // Le chemin résolu quand le modèle est sur le disque, le nom brut
        // sinon : un moteur qui télécharge lui-même reçoit l'identifiant de
        // dépôt tel quel, sans qu'on lui fabrique un chemin qui n'existe pas.
        map.insert(
            "model_path",
            model
                .and_then(|m| self.resolve_model_path(m))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| model.unwrap_or_default().to_string()),
        );
        map.insert("api_url", self.manifest.api_url.clone());
        map.insert("endpoint", self.endpoint());
        map.insert("models_url", format!("{}/v1/models", self.endpoint()));
        map.insert(
            "plugin_root",
            self.plugin_root.to_string_lossy().into_owned(),
        );
        map.insert(
            "plugin_bin_dir",
            self.plugin_root.join("bin").to_string_lossy().into_owned(),
        );
        map.insert(
            "extension_data_dir",
            self.extension_data_dir.to_string_lossy().into_owned(),
        );
        map.insert(
            "extension_models_dir",
            self.extension_data_dir
                .join("models")
                .to_string_lossy()
                .into_owned(),
        );
        map.insert(
            "models_dir",
            locaryn_config::models_dir().to_string_lossy().into_owned(),
        );
        map.insert(
            "data_dir",
            locaryn_config::storage_root()
                .to_string_lossy()
                .into_owned(),
        );
        map.insert(
            "hf_cache_dir",
            locaryn_config::hf_cache_dir()
                .to_string_lossy()
                .into_owned(),
        );
        map.insert(
            "temp_dir",
            locaryn_config::ensure_temp_dir()
                .to_string_lossy()
                .into_owned(),
        );
        map
    }

    /// Ce que l'auteur exige de la machine, et ce que cette machine offre.
    /// Renvoie la phrase à montrer quand ça ne correspond pas.
    pub fn unmet_requirement(&self) -> Option<String> {
        let req = &self.manifest.requires;
        if !req.os.is_empty() {
            let here = if cfg!(target_os = "windows") {
                "windows"
            } else if cfg!(target_os = "macos") {
                "macos"
            } else {
                "linux"
            };
            if !req.os.iter().any(|o| o.eq_ignore_ascii_case(here)) {
                let mut phrase = format!(
                    "« {} » tourne sur {} — pas sur {here}.",
                    self.label,
                    req.os.join(", ")
                );
                if let Some(note) = &req.note {
                    phrase.push(' ');
                    phrase.push_str(note);
                }
                return Some(phrase);
            }
        }
        None
    }
}

/// Un identifiant de dépôt Hugging Face : `propriétaire/nom`, sans schéma
/// d'URL, sans séparateur de chemin de la plateforme.
fn is_hf_repo_id(value: &str) -> bool {
    if value.contains("://") || value.contains('\\') || value.contains(' ') {
        return false;
    }
    // Un chemin absolu POSIX (`/home/…`) n'est pas un identifiant de dépôt.
    if value.starts_with('/') || value.starts_with('.') {
        return false;
    }
    // Un chemin Windows (`D:\…` déjà exclu, `D:/…` ici).
    if value.len() > 2 && value.as_bytes()[1] == b':' {
        return false;
    }
    let mut parts = value.split('/');
    let owner = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    parts.next().is_none()
        && !owner.is_empty()
        && !name.is_empty()
        && owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Remplace `{{clé}}` par sa valeur. Une clé inconnue est laissée telle
/// quelle : c'est visible dans le journal de lancement, alors qu'une chaîne
/// vide silencieuse ferait échouer le moteur sur un argument manquant.
fn substitute(template: &str, values: &HashMap<&'static str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in values {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}

/// Lance le serveur d'un moteur d'extension.
///
/// Jamais de shell : la première entrée de `lifecycle.start` est le programme,
/// les suivantes ses arguments (décision D12, doc 14). Les journaux du
/// processus vont dans un fichier nommé d'après le moteur, à côté de ceux des
/// runtimes intégrés — c'est là que l'utilisateur ira chercher pourquoi ça n'a
/// pas démarré.
pub async fn spawn(
    spec: &ExtensionEngineSpec,
    model: Option<&str>,
) -> Result<Child, SupervisorError> {
    let engine = spec.engine();

    if let Some(raison) = spec.unmet_requirement() {
        return Err(SupervisorError::SpawnFailed(engine, raison));
    }

    if spec.manifest.lifecycle.requires_model && model.unwrap_or("").trim().is_empty() {
        return Err(SupervisorError::SpawnFailed(
            engine,
            format!(
                "« {} » ne démarre pas sans modèle : choisissez-en un dans Réglages → Moteur.",
                spec.label
            ),
        ));
    }

    let values = spec.placeholders(model);
    let mut argv = spec
        .manifest
        .lifecycle
        .start
        .iter()
        .map(|a| substitute(a, &values))
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>();
    if argv.is_empty() {
        return Err(SupervisorError::SpawnFailed(
            engine,
            "lifecycle.start est vide après substitution".into(),
        ));
    }
    let program = argv.remove(0);

    // Le programme peut être livré dans le `bin/` de l'extension, ou attendu
    // sur le chemin du système (`install.kind = existing`). `resolve_program`
    // fait le même travail que pour les autres commandes de l'application :
    // il ajoute l'extension d'exécutable de la plateforme quand il faut.
    let resolved = if Path::new(&program).is_absolute() {
        std::ffi::OsString::from(&program)
    } else {
        locaryn_config::resolve_program(&program)
    };

    tracing::info!(
        moteur = %spec.id,
        programme = %resolved.to_string_lossy(),
        modele = model.unwrap_or("(aucun)"),
        "démarrage d'un moteur d'extension"
    );

    let mut cmd = Command::new(&resolved);
    cmd.args(&argv).stdin(Stdio::null());
    for (key, value) in &spec.manifest.lifecycle.env {
        cmd.env(key, substitute(value, &values));
    }
    // Les mêmes chemins qu'un serveur MCP de la même extension reçoit — donc
    // le même dossier privé. Deux calculs différents donneraient au lanceur du
    // moteur et au serveur de l'extension deux dossiers d'état distincts.
    for (key, value) in hostpaths::generic_env(&spec.extension_name, &spec.plugin_root) {
        cmd.env(key, value);
    }

    let log_path = log_file_path(&spec.id);
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::File::create(&log_path) {
        Ok(f) => {
            let err = f.try_clone().ok();
            cmd.stdout(Stdio::from(f));
            match err {
                Some(e) => cmd.stderr(Stdio::from(e)),
                None => cmd.stderr(Stdio::null()),
            };
        }
        Err(e) => {
            tracing::warn!(
                moteur = %spec.id,
                chemin = %log_path.display(),
                erreur = %e,
                "journal du moteur impossible à ouvrir — la sortie est perdue"
            );
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }

    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP : pas de console qui
        // clignote, et un groupe à tuer d'un bloc à l'arrêt.
        cmd.creation_flags(0x0800_0008);
    }

    cmd.spawn().map_err(|e| {
        SupervisorError::SpawnFailed(
            engine,
            format!(
                "{} : {e} — vérifiez que « {} » est installé ({})",
                resolved.to_string_lossy(),
                spec.manifest
                    .install
                    .package
                    .clone()
                    .unwrap_or_else(|| program.clone()),
                describe_install(&spec.manifest)
            ),
        )
    })
}

/// Les moteurs apportés par une liste d'extensions installées.
///
/// L'hôte passe ce qu'il a en base — le chemin du manifeste, et si l'extension
/// est active. Le chargement, la validation et le calcul des chemins vivent
/// ici, une seule fois pour le bureau et le daemon.
///
/// Une extension **désactivée** n'apporte pas son moteur : son processus ne
/// doit pas pouvoir démarrer alors que l'utilisateur l'a éteinte.
pub fn collect(sources: &[EngineSource]) -> Vec<ExtensionEngineSpec> {
    let mut out: Vec<ExtensionEngineSpec> = Vec::new();
    for source in sources.iter().filter(|s| s.enabled) {
        let Some(root) = hostpaths::plugin_root(&source.manifest_path) else {
            continue;
        };
        let manifest = match locaryn_extensions::manifest::load(&root) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(
                    dossier = %root.display(),
                    erreur = %e,
                    "manifeste illisible — extension ignorée pour les moteurs"
                );
                continue;
            }
        };
        let Some(spec) = ExtensionEngineSpec::from_manifest(&manifest, &root) else {
            continue;
        };
        // Deux extensions qui revendiquent le même identifiant de moteur : la
        // première installée garde la main, et la seconde est nommée dans le
        // journal. Se taire ferait apparaître un moteur qui répond pour un
        // autre.
        if let Some(deja) = out.iter().find(|s| s.id == spec.id) {
            tracing::warn!(
                moteur = %spec.id,
                garde = %deja.extension_name,
                ignoree = %spec.extension_name,
                "deux extensions déclarent le même moteur"
            );
            continue;
        }
        out.push(spec);
    }
    out
}

/// Une extension installée, vue du registre de l'hôte.
#[derive(Debug, Clone)]
pub struct EngineSource {
    pub manifest_path: PathBuf,
    pub enabled: bool,
}

/// Où le journal d'un moteur d'extension est écrit.
pub fn log_file_path(id: &str) -> PathBuf {
    locaryn_config::default_data_dir().join(format!("engine-{id}.log"))
}

/// Une phrase d'installation, pour que le message d'erreur dise quoi faire.
fn describe_install(m: &EngineManifest) -> String {
    match m.install.kind.as_str() {
        "pip" => {
            let pkg = m.install.package.clone().unwrap_or_default();
            let extras = if m.install.extras.is_empty() {
                String::new()
            } else {
                format!("[{}]", m.install.extras.join(","))
            };
            let version = m
                .install
                .version
                .as_ref()
                .map(|v| format!("=={v}"))
                .unwrap_or_default();
            format!("pip install \"{pkg}{extras}{version}\"")
        }
        "npm" => format!(
            "npm install -g {}",
            m.install.package.clone().unwrap_or_default()
        ),
        "binary" => "installez le runtime depuis la fiche de l'extension".to_string(),
        _ => "l'extension attend que le programme soit déjà sur le chemin".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use locaryn_extensions::manifest::{EngineLifecycle, EngineModelFormats};

    fn spec(formats: EngineModelFormats) -> ExtensionEngineSpec {
        ExtensionEngineSpec {
            id: "essai".into(),
            label: "Essai".into(),
            extension_version: "1.0.0".into(),
            manifest: EngineManifest {
                id: "essai".into(),
                driver: "openai_compat".into(),
                api_url: "http://127.0.0.1:1919".into(),
                port: 1919,
                model_formats: formats,
                lifecycle: EngineLifecycle {
                    start: vec!["ft".into(), "serve".into(), "--model".into(), "{{model}}".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
            plugin_root: PathBuf::from("/tmp/plugin"),
            extension_data_dir: PathBuf::from("/tmp/data"),
            extension_name: "plugin-essai".into(),
        }
    }

    #[test]
    fn l_endpoint_perd_le_chemin_de_l_api() {
        let mut s = spec(EngineModelFormats::default());
        s.manifest.api_url = "http://127.0.0.1:1919/v1/".into();
        assert_eq!(s.endpoint(), "http://127.0.0.1:1919");
        assert_eq!(s.health_url(), "http://127.0.0.1:1919/v1/models");
    }

    /// Un moteur qui n'annonce que le safetensors ne doit pas revendiquer un
    /// GGUF : c'est ce qui remettrait un modèle sur le mauvais runtime.
    #[test]
    fn les_formats_declares_font_l_eligibilite() {
        let s = spec(EngineModelFormats {
            files: vec!["safetensors".into()],
            hf_repo_ids: true,
            ..Default::default()
        });
        assert!(s.serves_model("modele.safetensors"));
        assert!(!s.serves_model("modele.gguf"));
        assert!(s.serves_model("Qwen/Qwen3.6-35B-A3B"));
        assert!(!s.serves_model(""));
    }

    #[test]
    fn un_chemin_n_est_pas_un_depot_hugging_face() {
        assert!(is_hf_repo_id("Qwen/Qwen3.6-35B-A3B"));
        assert!(is_hf_repo_id("nvidia/GLM-5.2-NVFP4"));
        assert!(!is_hf_repo_id("/home/moi/modeles/qwen"));
        assert!(!is_hf_repo_id("D:/modeles/qwen"));
        assert!(!is_hf_repo_id("D:\\modeles\\qwen"));
        assert!(!is_hf_repo_id("https://huggingface.co/Qwen/Qwen3.6-35B-A3B"));
        assert!(!is_hf_repo_id("Qwen/Qwen3.6/extra"));
        assert!(!is_hf_repo_id("qwen"));
        assert!(!is_hf_repo_id("./local"));
    }

    #[test]
    fn les_substitutions_remplissent_la_ligne_de_commande() {
        let s = spec(EngineModelFormats {
            hf_repo_ids: true,
            ..Default::default()
        });
        let values = s.placeholders(Some("Qwen/Qwen3.6-35B-A3B"));
        assert_eq!(
            substitute("--model {{model}} --port {{port}}", &values),
            "--model Qwen/Qwen3.6-35B-A3B --port 1919"
        );
    }

    /// Une clé inconnue reste visible plutôt que de devenir une chaîne vide :
    /// un argument manquant dans un journal se voit, un argument effacé non.
    #[test]
    fn une_cle_inconnue_reste_lisible() {
        let s = spec(EngineModelFormats::default());
        let values = s.placeholders(None);
        assert_eq!(substitute("{{inconnu}}", &values), "{{inconnu}}");
    }

    #[test]
    fn un_systeme_non_declare_est_refuse_avec_la_phrase_de_l_auteur() {
        let mut s = spec(EngineModelFormats {
            hf_repo_ids: true,
            ..Default::default()
        });
        s.manifest.requires.os = vec!["une-plateforme-qui-n-existe-pas".into()];
        s.manifest.requires.note = Some("Passez par WSL2.".into());
        let raison = s.unmet_requirement().expect("exigence non satisfaite");
        assert!(raison.contains("Passez par WSL2."));
    }
}
