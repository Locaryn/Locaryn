//! Les modèles de conversation du socle (moteur llama.cpp).
//!
//! Un seul fichier fait foi — `../chat-models.json` : le bureau l'importe, le
//! service l'embarque ici et le sert au téléphone et au web. Les modèles
//! spécialisés (image, voix, vidéo…) n'y figurent pas : chaque morph apporte
//! les siens dans son `dist/marketplace.json`.

/// Le contenu brut, embarqué à la compilation.
pub const CHAT_MODELS_JSON: &str = include_str!("../chat-models.json");

/// Les familles du socle, prêtes à servir (forme JSON, celle des clients).
pub fn familles() -> Vec<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(CHAT_MODELS_JSON)
        .expect("packages/shared-types/chat-models.json doit être un JSON valide");
    let mut tout = Vec::new();
    for cle in ["families", "largeLocal"] {
        if let Some(liste) = v.get(cle).and_then(|l| l.as_array()) {
            tout.extend(liste.iter().cloned());
        }
    }
    tout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_socle_se_lit_et_ne_porte_que_des_modeles_de_conversation() {
        let f = familles();
        assert!(f.len() >= 10, "le socle propose des modèles");
        for famille in &f {
            assert!(famille["id"].is_string() && famille["variants"].is_array());
            for drapeau in ["videoGen", "model3d", "musicGen", "tts", "imageGen"] {
                assert!(
                    famille.get(drapeau).is_none(),
                    "{} : un modèle spécialisé appartient à son morph",
                    famille["id"]
                );
            }
        }
    }
}
