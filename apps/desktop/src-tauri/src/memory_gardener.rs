//! Le jardinier de la mémoire : pendant que personne ne se sert du modèle, il
//! relit les conversations récentes et en tire ce qui vaut d'être retenu.
//!
//! Une mémoire qu'il faut remplir soi-même reste vide. Les préférences se
//! disent en passant — « fais court », « je code en Rust » — et c'est en
//! relisant les échanges qu'on les entend. Le faire pendant la conversation
//! ralentirait chaque réponse ; le faire quand le modèle chargé attend, ne
//! coûte rien à personne.
//!
//! Trois règles :
//!
//! - **Jamais devant quelqu'un.** Une réponse demandée par la personne
//!   interrompt le travail en cours (sa requête est abandonnée, le moteur
//!   passe à la conversation) et le jardinier attend qu'elle soit finie.
//! - **Jamais un chargement pour ça.** Il ne travaille qu'avec le modèle déjà
//!   en mémoire ; un modèle éjecté n'est pas rechargé.
//! - **Pas une utilisation.** Ce travail ne repousse pas l'éjection : le
//!   compte à rebours continue, et à l'échéance le modèle part, tri fini ou
//!   non (`Supervisor::arriere_plan`).

use crate::Core;
use locaryn_shared_types::{MessageRole, ProviderEngine};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

/// Le temps sans utilisation avant de commencer : on ne trie pas pendant que
/// la personne lit la réponse et tape la suite.
const ATTENTE_AVANT_TRI: u64 = 120;
/// Échanges lus au plus par passage : le travail reste court et reprend au
/// passage suivant.
const ECHANGES_PAR_PASSAGE: usize = 6;
/// Le fichier qui dit jusqu'où les conversations ont été relues.
const ETAT: &str = "memory_gardener.json";

/// Interrompre d'un coup tout le travail d'arrière-plan en cours.
#[derive(Default)]
pub struct Interrupteur {
    jetons: std::sync::Mutex<Vec<CancellationToken>>,
}

impl Interrupteur {
    /// Un jeton pour un travail qui commence ; il est annulé au prochain
    /// `cancel_all`.
    pub fn jeton(&self) -> CancellationToken {
        let t = CancellationToken::new();
        let mut v = self.jetons.lock().unwrap_or_else(|e| e.into_inner());
        v.retain(|t| !t.is_cancelled());
        v.push(t.clone());
        t
    }

    /// Une personne a besoin du modèle : tout s'arrête.
    pub fn cancel_all(&self) {
        let v = std::mem::take(&mut *self.jetons.lock().unwrap_or_else(|e| e.into_inner()));
        for t in v {
            t.cancel();
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Etat {
    /// Le dernier message relu, toutes conversations confondues.
    dernier: Option<chrono::DateTime<chrono::Utc>>,
}

fn lire_etat(data_dir: &std::path::Path) -> Etat {
    std::fs::read_to_string(data_dir.join(ETAT))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn ecrire_etat(data_dir: &std::path::Path, etat: &Etat) {
    if let Ok(t) = serde_json::to_string(etat) {
        let _ = std::fs::write(data_dir.join(ETAT), t);
    }
}

/// La mémoire apprend-elle toute seule ? Activé par défaut ; la personne peut
/// le couper dans les réglages de la mémoire.
fn apprentissage_actif() -> bool {
    locaryn_config::load(None)
        .map(|c| !c.assistance.memory_learning_off)
        .unwrap_or(true)
}

/// Lancer le jardinier. Il tourne tant que l'application tourne.
pub fn lancer(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Au premier passage, seules les conversations des sept derniers
        // jours sont relues : pas de rattrapage de tout l'historique d'un coup.
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(core) = app.try_state::<Core>() else {
                continue;
            };
            if !apprentissage_actif() {
                continue;
            }
            if let Err(e) = un_passage(&core).await {
                tracing::debug!(erreur = %e, "tri de la mémoire reporté");
            }
        }
    });
}

async fn un_passage(core: &Core) -> Result<(), String> {
    let sup = &core.supervisor;
    if sup.utilisateur_occupe() {
        return Ok(());
    }
    let Some(p) = core.storage.providers.active().await.ok().flatten() else {
        return Ok(());
    };
    // Le modèle local déjà chargé, et lui seul.
    if p.engine != ProviderEngine::LlamaCpp || !sup.is_healthy(&p.engine).await {
        return Ok(());
    }
    let Some((inactif, _, _)) = sup.residency(&p.engine).await else {
        return Ok(());
    };
    if inactif < ATTENTE_AVANT_TRI {
        return Ok(());
    }

    let mut etat = lire_etat(&core.data_dir);
    let depuis = etat
        .dernier
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));

    // Les échanges (question, réponse) écrits depuis le dernier passage, les
    // plus anciens d'abord.
    let mut echanges: Vec<(chrono::DateTime<chrono::Utc>, String)> = Vec::new();
    let projets = core.storage.projects.list().await.map_err(|e| e.to_string())?;
    for projet in projets {
        let Ok(sessions) = core.storage.sessions.list_for_project(projet.id).await else {
            continue;
        };
        for session in sessions {
            if session.ephemeral || session.last_message_at.is_none_or(|t| t <= depuis) {
                continue;
            }
            let Ok(msgs) = core.storage.messages.list_for_session(session.id).await else {
                continue;
            };
            let mut question: Option<&str> = None;
            for m in &msgs {
                match m.role {
                    MessageRole::User => question = Some(m.content.as_str()),
                    MessageRole::Assistant if m.created_at > depuis => {
                        if let Some(q) = question.take() {
                            if !q.trim().is_empty() && !m.content.trim().is_empty() {
                                echanges.push((
                                    m.created_at,
                                    format!("Personne : {q}\nAssistant : {}", m.content),
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if echanges.is_empty() {
        return Ok(());
    }
    echanges.sort_by_key(|(t, _)| *t);
    echanges.truncate(ECHANGES_PAR_PASSAGE);

    let micro = locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model)
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| locaryn_config::MICRO_MODEL_ACTIF.to_string());
    let micro = locaryn_config::micro_effectif(&micro, p.model.as_deref());

    let _arriere_plan = sup.arriere_plan();
    let stop = core.arriere_plan_stop.jeton();
    let mut appris = 0usize;
    for (quand, echange) in echanges {
        let faits = tokio::select! {
            biased;
            _ = stop.cancelled() => {
                // Une personne a repris la main : ce qui est lu est gardé,
                // le reste attendra le prochain passage.
                break;
            }
            f = locaryn_agent_runtime::titling::ask_for_memory(&p.endpoint, &core.http, &micro, &echange) => f,
        };
        for f in faits {
            if core
                .storage
                .memory
                .remember(None, &f.group, &f.title, &f.detail, "assistant")
                .await
                .is_ok()
            {
                appris += 1;
            }
        }
        etat.dernier = Some(quand);
        ecrire_etat(&core.data_dir, &etat);
        if sup.utilisateur_occupe() {
            break;
        }
    }
    if appris > 0 {
        tracing::info!(faits = appris, "mémoire enrichie pendant l'inactivité");
    }
    Ok(())
}
