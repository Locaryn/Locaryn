//! Les catalogues de modèles distants qu'apporte une extension.
//!
//! Une extension peut déclarer une section `cloud_provider` : une API
//! compatible OpenAI, l'adresse de sa liste de modèles, et l'endroit où
//! l'utilisateur va chercher sa clé.
//!
//! Deux formes. Un service distant — on lui parle, il n'y a rien à lancer. Ou
//! une passerelle auto-hébergée, comme OmniRoute : elle tourne sur la machine,
//! détient les clés des fournisseurs que l'utilisateur y a connectés, et route
//! vers eux. Locaryn ne réécrit rien de tout cela : il sonde, démarre au
//! besoin, ouvre le tableau de bord, et parle au `/v1` de la passerelle comme
//! il parlerait à n'importe quelle API compatible OpenAI.
//!
//! Ni l'une ni l'autre n'est un moteur : aucune ne calcule de jetons sur cette
//! machine.
//!
//! Trois choses vivent ici, et aucune ne peut vivre dans l'extension.
//!
//! **La clé.** Elle va dans le trousseau du système, jamais dans le dossier de
//! l'extension ni dans la base. Le panneau de l'extension peut demander à
//! l'écrire et savoir s'il en existe une ; il ne peut pas la relire. Une
//! extension compromise ne rend donc pas la clé de son utilisateur.
//!
//! **La liste des modèles.** Elle est lue chez le fournisseur, pas figée dans
//! le paquet : c'est ce qui fait qu'un modèle sorti ce matin apparaît sans
//! qu'une nouvelle version de l'extension soit publiée. Elle est gardée sur
//! disque quelques heures, pour que l'écran s'ouvre plein même hors ligne.
//!
//! **Le choix du modèle.** Il s'inscrit comme fournisseur actif, avec un
//! marqueur qui dit de quel catalogue il vient : c'est ce marqueur que la
//! conversation relit pour joindre la bonne clé à sa requête.

use crate::Core;
use locaryn_extensions::manifest::CloudProviderManifest;
use locaryn_storage::repos::ProviderRepo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

/// Préfixe des entrées de trousseau. Séparé de `locaryn/provider/<uuid>`, qui
/// désigne un fournisseur enregistré : ici la clé appartient au catalogue, pas
/// à la ligne de base de données, et elle survit à un changement de modèle.
fn keychain_key(provider_id: &str) -> String {
    format!("locaryn/cloud/{provider_id}")
}

// ============================================================================
// Ce que l'interface reçoit
// ============================================================================

/// Un fournisseur distant, tel que l'interface le montre.
#[derive(Debug, Clone, Serialize)]
pub struct CloudProviderInfo {
    /// Identifiant stable (`omniroute`).
    pub id: String,
    pub label: String,
    /// L'extension qui l'apporte — retirer l'extension retire le dossier.
    pub extension_id: String,
    pub extension_name: String,
    pub api_url: String,
    pub models_url: String,
    pub keys_url: Option<String>,
    pub docs_url: Option<String>,
    pub key_hint: Option<String>,
    /// Une clé est-elle enregistrée ? Jamais la clé elle-même.
    pub has_key: bool,
    /// Nombre de modèles dans le catalogue gardé, 0 si rien n'a encore été lu.
    pub model_count: usize,
    /// Quand la liste a été lue pour la dernière fois (RFC 3339).
    pub updated_at: Option<String>,
    /// Le modèle de ce catalogue actuellement actif, s'il l'est.
    pub active_model: Option<String>,
    /// Vrai quand la passerelle tourne sur la machine : le dossier montre
    /// alors son tableau de bord au lieu d'un simple formulaire de clé.
    pub is_local: bool,
    /// Le tableau de bord de la passerelle, quand elle en a un.
    pub dashboard_url: Option<String>,
    /// Comment l'installer, dit en une phrase quand elle ne répond pas.
    pub install_hint: Option<String>,
    /// Y a-t-il une commande de démarrage déclarée ?
    pub can_start: bool,
}

