//! Rapports anonymes envoyés au point de collecte de l'équipe.
//!
//! Trois occasions :
//! - une erreur (plantage, conversation qui échoue) : automatique seulement si
//!   la personne l'a accepté (accueil ou « À propos »), sinon à la demande via
//!   le bouton « Envoyer les journaux » de l'erreur ;
//! - une note (pouce levé ou baissé) sur une réponse : l'échange noté (question, réponse, outils
//!   appelés) part avec, puisque c'est lui qu'on juge ;
//! - « J'ai un problème » : la description de la personne, et le dernier
//!   échange si elle le joint ;
//! - un envoi manuel depuis les réglages.
//!
//! Ce qui part : les journaux du service et du modèle (`journaux::
//! rassembler_pour_envoi`, prompts retirés, secrets masqués), la version, les
//! morphs installés et le modèle utilisé. Jamais le nom de la personne, ses
//! chemins, son IP locale, ses clés. Les conversations ne partent que pour
//! l'échange qu'elle note, nettoyé de la même façon.
//!
//! Garde-fous : un même type d'erreur au plus une fois toutes les 10 minutes,
//! 20 rapports automatiques par jour au plus.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use locaryn_shared_types::ExtensionKind;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::Core;

const ADRESSE_PAR_DEFAUT: &str = "https://arthonetwork.fr/locaryn/api/logs";
const FICHIER_REGLAGE: &str = "rapports.json";
const FICHIER_EN_ATTENTE: &str = "rapport_en_attente.txt";
const INTERVALLE_MEME_ERREUR: Duration = Duration::from_secs(600);
const MAX_PAR_JOUR: u32 = 20;
/// Taille maximale d'un champ de l'échange noté (le serveur refuse au-delà de 3 Mo).
const MAX_CHAMP_ECHANGE: usize = 60_000;

static DERNIERS: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);
static DU_JOUR: LazyLock<Mutex<(u64, u32)>> = LazyLock::new(Default::default);
/// Le modèle et les morphs du moment, tenus à jour par l'interface : un
/// rapport automatique part d'endroits qui n'ont pas accès au noyau.
static CONTEXTE: LazyLock<Mutex<Contexte>> = LazyLock::new(Default::default);

#[derive(Debug, Clone, Default, Serialize)]
struct Morph {
    nom: String,
    version: String,
    actif: bool,
}

#[derive(Debug, Clone, Default)]
struct Contexte {
    modele: Option<String>,
    morphs: Vec<Morph>,
}

/// Un échange noté : la demande, la réponse, les outils appelés entre les deux.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Echange {
    pub question: String,
    pub reponse: String,
    #[serde(default)]
    pub outils: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Gravite {
    Critique,
    Erreur,
    Info,
}

/// Un rapport prêt à partir.
#[derive(Debug, Clone, Serialize)]
struct Rapport {
    genre: &'static str,
    gravite: Gravite,
    note: Option<&'static str>,
    titre: String,
    detail: String,
    echange: Option<Echange>,
}

/// L'adresse du point de collecte : celle fixée à la compilation si elle est
/// valide, sinon celle de l'équipe.
fn adresse() -> &'static str {
    match option_env!("LOCARYN_RAPPORTS_URL") {
        Some(a) if a.starts_with("https://") => a,
        _ => ADRESSE_PAR_DEFAUT,
    }
}

/// Un point de collecte est-il configuré dans cette version ?
pub fn disponible() -> bool {
    adresse().starts_with("https://")
}

pub fn actif() -> bool {
    disponible()
        && std::fs::read_to_string(locaryn_config::default_data_dir().join(FICHIER_REGLAGE))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("actif").and_then(|a| a.as_bool()))
            .unwrap_or(false)
}

fn jour() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// Ce rapport peut-il partir maintenant (cadence et plafond du jour) ?
fn autorise(cle: &str) -> bool {
    let mut d = DERNIERS.lock().unwrap_or_else(|e| e.into_inner());
    if d.get(cle)
        .is_some_and(|t| t.elapsed() < INTERVALLE_MEME_ERREUR)
    {
        return false;
    }
    let mut j = DU_JOUR.lock().unwrap_or_else(|e| e.into_inner());
    let aujourd_hui = jour();
    if j.0 != aujourd_hui {
        *j = (aujourd_hui, 0);
    }
    if j.1 >= MAX_PAR_JOUR {
        return false;
    }
    j.1 += 1;
    d.insert(cle.to_string(), Instant::now());
    true
}

