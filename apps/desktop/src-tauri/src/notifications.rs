//! Prévenir la personne quand elle ne regarde pas, et montrer l'avancement là
//! où elle regarde déjà : la barre des tâches.
//!
//! Le centre de notifications de l'application ne sert qu'à qui l'a sous les
//! yeux. Or on lance un téléchargement de plusieurs gigaoctets, un workflow
//! d'une heure, et on va faire autre chose. Deux choses manquaient alors : une
//! bannière du système à la fin, et un moyen de suivre l'avancement sans
//! rouvrir la fenêtre.
//!
//! **Les préférences vont par groupe, pas par événement.** Une case par
//! notification aurait donné une page de trente interrupteurs que personne ne
//! lit ; six groupes se parcourent d'un coup d'œil, et chacun répond à une
//! question qu'on se pose vraiment — « est-ce que je veux être dérangé pour
//! ça ? ».
//!
//! **Deux règles valent pour tous les groupes.** On ne prévient pas quelqu'un
//! qui regarde déjà la fenêtre : elle montre la même chose, et doubler chaque
//! fin de tâche d'une bannière devient vite insupportable. Et une tâche finie
//! en trois secondes ne mérite pas de bannière — seule celle qu'on a eu le
//! temps d'oublier la mérite, d'où un seuil de durée.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{Manager, State};

use crate::Core;

/// Ce pour quoi l'application accepte de déranger.
///
/// Chaque champ garde un défaut raisonnable pour qu'une installation neuve se
/// comporte bien sans que personne n'ouvre cet écran.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotificationPrefs {
    /// Le maître d'œuvre : à `false`, aucune bannière du système ne part.
    #[serde(default = "vrai")]
    pub enabled: bool,

    /// Un outil attend une approbation.
    ///
    /// Le plus important du lot, et le seul dont l'absence **arrête le
    /// travail** : tant que personne ne répond, l'agent est suspendu. C'est le
    /// dernier groupe qu'on devrait éteindre.
    #[serde(default = "vrai")]
    pub approvals: bool,

    /// Fin ou échec d'un téléchargement de modèle.
    #[serde(default = "vrai")]
    pub downloads: bool,

    /// Une tâche longue s'achève — réponse du modèle, workflow, génération.
    #[serde(default = "vrai")]
    pub long_tasks: bool,

    /// Le modèle a été déchargé de la mémoire après une longue inactivité.
    #[serde(default = "vrai")]
    pub model_lifecycle: bool,

    /// Appairage d'un appareil, connexion d'un nouvel appareil au mode serveur.
    #[serde(default = "vrai")]
    pub security: bool,

    /// Mise à jour disponible, espace disque bas.
    #[serde(default = "vrai")]
    pub maintenance: bool,

    /// Ne prévenir que si la fenêtre n'est pas au premier plan.
    ///
    /// À `false`, les bannières partent même quand on regarde — utile sur
    /// plusieurs écrans, insupportable sur un seul.
    #[serde(default = "vrai")]
    pub only_when_hidden: bool,

    /// En dessous de cette durée, une tâche se termine sans bannière.
    #[serde(default = "seuil_par_defaut")]
    pub min_duration_seconds: u32,

    /// Montrer l'avancement dans la barre des tâches du système.
    #[serde(default = "vrai")]
    pub taskbar_progress: bool,
}

fn vrai() -> bool {
    true
}

/// Vingt secondes : le temps qu'il faut pour partir faire autre chose.
fn seuil_par_defaut() -> u32 {
    20
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            enabled: true,
            approvals: true,
            downloads: true,
            long_tasks: true,
            model_lifecycle: true,
            security: true,
            maintenance: true,
            only_when_hidden: true,
            min_duration_seconds: seuil_par_defaut(),
            taskbar_progress: true,
        }
    }
}

impl NotificationPrefs {
    fn chemin(data_dir: &Path) -> std::path::PathBuf {
        data_dir.join("notifications.json")
    }

    /// Lit les préférences, ou rend les défauts.
    ///
    /// Un fichier illisible n'est pas une panne : on repart des défauts plutôt
    /// que de refuser de notifier, ce qui serait une régression silencieuse.
    pub fn load(data_dir: &Path) -> Self {
        std::fs::read_to_string(Self::chemin(data_dir))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let raw = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(Self::chemin(data_dir), raw)
    }
}

