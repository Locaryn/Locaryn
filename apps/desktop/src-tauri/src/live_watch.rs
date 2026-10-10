//! Suivre sur l'ordinateur une réponse lancée depuis le téléphone.
//!
//! Le téléphone parle au service (`locaryn-daemon`), pas à cette fenêtre : sa
//! réponse ne passait donc jamais par ici, et ne s'affichait qu'une fois
//! enregistrée, après avoir quitté puis rouvert la conversation. Le service
//! diffuse maintenant la réponse en cours (`/v1/sessions/:id/live`) ; cette
//! commande la relaie à l'écran, événement par événement, comme une réponse
//! produite ici.

use futures::StreamExt;

/// Une réponse peut durer : la connexion reste ouverte tant qu'elle tourne.
const DUREE_MAX: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// `true` si une réponse tournait et a été suivie jusqu'au bout ; `false` si
/// aucune ne tourne (ou si le service n'est pas lancé — ce qui revient au même
/// pour l'écran : rien à montrer).
#[tauri::command]
pub async fn watch_live_session(
    session_id: String,
    on_event: tauri::ipc::Channel<serde_json::Value>,
) -> Result<bool, String> {
    let port = locaryn_config::load(None)
        .map(|c| c.daemon.port)
        .map_err(|e| e.to_string())?;
    let client = crate::secure_client::build(None, None, None, DUREE_MAX)?;
    let reponse = match client
        .get(format!(
            "https://127.0.0.1:{port}/v1/sessions/{session_id}/live"
        ))
        .send()
        .await
    {
        Ok(r) => r,
        // Service éteint : le téléphone ne peut rien avoir lancé.
        Err(_) => return Ok(false),
    };
    if reponse.status() != reqwest::StatusCode::OK {
        return Ok(false);
    }
    let mut flux = reponse.bytes_stream();
    // Des octets, pas du texte : un caractère accentué peut tomber à cheval sur
    // deux morceaux, et le décoder morceau par morceau le cassait.
    let mut tampon: Vec<u8> = Vec::new();
    while let Some(morceau) = flux.next().await {
        let morceau = match morceau {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "suivi en direct interrompu");
                break;
            }
        };
        tampon.extend_from_slice(&morceau);
        // Un événement SSE se termine par une ligne vide.
        while let Some(fin) = tampon.windows(2).position(|w| w == b"\n\n") {
            let bloc: Vec<u8> = tampon.drain(..fin + 2).collect();
            let Some(donnees) = donnees_sse(&String::from_utf8_lossy(&bloc)) else {
                continue;
            };
            match serde_json::from_str::<serde_json::Value>(&donnees) {
                Ok(ev) => {
                    if on_event.send(ev).is_err() {
                        // L'écran a fermé la conversation : on lâche.
                        return Ok(true);
                    }
                }
                Err(e) => tracing::warn!(error = %e, "événement en direct illisible"),
            }
        }
    }
    Ok(true)
}

/// Les lignes `data:` d'un bloc SSE, recollées (une donnée peut tenir sur
/// plusieurs lignes).
fn donnees_sse(bloc: &str) -> Option<String> {
    let lignes: Vec<&str> = bloc
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|l| l.strip_prefix(' ').unwrap_or(l))
        .collect();
    (!lignes.is_empty()).then(|| lignes.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_donnees_d_un_bloc_sse_se_recollent() {
        assert_eq!(
            donnees_sse("data: {\"type\":\"token\"}\n\n").as_deref(),
            Some("{\"type\":\"token\"}")
        );
        assert_eq!(donnees_sse(": keep-alive\n\n"), None);
        assert_eq!(donnees_sse("data: a\ndata: b\n\n").as_deref(), Some("a\nb"));
    }
}
