//! Rapports d'erreur envoyés automatiquement, si la personne l'a accepté.
//!
//! Désactivé par défaut. Une fois coché (« À propos → Envoyer les rapports
//! d'erreur »), un plantage, un moteur qui ne démarre pas ou une conversation
//! qui échoue envoient le résumé de l'erreur et les journaux (masqués : voir
//! `journaux::masquer`) au point de collecte de l'équipe. L'adresse est fixée
//! à la compilation (`LOCARYN_RAPPORTS_URL`, un webhook Discord ou toute URL
//! qui reçoit du JSON) : sans elle, l'option n'apparaît pas.
//!
//! Garde-fous : un même type d'erreur au plus une fois toutes les 10 minutes,
//! 20 rapports par jour au plus.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const ADRESSE: Option<&str> = option_env!("LOCARYN_RAPPORTS_URL");
const FICHIER_REGLAGE: &str = "rapports.json";
const FICHIER_EN_ATTENTE: &str = "rapport_en_attente.txt";
const INTERVALLE_MEME_ERREUR: Duration = Duration::from_secs(600);
const MAX_PAR_JOUR: u32 = 20;

static DERNIERS: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(Default::default);
static DU_JOUR: LazyLock<Mutex<(u64, u32)>> = LazyLock::new(Default::default);

/// Un point de collecte est-il configuré dans cette version ?
pub fn disponible() -> bool {
    ADRESSE.is_some_and(|a| a.starts_with("https://"))
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

/// Signaler une erreur. Ne fait rien si l'option est coupée.
pub fn signaler(titre: &str, detail: &str) {
    if !actif() {
        return;
    }
    let cle: String = format!("{titre}|{}", detail.chars().take(80).collect::<String>());
    if !autorise(&cle) {
        return;
    }
    let titre = titre.to_string();
    let detail = crate::journaux::masquer(detail);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = envoyer(&titre, &detail).await {
            tracing::warn!(erreur = %e, "rapport d'erreur non envoyé");
        }
    });
}

/// Un plantage de la session précédente, noté par le gestionnaire de
/// panique : envoyé au lancement suivant.
pub fn envoyer_en_attente() {
    let chemin = locaryn_config::default_data_dir().join(FICHIER_EN_ATTENTE);
    let Ok(detail) = std::fs::read_to_string(&chemin) else {
        return;
    };
    let _ = std::fs::remove_file(&chemin);
    signaler("Plantage de l'application", &detail);
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

/// Le corps envoyé : un message court, et les journaux en pièce jointe.
async fn envoyer(titre: &str, detail: &str) -> Result<(), String> {
    let adresse = ADRESSE.ok_or("aucun point de collecte")?;
    let journaux = format!("{titre}\n\n{detail}\n\n{}", crate::journaux::rassembler());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resume: String = detail.chars().take(1400).collect();
    let entete = format!(
        "**Locaryn {} — {titre}** ({} {})\n```\n{resume}\n```",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let reponse = if adresse.contains("discord.com/api/webhooks") {
        // Discord : un message et un fichier joint (multipart fait main).
        let frontiere = format!("locaryn{}", jour());
        let charge = serde_json::json!({ "content": entete, "username": "Locaryn" });
        let mut corps = Vec::new();
        corps.extend(
            format!(
                "--{frontiere}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n{charge}\r\n"
            )
            .into_bytes(),
        );
        corps.extend(
            format!(
                "--{frontiere}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"locaryn-journaux.txt\"\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n"
            )
            .into_bytes(),
        );
        corps.extend(journaux.into_bytes());
        corps.extend(format!("\r\n--{frontiere}--\r\n").into_bytes());
        client
            .post(adresse)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={frontiere}"),
            )
            .body(corps)
            .send()
            .await
    } else {
        client
            .post(adresse)
            .json(&serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "titre": titre,
                "detail": detail,
                "journaux": journaux,
            }))
            .send()
            .await
    }
    .map_err(|e| e.to_string())?;
    if !reponse.status().is_success() {
        return Err(format!("réponse {}", reponse.status()));
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct EtatRapports {
    pub disponible: bool,
    pub actif: bool,
}

#[tauri::command]
pub fn rapports_etat() -> EtatRapports {
    EtatRapports {
        disponible: disponible(),
        actif: actif(),
    }
}

#[tauri::command]
pub fn rapports_regler(actif: bool) -> Result<EtatRapports, String> {
    std::fs::write(
        locaryn_config::default_data_dir().join(FICHIER_REGLAGE),
        serde_json::json!({ "actif": actif }).to_string(),
    )
    .map_err(|e| e.to_string())?;
    Ok(rapports_etat())
}

/// Envoyer un rapport tout de suite, à la demande (bouton « Envoyer »).
#[tauri::command]
pub async fn rapports_envoyer(message: Option<String>) -> Result<(), String> {
    if !disponible() {
        return Err("Aucun point de collecte dans cette version : copiez les journaux.".into());
    }
    let detail = message.unwrap_or_else(|| "Envoyé à la demande de l'utilisateur.".into());
    envoyer(
        "Rapport envoyé par l'utilisateur",
        &crate::journaux::masquer(&detail),
    )
    .await
}
