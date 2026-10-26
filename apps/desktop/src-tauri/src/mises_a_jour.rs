//! Les mises à jour automatiques : l'application et ses extensions se tiennent
//! à jour seules, en arrière-plan, quand personne ne s'en sert.
//!
//! Aller dans la liste des extensions pour cliquer « Mettre à jour » une par
//! une, ou accepter la fenêtre de mise à jour de l'application, ne devrait pas
//! être le travail de la personne. Une seule case dans « À propos », cochée
//! par défaut, couvre les deux.
//!
//! **Jamais pendant qu'on s'en sert.** Une extension n'est remplacée que si
//! aucune réponse ni travail d'arrière-plan ne tourne : arrêter son serveur
//! couperait un outil en plein travail. L'application, elle, ne s'installe que
//! si aucun modèle n'est chargé — son installation la relance.
//!
//! Chaque extension suit ses propres versions : elle se met à jour dès que sa
//! release sort, sans attendre une version de l'application.

use crate::Core;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

/// Le délai avant la première vérification : laisser l'application finir de
/// démarrer et la personne commencer ce qu'elle était venue faire.
const PREMIERE_VERIFICATION: Duration = Duration::from_secs(120);

/// Entre deux vérifications. Les releases d'extensions se lisent dans le flux
/// Atom de GitHub, hors quota : toutes les deux heures ne coûte rien.
const ENTRE_DEUX: Duration = Duration::from_secs(2 * 3600);

/// Quand une vérification tombe pendant qu'on travaille : réessayer bientôt.
const REESSAI_OCCUPE: Duration = Duration::from_secs(10 * 60);

/// La préférence, gardée à côté des autres réglages de l'application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePrefs {
    /// Mettre à jour l'application et les extensions sans rien demander.
    #[serde(default = "vrai")]
    pub automatic: bool,
}

fn vrai() -> bool {
    true
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        Self { automatic: true }
    }
}

impl UpdatePrefs {
    fn chemin(data_dir: &Path) -> PathBuf {
        data_dir.join("mises-a-jour.json")
    }

    /// Un fichier absent ou illisible rend le défaut : mises à jour actives.
    pub fn load(data_dir: &Path) -> Self {
        std::fs::read_to_string(Self::chemin(data_dir))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let raw = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(Self::chemin(data_dir), raw)
    }
}

#[tauri::command]
pub fn get_update_prefs(core: State<'_, Core>) -> UpdatePrefs {
    UpdatePrefs::load(&core.data_dir)
}

#[tauri::command]
pub fn set_update_prefs(core: State<'_, Core>, prefs: UpdatePrefs) -> Result<(), String> {
    prefs.save(&core.data_dir).map_err(|e| e.to_string())
}

/// Personne ne se sert de l'application : aucune réponse, aucun travail
/// d'arrière-plan.
fn personne_ne_travaille(core: &Core) -> bool {
    !core.supervisor.utilisateur_occupe() && !core.supervisor.arriere_plan_actif()
}

/// L'application peut-elle se remplacer maintenant ? Au repos, et sans modèle
/// chargé : son installation la relance et viderait la mémoire de la carte.
#[tauri::command]
pub async fn app_au_repos(core: State<'_, Core>) -> Result<bool, String> {
    if !personne_ne_travaille(&core) {
        return Ok(false);
    }
    let moteur_charge = core
        .supervisor
        .status_snapshot()
        .await
        .iter()
        .any(|e| e.owned && e.child_alive);
    Ok(!moteur_charge)
}

/// Lancer la boucle des mises à jour d'extensions. Elle vit autant que
/// l'application, fenêtre ouverte ou réduite dans la zone de notification.
pub fn demarrer(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PREMIERE_VERIFICATION).await;
        loop {
            let attente = un_passage(&app).await;
            tokio::time::sleep(attente).await;
        }
    });
}

/// Un passage : rien si la case est décochée ou si l'on travaille, sinon les
/// extensions qui ont une nouvelle version. Rend le délai avant le suivant.
async fn un_passage(app: &AppHandle) -> Duration {
    let core = app.state::<Core>();
    if !UpdatePrefs::load(&core.data_dir).automatic {
        return ENTRE_DEUX;
    }
    // Connecté à un serveur, les extensions vivent chez lui : ce n'est pas à
    // ce poste de les remplacer.
    if core.remote_client().is_some() {
        return ENTRE_DEUX;
    }
    if !personne_ne_travaille(&core) {
        return REESSAI_OCCUPE;
    }
    let verifications = match crate::extensions::verifier_mises_a_jour(&core).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(erreur = %e, "vérification des mises à jour d'extensions impossible");
            return ENTRE_DEUX;
        }
    };
    let a_faire: Vec<_> = verifications
        .into_iter()
        .filter(|v| v.update_available)
        .collect();
    let mut mises = 0;
    for v in a_faire {
        // Entre deux extensions, quelqu'un a pu se remettre au travail.
        if !personne_ne_travaille(&core) {
            return REESSAI_OCCUPE;
        }
        if mettre_a_jour(&core, &v.id, v.latest_version.as_deref()).await {
            mises += 1;
        }
    }
    if mises > 0 {
        tracing::info!(mises, "extensions mises à jour automatiquement");
        prevenir_l_interface(app);
    }
    ENTRE_DEUX
}

/// Une extension, et le journal de ce qui s'est passé. L'ancienne version
/// reste en place si la nouvelle ne s'installe pas.
async fn mettre_a_jour(core: &Core, id: &str, version: Option<&str>) -> bool {
    match crate::extensions::mettre_a_jour_extension(core, id).await {
        Ok(ext) => {
            tracing::info!(extension = %ext.name, version = %ext.version, "extension mise à jour automatiquement");
            true
        }
        Err(e) => {
            tracing::warn!(extension = %id, version = ?version, erreur = %e, "mise à jour automatique échouée, version actuelle gardée");
            false
        }
    }
}

/// La liste des extensions affichée se relit : sans ce signal, elle montrait
/// encore « mise à jour disponible » pour ce qui venait d'être installé.
fn prevenir_l_interface(app: &AppHandle) {
    use tauri::Emitter;
    if let Err(e) = app.emit("locaryn://extensions-mises-a-jour", ()) {
        tracing::warn!(erreur = %e, "interface non prévenue des mises à jour");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une installation neuve met à jour d'elle-même ; un réglage enregistré
    /// est relu tel quel.
    #[test]
    fn les_mises_a_jour_sont_actives_par_defaut_et_le_reglage_tient() {
        let dir = std::env::temp_dir().join(format!("locaryn-maj-{}", std::process::id()));
        assert!(UpdatePrefs::load(&dir).automatic);
        UpdatePrefs { automatic: false }
            .save(&dir)
            .expect("réglage écrit");
        assert!(!UpdatePrefs::load(&dir).automatic);
        std::fs::write(UpdatePrefs::chemin(&dir), "pas du json").expect("fichier abîmé");
        assert!(
            UpdatePrefs::load(&dir).automatic,
            "illisible : retour au défaut"
        );
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            eprintln!("dossier d'essai non retiré : {e}");
        }
    }
}
