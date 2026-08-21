//! Extension routes — manage plugins, MCP servers, commands, skills, hooks,
//! agents, rules, and LSP adapters.
//!
//! All routes delegate to `locaryn_extensions::ExtensionRegistry` which owns
//! the in-memory state and permission bookkeeping. Persistence to SQLite is
//! planned for V1.1 (the registry already has the skeleton).
//!
//! Routes (API contract 06-api-contract.md):
//!   GET    /v1/extensions                     — list all installed
//!   POST   /v1/extensions/install             — install {source, scope}
//!   POST   /v1/extensions/{name}/enable       — enable by name
//!   POST   /v1/extensions/{name}/disable      — disable by name
//!   DELETE /v1/extensions/{name}              — uninstall by name
//!   GET    /v1/extensions/{name}/permissions  — list requested vs granted
//!   POST   /v1/extensions/{name}/permissions  — approve/deny one permission
//!   POST   /v1/extensions/reload              — hot-reload one or all
//!   GET    /v1/capabilities                   — liste canonique des capacités

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use locaryn_extensions::RegistryError;
use locaryn_mcp::McpClient;
use locaryn_shared_types::{ExtensionScope, Permission};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::DaemonState;

// ============================================================================
// Request bodies
// ============================================================================

#[derive(Deserialize)]
pub struct InstallBody {
    /// Filesystem path or download URL to the plugin directory.
    pub source: String,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "user".to_string()
}

#[derive(Deserialize)]
pub struct PermissionBody {
    pub permission: String,
    pub granted: bool,
}

#[derive(Deserialize)]
pub struct ExtensionAssetBody {
    pub extension_id: String,
    pub asset_path: String,
}

#[derive(Deserialize)]
pub struct ExtensionMediaQuery {
    pub path: String,
}

// ============================================================================
// Helpers
// ============================================================================

fn parse_scope(s: &str) -> Result<ExtensionScope, &'static str> {
    match s {
        "global" => Ok(ExtensionScope::Global),
        "user" => Ok(ExtensionScope::User),
        "workspace" => Ok(ExtensionScope::Workspace),
        "session" => Ok(ExtensionScope::Session),
        _ => Err("scope must be 'global', 'user', 'workspace', or 'session'"),
    }
}

fn parse_permission(s: &str) -> Result<Permission, &'static str> {
    match s {
        "shell" => Ok(Permission::Shell),
        "files.read" => Ok(Permission::FilesRead),
        "files.write" => Ok(Permission::FilesWrite),
        "network" => Ok(Permission::Network),
        "extensions" => Ok(Permission::Extensions),
        "mcp" => Ok(Permission::Mcp),
        "preview" => Ok(Permission::Preview),
        "lsp" => Ok(Permission::Lsp),
        "env" => Ok(Permission::Env),
        _ => Err("unknown permission — see API contract for valid values"),
    }
}

fn registry_error_response(e: RegistryError) -> (StatusCode, Json<serde_json::Value>) {
    let (code, status) = match &e {
        RegistryError::NotFound(_) => ("not_found", StatusCode::NOT_FOUND),
        RegistryError::AlreadyInstalled(_) => ("conflict", StatusCode::CONFLICT),
        RegistryError::PermissionDenied(_) => ("permission_denied", StatusCode::FORBIDDEN),
        RegistryError::Manifest(_) | RegistryError::Io(_) => {
            ("install_error", StatusCode::UNPROCESSABLE_ENTITY)
        }
    };
    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": e.to_string() }
        })),
    )
}

/// Une contribution de slot d'interface.
fn to_slot_contribution(
    s: &locaryn_extensions::manifest::UiSlotContribution,
) -> locaryn_shared_types::ExtensionUiSlotContribution {
    locaryn_shared_types::ExtensionUiSlotContribution {
        id: s.id.clone(),
        slot: s.slot.clone(),
        order: s.order,
        kind: s.kind.clone(),
        label: s.label.clone(),
        icon: s.icon.clone(),
        hint: s.hint.clone(),
        action: s.action.clone(),
        value: s.value.clone(),
        entry: s.entry.clone(),
        tag: s.tag.clone(),
        category: s.category.clone(),
        platforms: s.platforms.clone(),
    }
}

/// Un bouton de composeur, tel que l'interface l'attend.
fn to_composer_action(
    a: &locaryn_extensions::manifest::ComposerAction,
) -> locaryn_shared_types::ExtensionComposerAction {
    locaryn_shared_types::ExtensionComposerAction {
        id: a.id.clone(),
        label: a.label.clone(),
        icon: a.icon.clone(),
        // Un comportement inconnu se lit comme une insertion : au pire un
        // texte est écrit dans le champ, jamais un outil appelé par surprise.
        action: if a.action == "tool" {
            "tool".to_string()
        } else {
            "insert".to_string()
        },
        value: a.value.clone(),
        hint: a.hint.clone(),
    }
}