/// L'état d'une passerelle locale, sondé à la demande.
#[derive(Debug, Clone, Serialize)]
pub struct CloudProviderStatus {
    pub running: bool,
    /// Ce qu'il faut faire quand elle ne répond pas — jamais un code.
    pub detail: String,
    pub dashboard_url: Option<String>,
}

/// Un modèle du catalogue distant.
///
/// Les champs sont ceux que tous les catalogues compatibles OpenAI publient,
/// plus ceux qu'une passerelle ajoute et dont l'utilisateur a réellement
/// besoin pour choisir : la fenêtre de contexte et le prix.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudModel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub context_length: u64,
    /// Prix pour un million de jetons d'entrée, en dollars. `None` quand le
    /// catalogue n'en publie pas — un modèle gratuit affiche 0.
    #[serde(default)]
    pub prompt_price_per_m: Option<f64>,
    #[serde(default)]
    pub completion_price_per_m: Option<f64>,
    /// `text`, `text+image`… tel que le catalogue le déclare.
    #[serde(default)]
    pub modality: String,
    /// Le modèle sait-il appeler des outils ? Sans cela, la boucle d'outils de
    /// Locaryn tourne à vide, et il vaut mieux le dire avant.
    #[serde(default)]
    pub supports_tools: bool,
}

/// Le catalogue gardé sur disque.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CachedCatalog {
    updated_at: String,
    models: Vec<CloudModel>,
}

impl CachedCatalog {
    fn path(data_dir: &std::path::Path, provider_id: &str) -> PathBuf {
        // Le nom du fichier vient d'un identifiant de manifeste : le réduire à
        // des caractères sûrs évite qu'un « ../ » aille écrire ailleurs.
        let safe: String = provider_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        data_dir.join("cloud").join(format!("{safe}.json"))
    }

    fn load(data_dir: &std::path::Path, provider_id: &str) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(data_dir, provider_id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save(&self, data_dir: &std::path::Path, provider_id: &str) -> std::io::Result<()> {
        let path = Self::path(data_dir, provider_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            serde_json::to_string(self).unwrap_or_else(|_| "{}".into()),
        )
    }

    /// Le catalogue est-il encore frais pour cette fenêtre de fraîcheur ?
    fn is_fresh(&self, refresh_hours: u32) -> bool {
        if self.models.is_empty() {
            return false;
        }
        if refresh_hours == 0 {
            return false;
        }
        chrono::DateTime::parse_from_rfc3339(&self.updated_at)
            .map(|t| {
                let age = chrono::Utc::now().signed_duration_since(t.with_timezone(&chrono::Utc));
                age < chrono::Duration::hours(i64::from(refresh_hours))
            })
            .unwrap_or(false)
    }
}

// ============================================================================
// Découverte
// ============================================================================

/// Un fournisseur déclaré, avec l'extension qui le porte.
pub struct DeclaredProvider {
    pub manifest: CloudProviderManifest,
    pub id: String,
    pub extension_id: uuid::Uuid,
    pub extension_name: String,
}

/// Les fournisseurs distants déclarés par les extensions **actives**.
///
/// Une extension désactivée n'apporte plus son dossier : c'est ce qui permet
/// de faire disparaître un catalogue sans le désinstaller, et ce qui garantit
/// qu'un catalogue visible correspond toujours à une extension en marche.
pub async fn declared(core: &Core) -> Vec<DeclaredProvider> {
    let rows = match core.storage.extensions.list().await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(erreur = %e, "liste des extensions illisible");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for row in rows.into_iter().filter(|r| r.enabled) {
        let Some(root) = crate::extensions::plugin_root(&row.manifest_path) else {
            continue;
        };
        let Ok(manifest) = locaryn_extensions::manifest::load(&root) else {
            continue;
        };
        let Some(cloud) = manifest.cloud_provider.clone() else {
            continue;
        };
        if cloud.api_url.trim().is_empty() {
            tracing::warn!(
                extension = %row.name,
                "section cloud_provider sans api_url : catalogue ignoré"
            );
            continue;
        }
        out.push(DeclaredProvider {
            id: cloud.effective_id(&row.name),
            manifest: cloud,
            extension_id: row.id,
            extension_name: row.name.clone(),
        });
    }
    out
}

/// Un fournisseur par son identifiant, ou l'erreur qui dit quoi faire.
async fn find(core: &Core, provider_id: &str) -> Result<DeclaredProvider, String> {
    declared(core)
        .await
        .into_iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| {
            format!(
                "Aucun fournisseur distant « {provider_id} » : l'extension qui l'apporte est \
                 désinstallée ou désactivée. Ouvrez Réglages → Extensions."
            )
        })
}

