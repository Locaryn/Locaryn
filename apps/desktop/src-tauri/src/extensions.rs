//! Extensions, from the application.
//!
//! The pieces existed separately: a manifest parser, a registry that flipped a
//! boolean, eleven unused SQL tables, and a store screen whose install button
//! set a React state variable. Nothing connected them, so no extension had
//! ever actually run.
//!
//! This module is the connection. Installing fetches and adapts a bundle
//! (`locaryn_extensions::install`), records it in SQLite, and — once the user
//! has granted the permissions it asked for — registers its components with
//! the runtimes that were already there: MCP servers into `core.mcp`, rules
//! and skills into the system prompt, commands into the slash palette.
//!
//! What is deliberately *not* wired: hook execution. Hooks are parsed, stored
//! and shown, but nothing fires them yet — the tool loop has no dispatch
//! point. Saying so in the UI is better than a plugin whose hooks quietly
//! never run.

use locaryn_extensions::loader::LoadedPlugin;
use locaryn_extensions::manifest::PluginManifest;
use locaryn_extensions::{latest_github_version, version_gt, SourceError};
use locaryn_mcp::{McpClient, McpConfig, McpServerEntry, Transport};
use locaryn_shared_types::{
    CatalogEntry, CatalogSnapshot, CatalogSource, ExtensionComponents, ExtensionEcosystem,
    ExtensionKind, ExtensionPermissionState, ExtensionScope, InstalledExtension, Permission,
};
use locaryn_storage::NewExtension;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

use crate::Core;

// ============================================================================
// Runtime state
// ============================================================================

/// Every enabled plugin, loaded and ready.
#[derive(Default)]
pub struct ExtensionRuntime {
    /// Loaded plugins, keyed by extension id.
    pub loaded: HashMap<Uuid, LoadedPlugin>,
    /// Rules and skill index of every enabled plugin, appended to the system
    /// prompt. Empty when nothing is enabled.
    pub system_prompt: String,
    /// MCP server names this runtime registered. A reload retracts exactly
    /// these, so a disabled plugin's servers do not linger.
    pub mcp_names: Vec<String>,
}

impl ExtensionRuntime {
    /// A command contributed by any enabled plugin, by its namespaced name.
    pub fn command(&self, name: &str) -> Option<(&str, &locaryn_command_runtime::CommandDef)> {
        for p in self.loaded.values() {
            for c in &p.commands {
                if c.name == name || format!("{}:{}", p.manifest.name, c.name) == name {
                    return Some((p.manifest.name.as_str(), c));
                }
            }
        }
        None
    }
}

/// A slash command as the palette sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionCommand {
    /// `<plugin>:<command>` — namespaced so two plugins can both ship `/review`.
    pub name: String,
    pub plugin: String,
    pub description: Option<String>,
    pub arguments: Vec<String>,
}

// ============================================================================
// Reload
// ============================================================================

/// Rebuild the runtime from the database. Idempotent: called at startup and
/// after every install, enable, disable or removal.
pub async fn reload(core: &Core) -> Result<(), String> {
    let rows = core
        .storage
        .extensions
        .list()
        .await
        .map_err(|e| e.to_string())?;

    let mut next = ExtensionRuntime::default();
    let mut prompt = String::new();

    for row in rows.iter().filter(|r| r.enabled) {
        let Some(root) = plugin_root(&row.manifest_path) else {
            continue;
        };
        match locaryn_extensions::loader::load(&root) {
            Ok(p) => {
                prompt.push_str(&p.system_prompt_fragment());
                next.loaded.insert(row.id, p);
            }
            Err(e) => {
                // A plugin whose files vanished stays in the list, disabled in
                // effect, with the reason visible rather than silently absent.
                tracing::warn!(name = %row.name, error = %e, "extension activée illisible");
            }
        }

        // Les figures du dépôt sont resynchronisées à chaque chargement :
        // une mise à jour de l'extension est reprise au redémarrage, et une
        // figure écrite à la main n'est jamais écrasée.
        let importees =
            locaryn_storage::figures_import::importer(&core.storage.figures, &root, &row.name)
                .await;
        if importees > 0 {
            tracing::info!(name = %row.name, importees, "figures du dépôt importées");
        }
    }
    next.system_prompt = prompt;

    // --- MCP servers --------------------------------------------------------
    // Retract what the previous generation registered, then publish the new
    // set. Names are `<plugin>__<serveur>` so two plugins may ship a server
    // with the same name, and so the model's tool names stay unambiguous.
    let previous: Vec<String> = {
        let rt = core.extensions.read().await;
        rt.mcp_names.clone()
    };

    let mut desired: HashMap<String, McpServerEntry> = HashMap::new();
    for row in rows.iter().filter(|r| r.enabled) {
        let Some(p) = next.loaded.get(&row.id) else {
            continue;
        };
        // A plugin only gets to register servers if the user allowed it.
        if !row.granted.contains(&Permission::Mcp) {
            continue;
        }
        for (name, entry) in &p.mcp {
            let scoped = format!("{}__{}", sanitize_server(&row.name), sanitize_server(name));
            let mut entry = entry.clone();
            // Les chemins génériques vivent dans `locaryn_extensions::hostpaths`
            // : le bureau, le daemon et le superviseur doivent donner le
            // **même** dossier privé à la même extension, sinon son serveur
            // MCP et son moteur d'inférence travaillent chacun dans son coin.
            // L'extension seule sait ce qu'elle y range — l'hôte ne fait que
            // fournir les chemins.
            for (key, value) in locaryn_extensions::hostpaths::generic_env(&row.name, &p.root) {
                entry.env.insert(key, value);
            }
            // Les faits que seul le socle mesure : la VRAM de la carte et
            // l'interpréteur Python qu'il gère. Une extension qui les ignore
            // doit deviner : celle de l'image répartissait ses poids « au cas
            // où », et un rendu d'une minute en prenait trois.
            if let Some(vram) = host_vram_gb() {
                entry
                    .env
                    .insert("LOCARYN_VRAM_GB".to_string(), format!("{vram:.2}"));
            }
            if let Some(python) = host_python() {
                entry.env.insert("LOCARYN_PYTHON".to_string(), python);
            }
            entry.owner = Some(row.name.clone());
            desired.insert(scoped, entry);
        }
    }

    let to_stop: Vec<String> = previous
        .iter()
        .filter(|n| !desired.contains_key(*n))
        .cloned()
        .collect();
    {
        let mut cfg = core.mcp.config.lock().unwrap();
        for name in &to_stop {
            cfg.mcp_servers.remove(name);
        }
        for (name, entry) in &desired {
            cfg.mcp_servers.insert(name.clone(), entry.clone());
        }
    }
    for name in &to_stop {
        if let Some(client) = core.mcp.running.write().await.remove(name) {
            let _ = client.shutdown().await;
        }
    }
    next.mcp_names = desired.keys().cloned().collect();
    next.mcp_names.sort();

    // Start the ones marked automatic that are not already running.
    for (name, entry) in desired.iter().filter(|(_, e)| e.auto_start) {
        if core.mcp.running.read().await.contains_key(name) {
            continue;
        }
        let client: Arc<dyn McpClient> = Arc::from(core.mcp.build_client(entry));
        match client.discover().await {
            Ok(caps) => {
                tracing::info!(server = %name, tools = caps.tools.len(), "serveur MCP d'extension démarré");
                core.mcp.running.write().await.insert(name.clone(), client);
            }
            Err(e) => {
                tracing::warn!(server = %name, error = %e, "serveur MCP d'extension injoignable")
            }
        }
    }

    // Les moteurs d'inférence apportés par ces extensions. Même cycle de vie
    // que les serveurs MCP — installation, activation, retrait — donc même
    // point de synchronisation : un moteur ne doit pas rester démarrable après
    // que son extension a été désactivée.
    let sources: Vec<locaryn_provider_supervisor::extension_engine::EngineSource> = rows
        .iter()
        .map(
            |row| locaryn_provider_supervisor::extension_engine::EngineSource {
                manifest_path: std::path::PathBuf::from(&row.manifest_path),
                enabled: row.enabled,
            },
        )
        .collect();
    let moteurs = locaryn_provider_supervisor::extension_engine::collect(&sources);
    if !moteurs.is_empty() {
        tracing::info!(
            moteurs = ?moteurs.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            "moteurs d'inférence apportés par des extensions"
        );
    }
    core.supervisor.set_extension_engines(moteurs).await;

    *core.extensions.write().await = next;
    Ok(())
}

