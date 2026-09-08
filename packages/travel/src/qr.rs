//! Turning the pairing link into something a phone camera can read.
//!
//! Two renderings for two places: an SVG for the application window, and
//! half-block characters for a server that only has a terminal. Both encode
//! exactly the same string, so a phone cannot tell them apart.

use qrcode::{EcLevel, QrCode};

#[derive(Debug, thiserror::Error)]
#[error("Ce lien est trop long pour un QR code ({0} caractères).")]
pub struct QrError(usize);

/// Le même JSON, sans un seul octet au-delà de l'ASCII.
///
/// # Pourquoi c'est nécessaire
///
/// Un QR code n'indique pas de lui-même en quel jeu de caractères ses octets
/// sont écrits. La norme (ISO/IEC 18004) prévoit pour cela un « désignateur
/// ECI », que la bibliothèque d'encodage n'émet pas : en son absence, le
/// lecteur doit supposer **ISO/IEC 8859-1**. Nos octets sont de l'UTF-8, donc
/// tout caractère accentué est relu de travers.
///
/// Le résultat n'est pas seulement laid, il est trompeur. Un « é » s'écrit
/// `C3 A9` en UTF-8 ; relu en ISO-8859-1 il donne `Ã©`, et relu en CP1251 —
/// ce que font beaucoup de lecteurs de téléphone — il donne `Г©`, où `Г` est
/// une lettre cyrillique. Un nom de machine français apparaît alors sous le
/// code comme du russe, sur l'écran de quelqu'un qui n'a jamais rien écrit en
/// russe. C'est exactement ce qui a été signalé.
///
/// La charge du QR porte le nom de la machine, qui vient de `COMPUTERNAME` :
/// « PC-de-Téano » suffit à déclencher le phénomène.
///
/// # Pourquoi cet échappement-là
///
/// Plutôt que d'annoncer l'UTF-8 — ce que l'encodeur ne sait pas faire — on
/// retire le problème : un texte purement ASCII se lit identiquement dans
/// **tous** les jeux de caractères que le lecteur pourrait supposer. Les
/// caractères non-ASCII deviennent des séquences `\uXXXX`, que n'importe
/// quel analyseur JSON restitue à l'identique. La valeur reçue est donc la
/// même, quel que soit le lecteur.
///
/// Réservé à du JSON : la syntaxe JSON est elle-même en ASCII, donc un
/// caractère non-ASCII ne peut apparaître que dans une chaîne, où
/// l'échappement est licite. Un lien `locaryn://` n'en a pas besoin — il est
/// déjà en ASCII par construction, ses parties variables étant en base64.
#[must_use]
pub fn ascii_seul(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            // Hors du plan de base, JSON demande une paire de substitution :
            // `\u{XXXX}` sur six chiffres n'existe pas dans la grammaire, et
            // un analyseur strict la refuserait.
            let mut tampon = [0u16; 2];
            for unite in c.encode_utf16(&mut tampon) {
                out.push_str(&format!("\\u{unite:04x}"));
            }
        }
    }
    out
}

fn encode(data: &str) -> Result<QrCode, QrError> {
    // Medium correction: enough that a slightly out-of-focus photograph still
    // reads, without inflating the code until the modules are too small.
    QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M)
        .map_err(|_| QrError(data.len()))
}

/// The code as a square grid, `true` where a module is dark.
///
/// Exposed because a client that draws its own code — a phone, a future web
/// view — should not have to re-encode the link and risk producing a
/// different one.
pub fn matrix(data: &str) -> Result<(usize, Vec<bool>), QrError> {
    let code = encode(data)?;
    let colors = code.to_colors();
    let width = (colors.len() as f64).sqrt() as usize;
    Ok((
        width,
        colors.iter().map(|c| *c == qrcode::Color::Dark).collect(),
    ))
}

