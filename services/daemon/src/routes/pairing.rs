//! `/v1/pairing` — le code à photographier pour qu'un téléphone connaisse ce
//! serveur.
//!
//! Trois façons de joindre une machine, donc trois codes. Ils ne diffèrent que
//! par l'adresse qu'ils portent, et ce qu'ils portent d'autre est identique :
//! l'autorité du déploiement. C'est elle qui permet ensuite au téléphone de
//! vérifier un certificat renouvelé et de reconnaître un lien de mode Remote —
//! une adresse tapée à la main ne l'apporte pas, et l'interface le dit.

use crate::DaemonState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

/// Un code d'appairage en attente de consommation.
///
/// Le QR porte l'adresse ; le code porte la preuve que celui qui scanne a
/// l'écran de l'hôte sous les yeux. Deux minutes, un seul essai : c'est le
/// second facteur de l'appairage.
pub struct PendingPairing {
    pub code: String,
    pub created_at: std::time::Instant,
    pub attempts: u32,
}

/// TTL et plafond d'essais. Six chiffres, deux minutes, cinq essais : le
/// brute-force perd avant d'avoir vu la moitié de l'espace.
pub const PAIRING_TTL: std::time::Duration = std::time::Duration::from_secs(120);
pub const PAIRING_MAX_ATTEMPTS: u32 = 5;

/// Génère un code à 6 chiffres uniformément (000000–999999).
///
/// Le code vient du CSPRNG de l'OS (le même que les tokens) : un code
/// prévisible annulerait le second facteur.
fn generer_code() -> String {
    use rand::RngCore;
    // Réjection : 10^6 ne divise pas 2^32, on tire jusqu'à tomber dans
    // [0, 1_000_000) pour éviter le biais modulo.
    const LIMITE: u32 = 1_000_000;
    const ZONE: u32 = u32::MAX - (u32::MAX % LIMITE);
    loop {
        let mut b = [0u8; 4];
        rand::rngs::OsRng.fill_bytes(&mut b);
        let n = u32::from_le_bytes(b);
        if n < ZONE {
            return format!("{:06}", n % LIMITE);
        }
    }
}

#[derive(serde::Deserialize)]
pub struct QrQuery {
    /// `local` (défaut), `public` ou `tunnel`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Pour `public` : l'adresse par laquelle on joint la machine de dehors.
    #[serde(default)]
    pub url: Option<String>,
}