/// Signaler une erreur. Ne fait rien si l'envoi automatique est coupé.
pub fn signaler(titre: &str, detail: &str) {
    signaler_avec(titre, detail, Gravite::Erreur);
}

fn signaler_avec(titre: &str, detail: &str, gravite: Gravite) {
    if !actif() {
        return;
    }
    let cle: String = format!("{titre}|{}", detail.chars().take(80).collect::<String>());
    if !autorise(&cle) {
        return;
    }
    let rapport = Rapport {
        genre: "erreur",
        gravite,
        note: None,
        titre: titre.to_string(),
        detail: crate::journaux::nettoyer_pour_envoi(detail),
        echange: None,
    };
    tauri::async_runtime::spawn(async move {
        if let Err(e) = envoyer(rapport).await {
            tracing::warn!(erreur = %e, "rapport d'erreur non envoyé");
        }
    });
}

/// Un plantage de la session précédente, noté par le gestionnaire de
/// panique : envoyé au lancement suivant, comme bug critique.
pub fn envoyer_en_attente() {
    let chemin = locaryn_config::default_data_dir().join(FICHIER_EN_ATTENTE);
    let Ok(detail) = std::fs::read_to_string(&chemin) else {
        return;
    };
    if let Err(e) = std::fs::remove_file(&chemin) {
        tracing::warn!(erreur = %e, "rapport en attente non effacé");
    }
    signaler_avec("Plantage de l'application", &detail, Gravite::Critique);
}

/// Gestionnaire de panique : `crash.log`, et un rapport pour le prochain
/// lancement (l'application ne peut plus rien envoyer en train de tomber).
pub fn installer_gestionnaire_de_panique() {
    let precedent = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let lieu = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let texte = format!(
            "=== Locaryn {} — panique ===\nlieu : {lieu}\nmessage : {message}\n{}\n",
            env!("CARGO_PKG_VERSION"),
            std::backtrace::Backtrace::force_capture()
        );
        let data = locaryn_config::default_data_dir();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(data.join("crash.log"))
        {
            use std::io::Write;
            let _ = writeln!(f, "{texte}");
        }
        let _ = std::fs::write(data.join(FICHIER_EN_ATTENTE), &texte);
        precedent(info);
    }));
}

/// Tronquer un texte à `max` caractères, sans couper un caractère en deux.
fn tronquer(t: &str, max: usize) -> String {
    if t.chars().count() <= max {
        return t.to_string();
    }
    let mut s: String = t.chars().take(max).collect();
    s.push_str("\n[…tronqué]");
    s
}

/// L'échange noté, nettoyé comme les journaux : ni chemin, ni e-mail, ni clé.
fn nettoyer_echange(e: Echange) -> Echange {
    let m = crate::journaux::masquer;
    Echange {
        question: tronquer(&m(&e.question), MAX_CHAMP_ECHANGE),
        reponse: tronquer(&m(&e.reponse), MAX_CHAMP_ECHANGE),
        outils: e.outils.iter().take(50).map(|o| m(o)).collect(),
    }
}

/// Le corps JSON envoyé : le rapport, la version, le modèle, les morphs, les journaux.
fn corps(r: &Rapport) -> serde_json::Value {
    let ctx = CONTEXTE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    serde_json::json!({
        "genre": r.genre,
        "gravite": r.gravite,
        "note": r.note,
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "modele": ctx.modele.as_deref().map(crate::journaux::masquer),
        "morphs": ctx.morphs,
        "titre": r.titre,
        "detail": tronquer(&r.detail, 20_000),
        "echange": r.echange,
        "journaux": crate::journaux::rassembler_pour_envoi(),
    })
}