/// MCP server names become part of the tool names the model sees
/// (`mcp__<serveur>__<outil>`), so anything outside `[A-Za-z0-9_-]` would
/// produce tools nobody can call.
/// La VRAM de la carte, en Gio, telle que le socle l'a déjà sondée.
fn host_vram_gb() -> Option<f32> {
    let hardware = crate::HARDWARE_CACHE
        .get()
        .cloned()
        .or_else(|| crate::probe_hardware().ok())?;
    (hardware.total_vram_gb > 0).then_some(hardware.total_vram_gb as f32)
}

/// L'interpréteur Python que le socle utilise pour ses propres travaux.
fn host_python() -> Option<String> {
    crate::find_python().filter(|path| path != "python")
}

fn sanitize_server(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub(crate) fn plugin_root(manifest_path: &str) -> Option<PathBuf> {
    let p = Path::new(manifest_path);
    if p.is_dir() {
        return Some(p.to_path_buf());
    }
    p.parent().map(|x| x.to_path_buf())
}

// ============================================================================
// Reading the installed set
// ============================================================================

/// Ce que les extensions actives apportent, réuni en une liste.
///
/// Sert deux décisions : quels écrans existent dans l'interface, et quels
/// outils sont offerts au modèle. Une seule source pour les deux, sinon un
/// menu pourrait promettre ce que le modèle ne sait pas faire.
pub async fn active_capabilities(core: &Core) -> Vec<String> {
    let Ok(installed) = build_installed(core).await else {
        return Vec::new();
    };
    let mut out: Vec<String> = installed
        .into_iter()
        .filter(|e| e.enabled)
        .flat_map(|e| e.capabilities)
        .collect();
    out.sort();
    out.dedup();
    out
}
/// Une contribution de slot d'interface apportée par une extension.
fn slot_contribution(
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

/// Un bouton de composeur apporté par une extension.
fn action_composeur(
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

/// Ramener le type déclaré à l'un des quatre rendus possibles.
///
/// La documentation offre six mots ; l'écran n'a que quatre façons de montrer
/// un réglage. `number` et `prompt` deviennent du texte : mieux vaut un champ
/// honnête qu'un rendu promis et absent.
fn rendu_du_champ(declare: &str) -> String {
    match declare {
        "boolean" | "toggle" => "toggle",
        "select" | "choice" => "choice",
        "model" => "model",
        _ => "text",
    }
    .to_string()
}

/// Une section de réglages apportée par une extension.
fn section_reglages(
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

fn ui_entry(e: &locaryn_extensions::manifest::UiEntry) -> locaryn_shared_types::ExtensionUiEntry {
    locaryn_shared_types::ExtensionUiEntry {
        id: e.id.clone(),
        label: e.label.clone(),
        icon: e.icon.clone(),
    }
}

async fn build_installed(core: &Core) -> Result<Vec<InstalledExtension>, String> {
    let rows = core
        .storage
        .extensions
        .list()
        .await
        .map_err(|e| e.to_string())?;
    let rt = core.extensions.read().await;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let root = plugin_root(&row.manifest_path);
        // The manifest on disk is the source of truth for metadata; the row
        // only holds identity and the user's decisions.
        let manifest = root
            .as_deref()
            .and_then(|r| locaryn_extensions::manifest::load(r).ok());

        // An enabled plugin is already loaded; for a disabled one, parse it now
        // so the card can still say what it would contribute.
        let (components, load_errors) = match rt.loaded.get(&row.id) {
            Some(p) => (p.counts(), p.errors.clone()),
            None => match (&root, &manifest) {
                (Some(r), Some(m)) => {
                    let p = locaryn_extensions::loader::load_with_manifest(r, m.clone());
                    (p.counts(), p.errors)
                }
                _ => (
                    ExtensionComponents::default(),
                    vec![format!("fichiers introuvables : {}", row.manifest_path)],
                ),
            },
        };

        // Une extension active dont la permission `mcp` n'a jamais été
        // accordée est enregistrée, visible, cochée — et pourtant son serveur
        // n'est pas démarré : la boucle de rechargement la saute. Sans ce
        // signal, l'utilisateur ne voit qu'un panneau vide et conclut que
        // l'extension est cassée.
        let mut load_errors = load_errors;
        if row.enabled && components.mcp_servers > 0 && !row.granted.contains(&Permission::Mcp) {
            load_errors.push(
                "permission « mcp » non accordée : le serveur de cette extension n'est pas démarré, ses outils et ses modèles restent indisponibles."
                    .to_string(),
            );
        }

        let requested = manifest
            .as_ref()
            .map(locaryn_extensions::manifest::requested_permissions)
            .unwrap_or_default();
        let permissions: Vec<ExtensionPermissionState> = requested
            .iter()
            .map(|(perm, req)| ExtensionPermissionState {
                permission: perm.clone(),
                reason: req.reason.clone(),
                granted: row.granted.contains(perm),
                undecided: row.undecided.contains(perm),
            })
            .collect();

        out.push(InstalledExtension {
            id: row.id,
            display_name: manifest
                .as_ref()
                .and_then(|m| m.description.as_ref())
                .map(|_| row.name.clone())
                .unwrap_or_else(|| row.name.clone()),
            name: row.name.clone(),
            version: row.version,
            api_version: row.api_version,
            description: manifest.as_ref().and_then(|m| m.description.clone()),
            author: manifest.as_ref().and_then(|m| m.author.clone()),
            homepage: manifest.as_ref().and_then(|m| m.homepage.clone()),
            kind: row.kind,
            scope: row.scope,
            ecosystem: row.ecosystem,
            source: row.source,
            install_dir: root
                .map(|r| r.display().to_string())
                .unwrap_or_else(|| row.manifest_path.clone()),
            enabled: row.enabled,
            components,
            // Une extension désactivée n'apporte plus rien à l'interface : ses
            // capacités ne comptent que tant qu'elle est active, sinon le
            // Studio survivrait à sa propre désactivation.
            capabilities: if row.enabled {
                manifest
                    .as_ref()
                    .map(|m| m.capabilities.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            ui: manifest
                .as_ref()
                .map(|m| locaryn_shared_types::ExtensionUi {
                    slots: if row.enabled {
                        m.ui.slots.iter().map(slot_contribution).collect()
                    } else {
                        Vec::new()
                    },
                    nav_items: m.ui.nav_items.iter().map(ui_entry).collect(),
                    studio_tabs: m.ui.studio_tabs.iter().map(ui_entry).collect(),
                    // Une extension éteinte ne pose plus rien près du champ de
                    // saisie ni dans les réglages : sinon on garderait un
                    // bouton qui ne fait plus rien.
                    composer_actions: if row.enabled {
                        m.ui.composer_actions.iter().map(action_composeur).collect()
                    } else {
                        Vec::new()
                    },
                    settings_sections: if row.enabled {
                        m.ui.settings_sections
                            .iter()
                            .map(section_reglages)
                            .collect()
                    } else {
                        Vec::new()
                    },
                })
                .unwrap_or_default(),
            permissions,
            load_errors,
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
            created_at: row.created_at,
            updated_at: row.updated_at,
        });
    }
    Ok(out)
}

// ============================================================================
// Commands — installed extensions
// ============================================================================

#[tauri::command]
pub async fn list_extensions(core: State<'_, Core>) -> Result<Vec<InstalledExtension>, String> {
    if let Some(client) = core.remote_client() {
        if let Ok(exts) = client.list_extensions().await {
            return Ok(exts);
        }
    }
    build_installed(&core).await
}

/// GET /v1/capabilities du daemon local — la liste canonique des capacités
/// reconnues par ce serveur. La récupérer ici permet à l'interface
/// d'afficher les labels du serveur sans être recompilée.
#[tauri::command]
pub async fn list_capabilities(
) -> Result<Vec<locaryn_shared_types::capabilities::Capability>, String> {
    let cfg = locaryn_config::load(None).map_err(|e| e.to_string())?;
    let port = cfg.daemon.port;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("http://127.0.0.1:{port}/v1/capabilities");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("daemon injoignable : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("le daemon a répondu {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_extension_commands(
    core: State<'_, Core>,
) -> Result<Vec<ExtensionCommand>, String> {
    let rt = core.extensions.read().await;
    let mut out = Vec::new();
    for p in rt.loaded.values() {
        for c in &p.commands {
            out.push(ExtensionCommand {
                name: format!("{}:{}", p.manifest.name, c.name),
                plugin: p.manifest.name.clone(),
                description: c.description.clone(),
                arguments: c.arguments.clone(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Expand `/<plugin>:<command> args…` into the prompt it stands for.
#[tauri::command]
pub async fn resolve_extension_command(
    core: State<'_, Core>,
    name: String,
    args: String,
) -> Result<String, String> {
    let rt = core.extensions.read().await;
    let (_, def) = rt
        .command(&name)
        .ok_or_else(|| format!("« {name} » n'est pas une commande d'extension."))?;
    let parts: Vec<String> = args.split_whitespace().map(str::to_string).collect();
    Ok(locaryn_command_runtime::resolve(&def.body, &parts))
}

/// Lit un asset textuel ou script d'une extension installée (ex. `dist/ui.js`).
#[tauri::command]
pub async fn read_extension_asset(
    core: State<'_, Core>,
    extension_id: String,
    asset_path: String,
) -> Result<String, String> {
    let target = asset_file(&core, &extension_id, &asset_path).await?;
    std::fs::read_to_string(&target).map_err(|e| format!("lecture asset impossible : {e}"))
}

/// Le chemin relatif que l'on accepte de lire, confiné au dossier de
/// l'extension.
///
/// `asset_path` vient de l'interface. Sans cette vérification, `../../` y
/// lisait n'importe quel fichier de la machine.
fn confined_asset_path(asset_path: &str) -> Result<&str, String> {
    let clean = asset_path.trim_start_matches(['/', '\\']);
    // `is_absolute` dépend de la plateforme de compilation : `C:/…` passe pour
    // relatif sous Unix. Le préfixe de lecteur est donc écarté à la main, pour
    // que la règle ne change pas selon l'endroit où le code est compilé.
    let drive_prefix = clean
        .as_bytes()
        .get(1)
        .is_some_and(|byte| *byte == b':' && clean.as_bytes()[0].is_ascii_alphabetic());
    let refuse = clean.is_empty()
        || drive_prefix
        || std::path::Path::new(clean).is_absolute()
        || clean
            .split(['/', '\\'])
            .any(|segment| segment == ".." || segment.is_empty());
    if refuse {
        return Err(format!("chemin d'asset invalide : {asset_path}"));
    }
    Ok(clean)
}

/// Le fichier désigné par `asset_path` dans l'arborescence de l'extension.
///
/// Le chemin vient de l'interface : il est confiné au dossier de l'extension.
/// Sans cette vérification, `../../` y lisait n'importe quel fichier de la
/// machine.
async fn asset_file(
    core: &Core,
    extension_id: &str,
    asset_path: &str,
) -> Result<std::path::PathBuf, String> {
    let rows = core
        .storage
        .extensions
        .list()
        .await
        .map_err(|e| e.to_string())?;
    let row = rows
        .into_iter()
        .find(|r| r.id.to_string() == extension_id || r.name == extension_id)
        .ok_or_else(|| format!("extension « {extension_id} » introuvable"))?;

    let root_dir = plugin_root(&row.manifest_path)
        .unwrap_or_else(|| std::path::PathBuf::from(&row.manifest_path));

    let target = root_dir.join(confined_asset_path(asset_path)?);
    if !target.exists() {
        return Err(format!("asset introuvable : {}", target.display()));
    }
    Ok(target)
}

/// Où l'on garde la dernière version distante d'un asset de données.
fn asset_cache_file(extension_name: &str, asset_path: &str) -> std::path::PathBuf {
    let file: String = asset_path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    locaryn_config::storage_root()
        .join("extensions")
        .join(sanitize_server(extension_name))
        .join("cache")
        .join(file)
}

/// Un remplacement plausible de l'asset livré : même famille de document.
///
/// L'hôte ne lit pas le contenu — il ne sait pas ce qu'une extension y met. Il
/// vérifie seulement que la réponse distante est du JSON et annonce la même
/// version de schéma que le fichier livré, pour ne pas remplacer un catalogue
/// par une page d'erreur.
fn replaces_asset(local: &str, remote: &str) -> bool {
    let (Ok(local), Ok(remote)) = (
        serde_json::from_str::<serde_json::Value>(local),
        serde_json::from_str::<serde_json::Value>(remote),
    ) else {
        return false;
    };
    remote.is_object() && remote.get("schemaVersion") == local.get("schemaVersion")
}

/// Relire un asset de données en suivant l'adresse de mise à jour qu'il
/// déclare lui-même dans `refreshUrl`.
///
/// Un catalogue figé dans un paquet vieillit : l'extension publie donc une
/// adresse, l'hôte la relit et garde la dernière copie valide. Hors-ligne, ou
/// si la réponse ne ressemble pas au document livré, c'est le fichier du
/// paquet qui sert — jamais rien de moins.
#[tauri::command]
pub async fn refresh_extension_asset(
    core: State<'_, Core>,
    extension_id: String,
    asset_path: String,
) -> Result<String, String> {
    let target = asset_file(&core, &extension_id, &asset_path).await?;
    let local =
        std::fs::read_to_string(&target).map_err(|e| format!("lecture asset impossible : {e}"))?;

    let name = target
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or(&extension_id)
        .to_string();
    let cache = asset_cache_file(&name, &asset_path);
    let cached = std::fs::read_to_string(&cache).ok();

    let url = serde_json::from_str::<serde_json::Value>(&local)
        .ok()
        .and_then(|value| {
            value
                .get("refreshUrl")
                .and_then(|u| u.as_str())
                .map(str::to_string)
        })
        .filter(|url| url.starts_with("https://"));

    if let Some(url) = url {
        match core.http.get(&url).send().await {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(remote) if replaces_asset(&local, &remote) => {
                    if let Some(dir) = cache.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&cache, &remote);
                    return Ok(remote);
                }
                Ok(_) => tracing::warn!(url = %url, "catalogue distant ignoré : forme inattendue"),
                Err(error) => tracing::warn!(url = %url, %error, "catalogue distant illisible"),
            },
            Ok(response) => {
                tracing::warn!(url = %url, status = %response.status(), "catalogue distant refusé")
            }
            Err(error) => tracing::warn!(url = %url, %error, "catalogue distant injoignable"),
        }
    }

    // Hors-ligne : la dernière copie valide vaut mieux que le paquet d'origine,
    // qui peut dater de plusieurs versions.
    match cached {
        Some(cached) if replaces_asset(&local, &cached) => Ok(cached),
        _ => Ok(local),
    }
}

/// Pourquoi aucun serveur d'extension ne tourne, en une phrase actionnable.
///
/// « aucune extension active n'expose de serveur MCP » est vrai mais muet :
/// l'utilisateur a bien installé et activé l'extension, et ne peut pas deviner
/// qu'il lui manque une permission ou que le paquet installé ne contient pas
/// son serveur.
async fn raison_absence_serveur(core: &Core) -> String {
    let Ok(rows) = core.storage.extensions.list().await else {
        return "aucune extension active n'expose de serveur MCP".to_string();
    };
    let manquantes: Vec<String> = rows
        .iter()
        .filter(|row| row.enabled && !row.granted.contains(&Permission::Mcp))
        .map(|row| row.name.clone())
        .collect();
    if manquantes.is_empty() {
        return "aucune extension active n'expose de serveur MCP".to_string();
    }
    format!(
        "la permission « mcp » n'est pas accordée à {} — ouvrez Paramètres › Extensions pour l'accorder, son serveur ne démarre pas sans elle.",
        manquantes.join(", ")
    )
}

/// Invoke an MCP tool exposed by an enabled extension. This is deliberately
/// generic: the host does not know whether the tool generates an image,
/// synthesizes audio, or performs another extension-owned operation.
#[tauri::command]
pub async fn invoke_extension_tool(
    core: State<'_, Core>,
    tool: String,
    args: serde_json::Value,
) -> Result<String, String> {
    let clients: Vec<Arc<dyn McpClient>> = {
        let running = core.mcp.running.read().await;
        running.values().cloned().collect()
    };
    if clients.is_empty() {
        return Err(raison_absence_serveur(&core).await);
    }
    for client in clients {
        let Ok(capabilities) = client.discover().await else {
            continue;
        };
        if !capabilities.tools.iter().any(|entry| entry.name == tool) {
            continue;
        }
        let value = client
            .invoke_tool(&tool, &args)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string()));
    }
    Err(format!("aucune extension active n'expose « {tool} »"))
}

#[tauri::command]
pub async fn install_extension(
    core: State<'_, Core>,
    source: String,
    scope: Option<String>,
    workspace: Option<String>,
) -> Result<InstalledExtension, String> {
    if let Some(client) = core.remote_client() {
        let sc = scope.as_deref().unwrap_or("global");
        if let Ok(val) = client.install_extension(&source, sc).await {
            if let Ok(installed) = serde_json::from_value::<InstalledExtension>(val) {
                return Ok(installed);
            }
            if let Ok(exts) = client.list_extensions().await {
                if let Some(last) = exts.last() {
                    return Ok(last.clone());
                }
            }
        }
    }
    let scope = parse_scope(scope.as_deref());
    let workspace_root = workspace.as_deref().map(Path::new);

    let outcome = locaryn_extensions::install(&core.http, source.trim(), scope, workspace_root)
        .await
        .map_err(|e| e.to_string())?;

    let requested: Vec<Permission> =
        locaryn_extensions::manifest::requested_permissions(&outcome.manifest)
            .into_iter()
            .map(|(p, _)| p)
            .collect();

    let record = core
        .storage
        .extensions
        .upsert(NewExtension {
            name: outcome.manifest.name.clone(),
            version: outcome.manifest.version.clone(),
            api_version: outcome.manifest.api_version.clone(),
            kind: ExtensionKind::Plugin,
            scope,
            ecosystem: outcome.ecosystem,
            source: Some(outcome.source.clone()),
            manifest_path: outcome.root.join("morph.json").display().to_string(),
            requested,
        })
        .await
        .map_err(|e| e.to_string())?;

    // Les figures du dépôt entrent en base dès l'installation : l'écran
    // Figures les montre aussitôt, et réinstaller les met à jour sans
    // toucher à celles écrites à la main.
    let importees = locaryn_storage::figures_import::importer(
        &core.storage.figures,
        &outcome.root,
        &outcome.manifest.name,
    )
    .await;
    if importees > 0 {
        tracing::info!(name = %outcome.manifest.name, importees, "figures du dépôt importées");
    }

    tracing::info!(
        name = %outcome.manifest.name,
        ecosystem = %outcome.ecosystem.as_str(),
        components = outcome.loaded.counts().total(),
        partial = outcome.partial,
        "extension installée"
    );
    for note in &outcome.notes {
        tracing::info!(name = %outcome.manifest.name, "{note}");
    }

    // Installed but not enabled: the permission modal decides that next.
    let all = build_installed(&core).await?;
    all.into_iter()
        .find(|e| e.id == record.id)
        .ok_or_else(|| "extension introuvable après installation".to_string())
}

/// Re-run the install pipeline from the extension's stored source, replacing
/// its files with the latest version.
///
/// The row is keyed by name+scope, so the upsert keeps the same id and never
/// touches `enabled` or `granted`: the user's decisions survive the update.
/// The runtime is then reloaded so the new components are live immediately.
#[tauri::command]
pub async fn update_extension(
    core: State<'_, Core>,
    id: String,
) -> Result<InstalledExtension, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let record = core
        .storage
        .extensions
        .get(uid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;
    let source = record.source.clone().ok_or_else(|| {
        "impossible de mettre à jour : aucune source enregistrée pour cette extension".to_string()
    })?;
    reinstall_from_source(&core, uid, &record.name, record.scope, &source).await
}

/// Comme `update_extension`, mais avec une source explicite : la source
/// d'installation d'une entrée du catalogue « Découvrir », pour une extension
/// déjà installée dont la source enregistrée manque ou diffère.
#[tauri::command]
pub async fn update_extension_source(
    core: State<'_, Core>,
    id: String,
    source: String,
) -> Result<InstalledExtension, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let record = core
        .storage
        .extensions
        .get(uid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;
    reinstall_from_source(&core, uid, &record.name, record.scope, source.trim()).await
}

/// Pipeline commun d'une réinstallation depuis une source (enregistrée ou du
/// catalogue) : télécharge le paquet, upsert (même id, `enabled` et `granted`
/// intacts), puis redémarre les serveurs MCP du plugin et recharge le runtime.
async fn reinstall_from_source(
    core: &Core,
    _id: Uuid,
    name: &str,
    scope: ExtensionScope,
    source: &str,
) -> Result<InstalledExtension, String> {
    // Libérer les fichiers avant de les remplacer : le serveur du plugin tient
    // son propre exécutable ouvert, et Windows refuse alors de l'écraser.
    stop_plugin_mcp(core, name).await;

    let outcome = match locaryn_extensions::install(&core.http, source.trim(), scope, None).await {
        Ok(outcome) => outcome,
        Err(error) => {
            // L'extension reste celle d'avant : lui rendre son serveur plutôt
            // que de la laisser installée et muette après un échec.
            let _ = reload(core).await;
            let raison = error.to_string();
            return Err(
                if raison.to_lowercase().contains("denied")
                    || raison.contains("accès refusé")
                    || raison.contains("os error 5")
                {
                    format!(
                    "{raison} — un fichier de l'extension est encore ouvert.                      Fermez ce qui l'utilise, ou redémarrez l'application, puis réessayez."
                )
                } else {
                    raison
                },
            );
        }
    };

    let requested: Vec<Permission> =
        locaryn_extensions::manifest::requested_permissions(&outcome.manifest)
            .into_iter()
            .map(|(p, _)| p)
            .collect();

    let updated = core
        .storage
        .extensions
        .upsert(NewExtension {
            name: outcome.manifest.name.clone(),
            version: outcome.manifest.version.clone(),
            api_version: outcome.manifest.api_version.clone(),
            kind: ExtensionKind::Plugin,
            scope,
            ecosystem: outcome.ecosystem,
            source: Some(outcome.source.clone()),
            manifest_path: outcome.root.join("morph.json").display().to_string(),
            requested,
        })
        .await
        .map_err(|e| e.to_string())?;

    let importees = locaryn_storage::figures_import::importer(
        &core.storage.figures,
        &outcome.root,
        &outcome.manifest.name,
    )
    .await;
    if importees > 0 {
        tracing::info!(name = %outcome.manifest.name, importees, "figures du dépôt importées");
    }

    tracing::info!(
        name = %outcome.manifest.name,
        version = %outcome.manifest.version,
        "extension mise à jour"
    );

    // `name` est l'ancien nom : il pointe les serveurs MCP actuellement
    // enregistrés. Un éventuel renommage dans la nouvelle version est rattrapé
    // par le reload que `restart_plugin_mcp` déclenche.
    restart_plugin_mcp(core, name).await?;

    build_installed(core)
        .await?
        .into_iter()
        .find(|e| e.id == updated.id)
        .ok_or_else(|| "extension introuvable après mise à jour".to_string())
}

/// Un seul reload du runtime des extensions, après la mise à jour en lot.
///
/// Chaque `update_extension` parallèle relance déjà un `reload(core)` global,
/// et deux reloads qui se chevauchent peuvent laisser l'état final sans la
/// dernière version d'une extension (chacun lit la base à un instant donné), ou
/// double-démarrer un serveur MCP auto (client orphelin sans shutdown). Un
/// dernier reload unique, après que toutes les upserts ont committé, garantit
/// un état cohérent — c'est ce passage final qui rattrape les chevauchements,
/// il ne faut pas le retirer. Idempotent, donc inoffensif quand une seule
/// extension a été mise à jour.
#[tauri::command]
pub async fn reload_extensions(core: State<'_, Core>) -> Result<Vec<InstalledExtension>, String> {
    reload(&core).await?;
    build_installed(&core).await
}

// ============================================================================
// Version checks (the "mise à jour dispo" badge)
// ============================================================================

/// One version check for an installed extension.
#[derive(Debug, Clone, Serialize)]
pub struct ExtensionUpdateCheck {
    pub id: String,
    /// Latest version on the default branch, when the source is checkable.
    pub latest_version: Option<String>,
    /// True when a newer version exists.
    pub update_available: bool,
    /// Why the check could not run (network, source form…).
    pub error: Option<String>,
}

/// Aperçu d'une source d'installation sans télécharger le paquet : ce que le
/// manifeste déclare (nom, version, écosystème, permissions demandées).
/// Alimente la carte de confirmation de la fenêtre d'ajout.
#[tauri::command]
pub async fn preview_extension_source(
    core: State<'_, Core>,
    source: String,
) -> Result<locaryn_extensions::SourcePreview, String> {
    locaryn_extensions::preview_source(&core.http, source.trim())
        .await
        .map_err(|e| e.to_string())
}

/// Compare every installed extension against its GitHub source's default
/// branch. Sources that cannot be checked (local path, pinned ref, no manifest
/// with a version) report `None` rather than an error.
///
/// The fetches run in parallel (`join_all`) so the badge of a panel with many
/// installed extensions is not held up by N sequential round-trips.
#[tauri::command]
pub async fn check_extension_updates(
    core: State<'_, Core>,
) -> Result<Vec<ExtensionUpdateCheck>, String> {
    let rows = core
        .storage
        .extensions
        .list()
        .await
        .map_err(|e| e.to_string())?;

    // Owned inputs first, so the futures below borrow no rows.
    let checks: Vec<(String, String, Option<String>)> = rows
        .into_iter()
        .map(|r| (r.id.to_string(), r.version, r.source))
        .collect();
    let http = &core.http;
    let results: Vec<(String, String, Result<Option<String>, SourceError>)> =
        futures::future::join_all(checks.iter().map(|(id, version, source)| async move {
            let latest = match source {
                Some(s) => latest_github_version(http, s).await,
                None => Ok(None),
            };
            (id.clone(), version.clone(), latest)
        }))
        .await;

    let mut out = Vec::with_capacity(results.len());
    for (id, installed, latest) in results {
        match latest {
            Ok(Some(v)) => out.push(ExtensionUpdateCheck {
                id,
                latest_version: Some(v.clone()),
                update_available: version_gt(&v, &installed),
                error: None,
            }),
            Ok(None) => out.push(ExtensionUpdateCheck {
                id,
                latest_version: None,
                update_available: false,
                error: None,
            }),
            Err(e) => out.push(ExtensionUpdateCheck {
                id,
                latest_version: None,
                update_available: false,
                error: Some(e.to_string()),
            }),
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Grant exactly this set of permissions, revoking anything not listed.
#[tauri::command]
pub async fn set_extension_permissions(
    core: State<'_, Core>,
    id: String,
    granted: Vec<Permission>,
) -> Result<Vec<InstalledExtension>, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    core.storage
        .extensions
        .set_granted(uid, &granted)
        .await
        .map_err(|e| e.to_string())?;
    reload(&core).await?;
    build_installed(&core).await
}

#[tauri::command]
pub async fn set_extension_enabled(
    core: State<'_, Core>,
    id: String,
    enabled: bool,
) -> Result<Vec<InstalledExtension>, String> {
    if let Some(client) = core.remote_client() {
        let _ = client.set_extension_enabled(&id, enabled).await;
        if let Ok(exts) = client.list_extensions().await {
            return Ok(exts);
        }
    }
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    core.storage
        .extensions
        .set_enabled(uid, enabled)
        .await
        .map_err(|e| e.to_string())?;
    reload(&core).await?;
    if enabled {
        preparer_passerelle(&core, uid).await;
    }
    build_installed(&core).await
}

/// Poser ce dont l'extension a besoin pour fonctionner, une fois activée.
///
/// Une extension qui apporte une passerelle locale n'apporte rien tant que
/// celle-ci n'est pas sur la machine : son dossier s'ouvre sur un catalogue
/// vide, et l'utilisateur doit deviner qu'il lui reste une commande à taper.
/// Activer le morph installe donc aussi sa passerelle, puis la démarre.
///
/// En arrière-plan, et sans jamais faire échouer l'activation : un
/// gestionnaire de paquets absent ou un réseau coupé sont des contretemps, pas
/// une raison de refuser une extension par ailleurs installée. Le dossier du
/// fournisseur dira ce qui manque, avec le bouton pour recommencer.
async fn preparer_passerelle(core: &Core, extension_id: Uuid) {
    let providers = locaryn_cloud_providers::list_infos(&locaryn_cloud_providers::Host {
        storage: &core.storage,
        data_dir: &core.data_dir,
        http: &core.http,
        keychain: core.keychain.as_ref(),
    })
    .await;
    let Some(fournisseur) = providers
        .into_iter()
        .find(|p| p.extension_id == extension_id.to_string() && p.is_local)
    else {
        return;
    };
    if fournisseur.installed && !fournisseur.can_install {
        return;
    }

    // La tâche emporte ce dont elle a besoin, pas le cœur de l'application :
    // une installation dure des dizaines de secondes, et l'activation ne doit
    // pas attendre.
    let storage = core.storage.clone();
    let data_dir = core.data_dir.clone();
    let http = core.http.clone();
    let keychain = core.keychain.clone();
    tokio::spawn(async move {
        let host = locaryn_cloud_providers::Host {
            storage: &storage,
            data_dir: &data_dir,
            http: &http,
            keychain: keychain.as_ref(),
        };
        let issue = match locaryn_cloud_providers::find(&host, &fournisseur.id).await {
            Ok(p) => locaryn_cloud_providers::gateway::start(&host, &p)
                .await
                .map(|s| s.running),
            Err(e) => Err(e),
        };
        match issue {
            Ok(true) => tracing::info!(
                fournisseur = %fournisseur.id,
                "passerelle installée et démarrée à l'activation de l'extension"
            ),
            Ok(false) => tracing::warn!(
                fournisseur = %fournisseur.id,
                "passerelle lancée mais sans réponse : son dossier dira quoi faire"
            ),
            Err(e) => tracing::warn!(
                fournisseur = %fournisseur.id,
                erreur = %e,
                "passerelle non préparée : son dossier dira quoi faire"
            ),
        }
    });
}

#[tauri::command]
pub async fn remove_extension(
    core: State<'_, Core>,
    id: String,
    workspace: Option<String>,
) -> Result<Vec<InstalledExtension>, String> {
    if let Some(client) = core.remote_client() {
        let _ = client.remove_extension(&id).await;
        if let Ok(exts) = client.list_extensions().await {
            return Ok(exts);
        }
    }
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let record = core
        .storage
        .extensions
        .get(uid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;

    // Rendre son serveur avant d'effacer ses fichiers : tant qu'il tourne,
    // Windows garde son exécutable verrouillé et la suppression échoue sur
    // « Accès refusé », en laissant l'extension à moitié désinstallée.
    stop_plugin_mcp(&core, &record.name).await;

    // Rendre son serveur avant d'effacer ses fichiers : tant qu'il tourne,
    // Windows garde son exécutable verrouillé, la suppression échoue sur
    // « Accès refusé » et l'extension reste à moitié désinstallée.
    stop_plugin_mcp(&core, &record.name).await;

    core.storage
        .extensions
        .delete(uid)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(root) = plugin_root(&record.manifest_path) {
        let workspace_root = workspace.as_deref().map(Path::new);
        if !locaryn_extensions::remove_files(&root, record.scope, workspace_root) {
            tracing::warn!(path = %root.display(), "fichiers de l'extension non supprimés");
        }
    }

    reload(&core).await?;
    build_installed(&core).await
}

fn parse_scope(s: Option<&str>) -> ExtensionScope {
    match s {
        Some("global") => ExtensionScope::Global,
        Some("workspace") => ExtensionScope::Workspace,
        Some("session") => ExtensionScope::Session,
        _ => ExtensionScope::User,
    }
}

// ============================================================================
// Extension-contributed configuration
// ============================================================================

/// An extension's settings form, as declared by the extension itself.
///
/// Locaryn renders `schema` and stores `values`; it knows nothing about what
/// any particular extension needs. That is the whole point: an extension says
/// what it wants on screen, the app draws it, and an app that has never seen
/// that extension carries no trace of it.
#[derive(Debug, Clone, Serialize)]
pub struct ExtensionConfig {
    /// Field map from the manifest's `config.schema`. Empty when the extension
    /// declares no settings — the UI then shows nothing rather than an empty form.
    pub schema: serde_json::Value,
    /// Current values, defaults filled in from the schema.
    pub values: serde_json::Value,
}

/// Where an extension's settings live: inside its own directory, so removing
/// the extension removes them with it. Nothing is written to the app's own
/// database or config.
fn config_file(root: &Path) -> PathBuf {
    root.join(".data").join("config.json")
}

fn read_values(root: &Path, schema: &serde_json::Value) -> serde_json::Value {
    let mut values = serde_json::Map::new();

    // Defaults come from the schema, so a field added in a plugin update shows
    // its declared default rather than a blank.
    if let Some(fields) = schema.as_object() {
        for (key, field) in fields {
            let default = field
                .get("default")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            values.insert(key.clone(), default);
        }
    }

    if let Ok(raw) = std::fs::read_to_string(config_file(root)) {
        if let Ok(serde_json::Value::Object(stored)) = serde_json::from_str(&raw) {
            for (key, value) in stored {
                values.insert(key, value);
            }
        }
    }

    serde_json::Value::Object(values)
}

/// Le formulaire d'une extension, tel qu'elle le déclare dans
/// `ui_contributions.settings_sections` — le mécanisme unique de réglages,
/// sur l'ordinateur comme sur le téléphone.
///
/// Une section devient un groupe du formulaire ; un champ y est converti au
/// vocabulaire canonique (`boolean`, `select`, `model`, `string`, `number`,
/// `prompt`). L'ancien `config.schema` reste lu en secours, pour ne pas
/// casser une extension qui ne connaît que lui.
fn manifest_schema(root: &Path) -> serde_json::Value {
    let Ok(m) = locaryn_extensions::manifest::load(root) else {
        return serde_json::Value::Null;
    };

    let mut schema = serde_json::Map::new();
    for section in &m.ui.settings_sections {
        for field in &section.fields {
            let kind = match field.kind.as_str() {
                "boolean" | "toggle" => "boolean",
                "select" | "choice" => "select",
                "model" => "model",
                "number" => "number",
                "prompt" => "prompt",
                _ => "string",
            };
            let mut f = serde_json::Map::new();
            f.insert("type".into(), serde_json::Value::String(kind.into()));
            f.insert(
                "title".into(),
                serde_json::Value::String(field.label.clone()),
            );
            if let Some(hint) = &field.hint {
                f.insert(
                    "description".into(),
                    serde_json::Value::String(hint.clone()),
                );
            }
            if !field.options.is_empty() {
                f.insert("options".into(), serde_json::json!(field.options));
            }
            if let Some(default) = &field.default {
                f.insert("default".into(), serde_json::Value::String(default.clone()));
            }
            // Le groupe du formulaire, c'est la section.
            f.insert(
                "group".into(),
                serde_json::Value::String(section.title.clone()),
            );
            schema.insert(field.key.clone(), serde_json::Value::Object(f));
        }
    }

    if schema.is_empty() {
        // Aucune section déclarée : secours sur l'ancien `config.schema`.
        if let Some(c) = m.config {
            return c.schema;
        }
        return serde_json::Value::Null;
    }
    serde_json::Value::Object(schema)
}

async fn config_for(core: &Core, id: Uuid) -> Result<(PathBuf, ExtensionConfig), String> {
    let record = core
        .storage
        .extensions
        .get(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;
    let root = plugin_root(&record.manifest_path)
        .ok_or_else(|| "dossier de l'extension introuvable".to_string())?;
    let schema = manifest_schema(&root);
    let values = read_values(&root, &schema);
    Ok((root, ExtensionConfig { schema, values }))
}

#[tauri::command]
pub async fn get_extension_config(
    core: State<'_, Core>,
    id: String,
) -> Result<ExtensionConfig, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    Ok(config_for(&core, uid).await?.1)
}

/// Merge a patch into the extension's settings and persist it.
///
/// Keys absent from the schema are dropped: the form is the contract, and a
/// stray key would sit in the file forever with nothing to display or clear it.
#[tauri::command]
pub async fn set_extension_config(
    core: State<'_, Core>,
    id: String,
    values: serde_json::Value,
) -> Result<ExtensionConfig, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let (root, current) = config_for(&core, uid).await?;

    let Some(patch) = values.as_object() else {
        return Err("les valeurs doivent être un objet".into());
    };
    let known = current.schema.as_object();
    let mut merged = current.values.as_object().cloned().unwrap_or_default();
    for (key, value) in patch {
        if known.is_some_and(|k| !k.contains_key(key)) {
            tracing::warn!(key = %key, "réglage inconnu ignoré");
            continue;
        }
        merged.insert(key.clone(), value.clone());
    }

    let path = config_file(&root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&merged).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    // An extension that already runs read the old values at startup. Restart
    // its MCP servers so a saved change takes effect now rather than at the
    // next launch.
    let record = core
        .storage
        .extensions
        .get(uid)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(record) = record.filter(|r| r.enabled) {
        restart_plugin_mcp(&core, &record.name).await?;
    }

    Ok(config_for(&core, uid).await?.1)
}

/// Stop and restart every MCP server this plugin registered, so a change to
/// its files takes effect now rather than at the next launch.
/// Arrêter les serveurs MCP d'un plugin, sans recharger le runtime.
///
/// À faire **avant** de remplacer ses fichiers : sous Windows, un exécutable en
/// cours d'exécution ne peut être ni supprimé ni écrasé. La mise à jour d'une
/// extension qui embarque son serveur échouait donc sur « accès refusé », et
/// l'utilisateur restait sur l'ancienne version sans savoir pourquoi.
async fn stop_plugin_mcp(core: &Core, plugin_name: &str) {
    let prefix = format!("{}__", sanitize_server(plugin_name));
    let names: Vec<String> = {
        let rt = core.extensions.read().await;
        rt.mcp_names
            .iter()
            .filter(|n| n.starts_with(&prefix))
            .cloned()
            .collect()
    };
    for name in names {
        if let Some(client) = core.mcp.running.write().await.remove(&name) {
            let _ = client.shutdown().await;
        }
    }
}

async fn restart_plugin_mcp(core: &Core, plugin_name: &str) -> Result<(), String> {
    stop_plugin_mcp(core, plugin_name).await;
    reload(core).await?;
    Ok(())
}

// ============================================================================
// Extension-declared MCP servers (env + auto-start, edited in the settings
// panel alongside the schema form)
// ============================================================================

/// One MCP server declared by an extension, as the settings panel edits it.
/// Only `env` and `auto_start` are user-editable; the command/URL and the
/// transport come from the plugin's own file and stay untouched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionMcpServer {
    /// Server name as declared in the plugin's mcp file.
    pub name: String,
    /// `stdio` | `http`.
    pub transport: String,
    /// Command line (stdio) or URL (http), shown read-only.
    pub target: String,
    pub env: HashMap<String, String>,
    pub auto_start: bool,
}

fn entry_to_dto(name: String, entry: McpServerEntry) -> ExtensionMcpServer {
    let (transport, target) = match entry.transport {
        Transport::Stdio => {
            let mut line = entry.command.unwrap_or_default();
            for a in &entry.args {
                line.push(' ');
                line.push_str(a);
            }
            ("stdio".to_string(), line)
        }
        Transport::Http => ("http".to_string(), entry.url.unwrap_or_default()),
    };
    ExtensionMcpServer {
        name,
        transport,
        target,
        env: entry.env,
        auto_start: entry.auto_start,
    }
}

/// The plugin's own MCP config file, mirroring the loader's lookup order.
/// Returns `None` when the plugin declares no MCP servers.
fn extension_mcp_file(root: &Path, manifest: &PluginManifest) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = manifest
        .components
        .mcp
        .as_ref()
        .map(|m| vec![root.join(m)])
        .unwrap_or_default();
    candidates.extend(
        ["mcp/mcp.json", ".mcp.json", "mcp.json"]
            .iter()
            .map(|n| root.join(n)),
    );
    candidates.into_iter().find(|p| p.is_file())
}

#[tauri::command]
pub async fn get_extension_mcp_servers(
    core: State<'_, Core>,
    id: String,
) -> Result<Vec<ExtensionMcpServer>, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let record = core
        .storage
        .extensions
        .get(uid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;
    let root = plugin_root(&record.manifest_path)
        .ok_or_else(|| "dossier de l'extension introuvable".to_string())?;
    let manifest = locaryn_extensions::manifest::load(&root).map_err(|e| e.to_string())?;
    let Some(path) = extension_mcp_file(&root, &manifest) else {
        return Ok(Vec::new());
    };
    let cfg = McpConfig::load(&path).map_err(|e| e.to_string())?;
    let mut out: Vec<ExtensionMcpServer> = cfg
        .mcp_servers
        .into_iter()
        .map(|(name, entry)| entry_to_dto(name, entry))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Merge `env` + `auto_start` back into the plugin's own mcp file, by server
/// name. Anything not sent (command, args, URL, headers) is preserved, and
/// servers not listed are left alone. The plugin's running servers are then
/// restarted so the change is live immediately.
#[tauri::command]
pub async fn set_extension_mcp_servers(
    core: State<'_, Core>,
    id: String,
    servers: Vec<ExtensionMcpServer>,
) -> Result<Vec<ExtensionMcpServer>, String> {
    let uid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let record = core
        .storage
        .extensions
        .get(uid)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;
    let root = plugin_root(&record.manifest_path)
        .ok_or_else(|| "dossier de l'extension introuvable".to_string())?;
    let manifest = locaryn_extensions::manifest::load(&root).map_err(|e| e.to_string())?;
    let path = extension_mcp_file(&root, &manifest)
        .ok_or_else(|| "cette extension ne déclare aucun serveur MCP modifiable".to_string())?;

    // Guard: `McpConfig::load` silently defaults on a parse error, and saving
    // would then overwrite a malformed file with an empty one. Refuse instead.
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|e| format!("fichier mcp illisible : {e}"))?;
    let mut cfg = McpConfig::load(&path).map_err(|e| e.to_string())?;
    for s in &servers {
        if let Some(entry) = cfg.mcp_servers.get_mut(&s.name) {
            entry.env = s.env.clone();
            entry.auto_start = s.auto_start;
        } else {
            tracing::warn!(server = %s.name, "serveur MCP inconnu ignoré");
        }
    }
    cfg.save(&path).map_err(|e| e.to_string())?;

    if record.enabled {
        restart_plugin_mcp(&core, &record.name).await?;
    }

    get_extension_mcp_servers(core, id).await
}

// ============================================================================
// Commands — the catalog
// ============================================================================

/// User choices about catalog sources. Built-in sources are not copied here;
/// only the ids the user switched off, plus any source they added.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SourcePrefs {
    #[serde(default)]
    disabled: Vec<String>,
    #[serde(default)]
    custom: Vec<CatalogSource>,
}

fn prefs_path() -> PathBuf {
    locaryn_config::global_dir().join("extension-sources.json")
}

fn load_prefs() -> SourcePrefs {
    std::fs::read_to_string(prefs_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_prefs(p: &SourcePrefs) {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(p) {
        let _ = std::fs::write(path, raw);
    }
}

fn effective_sources() -> Vec<CatalogSource> {
    let prefs = load_prefs();
    let mut all = locaryn_extensions::builtin_sources();
    for s in &mut all {
        s.enabled = !prefs.disabled.contains(&s.id);
    }
    all.extend(prefs.custom.iter().cloned().map(|mut s| {
        s.enabled = !prefs.disabled.contains(&s.id);
        s
    }));
    all
}

#[tauri::command]
pub async fn list_catalog_sources(_core: State<'_, Core>) -> Result<Vec<CatalogSource>, String> {
    Ok(effective_sources())
}

/// Add a Claude Code marketplace by `owner/repo` or by its GitHub URL.
#[tauri::command]
pub async fn add_catalog_source(
    _core: State<'_, Core>,
    spec: String,
) -> Result<Vec<CatalogSource>, String> {
    let parsed = locaryn_extensions::source::parse(spec.trim()).map_err(|e| e.to_string())?;
    let locaryn_extensions::InstallSource::GitHub { owner, repo, .. } = parsed else {
        return Err(
            "Indiquez un dépôt GitHub (owner/repo) contenant .claude-plugin/marketplace.json."
                .into(),
        );
    };
    let source = locaryn_extensions::catalog::marketplace_source(&owner, &repo);
    let mut prefs = load_prefs();
    if prefs.custom.iter().any(|s| s.id == source.id)
        || locaryn_extensions::builtin_sources()
            .iter()
            .any(|s| s.id == source.id)
    {
        return Err(format!("{owner}/{repo} est déjà dans la liste."));
    }
    prefs.custom.push(source);
    save_prefs(&prefs);
    Ok(effective_sources())
}

#[tauri::command]
pub async fn set_catalog_source_enabled(
    _core: State<'_, Core>,
    id: String,
    enabled: bool,
) -> Result<Vec<CatalogSource>, String> {
    let mut prefs = load_prefs();
    prefs.disabled.retain(|d| d != &id);
    if !enabled {
        prefs.disabled.push(id);
    }
    save_prefs(&prefs);
    Ok(effective_sources())
}

#[tauri::command]
pub async fn remove_catalog_source(
    _core: State<'_, Core>,
    id: String,
) -> Result<Vec<CatalogSource>, String> {
    let mut prefs = load_prefs();
    let before = prefs.custom.len();
    prefs.custom.retain(|s| s.id != id);
    if prefs.custom.len() == before {
        return Err(
            "Les sources fournies avec l'application ne peuvent qu'être désactivées.".into(),
        );
    }
    prefs.disabled.retain(|d| d != &id);
    save_prefs(&prefs);
    Ok(effective_sources())
}

/// Fetch every enabled source. Slow (a megabyte of Gemini index), so the UI
/// calls it on demand rather than on every visit to the store.
#[tauri::command]
pub async fn refresh_extension_catalog(core: State<'_, Core>) -> Result<CatalogSnapshot, String> {
    let client = locaryn_extensions::CatalogClient::new(core.http.clone());
    let snapshot = client.refresh(&effective_sources()).await;
    Ok(mark_installed(&core, snapshot).await)
}

/// Browse the last refresh. Filtering happens here so the 1300-entry Gemini
/// index never crosses the IPC boundary.
#[tauri::command]
pub async fn browse_extension_catalog(
    core: State<'_, Core>,
    query: Option<String>,
    ecosystem: Option<String>,
    limit: Option<u32>,
) -> Result<CatalogSnapshot, String> {
    let client = locaryn_extensions::CatalogClient::new(core.http.clone());
    let Some(snapshot) = client.cached() else {
        return Ok(CatalogSnapshot {
            entries: Vec::new(),
            sources: Vec::new(),
            fetched_at: None,
            stale: true,
        });
    };
    let eco = ecosystem.as_deref().and_then(parse_ecosystem);
    let filtered = locaryn_extensions::catalog::filter(
        &snapshot.entries,
        query.as_deref().unwrap_or(""),
        eco,
        limit.unwrap_or(60) as usize,
    );
    Ok(mark_installed(
        &core,
        CatalogSnapshot {
            entries: filtered,
            ..snapshot
        },
    )
    .await)
}

/// Flag entries already installed so the store shows "Installé" instead of
/// offering the same plugin twice.
async fn mark_installed(core: &Core, mut snapshot: CatalogSnapshot) -> CatalogSnapshot {
    let installed: Vec<(String, Option<String>)> = match core.storage.extensions.list().await {
        Ok(rows) => rows.into_iter().map(|r| (r.name, r.source)).collect(),
        Err(_) => Vec::new(),
    };
    for e in &mut snapshot.entries {
        e.installed = installed.iter().any(|(name, source)| {
            name == &e.name
                || source
                    .as_deref()
                    .map(|s| s == e.install_source)
                    .unwrap_or(false)
        });
    }
    snapshot
}

fn parse_ecosystem(s: &str) -> Option<ExtensionEcosystem> {
    match s {
        "locaryn" => Some(ExtensionEcosystem::Locaryn),
        "claude_code" => Some(ExtensionEcosystem::ClaudeCode),
        "gemini_cli" => Some(ExtensionEcosystem::GeminiCli),
        "opencode" => Some(ExtensionEcosystem::OpenCode),
        "mcp" => Some(ExtensionEcosystem::Mcp),
        _ => None,
    }
}

/// One catalog entry, resolved for the details pane.
#[tauri::command]
pub async fn catalog_entry_details(
    core: State<'_, Core>,
    id: String,
) -> Result<Option<CatalogEntry>, String> {
    let client = locaryn_extensions::CatalogClient::new(core.http.clone());
    Ok(client
        .cached()
        .and_then(|s| s.entries.into_iter().find(|e| e.id == id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_asset_path_never_escapes_the_extension() {
        assert_eq!(confined_asset_path("dist/ui.js").unwrap(), "dist/ui.js");
        assert_eq!(confined_asset_path("/dist/ui.js").unwrap(), "dist/ui.js");
        assert!(confined_asset_path("../../secrets.txt").is_err());
        assert!(confined_asset_path("dist/../../etc/passwd").is_err());
        assert!(confined_asset_path("").is_err());
        // Rejeté sur toute plateforme : la règle ne peut pas dépendre de
        // l'endroit où le binaire a été compilé.
        assert!(confined_asset_path("C:/Windows/win.ini").is_err());
        // Une barre de tête est une écriture du chemin dans le manifeste, pas
        // une racine : elle reste confinée au dossier de l'extension.
        assert_eq!(confined_asset_path("/etc/passwd").unwrap(), "etc/passwd");
    }

    #[test]
    fn a_remote_catalogue_replaces_only_a_document_of_the_same_shape() {
        let local = r#"{"schemaVersion":1,"models":[]}"#;
        assert!(replaces_asset(
            local,
            r#"{"schemaVersion":1,"models":[{"id":"x"}]}"#
        ));
        // Une page d'erreur, une redirection HTML, un schéma d'une autre
        // version : le fichier livré reste en place.
        assert!(!replaces_asset(local, "<html>404</html>"));
        assert!(!replaces_asset(local, r#"{"schemaVersion":2}"#));
        assert!(!replaces_asset(local, "[]"));
    }

    #[test]
    fn server_names_stay_callable() {
        // `mcp__<server>__<tool>` is the tool name the model sees.
        assert_eq!(sanitize_server("my plugin"), "my-plugin");
        assert_eq!(sanitize_server("a_b.c"), "a-b-c");
        assert_eq!(sanitize_server("ok-9"), "ok-9");
    }

    #[test]
    fn plugin_root_is_the_manifest_directory() {
        let r = plugin_root("/a/b/morph.json").unwrap();
        assert!(r.ends_with("b"));
    }

    #[test]
    fn scope_defaults_to_user() {
        assert_eq!(parse_scope(None), ExtensionScope::User);
        assert_eq!(parse_scope(Some("workspace")), ExtensionScope::Workspace);
        assert_eq!(parse_scope(Some("nonsense")), ExtensionScope::User);
    }
}
