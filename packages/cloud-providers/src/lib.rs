//! Les fournisseurs de modèles qu'apporte une extension.
//!
//! Une extension déclare une section `cloud_provider` : une API compatible
//! OpenAI, l'adresse de sa liste de modèles, et l'endroit où l'utilisateur va
//! chercher sa clé.
//!
//! Deux formes. Un service distant — on lui parle, il n'y a rien à lancer. Ou
//! une passerelle auto-hébergée, comme OmniRoute : elle s'installe, tourne sur
//! la machine, détient les clés des fournisseurs que l'utilisateur y a
//! connectés, et route vers eux. Locaryn ne réécrit rien de tout cela : il
//! installe, sonde, démarre, et parle à son `/v1`.
//!
//! Ni l'une ni l'autre n'est un moteur : aucune ne calcule de jetons ici.
//!
//! **Ce module vit à part de l'application et du service** parce que les deux
//! en ont besoin, et pour la même raison : l'application y branche son écran,
//! le service y branche son API compatible OpenAI. Deux copies auraient
//! divergé au premier correctif.
//!
//! Trois choses restent chez l'hôte, et aucune ne peut vivre dans l'extension.
//!
//! **La clé.** Elle va dans le trousseau du système — ou, sur un serveur sans
//! trousseau, dans une variable d'environnement. Jamais dans le dossier de
//! l'extension ni dans la base. Le panneau de l'extension peut demander à
//! l'écrire et savoir qu'elle existe ; il ne peut pas la relire.
//!
//! **La liste des modèles.** Lue chez le fournisseur, pas figée dans le
//! paquet : un modèle sorti ce matin apparaît sans nouvelle version de
//! l'extension. Gardée sur disque quelques heures, pour que l'écran s'ouvre
//! plein même hors ligne.
//!
//! **Le choix du modèle.** Il s'inscrit comme fournisseur actif, avec un
//! marqueur qui dit de quel catalogue il vient : c'est ce marqueur que la
//! conversation relit pour joindre la bonne clé.

use locaryn_auth::Keychain;
use locaryn_extensions::manifest::CloudProviderManifest;
use locaryn_storage::repos::ProviderRepo;
use locaryn_storage::Storage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod catalog;
pub mod gateway;

pub use catalog::{models, parse_models, CachedCatalog, CloudModel};
pub use gateway::{install, probe, start, CloudProviderStatus};

/// Ce dont la découverte a besoin, quel que soit l'hôte : l'application de
/// bureau comme le service.
///
/// Passer ces quatre poignées plutôt qu'un « contexte applicatif » garde ce
/// module utilisable des deux côtés — et testable sans ni l'un ni l'autre.
pub struct Host<'a> {
    pub storage: &'a Storage,
    pub data_dir: &'a Path,
    pub http: &'a reqwest::Client,
    pub keychain: &'a dyn Keychain,
}

/// Préfixe des entrées de trousseau. Séparé de `locaryn/provider/<uuid>`, qui
/// désigne un fournisseur enregistré : ici la clé appartient au catalogue, pas
/// à la ligne de base de données, et elle survit à un changement de modèle.
pub fn keychain_key(provider_id: &str) -> String {
    format!("locaryn/cloud/{provider_id}")
}

/// Nom de la variable d'environnement qui remplace le trousseau.
///
/// Un serveur n'a souvent ni session graphique ni trousseau : sans ce repli,
/// le mode serveur ne pourrait jamais parler à une passerelle. `omniroute`
/// donne `LOCARYN_CLOUD_OMNIROUTE_KEY`.
pub fn env_key_name(provider_id: &str) -> String {
    let up: String = provider_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("LOCARYN_CLOUD_{up}_KEY")
}

// ============================================================================
// Ce que l'interface et l'API reçoivent
// ============================================================================

/// Un fournisseur, tel qu'on le montre.
#[derive(Debug, Clone, Serialize)]
pub struct CloudProviderInfo {
    /// Identifiant stable (`omniroute`).
    pub id: String,
    pub label: String,
    /// L'extension qui l'apporte — la retirer retire le fournisseur.
    pub extension_id: String,
    pub extension_name: String,
    /// Base de l'API, sans `/v1`.
    pub api_url: String,
    pub models_url: String,
    pub keys_url: Option<String>,
    pub docs_url: Option<String>,
    pub key_hint: Option<String>,
    /// Une clé est-elle enregistrée ? Jamais la clé elle-même.
    pub has_key: bool,
    /// Modèles dans le catalogue gardé, 0 si rien n'a encore été lu.
    pub model_count: usize,
    /// Quand la liste a été lue pour la dernière fois (RFC 3339).
    pub updated_at: Option<String>,
    /// Le modèle de ce catalogue actuellement actif, s'il l'est.
    pub active_model: Option<String>,
    /// Vrai quand la passerelle tourne sur la machine.
    pub is_local: bool,
    pub dashboard_url: Option<String>,
    /// Comment l'installer, dit en une phrase.
    pub install_hint: Option<String>,
    /// Une commande de démarrage est-elle déclarée ?
    pub can_start: bool,
    /// L'hôte sait-il l'installer lui-même ?
    pub can_install: bool,
    /// Le programme est-il déjà présent sur le chemin ?
    pub installed: bool,
}