/// La clé enregistrée pour ce fournisseur, si elle existe.
///
/// Publique parce que la conversation en a besoin — et seulement elle. Le
/// panneau d'une extension, lui, n'apprend que son existence.
pub fn stored_key(core: &Core, provider_id: &str) -> Option<String> {
    core.keychain
        .get(&keychain_key(provider_id))
        .ok()
        .filter(|k| !k.trim().is_empty())
}

/// La clé du fournisseur actif, quand le fournisseur actif est un catalogue
/// distant. C'est le seul point d'entrée dont la conversation a besoin.
pub fn key_for_active_provider(
    core: &Core,
    provider: &locaryn_shared_types::Provider,
) -> Option<String> {
    let id = ProviderRepo::cloud_provider_of(provider)?;
    stored_key(core, &id)
}

// ============================================================================
// Commandes
// ============================================================================

/// Les catalogues distants disponibles, avec ce qu'on sait déjà d'eux.
#[tauri::command]
pub async fn cloud_providers(core: State<'_, Core>) -> Result<Vec<CloudProviderInfo>, String> {
    let active = core.storage.providers.active().await.ok().flatten();
    let actif = active
        .as_ref()
        .and_then(|p| ProviderRepo::cloud_provider_of(p).map(|id| (id, p.model.clone())));

    Ok(declared(&core)
        .await
        .into_iter()
        .map(|p| {
            let cache = CachedCatalog::load(&core.data_dir, &p.id);
            CloudProviderInfo {
                has_key: stored_key(&core, &p.id).is_some(),
                model_count: cache.as_ref().map(|c| c.models.len()).unwrap_or(0),
                updated_at: cache.map(|c| c.updated_at),
                active_model: match &actif {
                    Some((id, model)) if *id == p.id => model.clone(),
                    _ => None,
                },
                api_url: p.manifest.api_url.trim_end_matches('/').to_string(),
                models_url: p.manifest.effective_models_url(),
                keys_url: p.manifest.keys_url.clone(),
                docs_url: p.manifest.docs_url.clone(),
                key_hint: p.manifest.key_hint.clone(),
                is_local: p.manifest.local.is_some(),
                dashboard_url: p
                    .manifest
                    .local
                    .as_ref()
                    .and_then(|l| l.dashboard_url.clone()),
                install_hint: p
                    .manifest
                    .local
                    .as_ref()
                    .and_then(|l| l.install_hint.clone()),
                can_start: p
                    .manifest
                    .local
                    .as_ref()
                    .is_some_and(|l| !l.start.is_empty()),
                label: if p.manifest.label.trim().is_empty() {
                    p.extension_name.clone()
                } else {
                    p.manifest.label.clone()
                },
                extension_id: p.extension_id.to_string(),
                extension_name: p.extension_name,
                id: p.id,
            }
        })
        .collect())
}