async fn envoyer(r: Rapport) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let reponse = client
        .post(adresse())
        .json(&corps(&r))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    match reponse.status().as_u16() {
        200..=299 => Ok(()),
        429 => {
            Err("Trop d'envois depuis cette connexion : réessayez dans quelques minutes.".into())
        }
        413 => Err("Rapport trop volumineux.".into()),
        code => Err(format!("Le point de collecte a répondu {code}.")),
    }
}

/// Lire les morphs installés pour les rapports suivants.
async fn rafraichir_morphs(core: State<'_, Core>) {
    match crate::extensions::liste_installees(core).await {
        Ok(liste) => {
            let morphs = liste
                .into_iter()
                .filter(|e| e.kind == ExtensionKind::Plugin)
                .map(|e| Morph {
                    nom: e.name,
                    version: e.version,
                    actif: e.enabled,
                })
                .collect();
            CONTEXTE.lock().unwrap_or_else(|e| e.into_inner()).morphs = morphs;
        }
        Err(e) => tracing::warn!(erreur = %e, "morphs illisibles pour les rapports"),
    }
}

#[derive(Serialize)]
pub struct EtatRapports {
    pub disponible: bool,
    pub actif: bool,
}

fn etat() -> EtatRapports {
    EtatRapports {
        disponible: disponible(),
        actif: actif(),
    }
}

/// L'état de l'envoi automatique. L'interface y joint le modèle utilisé : il
/// accompagnera les rapports suivants.
#[tauri::command]
pub async fn rapports_etat(
    core: State<'_, Core>,
    modele: Option<String>,
) -> Result<EtatRapports, String> {
    if let Some(m) = modele.filter(|m| !m.trim().is_empty()) {
        CONTEXTE.lock().unwrap_or_else(|e| e.into_inner()).modele = Some(m);
    }
    rafraichir_morphs(core).await;
    Ok(etat())
}

#[tauri::command]
pub fn rapports_regler(actif: bool) -> Result<EtatRapports, String> {
    std::fs::write(
        locaryn_config::default_data_dir().join(FICHIER_REGLAGE),
        serde_json::json!({ "actif": actif }).to_string(),
    )
    .map_err(|e| e.to_string())?;
    Ok(etat())
}

/// Ce que l'interface demande d'envoyer.
#[derive(Debug, Default, Deserialize)]
pub struct Demande {
    pub message: Option<String>,
    pub genre: Option<String>,
    pub note: Option<String>,
    /// `critique` : un bug que la personne juge grave (pouce baissé, problème décrit).
    pub gravite: Option<String>,
    pub echange: Option<Echange>,
}

fn titre_de_note(n: &str, critique: bool) -> String {
    match (n, critique) {
        ("up", _) => "Réponse notée : pouce levé".into(),
        (_, true) => "Réponse notée : pouce baissé, bug critique".into(),
        _ => "Réponse notée : pouce baissé".into(),
    }
}

fn rapport_demande(d: Demande) -> Result<Rapport, String> {
    let critique = d.gravite.as_deref() == Some("critique");
    let detail = crate::journaux::nettoyer_pour_envoi(
        &d.message
            .unwrap_or_else(|| "Envoyé à la demande de l'utilisateur.".into()),
    );
    let echange = d.echange;
    let rapport = match (d.genre.as_deref(), d.note.as_deref()) {
        (Some("note"), Some(n @ ("up" | "down"))) => Rapport {
            genre: "note",
            gravite: if critique && n == "down" {
                Gravite::Critique
            } else {
                Gravite::Info
            },
            note: Some(if n == "up" { "up" } else { "down" }),
            titre: titre_de_note(n, critique),
            detail,
            echange: echange.map(nettoyer_echange),
        },
        (Some("probleme"), _) => Rapport {
            genre: "probleme",
            gravite: if critique {
                Gravite::Critique
            } else {
                Gravite::Erreur
            },
            note: None,
            titre: "Problème signalé par l'utilisateur".into(),
            detail,
            echange: echange.map(nettoyer_echange),
        },
        (Some("note"), _) => return Err("Note inconnue : « up » ou « down » attendu.".into()),
        (Some("erreur"), _) => Rapport {
            genre: "erreur",
            gravite: Gravite::Erreur,
            note: None,
            titre: "Erreur envoyée par l'utilisateur".into(),
            detail,
            echange: None,
        },
        _ => Rapport {
            genre: "manuel",
            gravite: Gravite::Info,
            note: None,
            titre: "Rapport envoyé par l'utilisateur".into(),
            detail,
            echange: None,
        },
    };
    Ok(rapport)
}