/// GET /v1/pairing — l'adresse, la configuration, et le code qui la porte.
pub async fn qr(State(s): State<Arc<DaemonState>>, Query(q): Query<QrQuery>) -> Response {
    let mode = q.mode.as_deref().unwrap_or("local");

    let url = match mode {
        "local" => {
            // Le service n'écoute sur le réseau que si le mode serveur est
            // actif. Produire un code portant l'adresse locale sans cela
            // donnerait un carré parfaitement valide menant à une adresse qui
            // ne répond à personne — un échec que le téléphone constaterait
            // sans pouvoir l'expliquer.
            if !s.auth_required {
                return erreur(
                    StatusCode::CONFLICT,
                    "Cette machine n'écoute que sur elle-même. Activez le mode serveur \
                     dans les réglages pour qu'un téléphone du réseau local puisse la joindre."
                        .into(),
                );
            }
            s.local_url.clone()
        }
        "tunnel" => {
            let Some(u) = s.travel.tunnel_url().await.filter(|u| !u.is_empty()) else {
                return erreur(
                    StatusCode::CONFLICT,
                    "Le mode Remote n'est pas actif : il n'y a pas encore d'adresse extérieure \
                     à mettre dans un code."
                        .into(),
                );
            };
            u
        }
        "public" => {
            let Some(u) = q.url.filter(|u| !u.trim().is_empty()) else {
                return erreur(
                    StatusCode::BAD_REQUEST,
                    "Indiquez l'adresse publique par laquelle on joint cette machine.".into(),
                );
            };
            normaliser(&u)
        }
        autre => {
            return erreur(
                StatusCode::BAD_REQUEST,
                format!("Mode inconnu : « {autre} » (local, public ou tunnel)."),
            );
        }
    };

    let ca = match locaryn_config::mtls::authority(&s.data_dir) {
        Ok(ca) => ca,
        Err(e) => {
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("autorité locale illisible : {e}"),
            );
        }
    };

    // Le mode Remote avec le code : sans lui, le téléphone ne sait pas s'il
    // reçoit une adresse de réseau local, un port ouvert, ou un tunnel dont
    // l'adresse expirera. Les trois se comportent différemment, et la
    // différence se voit le jour où ça ne marche plus.
    //
    // Construit le vrai type plutôt qu'un objet écrit à la main : celui-ci
    // porte `#[serde(rename_all = "camelCase")]`, et un `json!({"server_url":
    // ...})` composé indépendamment produisait des clés que le téléphone ne
    // reconnaissait pas — chaque code scanné échouait au décodage, en silence
    // jusqu'à ce qu'une personne essaie vraiment de s'appairer.
    let provisioning = locaryn_config::provision::Provisioning {
        server_url: url.clone(),
        organisation: nom_du_serveur(),
        certificate_fingerprint: None,
        authority_pem: Some(ca.cert_pem),
        access_mode: Some(mode.to_string()),
        note: String::new(),
    };
    let charge = match serde_json::to_string(&provisioning) {
        Ok(c) => c,
        Err(e) => {
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("configuration illisible ({e})"),
            );
        }
    };

    let svg = match locaryn_travel::qr::svg(&charge) {
        Ok(svg) => svg,
        Err(e) => {
            // Un PEM d'autorité tient dans un code, mais pas dans n'importe
            // lequel : le dire est plus utile qu'un carré vide.
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("code impossible à produire ({e}) — l'autorité est trop longue."),
            );
        }
    };

    // Le code à usage unique : généré à chaque affichage du QR, consommé au
    // premier confirm valide. Tant qu'il n'est pas confirmé, il ne sert à
    // personne — c'est pourquoi il n'est pas dans le QR lui-même.
    let code = generer_code();
    {
        let mut pending = s.pairing_pending
            .lock()
            .expect("verrou pairing");
        *pending = Some(PendingPairing {
            code: code.clone(),
            created_at: std::time::Instant::now(),
            attempts: 0,
        });
    }

    Json(serde_json::json!({
        "mode": mode,
        "url": url,
        "provisioning": charge,
        "qr_svg": svg,
        "pairing_code": code,
        "pairing_ttl_seconds": PAIRING_TTL.as_secs(),
    }))
    .into_response()
}