/// Un fournisseur déclaré, avec l'extension qui le porte.
#[derive(Debug, Clone)]
pub struct DeclaredProvider {
    pub manifest: CloudProviderManifest,
    pub id: String,
    pub extension_id: uuid::Uuid,
    pub extension_name: String,
    /// Racine du paquet, pour lire ses fichiers.
    pub plugin_root: PathBuf,
}

impl DeclaredProvider {
    /// Nom affiché : celui du manifeste, sinon celui de l'extension.
    pub fn label(&self) -> String {
        if self.manifest.label.trim().is_empty() {
            self.extension_name.clone()
        } else {
            self.manifest.label.clone()
        }
    }
}

/// Racine du paquet à partir du chemin de son manifeste.
pub fn plugin_root(manifest_path: &str) -> Option<PathBuf> {
    let p = Path::new(manifest_path);
    if p.is_dir() {
        return Some(p.to_path_buf());
    }
    p.parent().map(Path::to_path_buf)
}

// ============================================================================
// Découverte
// ============================================================================

/// Les fournisseurs déclarés par les extensions **actives**.
///
/// Une extension désactivée n'apporte plus son catalogue : c'est ce qui permet
/// de le faire disparaître sans désinstaller, et ce qui garantit qu'un
/// catalogue visible correspond toujours à une extension en marche.
pub async fn declared(host: &Host<'_>) -> Vec<DeclaredProvider> {
    let rows = match host.storage.extensions.list().await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(erreur = %e, "liste des extensions illisible");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for row in rows.into_iter().filter(|r| r.enabled) {
        let Some(root) = plugin_root(&row.manifest_path) else {
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
            plugin_root: root,
        });
    }
    out
}

/// Un fournisseur par son identifiant, ou l'erreur qui dit quoi faire.
pub async fn find(host: &Host<'_>, provider_id: &str) -> Result<DeclaredProvider, String> {
    declared(host)
        .await
        .into_iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| {
            format!(
                "Aucun fournisseur « {provider_id} » : l'extension qui l'apporte est \
                 désinstallée ou désactivée."
            )
        })
}

/// Quel fournisseur sert ce modèle ?
///
/// C'est la question de l'API compatible OpenAI : elle reçoit un nom de modèle
/// et doit décider qui le sert. La réponse vient du catalogue gardé — sans
/// appel réseau, puisqu'elle est posée à chaque requête.
pub async fn provider_of_model(host: &Host<'_>, model: &str) -> Option<DeclaredProvider> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    for p in declared(host).await {
        // Deux formes acceptées : l'identifiant tel que le fournisseur le
        // publie, et sa forme préfixée `<fournisseur>/<id>` — utile quand deux
        // catalogues publient le même nom.
        if let Some(reste) = model.strip_prefix(&format!("{}/", p.id)) {
            if !reste.is_empty() {
                return Some(p);
            }
        }
        if let Some(cache) = CachedCatalog::load(host.data_dir, &p.id) {
            if cache.models.iter().any(|m| m.id == model) {
                return Some(p);
            }
        }
    }
    None
}

/// L'identifiant que le fournisseur attend, une fois retiré le préfixe que
/// l'API accepte par commodité.
pub fn strip_provider_prefix(provider_id: &str, model: &str) -> String {
    model
        .strip_prefix(&format!("{provider_id}/"))
        .unwrap_or(model)
        .to_string()
}

// ============================================================================
// La clé
// ============================================================================

/// La clé enregistrée pour ce fournisseur, si elle existe.
///
/// Le trousseau d'abord, l'environnement ensuite : une machine de bureau garde
/// sa clé chiffrée, un serveur la reçoit de son gestionnaire de secrets.
pub fn stored_key(host: &Host<'_>, provider_id: &str) -> Option<String> {
    if let Ok(k) = host.keychain.get(&keychain_key(provider_id)) {
        if !k.trim().is_empty() {
            return Some(k);
        }
    }
    std::env::var(env_key_name(provider_id))
        .ok()
        .filter(|k| !k.trim().is_empty())
}

