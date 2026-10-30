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
use std::collections::HashSet;
use std::sync::Mutex;
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

/// Ce que dit l'écran de la mémoire pendant un tri forcé.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progression {
    pub running: bool,
    /// Ce que le tri fait en ce moment, en une phrase.
    pub phase: String,
    pub conversations_total: u32,
    pub conversations_done: u32,
    pub tasks_total: u32,
    pub tasks_done: u32,
    pub facts_learned: u32,
    pub cards_merged: u32,
    pub suggestions_written: u32,
    /// Les dernières conversations lues, de la plus récente à la plus ancienne.
    pub recent: Vec<String>,
    pub finished: bool,
    pub error: Option<String>,
    /// Une fois fini : dans combien de secondes le modèle se décharge s'il
    /// reste inutilisé.
    pub unload_in_secs: Option<u64>,
}

static PROGRESSION: Mutex<Option<Progression>> = Mutex::new(None);

fn progression_modifier(f: impl FnOnce(&mut Progression)) {
    let mut g = PROGRESSION.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Progression::default));
}

/// L'état du dernier tri (vide tant qu'aucun n'a eu lieu).
pub fn progression() -> Progression {
    PROGRESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// Un échange lu dans une conversation.
struct Echange {
    quand: chrono::DateTime<chrono::Utc>,
    session: uuid::Uuid,
    titre: String,
    texte: String,
}

/// Les échanges (question, réponse) écrits après `depuis` — ou tous —, dans
/// toutes les conversations non éphémères, y compris les conversations
/// libres : c'est là que se disent le métier et les goûts. Les plus anciens
/// d'abord.
async fn collecter(
    core: &Core,
    depuis: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<Echange>, String> {
    let mut echanges = Vec::new();
    let projets = core
        .storage
        .projects
        .list()
        .await
        .map_err(|e| e.to_string())?;
    for projet in projets {
        let Ok(sessions) = core.storage.sessions.list_for_project(projet.id).await else {
            continue;
        };
        for session in sessions {
            let recent = depuis.is_none_or(|d| session.last_message_at.is_some_and(|t| t > d));
            if session.ephemeral || !recent {
                continue;
            }
            let Ok(msgs) = core.storage.messages.list_for_session(session.id).await else {
                continue;
            };
            let titre = session
                .title
                .clone()
                .unwrap_or_else(|| "Conversation".into());
            let mut question: Option<&str> = None;
            for m in &msgs {
                match m.role {
                    MessageRole::User => question = Some(m.content.as_str()),
                    MessageRole::Assistant if depuis.is_none_or(|d| m.created_at > d) => {
                        if let Some(q) = question.take() {
                            if !q.trim().is_empty() && !m.content.trim().is_empty() {
                                echanges.push(Echange {
                                    quand: m.created_at,
                                    session: session.id,
                                    titre: titre.clone(),
                                    texte: format!(
                                        "Personne : {q}
Assistant : {}",
                                        m.content
                                    ),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    echanges.sort_by_key(|e| e.quand);
    Ok(echanges)
}

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

/// Le modèle de micro-tâches et l'endpoint du moteur de conversation, quand un
/// modèle local est chargé et répond. `None` : rien à faire, et rien n'est
/// rechargé pour ça.
async fn moteur_pret(core: &Core) -> Option<(String, String)> {
    let sup = &core.supervisor;
    let p = core.storage.providers.active().await.ok().flatten()?;
    if p.engine != ProviderEngine::LlamaCpp || !sup.is_healthy(&p.engine).await {
        return None;
    }
    let micro = locaryn_config::load(None)
        .ok()
        .and_then(|c| c.assistance.micro_model)
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| locaryn_config::MICRO_MODEL_ACTIF.to_string());
    let micro = locaryn_config::micro_effectif(&micro, p.model.as_deref());
    Some((p.endpoint, micro))
}

/// Lire un échange et retenir ce qu'il apprend. Rend le nombre de faits.
async fn apprendre(core: &Core, endpoint: &str, micro: &str, echange: &str) -> u32 {
    let faits =
        locaryn_agent_runtime::titling::ask_for_memory(endpoint, &core.http, micro, echange).await;
    let mut appris = 0;
    for f in faits {
        match core
            .storage
            .memory
            .remember(None, &f.group, &f.title, &f.detail, "assistant")
            .await
        {
            Ok(_) => appris += 1,
            Err(e) => tracing::debug!(erreur = %e, "fait non enregistré"),
        }
    }
    appris
}

// ── Les phrases d'accueil, écrites d'avance ─────────────────────────────────

const SUGGESTIONS: &str = "welcome_suggestions.json";
/// Au-delà, les phrases sont réécrites au prochain moment calme.
const SUGGESTIONS_VALIDITE_H: i64 = 12;

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhrasesAccueil {
    #[serde(default)]
    pub simple: Vec<String>,
    #[serde(default)]
    pub expert: Vec<String>,
    #[serde(default)]
    pub generated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Les phrases d'accueil déjà écrites, si elles existent.
pub fn lire_phrases(data_dir: &std::path::Path) -> PhrasesAccueil {
    std::fs::read_to_string(data_dir.join(SUGGESTIONS))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn phrases_perimees(data_dir: &std::path::Path) -> bool {
    let p = lire_phrases(data_dir);
    p.simple.is_empty()
        || p.expert.is_empty()
        || p.generated_at.is_none_or(|t| {
            chrono::Utc::now() - t > chrono::Duration::hours(SUGGESTIONS_VALIDITE_H)
        })
}

/// Écrire les phrases d'accueil des deux styles d'après la mémoire. Rend le
/// nombre de phrases obtenues ; un style qui échoue garde ses anciennes phrases.
async fn ecrire_phrases(core: &Core, endpoint: &str, micro: &str) -> u32 {
    use locaryn_agent_runtime::titling::{ask_for_suggestions, StyleSuggestion};
    let memoire = core
        .storage
        .memory
        .as_system_block(None)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut phrases = lire_phrases(&core.data_dir);
    let mut ecrites = 0;
    for style in [StyleSuggestion::Simple, StyleSuggestion::Expert] {
        let v = ask_for_suggestions(endpoint, &core.http, micro, &memoire, style).await;
        if v.is_empty() {
            continue;
        }
        ecrites += v.len() as u32;
        match style {
            StyleSuggestion::Simple => phrases.simple = v,
            StyleSuggestion::Expert => phrases.expert = v,
        }
    }
    if ecrites > 0 {
        phrases.generated_at = Some(chrono::Utc::now());
        match serde_json::to_string(&phrases) {
            Ok(t) => {
                if let Err(e) = std::fs::write(core.data_dir.join(SUGGESTIONS), t) {
                    tracing::warn!(erreur = %e, "phrases d'accueil non enregistrées");
                }
            }
            Err(e) => tracing::warn!(erreur = %e, "phrases d'accueil illisibles"),
        }
    }
    ecrites
}

// ── Le passage automatique, pendant l'inactivité ────────────────────────────

async fn un_passage(core: &Core) -> Result<(), String> {
    let sup = &core.supervisor;
    if sup.utilisateur_occupe() {
        return Ok(());
    }
    let Some((endpoint, micro)) = moteur_pret(core).await else {
        return Ok(());
    };
    let Some(p) = core.storage.providers.active().await.ok().flatten() else {
        return Ok(());
    };
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
    let mut echanges = collecter(core, Some(depuis)).await?;
    echanges.truncate(ECHANGES_PAR_PASSAGE);

    let _arriere_plan = sup.arriere_plan();
    let stop = core.arriere_plan_stop.jeton();
    let mut appris = 0u32;
    for e in echanges {
        let faits = tokio::select! {
            biased;
            // Une personne a repris la main : ce qui est lu est gardé, le
            // reste attendra le prochain passage.
            _ = stop.cancelled() => return Ok(()),
            n = apprendre(core, &endpoint, &micro, &e.texte) => n,
        };
        appris += faits;
        etat.dernier = Some(e.quand);
        ecrire_etat(&core.data_dir, &etat);
        if sup.utilisateur_occupe() {
            return Ok(());
        }
    }
    if appris > 0 {
        tracing::info!(faits = appris, "mémoire enrichie pendant l'inactivité");
        if let Err(e) = core.storage.memory.consolidate(None).await {
            tracing::debug!(erreur = %e, "tri de la mémoire reporté");
        }
    }
    // Les phrases d'accueil se préparent dans le même temps mort : à
    // l'ouverture d'une conversation, elles sont déjà là.
    if (appris > 0 || phrases_perimees(&core.data_dir)) && !sup.utilisateur_occupe() {
        tokio::select! {
            biased;
            _ = stop.cancelled() => {}
            n = ecrire_phrases(core, &endpoint, &micro) => {
                if n > 0 {
                    tracing::info!(phrases = n, "phrases d'accueil préparées");
                }
            }
        }
    }
    Ok(())
}

// ── Le tri forcé, demandé depuis les réglages de la mémoire ─────────────────

/// Trier toute la mémoire maintenant, sans attendre l'inactivité.
///
/// Tout l'historique est relu, la mémoire est rangée, les phrases d'accueil
/// sont réécrites. Pendant ce temps le modèle est tenu comme utilisé : le
/// délai avant déchargement ne court pas, et il repart de zéro à la fin.
pub async fn trier_maintenant(core: &Core) -> Result<(), String> {
    if progression().running {
        return Err("Un tri est déjà en cours.".into());
    }
    let Some((endpoint, micro)) = moteur_pret(core).await else {
        return Err(
            "Aucun modèle local n'est chargé. Envoyez un message pour le charger, puis relancez \
             le tri."
                .into(),
        );
    };
    let echanges = collecter(core, None).await?;
    let sessions: HashSet<uuid::Uuid> = echanges.iter().map(|e| e.session).collect();
    progression_modifier(|p| {
        *p = Progression {
            running: true,
            phase: "Lecture des conversations".into(),
            conversations_total: sessions.len() as u32,
            tasks_total: echanges.len() as u32 + 3,
            ..Progression::default()
        };
    });

    // Tant que ce garde vit, le modèle sert : aucune éjection. À sa chute, le
    // compte à rebours repart.
    let garde = core.supervisor.utilisation();
    let resultat = trier(core, &endpoint, &micro, echanges).await;
    drop(garde);

    let delai = core.supervisor.idle_timeout_secs();
    progression_modifier(|p| {
        p.running = false;
        p.finished = true;
        p.unload_in_secs = Some(delai);
        p.phase = "Terminé".into();
        if let Err(e) = &resultat {
            p.error = Some(e.clone());
        }
    });
    resultat
}

async fn trier(
    core: &Core,
    endpoint: &str,
    micro: &str,
    echanges: Vec<Echange>,
) -> Result<(), String> {
    let mut vues: HashSet<uuid::Uuid> = HashSet::new();
    let mut dernier = None;
    for e in echanges {
        let faits = apprendre(core, endpoint, micro, &e.texte).await;
        let premiere = vues.insert(e.session);
        dernier = Some(e.quand);
        progression_modifier(|p| {
            p.tasks_done += 1;
            p.facts_learned += faits;
            p.conversations_done = vues.len() as u32;
            if premiere {
                p.recent.insert(0, e.titre.clone());
                p.recent.truncate(6);
            }
        });
    }
    if let Some(d) = dernier {
        let mut etat = lire_etat(&core.data_dir);
        etat.dernier = Some(etat.dernier.map_or(d, |v| v.max(d)));
        ecrire_etat(&core.data_dir, &etat);
    }

    progression_modifier(|p| p.phase = "Rangement de la mémoire".into());
    let bilan = core
        .storage
        .memory
        .consolidate(None)
        .await
        .map_err(|e| e.to_string())?;
    progression_modifier(|p| {
        p.tasks_done += 1;
        p.cards_merged = bilan.fiches_reunies;
    });

    progression_modifier(|p| p.phase = "Écriture des phrases d'accueil".into());
    let ecrites = ecrire_phrases(core, endpoint, micro).await;
    progression_modifier(|p| {
        p.tasks_done += 3;
        p.suggestions_written = ecrites;
    });
    Ok(())
}