/// POST /v1/auth/pair/confirm — le client qui a scanné le QR renvoie le code
/// affiché à l'écran. Valide une fois, dans les deux minutes : le serveur
/// délivre alors un token de session dédié à l'appareil.
pub async fn confirm(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<PairConfirmBody>,
) -> Response {
    let code_saisi = body.pairing_code.trim();
    if code_saisi.len() != 6 || !code_saisi.chars().all(|c| c.is_ascii_digit()) {
        return erreur(
            StatusCode::BAD_REQUEST,
            "Le code d'appairage attendu compte six chiffres.".into(),
        );
    }

    let Some(admin_id) = s.pairing_admin_user_id else {
        return erreur(
            StatusCode::CONFLICT,
            "Aucun compte administrateur sur ce serveur : l'appairage par code              exige un compte à appairer."
                .into(),
        );
    };

    let verdict = {
        let mut pending = s.pairing_pending.lock().expect("verrou pairing");
        match pending.as_mut() {
            None => Err("Aucun code d'appairage en attente. Affichez le QR sur                          l'hôte, puis réessayez."
                .to_string()),
            Some(p) if p.created_at.elapsed() > PAIRING_TTL => {
                *pending = None;
                Err("Code d'appairage expiré (2 minutes). Affichez un nouveau QR.".into())
            }
            Some(p) if p.attempts >= PAIRING_MAX_ATTEMPTS => {
                *pending = None;
                Err("Trop d'essais. Un nouveau QR génère un nouveau code.".into())
            }
            Some(p) => {
                if constant_time_eq(p.code.as_bytes(), code_saisi.as_bytes()) {
                    // Consommé à la première réussite : jamais rejouable.
                    *pending = None;
                    Ok(())
                } else {
                    p.attempts += 1;
                    Err(format!(
                        "Code incorrect ({} essai{} restant{}).",
                        PAIRING_MAX_ATTEMPTS - p.attempts,
                        if PAIRING_MAX_ATTEMPTS - p.attempts > 1 { "s" } else { "" },
                        if PAIRING_MAX_ATTEMPTS - p.attempts > 1 { "s" } else { "" },
                    ))
                }
            }
        }
    };

    if let Err(message) = verdict {
        return erreur(StatusCode::UNAUTHORIZED, message);
    }

    // Un appareil appairé est une session de longue durée : 180 jours,
    // renouvelable en re-scannant. Le kind reste 'session' — c'est un
    // appareil, pas une clé développeur.
    match s
        .users
        .issue_token(admin_id, Some(&label_appareil(&body.device_label)), 180)
        .await
    {
        Ok(tok) => {
            tracing::info!("appairage confirmé, token de session appareil émis");
            Json(serde_json::json!({
                "token": tok.plaintext,
                "expires_at": tok.expires_at,
                "device_label": label_appareil(&body.device_label),
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "émission du token d'appairage impossible");
            erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("token d'appairage impossible ({e})"),
            )
        }
    }
}

/// Comparaison à temps constant : la longueur est publique (6 chiffres),
/// mais prendre l'habitude ne coûte rien.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Une étiquette d'appareil lisible : tronquée, vide par défaut.
fn label_appareil(brut: &Option<String>) -> String {
    let l = brut
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Appareil appairé");
    l.chars().take(40).collect()
}

#[derive(serde::Deserialize)]
pub struct PairConfirmBody {
    pub pairing_code: String,
    #[serde(default)]
    pub device_label: Option<String>,
}

/// Le nom affiché sur le téléphone au moment de se connecter.
fn nom_du_serveur() -> String {
    hostname().unwrap_or_else(|| "Locaryn".to_string())
}

fn hostname() -> Option<String> {
    // Rien de critique n'en dépend : c'est une étiquette. Les variables
    // couvrent Windows et les systèmes Unix sans dépendance de plus.
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|h| !h.trim().is_empty())
}

/// Une adresse publique tapée par un humain, ramenée à une URL.
fn normaliser(brut: &str) -> String {
    let brut = brut.trim().trim_end_matches('/');
    if brut.starts_with("http://") || brut.starts_with("https://") {
        return brut.to_string();
    }
    // Hors du réseau local, le chiffrement n'est pas optionnel.
    format!("https://{brut}")
}

fn erreur(code: StatusCode, message: String) -> Response {
    (
        code,
        Json(serde_json::json!({ "error": { "code": "pairing", "message": message } })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{constant_time_eq, normaliser};

    #[test]
    fn la_comparaison_a_temps_constant_est_exacte() {
        assert!(constant_time_eq(b"123456", b"123456"));
        assert!(!constant_time_eq(b"123456", b"123457"));
        assert!(!constant_time_eq(b"123456", b"12345"));
        assert!(!constant_time_eq(b"", b"123456"));
    }

    #[test]
    fn une_adresse_publique_passe_en_https() {
        assert_eq!(
            normaliser("maison.exemple:7474"),
            "https://maison.exemple:7474"
        );
        assert_eq!(normaliser(" 88.120.4.3:7474/ "), "https://88.120.4.3:7474");
    }

    #[test]
    fn un_schema_deja_ecrit_est_respecte() {
        assert_eq!(
            normaliser("http://192.168.1.20:7474"),
            "http://192.168.1.20:7474"
        );
        assert_eq!(normaliser("https://a.b:1/"), "https://a.b:1");
    }
}
