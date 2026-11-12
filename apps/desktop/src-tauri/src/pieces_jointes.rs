//! Ce qu'un message garde de ses pièces jointes.
//!
//! Le message enregistré était le seul texte tapé : en rouvrant l'application,
//! le fichier joint avait disparu de la bulle, et l'image aussi. Les noms des
//! documents suivent maintenant le message dans un marqueur que l'interface
//! relit, et les images sont posées sur le disque à côté des données.
//!
//! Ces marqueurs ne partent jamais au modèle : `compaction::sans_marqueurs`
//! les retire de l'historique.

use crate::JointDocumentIn;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Le dossier des images jointes d'une conversation.
pub fn dossier(data_dir: &Path, session: Uuid) -> PathBuf {
    data_dir.join("joints").join(session.to_string())
}

/// La conversation est supprimée : ses images jointes partent avec elle.
pub fn oublier(data_dir: &Path, session: Uuid) {
    let dir = dossier(data_dir, session);
    if !dir.exists() {
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        tracing::warn!(dossier = %dir.display(), erreur = %e, "images jointes non supprimées");
    }
}

/// La nature d'un document, d'après son nom : ce que la bulle affiche.
fn nature(nom: &str) -> &'static str {
    let ext = Path::new(nom)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "opus" | "aac" => "audio",
        "mp4" | "webm" | "mov" | "mkv" | "avi" => "video",
        _ => "text",
    }
}

/// L'extension d'une image, lue dans ses premiers octets.
fn extension_image(octets: &[u8]) -> &'static str {
    match octets {
        [0x89, b'P', b'N', b'G', ..] => "png",
        [0xFF, 0xD8, ..] => "jpg",
        [b'G', b'I', b'F', ..] => "gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "webp",
        _ => "png",
    }
}

/// Poser une image jointe sur le disque. `None` si elle est illisible ou si
/// l'écriture échoue : le message part quand même, sans sa vignette.
fn enregistrer_image(dir: &Path, base64: &str) -> Option<PathBuf> {
    use base64::Engine as _;
    let octets = match base64::engine::general_purpose::STANDARD.decode(base64.trim()) {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(erreur = %e, "image jointe illisible : non gardée avec le message");
            return None;
        }
    };
    let chemin = dir.join(format!("{}.{}", Uuid::new_v4(), extension_image(&octets)));
    let ecrit = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&chemin, &octets));
    match ecrit {
        Ok(()) => Some(chemin),
        Err(e) => {
            tracing::warn!(chemin = %chemin.display(), erreur = %e, "image jointe non enregistrée");
            None
        }
    }
}

/// Le message tel qu'il est enregistré : le texte tapé, suivi des marqueurs de
/// ses pièces jointes.
pub fn message_enregistre(
    data_dir: &Path,
    session: Uuid,
    texte: &str,
    documents: &[JointDocumentIn],
    images: &[String],
) -> String {
    let mut sortie = texte.to_string();
    if !documents.is_empty() {
        let joints: Vec<serde_json::Value> = documents
            .iter()
            .map(|d| serde_json::json!({ "kind": nature(&d.name), "name": d.name }))
            .collect();
        sortie.push_str(&format!(
            "\n<!--locaryn-joints:{}-->",
            serde_json::Value::Array(joints)
        ));
    }
    let dir = dossier(data_dir, session);
    for image in images {
        if let Some(chemin) = enregistrer_image(&dir, image) {
            sortie.push('\n');
            sortie.push_str(&crate::image_marker(&chemin.to_string_lossy()));
        }
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un dossier d'essai à soi, retiré à la fin.
    struct Essai(PathBuf);

    impl Essai {
        fn nouveau() -> Self {
            Self(std::env::temp_dir().join(format!("locaryn-joints-{}", Uuid::new_v4())))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Essai {
        fn drop(&mut self) {
            // Rien n'a été écrit dans certains essais : le dossier peut manquer.
            if self.0.exists() {
                std::fs::remove_dir_all(&self.0).expect("dossier d'essai retiré");
            }
        }
    }

    fn document(nom: &str) -> JointDocumentIn {
        JointDocumentIn {
            name: nom.into(),
            text: String::new(),
            base64: None,
        }
    }

    #[test]
    fn un_message_sans_piece_reste_tel_quel() {
        let dir = Essai::nouveau();
        let m = message_enregistre(dir.path(), Uuid::new_v4(), "bonjour", &[], &[]);
        assert_eq!(m, "bonjour");
    }

    #[test]
    fn les_documents_et_les_images_suivent_le_message() {
        use base64::Engine as _;
        let dir = Essai::nouveau();
        let session = Uuid::new_v4();
        let png = base64::engine::general_purpose::STANDARD.encode([0x89, b'P', b'N', b'G', 1, 2]);
        let m = message_enregistre(
            dir.path(),
            session,
            "regarde",
            &[document("TP_Module1.pdf"), document("note.mp3")],
            &[png],
        );
        assert!(m.starts_with("regarde\n<!--locaryn-joints:"));
        assert!(m.contains(r#"{"kind":"text","name":"TP_Module1.pdf"}"#));
        assert!(m.contains(r#"{"kind":"audio","name":"note.mp3"}"#));
        assert!(m.contains("<!--locaryn-image:"));
        let poses: Vec<_> = std::fs::read_dir(dossier(dir.path(), session))
            .expect("dossier des images")
            .collect();
        assert_eq!(poses.len(), 1);
        // Le modèle ne relit ni le marqueur ni le chemin.
        assert_eq!(
            locaryn_agent_runtime::compaction::sans_marqueurs(&m),
            "regarde"
        );
        oublier(dir.path(), session);
        assert!(!dossier(dir.path(), session).exists());
    }

    #[test]
    fn une_image_illisible_ne_bloque_pas_le_message() {
        let dir = Essai::nouveau();
        let m = message_enregistre(
            dir.path(),
            Uuid::new_v4(),
            "x",
            &[],
            &["pas du base64 !".into()],
        );
        assert_eq!(m, "x");
    }
}
