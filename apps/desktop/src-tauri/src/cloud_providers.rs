//! Les fournisseurs de modèles apportés par une extension, côté application.
//!
//! Toute la mécanique — découverte, clé, catalogue, passerelle locale — vit
//! dans `locaryn-cloud-providers`, parce que le service en a besoin autant que
//! l'application : c'est le même fournisseur qui alimente le dossier de « Mes
//! modèles » et l'API compatible OpenAI du mode serveur. Ce module n'est que
//! la façade Tauri : il ouvre les poignées de l'application au socle partagé,
//! et vérifie les permissions que l'interface ne peut pas vérifier seule.

use crate::Core;
use locaryn_cloud_providers as cloud;
use locaryn_cloud_providers::{CloudModel, CloudProviderInfo, CloudProviderStatus, Host};
use tauri::State;

/// Les poignées de l'application, prêtées au socle partagé.
fn host(core: &Core) -> Host<'_> {
    Host {
        storage: &core.storage,
        data_dir: &core.data_dir,
        http: &core.http,
        keychain: core.keychain.as_ref(),
    }
}

/// La clé du fournisseur actif, quand le fournisseur actif est un catalogue
/// distant. C'est le seul point d'entrée dont la conversation a besoin.
pub fn key_for_active_provider(
    core: &Core,
    provider: &locaryn_shared_types::Provider,
) -> Option<String> {
    cloud::key_for_active_provider(&host(core), provider)
}

// ============================================================================
// Commandes
// ============================================================================

/// Les catalogues disponibles, avec ce qu'on sait déjà d'eux.
#[tauri::command]
pub async fn cloud_providers(core: State<'_, Core>) -> Result<Vec<CloudProviderInfo>, String> {
    Ok(cloud::list_infos(&host(&core)).await)
}

/// Enregistrer la clé d'un fournisseur dans le trousseau du système.
#[tauri::command]
pub async fn cloud_provider_set_key(
    core: State<'_, Core>,
    provider: String,
    key: String,
) -> Result<(), String> {
    let h = host(&core);
    let p = cloud::find(&h, &provider).await?;
    cloud::set_key(&h, &p.id, &key)
}

/// Oublier la clé. Le catalogue reste consultable s'il est public.
#[tauri::command]
pub async fn cloud_provider_clear_key(
    core: State<'_, Core>,
    provider: String,
) -> Result<(), String> {
    let h = host(&core);
    let p = cloud::find(&h, &provider).await?;
    cloud::clear_key(&h, &p.id)
}

/// La liste des modèles du fournisseur.
///
/// Lue chez lui quand le catalogue gardé a vieilli, ou quand `refresh` le
/// demande. En cas de panne, le catalogue gardé est renvoyé tel quel : une
/// liste d'hier vaut mieux qu'un écran vide.
#[tauri::command]
pub async fn cloud_provider_models(
    core: State<'_, Core>,
    provider: String,
    refresh: Option<bool>,
) -> Result<Vec<CloudModel>, String> {
    let h = host(&core);
    let p = cloud::find(&h, &provider).await?;
    cloud::catalog::models(&h, &p, refresh.unwrap_or(false)).await
}

/// La passerelle répond-elle, et est-elle seulement installée ?
///
/// Sondée à la demande, jamais en boucle : un dossier qu'on n'ouvre pas n'a
/// pas à interroger un port toutes les secondes.
#[tauri::command]
pub async fn cloud_provider_status(
    core: State<'_, Core>,
    provider: String,
) -> Result<CloudProviderStatus, String> {
    let h = host(&core);
    let p = cloud::find(&h, &provider).await?;
    Ok(cloud::gateway::status(&h, &p).await)
}

/// Installer la passerelle avec la commande déclarée par le manifeste.
#[tauri::command]
pub async fn cloud_provider_install(
    core: State<'_, Core>,
    provider: String,
) -> Result<String, String> {
    let h = host(&core);
    let p = cloud::find(&h, &provider).await?;
    exiger_shell(&core, &p).await?;
    cloud::gateway::install(&h, &p).await
}

/// Démarrer la passerelle — en l'installant d'abord si elle manque.
#[tauri::command]
pub async fn cloud_provider_start(
    core: State<'_, Core>,
    provider: String,
) -> Result<CloudProviderStatus, String> {
    let h = host(&core);
    let p = cloud::find(&h, &provider).await?;
    exiger_shell(&core, &p).await?;
    cloud::gateway::start(&h, &p).await
}

/// Installer ou démarrer un programme au nom de l'utilisateur n'est pas
/// anodin : l'extension doit en avoir reçu la permission.
async fn exiger_shell(core: &Core, p: &cloud::DeclaredProvider) -> Result<(), String> {
    let row = core
        .storage
        .extensions
        .get(p.extension_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;
    if row
        .granted
        .contains(&locaryn_shared_types::Permission::Shell)
    {
        return Ok(());
    }
    Err(format!(
        "L'extension {} n'a pas la permission « shell » : accordez-la dans Réglages → \
         Extensions pour qu'elle puisse installer et démarrer sa passerelle.",
        p.extension_name
    ))
}

/// Ouvrir le tableau de bord de la passerelle dans le navigateur du système.
///
/// L'URL vient du manifeste, jamais de l'interface : une commande qui
/// accepterait une adresse quelconque ferait de l'application un ouvreur de
/// liens pour n'importe quel panneau d'extension.
#[tauri::command]
pub async fn cloud_provider_open_dashboard(
    core: State<'_, Core>,
    provider: String,
) -> Result<String, String> {
    let h = host(&core);
    let p = cloud::find(&h, &provider).await?;
    let url = p
        .manifest
        .local
        .as_ref()
        .and_then(|l| l.dashboard_url.clone())
        .or_else(|| p.manifest.keys_url.clone())
        .ok_or_else(|| format!("{} ne déclare pas de tableau de bord à ouvrir.", p.label()))?;
    // Une URL de manifeste reste une entrée : on refuse tout ce qui n'est pas
    // du web, plutôt que de passer « file:// » ou pire à l'ouvreur du système.
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("Adresse de tableau de bord inattendue : {url}"));
    }

    let mut command = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", &url]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(&url);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&url);
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map_err(|e| format!("Le navigateur n'a pas pu être ouvert : {e}"))?;
    Ok(url)
}

/// Choisir un modèle du catalogue : il devient le modèle actif.
#[tauri::command]
pub async fn cloud_provider_select(
    core: State<'_, Core>,
    provider: String,
    model: String,
) -> Result<(), String> {
    cloud::select(&host(&core), &provider, &model).await?;
    // Le moteur local n'a plus rien à servir : le laisser tourner garderait
    // plusieurs gigaoctets en mémoire pour un modèle que personne n'appelle.
    core.supervisor
        .set_pinned(&locaryn_shared_types::ProviderEngine::LlamaCpp, false)
        .await;
    Ok(())
}