fn to_ui_entry(
    e: &locaryn_extensions::manifest::UiEntry,
) -> locaryn_shared_types::ExtensionUiEntry {
    locaryn_shared_types::ExtensionUiEntry {
        id: e.id.clone(),
        label: e.label.clone(),
        icon: e.icon.clone(),
    }
}

/// Ramener le type déclaré à l'un des quatre rendus possibles.
///
/// La documentation offre six mots ; l'écran n'a que quatre façons de montrer
/// un réglage. `number` et `prompt` deviennent du texte : mieux vaut un champ
/// honnête qu'un rendu promis et absent.
fn rendu_du_champ(declare: &str) -> String {
    // Le vocabulaire canonique est celui de la documentation : boolean,
    // select, model, string, number, prompt. Les anciens mots (toggle,
    // choice, text) restent acceptés et sont ramenés à leur équivalent.
    match declare {
        "boolean" | "toggle" => "boolean",
        "select" | "choice" => "select",
        "model" => "model",
        "number" => "number",
        "prompt" => "prompt",
        _ => "string",
    }
    .to_string()
}

/// Une section de réglages, telle que l'interface l'attend.
fn to_settings_section(
    s: &locaryn_extensions::manifest::SettingsSection,
) -> locaryn_shared_types::ExtensionSettingsSection {
    locaryn_shared_types::ExtensionSettingsSection {
        id: s.id.clone(),
        title: s.title.clone(),
        description: s.description.clone(),
        fields: s
            .fields
            .iter()
            .map(|f| locaryn_shared_types::ExtensionSettingsField {
                key: f.key.clone(),
                label: f.label.clone(),
                kind: rendu_du_champ(&f.kind),
                hint: f.hint.clone(),
                options: f.options.clone(),
                default: f.default.clone(),
            })
            .collect(),
    }
}

/// Une extension telle que tous les clients la lisent.
///
/// Le service renvoyait auparavant une forme à lui, où `permissions` était un
/// objet `{requested, granted}` — la CLI et le téléphone, qui attendent le type
/// partagé, échouaient au décodage. Un seul contrat, celui du type partagé.
fn entry_to_installed(
    e: &locaryn_extensions::ExtensionEntry,
) -> locaryn_shared_types::InstalledExtension {
    let dir = e.manifest_path.parent().map(|p| p.to_path_buf());
    let manifest = dir
        .as_deref()
        .and_then(|d| locaryn_extensions::manifest::load(d).ok());
    let components = match (&dir, &manifest) {
        (Some(d), Some(m)) => locaryn_extensions::loader::load_with_manifest(d, m.clone()).counts(),
        _ => locaryn_shared_types::ExtensionComponents::default(),
    };
    let now = chrono::Utc::now();
    locaryn_shared_types::InstalledExtension {
        id: e.id,
        name: e.name.clone(),
        display_name: e.name.clone(),
        version: e.version.clone(),
        api_version: e.api_version.clone(),
        description: manifest.as_ref().and_then(|m| m.description.clone()),
        author: manifest.as_ref().and_then(|m| m.author.clone()),
        homepage: manifest.as_ref().and_then(|m| m.homepage.clone()),
        kind: e.kind.clone(),
        scope: e.scope,
        ecosystem: locaryn_shared_types::ExtensionEcosystem::Locaryn,
        source: dir.as_ref().map(|d| d.display().to_string()),
        install_dir: dir
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
        enabled: e.enabled,
        components,
        // Une extension désactivée n'apporte rien : sinon un écran survivrait
        // à l'extinction de ce qui le portait.
        capabilities: if e.enabled {
            e.capabilities.clone()
        } else {
            Vec::new()
        },
        ui: locaryn_shared_types::ExtensionUi {
            slots: if e.enabled {
                e.ui.slots.iter().map(to_slot_contribution).collect()
            } else {
                Vec::new()
            },
            nav_items: e.ui.nav_items.iter().map(to_ui_entry).collect(),
            studio_tabs: e.ui.studio_tabs.iter().map(to_ui_entry).collect(),
            // Une extension éteinte ne pose plus rien : ni bouton près du
            // champ de saisie, ni section de réglages. Sinon on garderait un
            // micro qui ne dicte plus.
            composer_actions: if e.enabled {
                e.ui.composer_actions
                    .iter()
                    .map(to_composer_action)
                    .collect()
            } else {
                Vec::new()
            },
            settings_sections: if e.enabled {
                e.ui.settings_sections
                    .iter()
                    .map(to_settings_section)
                    .collect()
            } else {
                Vec::new()
            },
        },
        permissions: e
            .permissions
            .requested
            .iter()
            .map(|(p, r)| locaryn_shared_types::ExtensionPermissionState {
                permission: p.clone(),
                reason: r.reason.clone(),
                granted: e.permissions.granted.contains(p),
                // Le registre du service ne garde pas la date de décision :
                // il ne peut pas distinguer un refus d'une question jamais
                // posée, et ne prétend donc rien.
                undecided: false,
            })
            .collect(),
        // Une capacité hors de la liste canonique ne crée aucun écran : on
        // le dit plutôt que de laisser croire qu'elle a un effet. Non
        // bloquant — une extension plus récente peut déclarer un mot que ce
        // build ne connaît pas encore, et l'interface l'ignore simplement.
        load_errors: e
            .capabilities
            .iter()
            .filter(|c| !locaryn_shared_types::capabilities::is_known(c))
            .map(|c| format!("capacité inconnue : {c}"))
            .collect(),
        // Une extension de noyau expose sa section `core` : c'est ce qui
        // fait apparaître la carte « Noyau » dans les réglages.
        core: manifest.as_ref().and_then(|m| m.core.as_ref()).map(|c| {
            locaryn_shared_types::ExtensionCoreInfo {
                driver: c.driver.clone(),
                api_url: c.api_url.clone(),
                port: c.port,
                model: c.model.clone(),
                skills_index: c.skills.index.clone(),
                skills_install: c.skills.install.clone(),
            }
        }),
        created_at: now,
        updated_at: now,
    }
}