/// Envoyer un rapport tout de suite, à la demande : bouton « Envoyer les
/// journaux » d'une erreur (`genre = "erreur"`), note d'une réponse
/// (`genre = "note"`, `note = "up" | "down"`, avec l'échange) ou réglages.
#[tauri::command]
pub async fn rapports_envoyer(
    core: State<'_, Core>,
    message: Option<String>,
    genre: Option<String>,
    note: Option<String>,
    gravite: Option<String>,
    echange: Option<Echange>,
    modele: Option<String>,
) -> Result<(), String> {
    if !disponible() {
        return Err("Aucun point de collecte dans cette version : copiez les journaux.".into());
    }
    if let Some(m) = modele.filter(|m| !m.trim().is_empty()) {
        CONTEXTE.lock().unwrap_or_else(|e| e.into_inner()).modele = Some(m);
    }
    rafraichir_morphs(core).await;
    let demande = Demande {
        message,
        genre,
        note,
        gravite,
        echange,
    };
    envoyer(rapport_demande(demande)?).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_note_porte_l_echange_nettoye() {
        let r = rapport_demande(Demande {
            genre: Some("note".into()),
            note: Some("down".into()),
            gravite: Some("critique".into()),
            echange: Some(Echange {
                question: r"lis C:\Users\Paul\notes.txt et écris à paul@ex.fr".into(),
                reponse: "voici la clé sk-abcdef1234567890xyz".into(),
                outils: vec!["read_file".into()],
            }),
            ..Default::default()
        })
        .expect("note valide");
        assert_eq!(r.genre, "note");
        assert_eq!(r.gravite, Gravite::Critique);
        assert_eq!(r.note, Some("down"));
        let e = r.echange.expect("échange joint");
        assert!(!e.question.contains("Paul") && !e.question.contains("paul@"));
        assert!(!e.reponse.contains("1234567890xyz"));
        assert_eq!(e.outils, vec!["read_file".to_string()]);
    }

    #[test]
    fn hors_note_aucun_echange_ne_part() {
        let r = rapport_demande(Demande {
            message: Some("Erreur : le moteur ne démarre pas".into()),
            genre: Some("erreur".into()),
            echange: Some(Echange::default()),
            ..Default::default()
        })
        .expect("erreur valide");
        assert_eq!(r.gravite, Gravite::Erreur);
        assert!(r.echange.is_none());
    }

    #[test]
    fn une_note_inconnue_est_refusee() {
        let d = Demande {
            genre: Some("note".into()),
            note: Some("bof".into()),
            ..Default::default()
        };
        assert!(rapport_demande(d).is_err());
    }

    #[test]
    fn un_pouce_leve_n_est_jamais_critique() {
        let r = rapport_demande(Demande {
            genre: Some("note".into()),
            note: Some("up".into()),
            gravite: Some("critique".into()),
            ..Default::default()
        })
        .expect("note valide");
        assert_eq!(r.gravite, Gravite::Info);
    }

    #[test]
    fn un_probleme_decrit_garde_sa_description_nettoyee() {
        let r = rapport_demande(Demande {
            message: Some("ça plante quand j'ouvre /home/lucie/doc.pdf".into()),
            genre: Some("probleme".into()),
            gravite: Some("critique".into()),
            ..Default::default()
        })
        .expect("problème valide");
        assert_eq!((r.genre, r.gravite), ("probleme", Gravite::Critique));
        assert!(!r.detail.contains("lucie") && r.detail.contains("~/doc.pdf"));
    }

    #[test]
    fn la_troncature_respecte_les_caracteres() {
        let t = tronquer("ééééé", 3);
        assert!(t.starts_with("ééé") && t.contains("tronqué"));
    }
}