/// Écrire la clé.
pub fn set_key(host: &Host<'_>, provider_id: &str, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("La clé est vide.".into());
    }
    host.keychain
        .put(&keychain_key(provider_id), key)
        .map_err(|e| format!("Le trousseau du système a refusé la clé : {e}"))?;
    tracing::info!(fournisseur = %provider_id, "clé enregistrée dans le trousseau");
    Ok(())
}

/// L'oublier. Le catalogue reste consultable s'il est public.
pub fn clear_key(host: &Host<'_>, provider_id: &str) -> Result<(), String> {
    host.keychain
        .delete(&keychain_key(provider_id))
        .map_err(|e| format!("Le trousseau du système a refusé l'effacement : {e}"))
}

/// La clé du fournisseur actif, quand le fournisseur actif est un catalogue.
pub fn key_for_active_provider(
    host: &Host<'_>,
    provider: &locaryn_shared_types::Provider,
) -> Option<String> {
    let id = ProviderRepo::cloud_provider_of(provider)?;
    stored_key(host, &id)
}

// ============================================================================
// Vue d'ensemble
// ============================================================================

/// Tous les fournisseurs, avec ce qu'on sait déjà d'eux.
pub async fn list_infos(host: &Host<'_>) -> Vec<CloudProviderInfo> {
    let active = host.storage.providers.active().await.ok().flatten();
    let actif = active
        .as_ref()
        .and_then(|p| ProviderRepo::cloud_provider_of(p).map(|id| (id, p.model.clone())));

    declared(host)
        .await
        .into_iter()
        .map(|p| {
            let cache = CachedCatalog::load(host.data_dir, &p.id);
            let local = p.manifest.local.clone();
            CloudProviderInfo {
                has_key: stored_key(host, &p.id).is_some(),
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
                is_local: local.is_some(),
                dashboard_url: local.as_ref().and_then(|l| l.dashboard_url.clone()),
                install_hint: local.as_ref().and_then(|l| l.install_hint.clone()),
                can_start: local.as_ref().is_some_and(|l| !l.start.is_empty()),
                can_install: local
                    .as_ref()
                    .is_some_and(|l| l.install.as_ref().is_some_and(|i| i.is_runnable())),
                installed: local.as_ref().map(gateway::is_installed).unwrap_or(true),
                label: p.label(),
                extension_id: p.extension_id.to_string(),
                extension_name: p.extension_name,
                id: p.id,
            }
        })
        .collect()
}

/// Choisir un modèle : il devient le fournisseur actif de la conversation.
pub async fn select(host: &Host<'_>, provider_id: &str, model: &str) -> Result<(), String> {
    let p = find(host, provider_id).await?;
    if stored_key(host, &p.id).is_none() {
        return Err(format!(
            "Aucune clé enregistrée pour {}. Collez la vôtre dans son dossier avant de choisir \
             un modèle.",
            p.label()
        ));
    }
    let model = strip_provider_prefix(&p.id, model.trim());
    host.storage
        .providers
        .upsert_cloud(&p.id, p.manifest.api_url.trim_end_matches('/'), &model)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(fournisseur = %p.id, %model, "modèle distant activé");
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rien {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le nom de variable est dérivé, pas inventé : c'est ce qu'un serveur
    /// doit pouvoir deviner depuis l'identifiant du fournisseur.
    #[test]
    fn le_nom_de_variable_se_deduit_de_lidentifiant() {
        assert_eq!(env_key_name("omniroute"), "LOCARYN_CLOUD_OMNIROUTE_KEY");
        assert_eq!(env_key_name("mon-service"), "LOCARYN_CLOUD_MON_SERVICE_KEY");
    }

    /// L'entrée de trousseau appartient au catalogue, pas à la ligne de base :
    /// changer de modèle ne doit pas faire perdre la clé.
    #[test]
    fn la_cle_est_nommee_par_le_catalogue() {
        assert_eq!(keychain_key("omniroute"), "locaryn/cloud/omniroute");
    }

    /// L'API accepte les deux formes ; le fournisseur, lui, ne connaît que la
    /// sienne.
    #[test]
    fn le_prefixe_est_retire_avant_dappeler() {
        assert_eq!(
            strip_provider_prefix("omniroute", "omniroute/anthropic/claude-opus-5"),
            "anthropic/claude-opus-5"
        );
        assert_eq!(
            strip_provider_prefix("omniroute", "anthropic/claude-opus-5"),
            "anthropic/claude-opus-5"
        );
    }

    /// Une racine de paquet se déduit du chemin du manifeste, qu'il désigne le
    /// dossier ou le fichier.
    #[test]
    fn la_racine_du_paquet_se_deduit() {
        let p = plugin_root("/plugins/morph-omniroute/morph.json").expect("racine");
        assert!(p.ends_with("morph-omniroute"));
    }
}