/// Recharge le registre depuis la base au démarrage du service.
///
/// Le registre vivait en mémoire seule : installer une extension puis
/// redémarrer le service la faisait disparaître, avec l'écran qu'elle
/// apportait. Une installation doit survivre à un redémarrage — c'est le
/// minimum qu'on attend d'une installation.
pub async fn restore_from_storage(state: &DaemonState) {
    let records = match state.storage.extensions.list().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "extensions : lecture de la base impossible");
            return;
        }
    };
    let mut restored = 0usize;
    for rec in records {
        let dir = std::path::Path::new(&rec.manifest_path)
            .parent()
            .map(|p| p.to_path_buf());
        let Some(dir) = dir else { continue };
        match state.extensions.install_from_dir(&dir, rec.scope) {
            Ok(entry) => {
                if rec.enabled {
                    let _ = state.extensions.enable(&entry.name);
                }
                // Les figures du dépôt sont versées en base à chaque démarrage :
                // une réinstallation les met à jour par le nom, sans jamais
                // toucher à une figure écrite à la main.
                let importees = locaryn_storage::figures_import::importer(
                    &state.storage.figures,
                    &dir,
                    &entry.name,
                )
                .await;
                if importees > 0 {
                    tracing::info!(name = %entry.name, importees, "figures du dépôt importées");
                }
                restored += 1;
            }
            Err(e) => tracing::warn!(
                name = %rec.name,
                error = %e,
                "extension enregistrée mais introuvable sur le disque"
            ),
        }
    }
    if restored > 0 {
        tracing::info!(restored, "extensions rechargées depuis la base");
    }
}

