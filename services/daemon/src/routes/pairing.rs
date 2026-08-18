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

    Json(serde_json::json!({
        "mode": mode,
        "url": url,
        "provisioning": charge,
        "qr_svg": svg,
    }))
    .into_response()
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
    use super::normaliser;

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