/// Enregistrer la clé d'un fournisseur dans le trousseau du système.
#[tauri::command]
pub async fn cloud_provider_set_key(
    core: State<'_, Core>,
    provider: String,
    key: String,
) -> Result<(), String> {
    let p = find(&core, &provider).await?;
    let key = key.trim();
    if key.is_empty() {
        return Err("La clé est vide.".into());
    }
    core.keychain
        .put(&keychain_key(&p.id), key)
        .map_err(|e| format!("Le trousseau du système a refusé la clé : {e}"))?;
    tracing::info!(fournisseur = %p.id, "clé enregistrée dans le trousseau");
    Ok(())
}

/// Oublier la clé. Le catalogue reste consultable s'il est public.
#[tauri::command]
pub async fn cloud_provider_clear_key(
    core: State<'_, Core>,
    provider: String,
) -> Result<(), String> {
    let p = find(&core, &provider).await?;
    core.keychain
        .delete(&keychain_key(&p.id))
        .map_err(|e| format!("Le trousseau du système a refusé l'effacement : {e}"))?;
    Ok(())
}

/// La liste des modèles du fournisseur.
///
/// Lue chez lui quand le catalogue gardé a vieilli, ou quand `refresh` le
/// demande. En cas de panne réseau, le catalogue gardé est renvoyé tel quel :
/// une liste d'hier vaut mieux qu'un écran vide.
#[tauri::command]
pub async fn cloud_provider_models(
    core: State<'_, Core>,
    provider: String,
    refresh: Option<bool>,
) -> Result<Vec<CloudModel>, String> {
    let p = find(&core, &provider).await?;
    let cache = CachedCatalog::load(&core.data_dir, &p.id);

    if !refresh.unwrap_or(false) {
        if let Some(c) = cache.as_ref().filter(|c| c.is_fresh(p.manifest.refresh_hours)) {
            return Ok(c.models.clone());
        }
    }

    match fetch_models(&core, &p).await {
        Ok(models) => {
            let catalog = CachedCatalog {
                updated_at: chrono::Utc::now().to_rfc3339(),
                models,
            };
            if let Err(e) = catalog.save(&core.data_dir, &p.id) {
                tracing::warn!(erreur = %e, "catalogue distant non enregistré");
            }
            Ok(catalog.models)
        }
        Err(e) => match cache {
            Some(c) if !c.models.is_empty() => {
                tracing::warn!(fournisseur = %p.id, erreur = %e, "liste distante illisible, catalogue gardé");
                Ok(c.models)
            }
            _ => Err(e),
        },
    }
}

/// Lire la liste chez le fournisseur et la ramener à ce dont l'écran a besoin.
async fn fetch_models(core: &Core, p: &DeclaredProvider) -> Result<Vec<CloudModel>, String> {
    let url = p.manifest.effective_models_url();
    let mut req = core.http.get(&url);
    // La clé n'est pas toujours nécessaire pour lire un catalogue ; quand elle
    // l'est, l'absence se voit sur un 401 explicite plutôt que sur une liste
    // vide.
    if let Some(key) = stored_key(core, &p.id) {
        req = req.bearer_auth(key);
    }
    for (name, value) in &p.manifest.headers {
        req = req.header(name.as_str(), value.as_str());
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("{} est injoignable : {e}", p.manifest.label))?;
    if !resp.status().is_success() {
        let code = resp.status();
        return Err(match code.as_u16() {
            401 | 403 => format!(
                "{} a refusé la clé ({code}). Vérifiez-la dans le panneau du fournisseur.",
                p.manifest.label
            ),
            _ => format!("{} a répondu {code}.", p.manifest.label),
        });
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Réponse illisible de {} : {e}", p.manifest.label))?;
    Ok(parse_models(&body))
}

/// Extraire les modèles d'une réponse au format OpenAI.
///
/// Isolée du réseau pour être testable : c'est la partie qui casse quand un
/// fournisseur change la forme de sa réponse.
pub fn parse_models(body: &serde_json::Value) -> Vec<CloudModel> {
    let list = body
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| body.get("models").and_then(|d| d.as_array()))
        .or_else(|| body.as_array());
    let Some(list) = list else {
        return Vec::new();
    };

    list.iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(|v| v.as_str())?.trim().to_string();
            if id.is_empty() {
                return None;
            }
            let pricing = m.get("pricing");
            // Les prix sont publiés par jeton, en chaîne. Par million, ils
            // deviennent lisibles — et comparables à ce qu'affichent les
            // fournisseurs eux-mêmes.
            let per_million = |champ: &str| -> Option<f64> {
                pricing?
                    .get(champ)
                    .and_then(|v| v.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| v.as_f64()))
                    .map(|prix| prix * 1_000_000.0)
            };
            let architecture = m.get("architecture");
            Some(CloudModel {
                name: m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&id)
                    .to_string(),
                description: m
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                context_length: m
                    .get("context_length")
                    .and_then(serde_json::Value::as_u64)
                    .or_else(|| {
                        m.get("top_provider")?
                            .get("context_length")?
                            .as_u64()
                    })
                    .unwrap_or(0),
                prompt_price_per_m: per_million("prompt"),
                completion_price_per_m: per_million("completion"),
                modality: architecture
                    .and_then(|a| a.get("modality"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                supports_tools: m
                    .get("supported_parameters")
                    .and_then(|v| v.as_array())
                    .map(|params| {
                        params
                            .iter()
                            .filter_map(|p| p.as_str())
                            .any(|p| p == "tools" || p == "tool_choice")
                    })
                    .unwrap_or(false),
                id,
            })
        })
        .collect()
}