/// Register and auto-start the MCP servers shipped by enabled extensions.
///
/// The desktop has an equivalent refresh path. The daemon needs its own one:
/// mobile and web clients never launch plugin processes themselves, so an
/// extension installed on the server would otherwise appear in `/v1/extensions`
/// while its tools remained invisible to the agent.
pub async fn sync_mcp_servers(state: &DaemonState) {
    let records = match state.storage.extensions.list().await {
        Ok(records) => records,
        Err(error) => {
            tracing::warn!(error = %error, "MCP extensions : lecture des permissions impossible");
            return;
        }
    };
    let mut desired: HashMap<String, locaryn_mcp::McpServerEntry> = HashMap::new();
    for record in records.into_iter().filter(|record| record.enabled) {
        if !record.granted.contains(&Permission::Mcp) {
            continue;
        }
        let Some(entry) = state.extensions.get(&record.name) else {
            continue;
        };
        let Some(root) = std::path::Path::new(&record.manifest_path).parent() else {
            continue;
        };
        let Ok(loaded) = locaryn_extensions::loader::load(root) else {
            continue;
        };
        let safe_name: String = record
            .name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        let extension_data_dir = state.data_dir.join("extensions").join(&safe_name);
        let _ = std::fs::create_dir_all(&extension_data_dir);
        for (server_name, mut server) in loaded.mcp {
            let scoped = format!("{safe_name}__{server_name}");
            server.env.insert(
                "LOCARYN_DATA_DIR".into(),
                state.data_dir.display().to_string(),
            );
            server.env.insert(
                "LOCARYN_EXTENSION_DATA_DIR".into(),
                extension_data_dir.display().to_string(),
            );
            server.env.insert(
                "LOCARYN_EXTENSION_MODELS_DIR".into(),
                extension_data_dir.join("models").display().to_string(),
            );
            server.env.insert(
                "LOCARYN_EXTENSION_MEDIA_DIR".into(),
                extension_data_dir.join("media").display().to_string(),
            );
            // Même raison que côté bureau : sans la bibliothèque de poids de
            // l'utilisateur, une extension ne voit que son dossier privé et
            // croit qu'aucun modèle n'est installé.
            server.env.insert(
                "LOCARYN_MODELS_DIR".into(),
                locaryn_config::models_dir().display().to_string(),
            );
            // Les dossiers de cache et de travail que le socle tient hors du
            // disque système. Une extension qui les ignore écrit dans
            // `~/.cache`, et c'est ainsi qu'un disque système se remplit.
            server.env.insert(
                "LOCARYN_HF_CACHE_DIR".into(),
                locaryn_config::hf_cache_dir().display().to_string(),
            );
            server.env.insert(
                "LOCARYN_TEMP_DIR".into(),
                locaryn_config::ensure_temp_dir().display().to_string(),
            );
            server
                .env
                .insert("LOCARYN_PLUGIN_ROOT".into(), root.display().to_string());
            server.env.insert(
                "LOCARYN_PLUGIN_BIN_DIR".into(),
                root.join("bin").display().to_string(),
            );
            server.owner = Some(entry.name.clone());
            desired.insert(scoped, server);
        }
    }

    let stale = {
        let mut config = state.mcp_state.config.lock().unwrap();
        let stale: Vec<String> = config
            .mcp_servers
            .iter()
            .filter(|(_, server)| server.owner.is_some())
            .map(|(name, _)| name.clone())
            .filter(|name| !desired.contains_key(name))
            .collect();
        for name in &stale {
            config.mcp_servers.remove(name);
        }
        for (name, server) in &desired {
            config.mcp_servers.insert(name.clone(), server.clone());
        }
        drop(config);
        state.mcp_state.save();
        stale
    };
    for name in stale {
        if let Some(client) = state.mcp_state.running.write().await.remove(&name) {
            let _ = client.shutdown().await;
        }
    }

    for (name, server) in desired {
        if !server.auto_start || state.mcp_state.running.read().await.contains_key(&name) {
            continue;
        }
        let client: Arc<dyn McpClient> = Arc::from(state.mcp_state.build_client(&server));
        match client.discover().await {
            Ok(capabilities) => {
                tracing::info!(server = %name, tools = capabilities.tools.len(), "serveur MCP d'extension démarré");
                state.mcp_state.running.write().await.insert(name, client);
            }
            Err(error) => {
                tracing::warn!(server = %name, error = %error, "serveur MCP d'extension indisponible")
            }
        }
    }
}

/// Écrit dans la base ce que le registre vient de faire.
async fn persist(state: &DaemonState, entry: &locaryn_extensions::ExtensionEntry) {
    let new = locaryn_storage::repos::NewExtension {
        name: entry.name.clone(),
        version: entry.version.clone(),
        api_version: entry.api_version.clone(),
        kind: entry.kind.clone(),
        scope: entry.scope,
        ecosystem: locaryn_shared_types::ExtensionEcosystem::Locaryn,
        source: entry
            .manifest_path
            .parent()
            .map(|p| p.display().to_string()),
        manifest_path: entry.manifest_path.display().to_string(),
        requested: entry
            .permissions
            .requested
            .iter()
            .map(|(p, _)| p.clone())
            .collect(),
    };
    if let Err(e) = state.storage.extensions.upsert(new).await {
        tracing::warn!(name = %entry.name, error = %e, "extension non enregistrée en base");
    }
}

/// Met à jour l'état actif en base, par nom.
async fn persist_enabled(state: &DaemonState, name: &str, enabled: bool) {
    let Some(rec) = find_record(state, name).await else {
        tracing::warn!(name, "extension absente de la base");
        return;
    };
    if let Err(e) = state.storage.extensions.set_enabled(rec.id, enabled).await {
        tracing::warn!(name, error = %e, "état de l'extension non enregistré");
    }
}

