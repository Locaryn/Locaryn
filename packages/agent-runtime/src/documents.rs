//! Le texte d'un document joint à un message.
//!
//! Un PDF lu comme du texte ne donne que du charabia — et beaucoup : un
//! bulletin scolaire de quelques pages faisait 32 000 jetons de bruit pour une
//! fenêtre de 8 192. On en tire le texte, comme le ferait un lecteur, et c'est
//! ce texte qui rejoint le message. Partagé par le bureau et le démon (que le
//! téléphone appelle avec les octets du fichier).

/// Ce qu'un document joint garde au plus, en caractères. Au-delà, il étoufferait
/// la fenêtre de n'importe quel modèle local ; la coupure est annoncée.
pub const TEXTE_MAX: usize = 60_000;

/// Le texte tiré d'un document, prêt à joindre.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TexteExtrait {
    pub texte: String,
    /// Le document dépassait `TEXTE_MAX` : seul son début est joint.
    pub tronque: bool,
}

/// Le texte d'un fichier, d'après ses octets et son nom.
///
/// PDF : le texte de ses pages. Tout ce qui se lit en UTF-8 : tel quel.
/// Le reste (image dans un PDF scanné, archive, binaire) : une erreur qui dit
/// pourquoi, plutôt que des octets que le modèle prendrait pour des mots.
pub fn extraire(nom: &str, octets: &[u8]) -> Result<TexteExtrait, String> {
    let brut = if est_pdf(nom, octets) {
        // L'extracteur panique sur certains PDF mal formés : une panique ici
        // ferait tomber la requête entière, pas seulement la pièce jointe.
        let lu = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(octets))
            .map_err(|_| format!("« {nom} » : PDF illisible (structure non reconnue)."))?;
        let texte = lu.map_err(|e| format!("« {nom} » : PDF illisible ({e})."))?;
        let texte = compacter(&texte);
        if texte.trim().is_empty() {
            return Err(format!(
                "« {nom} » ne contient pas de texte lisible — c'est sans doute un document \
                 scanné. Envoyez-le en photo à un modèle qui sait lire les images."
            ));
        }
        texte
    } else {
        match std::str::from_utf8(octets) {
            Ok(t) if !t.contains('\0') => t.to_string(),
            _ => {
                return Err(format!(
                    "« {nom} » n'est ni un texte ni un PDF : le modèle ne pourrait rien en lire."
                ))
            }
        }
    };
    Ok(tronquer(brut))
}

fn est_pdf(nom: &str, octets: &[u8]) -> bool {
    octets.starts_with(b"%PDF") || nom.to_lowercase().ends_with(".pdf")
}

/// Les extracteurs de PDF laissent des lignes vides par paquets et des espaces
/// de mise en page : autant de jetons pour rien.
fn compacter(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut vides = 0;
    for ligne in texte.lines() {
        let ligne = ligne.split_whitespace().collect::<Vec<_>>().join(" ");
        if ligne.is_empty() {
            vides += 1;
            if vides > 1 {
                continue;
            }
        } else {
            vides = 0;
        }
        sortie.push_str(&ligne);
        sortie.push('\n');
    }
    sortie.trim().to_string()
}

fn tronquer(texte: String) -> TexteExtrait {
    if texte.chars().count() <= TEXTE_MAX {
        return TexteExtrait {
            texte,
            tronque: false,
        };
    }
    let debut: String = texte.chars().take(TEXTE_MAX).collect();
    TexteExtrait {
        texte: format!(
            "{debut}\n\n[Document tronqué : seuls les {TEXTE_MAX} premiers caractères ont été joints.]"
        ),
        tronque: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_texte_passe_tel_quel_et_un_binaire_est_refuse() {
        let t = extraire("notes.md", "# Titre\nligne".as_bytes()).unwrap();
        assert_eq!(t.texte, "# Titre\nligne");
        assert!(!t.tronque);
        assert!(extraire("photo.bin", &[0u8, 159, 146, 150]).is_err());
    }

    #[test]
    fn un_long_texte_est_tronque_et_le_dit() {
        let long = "a".repeat(TEXTE_MAX + 10);
        let t = extraire("long.txt", long.as_bytes()).unwrap();
        assert!(t.tronque);
        assert!(t.texte.ends_with("ont été joints.]"));
    }

    #[test]
    fn les_blancs_de_mise_en_page_disparaissent() {
        assert_eq!(compacter("a   b\n\n\n\nc  \n"), "a b\n\nc");
    }

    #[test]
    fn le_texte_d_un_pdf_est_extrait() {
        let t = extraire("bulletin.pdf", include_bytes!("../testdata/mini.pdf")).unwrap();
        assert!(t.texte.contains("Moyenne generale 14,5"), "{}", t.texte);
    }

    #[test]
    fn un_pdf_casse_se_dit_en_clair() {
        let e = extraire("bulletin.pdf", b"%PDF-1.4 tronque").unwrap_err();
        assert!(e.contains("bulletin.pdf"));
    }
}