/// La passerelle répond-elle ?
///
/// Sondée à la demande, jamais en boucle : un dossier qu'on n'ouvre pas n'a
/// pas à interroger un port toutes les secondes.
#[tauri::command]
pub async fn cloud_provider_status(
    core: State<'_, Core>,
    provider: String,
) -> Result<CloudProviderStatus, String> {
    let p = find(&core, &provider).await?;
    let Some(local) = p.manifest.local.clone() else {
        // Un service distant est joignable ou ne l'est pas ; c'est la lecture
        // du catalogue qui le dira, pas une sonde de plus.
        return Ok(CloudProviderStatus {
            running: true,
            detail: format!(
                "{} est un service distant : rien à démarrer ici.",
                p.manifest.label
            ),
            dashboard_url: None,
        });
    };

    let url = local
        .health_url
        .clone()
        .unwrap_or_else(|| p.manifest.effective_models_url());
    let running = probe(&core, &url, stored_key(&core, &p.id).as_deref()).await;
    Ok(CloudProviderStatus {
        running,
        detail: if running {
            format!("{} répond sur {}.", p.manifest.label, p.manifest.api_url)
        } else if local.start.is_empty() {
            local.install_hint.clone().unwrap_or_else(|| {
                format!(
                    "{} ne répond pas. Démarrez la passerelle, puis actualisez.",
                    p.manifest.label
                )
            })
        } else {
            format!(
                "{} ne répond pas sur {}. Démarrez-la depuis ce dossier{}",
                p.manifest.label,
                p.manifest.api_url,
                local
                    .install_hint
                    .as_ref()
                    .map(|h| format!(", ou installez-la : {h}"))
                    .unwrap_or_else(|| ".".to_string())
            )
        },
        dashboard_url: local.dashboard_url,
    })
}

/// Une requête courte : la passerelle est-elle là ?
async fn probe(core: &Core, url: &str, key: Option<&str>) -> bool {
    let mut req = core.http.get(url).timeout(std::time::Duration::from_secs(3));
    if let Some(k) = key {
        req = req.bearer_auth(k);
    }
    match req.send().await {
        // Une passerelle qui exige une clé répond 401 : elle est bien là, et
        // la traiter comme éteinte enverrait l'utilisateur réinstaller ce qui
        // tourne déjà.
        Ok(r) => r.status().is_success() || r.status().as_u16() == 401,
        Err(_) => false,
    }
}