/// Retrouve l'enregistrement quelle que soit sa portée : le nom est unique
/// dans le registre du service, la portée n'est qu'un détail de rangement.
async fn find_record(
    state: &DaemonState,
    name: &str,
) -> Option<locaryn_storage::repos::ExtensionRecord> {
    for scope in [
        ExtensionScope::User,
        ExtensionScope::Global,
        ExtensionScope::Workspace,
        ExtensionScope::Session,
    ] {
        if let Ok(Some(rec)) = state.storage.extensions.get_by_name(name, scope).await {
            return Some(rec);
        }
    }
    None
}

async fn persist_removed(state: &DaemonState, name: &str) {
    if let Some(rec) = find_record(state, name).await {
        if let Err(e) = state.storage.extensions.delete(rec.id).await {
            tracing::warn!(name, error = %e, "extension non retirée de la base");
        }
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /v1/extensions — list all installed extensions.
pub async fn list_extensions(State(s): State<Arc<DaemonState>>) -> Response {
    let entries = s.extensions.list();
    let json: Vec<_> = entries.iter().map(entry_to_installed).collect();
    (StatusCode::OK, Json(json)).into_response()
}

/// GET /v1/capabilities — la liste canonique des capacités reconnues.
///
/// C'est la même liste que celle embarquée par les clients à la compilation
/// (`@locaryn/ui-core`) et validée au chargement des extensions. La consulter
/// permet à une interface d'afficher les capacités — labels compris — telles
/// que ce serveur les reconnaît, sans être recompilée quand de nouvelles
/// capacités sont ajoutées.
pub async fn list_capabilities() -> Response {
    let json: Vec<_> = locaryn_shared_types::capabilities::all().to_vec();
    (StatusCode::OK, Json(json)).into_response()
}

/// POST /v1/extensions/install — install an extension from a local path.
///
/// Body: `{ source: "/path/to/plugin/dir", scope: "user" | "workspace" | "global" }`
/// URL-based install is not yet implemented (V1.1).
pub async fn install_extension(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<InstallBody>,
) -> Response {
    let scope = match parse_scope(&body.scope) {
        Ok(s) => s,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };

    // Un dossier du serveur, ou un dépôt du catalogue. Le second cas est le
    // seul possible depuis un téléphone, qui ne connaît aucun chemin de la
    // machine d'en face — et c'est aussi ce que la CLI annonçait déjà sans que
    // le service sache le faire.
    let local;
    let dir = if std::path::Path::new(&body.source).is_dir() {
        std::path::PathBuf::from(&body.source)
    } else {
        match fetch_from_catalogue(&body.source).await {
            Ok(p) => {
                local = p;
                local.clone()
            }
            Err(msg) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": { "code": "bad_request", "message": msg }
                    })),
                )
                    .into_response();
            }
        }
    };
    let dir = dir.as_path();

    match s.extensions.install_from_dir(dir, scope) {
        Ok(entry) => {
            persist(&s, &entry).await;
            // Ce que l'extension transporte comme figures entre en base dès
            // l'installation : l'écran Figures les montre aussitôt, sur
            // l'ordinateur comme sur le téléphone.
            let importees =
                locaryn_storage::figures_import::importer(&s.storage.figures, dir, &entry.name)
                    .await;
            if importees > 0 {
                tracing::info!(name = %entry.name, importees, "figures du dépôt importées");
            }
            (StatusCode::CREATED, Json(entry_to_installed(&entry))).into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// Ramener `owner/repo` à ce qu'il désigne vraiment.
///
/// On accepte ce qu'une personne écrit : `Locaryn/locaryn-image`,
/// l'adresse complète du dépôt, ou la forme `github:owner/repo`. Tout le
/// reste est refusé — une source arbitraire téléchargée et exécutée par le
/// service serait une porte d'entrée, pas une commodité.
fn parse_repo(source: &str) -> Result<(String, String), String> {
    let s = source
        .trim()
        .trim_end_matches('/')
        .trim_start_matches("github:")
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_end_matches(".git");
    let mut parts = s.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!(
            "« {source} » n'est ni un dossier du serveur, ni un dépôt « propriétaire/nom »."
        ));
    };
    let valide = |x: &str| {
        !x.is_empty()
            && x.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    };
    if !valide(owner) || !valide(repo) {
        return Err(format!("Nom de dépôt inattendu : « {source} »."));
    }
    Ok((owner.to_string(), repo.to_string()))
}

/// Trouver, dans une archive dépliée, le dossier qui porte le manifeste.
///
/// GitHub emballe tout dans un dossier `repo-main/`, et une extension peut y
/// être rangée dans un sous-dossier. On cherche donc `plugin.json` sur deux
/// niveaux plutôt que de supposer une disposition.
fn find_manifest_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    if root.join("plugin.json").is_file() {
        return Some(root.to_path_buf());
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if p.join("plugin.json").is_file() {
                return Some(p);
            }
            if let Ok(sous) = std::fs::read_dir(&p) {
                for s in sous.flatten() {
                    let sp = s.path();
                    if sp.is_dir() && sp.join("plugin.json").is_file() {
                        return Some(sp);
                    }
                }
            }
        }
    }
    None
}

