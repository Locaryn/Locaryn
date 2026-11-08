//! Le modèle qui fait les tâches de fond : nommer les conversations, trier la
//! mémoire.
//!
//! Avec un modèle local, rien ne change : le modèle des petites tâches choisi
//! dans les réglages, ou celui déjà chargé. Avec un modèle d'un fournisseur
//! distant (abonnement, passerelle, clé d'API), ces tâches consommeraient un
//! abonnement ou des crédits pour presque rien. Elles ne s'y font donc que si
//! la personne l'a autorisé dans le dossier de ce fournisseur — coupé par
//! défaut — avec le modèle de la conversation ou un modèle choisi. Sinon un
//! moteur local déjà chargé s'en charge, et à défaut elles n'ont pas lieu.
//!
//! Le nommage échouait avant cela en silence avec un modèle distant : la
//! requête partait vers la passerelle avec le nom du micro-modèle local et
//! sans clé.

use crate::Core;
use locaryn_shared_types::{Provider, ProviderEngine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use tauri::State;

/// Le fichier des autorisations, par fournisseur.
const FICHIER: &str = "taches-distantes.json";

/// Les deux tâches de fond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tache {
    Titre,
    Memoire,
}

/// Autoriser un fournisseur pour une tâche, et avec quel modèle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Autorisation {
    #[serde(default)]
    pub actif: bool,
    /// `None` : le modèle de la conversation.
    #[serde(default)]
    pub modele: Option<String>,
}

/// Les autorisations d'un fournisseur.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reglages {
    #[serde(default)]
    pub memoire: Autorisation,
    #[serde(default)]
    pub titres: Autorisation,
}

impl Reglages {
    fn pour(&self, tache: Tache) -> &Autorisation {
        match tache {
            Tache::Titre => &self.titres,
            Tache::Memoire => &self.memoire,
        }
    }
}

/// Où envoyer la requête d'une tâche de fond.
pub struct Cible {
    pub endpoint: String,
    pub modele: String,
    /// Porte la clé du fournisseur quand il en faut une.
    pub http: reqwest::Client,
    /// Un modèle de fournisseur, pas le moteur de la machine.
    pub distant: bool,
}