/// Démarrer la passerelle déclarée par l'extension.
///
/// La commande vient du manifeste et de nulle part ailleurs : ni l'interface
/// ni le panneau de l'extension ne peuvent en proposer une autre. Elle exige
/// la permission `shell`, accordée à l'installation — démarrer un programme au
/// nom de l'utilisateur n'est pas une opération anodine.
#[tauri::command]
pub async fn cloud_provider_start(
    core: State<'_, Core>,
    provider: String,
) -> Result<CloudProviderStatus, String> {
    let p = find(&core, &provider).await?;
    let local = p.manifest.local.clone().ok_or_else(|| {
        format!(
            "{} est un service distant : il n'y a rien à démarrer.",
            p.manifest.label
        )
    })?;
    if local.start.is_empty() {
        return Err(local.install_hint.unwrap_or_else(|| {
            format!(
                "{} ne déclare aucune commande de démarrage.",
                p.manifest.label
            )
        }));
    }

    let row = core
        .storage
        .extensions
        .get(p.extension_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "extension introuvable".to_string())?;
    if !row
        .granted
        .contains(&locaryn_shared_types::Permission::Shell)
    {
        return Err(format!(
            "L'extension {} n'a pas la permission « shell » : accordez-la dans Réglages → \
             Extensions pour qu'elle puisse démarrer la passerelle.",
            p.extension_name
        ));
    }

    let url = local
        .health_url
        .clone()
        .unwrap_or_else(|| p.manifest.effective_models_url());
    if probe(&core, &url, stored_key(&core, &p.id).as_deref()).await {
        return cloud_provider_status(core, provider).await;
    }

    let mut command = std::process::Command::new(&local.start[0]);
    command.args(&local.start[1..]);
    for (k, v) in &local.env {
        command.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    command.spawn().map_err(|e| {
        format!(
            "« {} » n'a pas pu être lancé ({e}). {}",
            local.start.join(" "),
            local
                .install_hint
                .clone()
                .unwrap_or_else(|| "La passerelle est-elle installée ?".to_string())
        )
    })?;

    // Une passerelle met quelques secondes à ouvrir son port. Rendre la main
    // avant qu'elle réponde ferait afficher « éteinte » à l'écran qui vient de
    // la démarrer.
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if probe(&core, &url, stored_key(&core, &p.id).as_deref()).await {
            break;
        }
    }
    tracing::info!(fournisseur = %p.id, "passerelle locale démarrée");
    cloud_provider_status(core, provider).await
}