/// Télécharger un dépôt du catalogue et rendre le dossier à installer.
async fn fetch_from_catalogue(source: &str) -> Result<std::path::PathBuf, String> {
    let (owner, repo) = parse_repo(source)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("locaryn-daemon")
        .build()
        .map_err(|e| e.to_string())?;

    // `main` d'abord, `master` ensuite : les deux existent dans la nature.
    let mut derniere = String::new();
    let mut octets = None;
    for branche in ["main", "master"] {
        let url = format!("https://github.com/{owner}/{repo}/archive/refs/heads/{branche}.zip");
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                octets = Some(r.bytes().await.map_err(|e| e.to_string())?);
                break;
            }
            Ok(r) => derniere = format!("{} a répondu {}", url, r.status()),
            Err(e) => derniere = format!("{url} : {e}"),
        }
    }
    let Some(octets) = octets else {
        return Err(format!(
            "Impossible de récupérer {owner}/{repo} ({derniere})."
        ));
    };

    let cible = locaryn_config::default_data_dir()
        .join("extensions-telechargees")
        .join(format!("{owner}-{repo}"));
    let _ = std::fs::remove_dir_all(&cible);
    std::fs::create_dir_all(&cible).map_err(|e| format!("dossier : {e}"))?;

    let curseur = std::io::Cursor::new(octets);
    let mut archive =
        zip::ZipArchive::new(curseur).map_err(|e| format!("archive illisible : {e}"))?;
    for i in 0..archive.len() {
        let mut entree = archive
            .by_index(i)
            .map_err(|e| format!("entrée illisible : {e}"))?;
        // `enclosed_name` refuse les chemins qui sortent du dossier cible.
        let Some(rel) = entree.enclosed_name() else {
            continue;
        };
        let out = cible.join(rel);
        if entree.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut fichier = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entree, &mut fichier).map_err(|e| e.to_string())?;
    }

    find_manifest_dir(&cible).ok_or_else(|| {
        format!("{owner}/{repo} ne contient pas de plugin.json : ce n'est pas une extension.")
    })
}

/// POST /v1/extensions/{name}/enable — enable an extension by name.
pub async fn enable_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.enable(&name) {
        Ok(()) => {
            persist_enabled(&s, &name, true).await;
            sync_mcp_servers(&s).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "enabled", "name": name })),
            )
                .into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// POST /v1/extensions/{name}/disable — disable an extension by name.
pub async fn disable_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.disable(&name) {
        Ok(()) => {
            persist_enabled(&s, &name, false).await;
            sync_mcp_servers(&s).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "disabled", "name": name })),
            )
                .into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// DELETE /v1/extensions/{name} — uninstall (remove) an extension by name.
pub async fn remove_extension(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.remove(&name) {
        Ok(()) => {
            persist_removed(&s, &name).await;
            sync_mcp_servers(&s).await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "removed", "name": name })),
            )
                .into_response()
        }
        Err(e) => registry_error_response(e).into_response(),
    }
}

/// POST /v1/extensions/asset — read a text asset declared by an enabled extension.
///
/// Assets are resolved below the extension root and never through an arbitrary
/// filesystem path. The endpoint exists for the web client; desktop and mobile
/// use the same contract through their thin bridges.
pub async fn read_extension_asset(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<ExtensionAssetBody>,
) -> Response {
    let Some(root) = s
        .extensions
        .list()
        .into_iter()
        .find(|entry| {
            (entry.name == body.extension_id || entry.id.to_string() == body.extension_id)
                && entry.enabled
        })
        .and_then(|entry| entry.manifest_path.parent().map(|path| path.to_path_buf()))
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": { "code": "not_found", "message": "extension inconnue ou désactivée" } })),
        )
            .into_response();
    };

    let relative = std::path::Path::new(&body.asset_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": { "code": "bad_request", "message": "asset hors du dossier de l'extension" } })),
        )
            .into_response();
    }
    let root = match std::fs::canonicalize(&root) {
        Ok(path) => path,
        Err(error) => return extension_asset_error(StatusCode::NOT_FOUND, error.to_string()),
    };
    let target = match std::fs::canonicalize(root.join(relative)) {
        Ok(path) if path.starts_with(&root) => path,
        Ok(_) => {
            return extension_asset_error(
                StatusCode::FORBIDDEN,
                "asset hors du dossier de l'extension".into(),
            )
        }
        Err(error) => return extension_asset_error(StatusCode::NOT_FOUND, error.to_string()),
    };
    match tokio::fs::read_to_string(&target).await {
        Ok(content) => (StatusCode::OK, Json(content)).into_response(),
        Err(error) => extension_asset_error(StatusCode::NOT_FOUND, error.to_string()),
    }
}

