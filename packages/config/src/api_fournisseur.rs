//! L'API fournisseur : ce que la machine sert aux autres applications.
//!
//! Le mode serveur ouvre Locaryn à ses propres clients (téléphone, client
//! web). L'API fournisseur ouvre ses modèles aux applications qui parlent
//! OpenAI ou Anthropic — Cursor, VS Code, Claude Code, OpenCode, un script —
//! avec une clé d'API créée par un administrateur. Éteinte par défaut.

use serde_json::json;

const BRANCHE: &str = "provider_api";

/// L'API fournisseur est-elle allumée ?
pub fn active() -> bool {
    crate::global_value(BRANCHE)
        .and_then(|v| v.get("enabled").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

/// L'allumer ou l'éteindre. Le service relit le réglage à chaque requête :
/// rien à redémarrer.
pub fn regler(actif: bool) -> Result<(), crate::ConfigError> {
    crate::set_global(BRANCHE, json!({ "enabled": actif }))
}

/// Le port en HTTP simple, réservé à cette machine (`127.0.0.1`), à côté du
/// port HTTPS du mode serveur : un éditeur local n'a pas à accepter le
/// certificat auto-signé du serveur.
pub fn port_local(port_serveur: u16) -> u16 {
    port_serveur.saturating_add(1)
}

/// Les routes que sert une clé d'API, et rien d'autre : une clé confiée à un
/// éditeur ne lit ni les conversations, ni les fichiers, ni les comptes.
pub fn est_route_api(chemin: &str) -> bool {
    matches!(
        chemin,
        "/v1/models" | "/v1/chat/completions" | "/v1/messages" | "/v1/provider-api"
    )
}