/// Choisir un modèle du catalogue : il devient le modèle actif.
#[tauri::command]
pub async fn cloud_provider_select(
    core: State<'_, Core>,
    provider: String,
    model: String,
) -> Result<(), String> {
    let p = find(&core, &provider).await?;
    if stored_key(&core, &p.id).is_none() {
        return Err(format!(
            "Aucune clé enregistrée pour {}. Ouvrez son dossier dans « Mes modèles » et collez \
             votre clé avant de choisir un modèle.",
            p.manifest.label
        ));
    }
    let endpoint = p.manifest.api_url.trim_end_matches('/');
    core.storage
        .providers
        .upsert_cloud(&p.id, endpoint, model.trim())
        .await
        .map_err(|e| e.to_string())?;

    // Le moteur local n'a plus rien à servir : le laisser tourner garderait
    // plusieurs gigaoctets en mémoire pour un modèle que personne n'appelle.
    core.supervisor
        .set_pinned(&locaryn_shared_types::ProviderEngine::LlamaCpp, false)
        .await;

    tracing::info!(fournisseur = %p.id, %model, "modèle distant activé");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La forme publiée par OpenRouter, réduite à ce que l'écran montre.
    #[test]
    fn la_reponse_dun_catalogue_se_lit() {
        let body = serde_json::json!({
            "data": [{
                "id": "anthropic/claude-opus-5",
                "name": "Claude Opus 5",
                "description": "Le modèle phare.",
                "context_length": 1_000_000,
                "architecture": { "modality": "text+image->text" },
                "pricing": { "prompt": "0.000005", "completion": "0.000025" },
                "supported_parameters": ["tools", "max_tokens"]
            }]
        });
        let models = parse_models(&body);
        assert_eq!(models.len(), 1);
        let m = &models[0];
        assert_eq!(m.id, "anthropic/claude-opus-5");
        assert_eq!(m.name, "Claude Opus 5");
        assert_eq!(m.context_length, 1_000_000);
        // Publié par jeton, montré par million : 0,000005 → 5 $.
        assert_eq!(m.prompt_price_per_m, Some(5.0));
        assert_eq!(m.completion_price_per_m, Some(25.0));
        assert!(m.supports_tools, "« tools » est dans les paramètres");
        assert_eq!(m.modality, "text+image->text");
    }

    /// Un modèle sans identifiant n'est pas sélectionnable : le garder
    /// mettrait dans la liste une ligne qu'aucun clic ne peut activer.
    #[test]
    fn un_modele_sans_identifiant_est_ecarte() {
        let body = serde_json::json!({ "data": [{ "name": "sans id" }, { "id": "  " }] });
        assert!(parse_models(&body).is_empty());
    }

    /// Tous les catalogues ne publient pas `data` : certains renvoient un
    /// tableau nu, d'autres `models`. Les trois formes se lisent.
    #[test]
    fn les_trois_formes_de_liste_se_lisent() {
        let nu = serde_json::json!([{ "id": "a/b" }]);
        let sous_models = serde_json::json!({ "models": [{ "id": "a/b" }] });
        let sous_data = serde_json::json!({ "data": [{ "id": "a/b" }] });
        for body in [nu, sous_models, sous_data] {
            assert_eq!(parse_models(&body).len(), 1, "forme non reconnue : {body}");
        }
    }

    /// Sans `name`, l'identifiant sert de nom : une ligne vide dans la liste
    /// serait pire qu'un identifiant technique.
    #[test]
    fn sans_nom_lidentifiant_fait_office() {
        let body = serde_json::json!({ "data": [{ "id": "meta/llama-4" }] });
        assert_eq!(parse_models(&body)[0].name, "meta/llama-4");
    }

    /// Un catalogue vieilli doit être relu ; un catalogue vide n'est jamais
    /// « frais », sinon une panne du premier jour figerait un écran vide.
    #[test]
    fn la_fraicheur_du_catalogue() {
        let frais = CachedCatalog {
            updated_at: chrono::Utc::now().to_rfc3339(),
            models: vec![CloudModel::default()],
        };
        assert!(frais.is_fresh(12));
        assert!(!frais.is_fresh(0), "0 heure veut dire « relis à chaque fois »");

        let vieux = CachedCatalog {
            updated_at: (chrono::Utc::now() - chrono::Duration::hours(30)).to_rfc3339(),
            models: vec![CloudModel::default()],
        };
        assert!(!vieux.is_fresh(12));

        let vide = CachedCatalog {
            updated_at: chrono::Utc::now().to_rfc3339(),
            models: Vec::new(),
        };
        assert!(!vide.is_fresh(12));
    }

    /// Le nom de fichier du cache vient d'un manifeste : un identifiant
    /// tordu ne doit pas pouvoir écrire hors du dossier de données.
    #[test]
    fn un_identifiant_tordu_ne_sort_pas_du_dossier() {
        let base = std::path::Path::new("/donnees");
        let chemin = CachedCatalog::path(base, "../../etc/passwd");
        assert!(chemin.starts_with(base.join("cloud")));
        assert!(!chemin.to_string_lossy().contains(".."));
    }

    /// L'entrée de trousseau appartient au catalogue, pas à la ligne de base :
    /// changer de modèle ne doit pas faire perdre la clé.
    #[test]
    fn la_cle_est_nommee_par_le_catalogue() {
        assert_eq!(keychain_key("openrouter"), "locaryn/cloud/openrouter");
    }
}