#[tauri::command]
pub fn get_notification_prefs(core: State<'_, Core>) -> NotificationPrefs {
    NotificationPrefs::load(&core.data_dir)
}

#[tauri::command]
pub fn set_notification_prefs(
    core: State<'_, Core>,
    prefs: NotificationPrefs,
) -> Result<(), String> {
    prefs.save(&core.data_dir).map_err(|e| e.to_string())
}

/// L'état de la barre de progression, tel que l'interface le demande.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskbarProgress {
    /// 0 à 100. Absent : avancement inconnu, la barre défile.
    pub progress: Option<u64>,
    /// `none`, `normal`, `indeterminate`, `paused`, `error`.
    pub status: String,
}

/// Pose l'avancement sur l'icône de la barre des tâches.
///
/// Windows la dessine dans l'icône, macOS dans le Dock, Linux selon
/// l'environnement de bureau. C'est le seul endroit où l'on peut suivre un
/// téléchargement sans rouvrir la fenêtre — et il ne coûte rien à celui qui la
/// garde ouverte.
///
/// Un état inconnu est refusé plutôt que traduit au hasard : une barre qui
/// annonce « terminé » sur une erreur ment.
#[tauri::command]
pub fn set_taskbar_progress(
    app: tauri::AppHandle,
    core: State<'_, Core>,
    state: TaskbarProgress,
) -> Result<(), String> {
    if !NotificationPrefs::load(&core.data_dir).taskbar_progress {
        return Ok(());
    }
    let status = match state.status.as_str() {
        "none" => tauri::window::ProgressBarStatus::None,
        "normal" => tauri::window::ProgressBarStatus::Normal,
        "indeterminate" => tauri::window::ProgressBarStatus::Indeterminate,
        "paused" => tauri::window::ProgressBarStatus::Paused,
        "error" => tauri::window::ProgressBarStatus::Error,
        autre => return Err(format!("état de barre de progression inconnu : {autre}")),
    };
    let Some(fenetre) = app.get_webview_window("main") else {
        // Pas de fenêtre — l'application tourne réduite dans la zone de
        // notification. Il n'y a alors pas d'icône à décorer, et ce n'est pas
        // une erreur.
        return Ok(());
    };
    fenetre
        .set_progress_bar(tauri::window::ProgressBarState {
            status: Some(status),
            progress: state.progress.map(|p| p.min(100)),
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une installation neuve doit se comporter bien sans que personne n'ouvre
    /// l'écran des préférences.
    #[test]
    fn les_defauts_notifient_ce_qui_compte() {
        let p = NotificationPrefs::default();
        assert!(p.enabled);
        assert!(
            p.approvals,
            "l'approbation bloque le travail : jamais muette"
        );
        assert!(p.only_when_hidden, "ne pas doubler ce qu'on regarde deja");
        assert_eq!(p.min_duration_seconds, 20);
    }

    /// Un fichier ecrit par une version anterieure n'a pas tous les champs :
    /// il doit se charger, pas faire taire les notifications.
    #[test]
    fn un_fichier_incomplet_garde_les_defauts() {
        let p: NotificationPrefs = serde_json::from_str(r#"{"downloads": false}"#).unwrap();
        assert!(!p.downloads);
        assert!(p.approvals);
        assert!(p.enabled);
        assert_eq!(p.min_duration_seconds, 20);
    }

    #[test]
    fn les_preferences_font_l_aller_retour_sur_le_disque() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn-notif-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let p = NotificationPrefs {
            downloads: false,
            min_duration_seconds: 90,
            ..Default::default()
        };
        p.save(&dir).expect("l'ecriture doit reussir");

        let relu = NotificationPrefs::load(&dir);
        assert!(!relu.downloads);
        assert_eq!(relu.min_duration_seconds, 90);
        assert!(relu.approvals);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un dossier sans fichier rend les defauts, sans erreur.
    #[test]
    fn un_dossier_vide_rend_les_defauts() {
        let dir = std::env::temp_dir().join("locaryn-notif-inexistant-xyz");
        let _ = std::fs::remove_dir_all(&dir);
        let p = NotificationPrefs::load(&dir);
        assert!(p.enabled);
    }
}