/// GET /v1/extension-assets?path=... — serve an extension-produced file.
///
/// Generated files are kept below Locaryn's private extension data root. This
/// is a generic media route, not an image-generation API.
pub async fn get_extension_media(
    State(_s): State<Arc<DaemonState>>,
    Query(query): Query<ExtensionMediaQuery>,
) -> Response {
    let requested = std::path::PathBuf::from(&query.path);
    let extension_root = locaryn_config::storage_root().join("extensions");
    let root = match std::fs::canonicalize(&extension_root) {
        Ok(path) => path,
        Err(error) => return extension_asset_error(StatusCode::NOT_FOUND, error.to_string()),
    };
    let target = match std::fs::canonicalize(&requested) {
        Ok(path) if path.starts_with(&root) && path.is_file() => path,
        Ok(_) => {
            return extension_asset_error(
                StatusCode::FORBIDDEN,
                "fichier d'extension invalide".into(),
            )
        }
        Err(error) => return extension_asset_error(StatusCode::NOT_FOUND, error.to_string()),
    };
    let bytes = match tokio::fs::read(&target).await {
        Ok(bytes) => bytes,
        Err(error) => return extension_asset_error(StatusCode::NOT_FOUND, error.to_string()),
    };
    let mime = match target
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn extension_asset_error(status: StatusCode, message: String) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": { "code": "asset_error", "message": message } })),
    )
        .into_response()
}

/// GET /v1/extensions/{name}/permissions — list requested vs granted permissions.
pub async fn get_extension_permissions(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
) -> Response {
    match s.extensions.get(&name) {
        Some(entry) => {
            let json = serde_json::json!({
                "name": entry.name,
                "permissions": {
                    "requested": entry.permissions.requested.iter().map(|(p, r)| {
                        serde_json::json!({
                            "permission": format!("{p:?}"),
                            "reason": r.reason,
                            "scope": r.scope,
                            "require_approval": r.require_approval,
                        })
                    }).collect::<Vec<_>>(),
                    "granted": entry.permissions.granted.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
                }
            });
            (StatusCode::OK, Json(json)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("extension not found: {name}") }
            })),
        )
            .into_response(),
    }
}

/// POST /v1/extensions/{name}/permissions — approve or deny a specific permission.
///
/// Body: `{ permission: "files.write", granted: true }`
pub async fn set_extension_permission(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
    Json(body): Json<PermissionBody>,
) -> Response {
    let perm = match parse_permission(&body.permission) {
        Ok(p) => p,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "bad_request", "message": msg }
                })),
            )
                .into_response();
        }
    };

    if body.granted {
        match s.extensions.grant_permission(&name, perm) {
            Ok(()) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "granted",
                    "name": name,
                    "permission": body.permission,
                })),
            )
                .into_response(),
            Err(e) => registry_error_response(e).into_response(),
        }
    } else {
        // Denying a permission is a no-op in the current registry (permissions
        // are addition-only). We still return OK for API consistency.
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "denied",
                "name": name,
                "permission": body.permission,
            })),
        )
            .into_response()
    }
}

/// POST /v1/extensions/reload — hot-reload one extension by name in the body,
/// or all extensions if no name is provided.
///
/// Body (optional): `{ name: "my-plugin" }`
pub async fn reload_extensions(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        match s.extensions.reload(name) {
            Ok(entry) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "reloaded",
                    "name": name,
                    "entry": entry_to_installed(&entry),
                })),
            )
                .into_response(),
            Err(e) => registry_error_response(e).into_response(),
        }
    } else {
        // Reload all: iterate through entries, re-read each manifest.
        let names: Vec<String> = s.extensions.list().into_iter().map(|e| e.name).collect();
        let mut results: Vec<serde_json::Value> = Vec::new();
        for name in &names {
            match s.extensions.reload(name) {
                Ok(_entry) => results.push(serde_json::json!({
                    "name": name, "status": "reloaded"
                })),
                Err(e) => results.push(serde_json::json!({
                    "name": name, "status": "error", "error": e.to_string()
                })),
            }
        }
        (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "completed", "results": results })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod catalogue_tests {
    use super::parse_repo;

    #[test]
    fn les_formes_courantes_donnent_le_meme_depot() {
        for source in [
            "Locaryn/locaryn-image",
            "github:Locaryn/locaryn-image",
            "https://github.com/Locaryn/locaryn-image",
            "https://github.com/Locaryn/locaryn-image.git",
            "https://github.com/Locaryn/locaryn-image/",
        ] {
            assert_eq!(
                parse_repo(source).unwrap(),
                ("Locaryn".to_string(), "locaryn-image".to_string()),
                "source : {source}"
            );
        }
    }

    #[test]
    fn tout_le_reste_est_refuse() {
        // Une source arbitraire téléchargée par le service serait une porte
        // d'entrée : rien qui ne ressemble pas à un dépôt ne passe.
        for source in [
            "",
            "juste-un-mot",
            "trop/de/segments",
            "https://exemple.invalide/paquet.zip",
            "../../etc",
            "Locaryn/nom avec espace",
        ] {
            assert!(parse_repo(source).is_err(), "aurait dû refuser : {source}");
        }
    }
}