/// An SVG square, sized by CSS rather than by pixels.
///
/// Deliberately black on white whatever the interface theme: phone cameras
/// are trained on dark-on-light codes, and an inverted one is markedly harder
/// to read.
pub fn svg(data: &str) -> Result<String, QrError> {
    let code = encode(data)?;
    let colors = code.to_colors();
    let width = (colors.len() as f64).sqrt() as usize;
    // A quiet margin is part of the specification, not decoration: without it
    // many readers never find the code at all.
    const QUIET: usize = 4;
    let side = width + QUIET * 2;

    let mut dark = String::new();
    for (i, c) in colors.iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = (i % width + QUIET, i / width + QUIET);
            dark.push_str(&format!("M{x} {y}h1v1h-1z"));
        }
    }

    Ok(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {side} {side}" shape-rendering="crispEdges" role="img" aria-label="Code à scanner"><rect width="{side}" height="{side}" fill="#fff"/><path d="{dark}" fill="#000"/></svg>"##
    ))
}

/// Half-block characters, two rows of the code per line of text.
///
/// The obvious rendering — one space per module — comes out twice as tall as
/// it is wide in any terminal, and unreadable past a certain size.
pub fn terminal(data: &str) -> Result<String, QrError> {
    let code = encode(data)?;
    let colors = code.to_colors();
    let width = (colors.len() as f64).sqrt() as usize;
    let dark = |x: usize, y: usize| -> bool {
        y < width && x < width && colors[y * width + x] == qrcode::Color::Dark
    };

    // Four modules of margin, as the specification asks. Two is enough for a
    // tolerant reader and not for a strict one, and the cost here is four
    // characters of terminal width.
    const QUIET: usize = 4;
    let side = width + QUIET * 2;
    let mut out = String::new();
    // Light background, as printed codes are: terminals are dark far more
    // often than not, and an inverted code loses many readers.
    let mut y = 0;
    while y < side {
        for x in 0..side {
            let top = x >= QUIET && y >= QUIET && dark(x - QUIET, y - QUIET);
            let bottom = x >= QUIET && (y + 1) >= QUIET && dark(x - QUIET, y + 1 - QUIET);
            out.push(match (top, bottom) {
                (true, true) => ' ',
                (true, false) => '▄',
                (false, true) => '▀',
                (false, false) => '█',
            });
        }
        out.push('\n');
        y += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "locaryn://travel?v=1&m=travel&u=aHR0cHM6Ly9leGVtcGxlLnRyeWNsb3VkZmxhcmUuY29t&e=1800000600&k=AbCdEfGh&s=MEUCIQDx";

    #[test]
    fn the_svg_is_self_contained_and_square() {
        let s = svg(LINK).unwrap();
        assert!(s.starts_with("<svg"), "pas un SVG");
        assert!(
            !s.contains("http://") || s.contains("www.w3.org"),
            "référence externe"
        );
        // Same number twice in the viewBox: a rectangle would not scan.
        let vb = s
            .split("viewBox=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let n: Vec<&str> = vb.split(' ').collect();
        assert_eq!(n[2], n[3], "viewBox non carrée : {vb}");
        assert!(s.contains("fill=\"#000\""), "modules non dessinés");
    }

    #[test]
    fn the_quiet_margin_is_present() {
        // Without it many readers never lock onto the code.
        let s = svg(LINK).unwrap();
        let side: usize = s
            .split("viewBox=\"0 0 ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let code = encode(LINK).unwrap();
        let width = (code.to_colors().len() as f64).sqrt() as usize;
        assert_eq!(side, width + 8, "marge absente ou de mauvaise taille");
    }

    #[test]
    fn the_terminal_rendering_is_as_wide_as_it_is_tall() {
        // Two rows of modules per line of text; a code twice as tall as wide
        // is what one module per line produces, and it does not read.
        let t = terminal(LINK).unwrap();
        let lines: Vec<&str> = t.lines().collect();
        let cols = lines[0].chars().count();
        assert!(
            lines.len() * 2 >= cols - 1 && lines.len() * 2 <= cols + 1,
            "{}x{} : proportions fausses",
            cols,
            lines.len()
        );
        assert!(
            lines.iter().all(|l| l.chars().count() == cols),
            "lignes inégales"
        );
    }

    #[test]
    fn the_three_finder_squares_are_where_a_reader_looks_for_them() {
        // A code with its finder patterns missing or inverted is a picture,
        // not a QR code — and nothing else in these tests would notice.
        let (w, m) = matrix(LINK).unwrap();
        let at = |x: usize, y: usize| m[y * w + x];
        for (ox, oy) in [(0, 0), (w - 7, 0), (0, w - 7)] {
            // Outer ring dark, inner ring light, 3x3 core dark.
            for i in 0..7 {
                assert!(at(ox + i, oy), "bord haut du repère ({ox},{oy})");
                assert!(at(ox + i, oy + 6), "bord bas du repère ({ox},{oy})");
            }
            assert!(!at(ox + 1, oy + 1), "anneau clair absent ({ox},{oy})");
            assert!(at(ox + 3, oy + 3), "cœur du repère ({ox},{oy})");
        }
        // The fourth corner must *not* have one; that is how orientation works.
        assert!(
            !at(w - 1, w - 1) || !at(w - 4, w - 4),
            "quatrième repère : orientation perdue"
        );
    }

    #[test]
    fn a_link_that_cannot_fit_is_reported_rather_than_truncated() {
        // A truncated code scans into a wrong address, which is worse than
        // no code at all.
        let huge = "x".repeat(5000);
        let err = svg(&huge).unwrap_err();
        assert!(err.to_string().contains("trop long"));
    }
}

#[cfg(test)]
mod tests_ascii {
    use super::ascii_seul;

    /// De l'ASCII reste tel quel : rien à échapper, et surtout rien à
    /// réécrire au hasard.
    #[test]
    fn l_ascii_ne_bouge_pas() {
        let brut = r#"{"organisation":"PC-DE-TEANO","serverUrl":"https://192.168.1.20:7474"}"#;
        assert_eq!(ascii_seul(brut), brut);
    }

    /// Le cas signalé : un accent devient une séquence que tout analyseur
    /// JSON relit à l'identique, quel que soit le jeu de caractères que le
    /// lecteur de QR aura supposé.
    #[test]
    fn un_accent_devient_une_sequence_json() {
        let sortie = ascii_seul(r#"{"organisation":"PC-de-Téano"}"#);
        assert_eq!(sortie, r#"{"organisation":"PC-de-T\u00e9ano"}"#);
        assert!(sortie.is_ascii(), "plus un octet au-dela de l'ASCII");
        // Et la valeur survit à l'aller-retour : c'est tout ce qui compte.
        let relu: serde_json::Value = serde_json::from_str(&sortie).expect("JSON valide");
        assert_eq!(relu["organisation"], "PC-de-Téano");
    }

    /// Hors du plan de base, JSON exige une paire de substitution. Une
    /// séquence à six chiffres serait refusée par un analyseur strict.
    #[test]
    fn hors_du_plan_de_base_on_emet_une_paire() {
        let sortie = ascii_seul(r#"{"n":"🔒"}"#);
        assert_eq!(sortie, r#"{"n":"\ud83d\udd12"}"#);
        let relu: serde_json::Value = serde_json::from_str(&sortie).expect("JSON valide");
        assert_eq!(relu["n"], "🔒");
    }

    /// Une charge complète passe l'aller-retour sans rien perdre, y compris
    /// les caractères qui ne sont pas des lettres.
    #[test]
    fn une_charge_complete_survit_a_l_aller_retour() {
        let charge = serde_json::json!({
            "organisation": "Atelier Téano — mesures à 20 °C",
            "note": "Rendu final en A2, papier grain torchon",
            "serverUrl": "https://192.168.1.20:7474",
        });
        let brut = serde_json::to_string(&charge).expect("serialisation");
        let propre = ascii_seul(&brut);
        assert!(propre.is_ascii());
        let relu: serde_json::Value = serde_json::from_str(&propre).expect("JSON valide");
        assert_eq!(relu, charge);
    }
}