fn lire(dossier: &Path) -> BTreeMap<String, Reglages> {
    let chemin = dossier.join(FICHIER);
    match std::fs::read_to_string(&chemin) {
        Ok(texte) => serde_json::from_str(&texte).unwrap_or_else(|e| {
            tracing::warn!(erreur = %e, "autorisations des tâches de fond illisibles");
            BTreeMap::new()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => {
            tracing::warn!(erreur = %e, "autorisations des tâches de fond illisibles");
            BTreeMap::new()
        }
    }
}

fn ecrire(dossier: &Path, carte: &BTreeMap<String, Reglages>) -> Result<(), String> {
    let texte = serde_json::to_string_pretty(carte).map_err(|e| e.to_string())?;
    std::fs::write(dossier.join(FICHIER), texte).map_err(|e| e.to_string())
}

/// Un client qui porte la clé du fournisseur.
fn client(cle: Option<&str>) -> reqwest::Client {
    let mut entetes = reqwest::header::HeaderMap::new();
    if let Some(cle) = cle.filter(|c| !c.is_empty()) {
        match reqwest::header::HeaderValue::from_str(&format!("Bearer {cle}")) {
            Ok(mut v) => {
                v.set_sensitive(true);
                entetes.insert(reqwest::header::AUTHORIZATION, v);
            }
            Err(e) => tracing::warn!(erreur = %e, "clé du fournisseur inutilisable"),
        }
    }
    reqwest::Client::builder()
        .default_headers(entetes)
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_default()
}

/// Le modèle des petites tâches choisi dans les réglages ; `None` = « Aucun ».
fn micro_choisi() -> Option<String> {
    locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model)
        .filter(|m| !m.trim().is_empty())
}

/// Le moteur de la machine, s'il tourne déjà : jamais un chargement pour ça.
async fn moteur_local(core: &Core, tache: Tache, explicite: bool) -> Option<Cible> {
    let p = core
        .storage
        .providers
        .list()
        .await
        .ok()?
        .into_iter()
        .find(|p| p.engine == ProviderEngine::LlamaCpp)?;
    if !core.supervisor.is_healthy(&p.engine).await {
        return None;
    }
    cible_locale(&p, tache, explicite)
}

/// Le calcul d'avant pour un modèle local : le micro-modèle choisi, ou le
/// modèle chargé. Nommer exige un micro-modèle choisi (« Aucun » = pas de
/// nommage), sauf demande explicite.
fn cible_locale(p: &Provider, tache: Tache, explicite: bool) -> Option<Cible> {
    let choisi = micro_choisi();
    let modele = match (choisi, tache, explicite) {
        (_, _, true) => p.model.clone()?,
        (None, Tache::Titre, false) => return None,
        (None, Tache::Memoire, false) => p.model.clone()?,
        (Some(c), _, false) => locaryn_config::micro_effectif(&c, p.model.as_deref()),
    };
    Some(Cible {
        endpoint: p.endpoint.clone(),
        modele,
        http: client(None),
        distant: false,
    })
}

/// Le modèle d'une tâche de fond, ou `None` : elle n'a pas lieu.
///
/// `explicite` : la personne l'a demandée (« Renommer avec l'IA ») ; le modèle
/// de la conversation la fait, même distant.
pub async fn cible(core: &Core, tache: Tache, explicite: bool) -> Option<Cible> {
    let actif = core.storage.providers.active().await.ok().flatten()?;
    let Some(fournisseur) = locaryn_storage::repos::ProviderRepo::cloud_provider_of(&actif) else {
        if crate::moteur_supervise(&actif.engine)
            && !core.supervisor.is_healthy(&actif.engine).await
        {
            // Le nommage attend le moteur qui démarre ; le tri, jamais.
            // Un modèle que la prudence refuse ne se charge pas pour un titre :
            // la conversation garde son nom provisoire.
            if tache == Tache::Titre {
                if let Err(e) =
                    crate::model_residency::demarrer_si_permis(core, &actif.engine).await
                {
                    tracing::info!(raison = %e, "nommage reporté : moteur non démarré");
                    return None;
                }
            } else {
                return None;
            }
        }
        return cible_locale(&actif, tache, explicite);
    };
    let reglages = lire(&core.data_dir)
        .remove(&fournisseur)
        .unwrap_or_default();
    let autorisation = reglages.pour(tache);
    if !explicite && !autorisation.actif {
        return moteur_local(core, tache, false).await;
    }
    let modele = if explicite {
        actif.model.clone()?
    } else {
        autorisation
            .modele
            .clone()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| actif.model.clone())?
    };
    let http = match crate::cloud_providers::http_for_active_provider(core, &actif).await {
        Some(h) => h,
        None => client(crate::cloud_providers::key_for_active_provider(core, &actif).as_deref()),
    };
    Some(Cible {
        endpoint: actif.endpoint.clone(),
        modele,
        http,
        distant: true,
    })
}

/// Les autorisations d'un fournisseur, pour son dossier.
#[tauri::command]
pub fn taches_distantes(core: State<'_, Core>, provider_id: String) -> Reglages {
    lire(&core.data_dir)
        .remove(&provider_id)
        .unwrap_or_default()
}

/// Enregistrer les autorisations d'un fournisseur.
#[tauri::command]
pub fn taches_distantes_ecrire(
    core: State<'_, Core>,
    provider_id: String,
    reglages: Reglages,
) -> Result<(), String> {
    let mut carte = lire(&core.data_dir);
    if reglages == Reglages::default() {
        carte.remove(&provider_id);
    } else {
        carte.insert(provider_id, reglages);
    }
    ecrire(&core.data_dir, &carte)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("locaryn-taches-{nom}-{}", std::process::id()));
        std::fs::create_dir_all(&d).expect("dossier");
        d
    }

    /// Rien d'enregistré : aucune tâche de fond chez un fournisseur.
    #[test]
    fn coupe_par_defaut() {
        let d = dossier("defaut");
        let carte = lire(&d);
        let r = carte.get("fournisseur-a").cloned().unwrap_or_default();
        assert!(!r.pour(Tache::Titre).actif);
        assert!(!r.pour(Tache::Memoire).actif);
    }

    #[test]
    fn se_relit_tel_qu_ecrit() {
        let d = dossier("relit");
        let mut carte = BTreeMap::new();
        carte.insert(
            "fournisseur-b".to_string(),
            Reglages {
                memoire: Autorisation::default(),
                titres: Autorisation {
                    actif: true,
                    modele: Some("petit-modele".into()),
                },
            },
        );
        ecrire(&d, &carte).expect("écrit");
        assert_eq!(lire(&d), carte);
        std::fs::remove_dir_all(&d).expect("nettoyage");
    }
}