/// Où vivent les réglages d'une extension : dans son propre dossier.
///
/// Le même fichier que celui qu'écrit l'application de bureau. Deux magasins
/// pour la même chose divergeraient au premier changement, et un réglage dont
/// on ne sait plus lequel fait foi vaut moins que pas de réglage. Le ranger
/// là a un autre mérite : retirer l'extension emporte ses réglages avec elle.
fn fichier_de_reglages(dossier: &std::path::Path) -> std::path::PathBuf {
    dossier.join(".data").join("config.json")
}

/// Tous les réglages, à plat, clés `extension.champ`.
///
/// Un seul objet suffit à peupler tous les écrans : l'application n'a pas à
/// demander extension par extension, et le téléphone fait un aller-retour.
fn tous_les_reglages(s: &DaemonState) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for e in s.extensions.list() {
        let Some(dossier) = e.manifest_path.parent() else {
            continue;
        };
        let Ok(brut) = std::fs::read_to_string(fichier_de_reglages(dossier)) else {
            continue;
        };
        let Ok(serde_json::Value::Object(valeurs)) = serde_json::from_str(&brut) else {
            continue;
        };
        for (k, v) in valeurs {
            // Les valeurs sont rendues en texte : c'est ce que les champs
            // affichent, et cela évite qu'un booléen arrive tantôt `true`,
            // tantôt `"true"`, selon qui l'a écrit.
            let texte = match v {
                serde_json::Value::String(t) => t,
                autre => autre.to_string(),
            };
            out.insert(format!("{}.{k}", e.name), serde_json::Value::String(texte));
        }
    }
    out
}

/// GET /v1/extensions/config — tous les réglages, à plat.
pub async fn get_extension_config(State(s): State<Arc<DaemonState>>) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::Value::Object(tous_les_reglages(&s))),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
pub struct ReglageBody {
    pub key: String,
    #[serde(default)]
    pub value: String,
}

/// POST /v1/extensions/{name}/config — écrire un réglage.
pub async fn set_extension_config(
    State(s): State<Arc<DaemonState>>,
    Path(name): Path<String>,
    Json(body): Json<ReglageBody>,
) -> Response {
    if body.key.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": { "code": "bad_request", "message": "nom de réglage vide" }
            })),
        )
            .into_response();
    }
    let Some(dossier) = s
        .extensions
        .list()
        .iter()
        .find(|e| e.name == name)
        .and_then(|e| e.manifest_path.parent().map(|p| p.to_path_buf()))
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "not_found", "message": format!("extension inconnue : {name}") }
            })),
        )
            .into_response();
    };

    let chemin = fichier_de_reglages(&dossier);
    let mut valeurs: serde_json::Map<String, serde_json::Value> = std::fs::read_to_string(&chemin)
        .ok()
        .and_then(|b| serde_json::from_str(&b).ok())
        .unwrap_or_default();
    // Une valeur vide efface le réglage : c'est ce que fait « Aucun » dans une
    // liste, et laisser une chaîne vide traîner ferait croire à un choix.
    if body.value.is_empty() {
        valeurs.remove(&body.key);
    } else {
        valeurs.insert(body.key.clone(), serde_json::Value::String(body.value));
    }

    if let Some(parent) = chemin.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return erreur_ecriture(&e.to_string());
        }
    }
    let joli = serde_json::to_string_pretty(&valeurs).unwrap_or_default();
    if let Err(e) = std::fs::write(&chemin, joli) {
        return erreur_ecriture(&e.to_string());
    }

    (
        StatusCode::OK,
        Json(serde_json::Value::Object(tous_les_reglages(&s))),
    )
        .into_response()
}

fn erreur_ecriture(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": { "code": "write_failed", "message": message }
        })),
    )
        .into_response()
}
