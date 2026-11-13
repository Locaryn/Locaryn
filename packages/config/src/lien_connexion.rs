//! Le QR d'appairage et de connexion, sous forme de lien `locaryn://connect`.
//!
//! # Pourquoi un lien plutôt que le JSON
//!
//! Un QR qui contient du JSON brut n'est lu que par le scanner de l'app :
//! Google Lens ou l'appareil photo n'y voient que du texte. Un lien d'un
//! schéma déclaré par l'app (intent-filter Android `locaryn://connect`) leur
//! donne quelque chose à ouvrir, et Android le remet à Locaryn.
//!
//! # Pourquoi cette forme-là
//!
//! Le QR porte déjà l'autorité du déploiement, et il est dense. Mesuré avec une
//! autorité réelle (ECDSA P-256) :
//!
//! | contenu                             | octets | version du QR (niveau M) |
//! |-------------------------------------|-------:|-------------------------:|
//! | JSON brut (avant)                   |    822 | 23 (109 modules)         |
//! | `?d=` + JSON en base64url           |   1116 | 27 (125 modules)         |
//! | ce format : champs courts, DER      |    651 | 20 (97 modules)          |
//!
//! Le JSON emballe le PEM, lui-même du base64 coupé de sauts de ligne
//! échappés ; repasser le tout en base64 l'aurait gonflé d'un tiers. Ici
//! l'autorité voyage en DER base64url, sans en-tête ni saut de ligne, et les
//! autres champs gardent des noms d'une lettre : le lien est plus court que
//! le JSON qu'il remplace.
//!
//! # Ce qu'il ne porte jamais
//!
//! Aucun code de connexion : `l=1` dit seulement « ouvre l'écran du code ».
//! Le code se lit sur l'ordinateur et se tape : c'est le second facteur, et
//! une photo du QR ne doit pas suffire à ouvrir un compte.
//!
//! `v=1` distingue ce lien de `locaryn://connect?server=…` que le bureau
//! reçoit d'un autre PC : même action, autres paramètres.

use crate::provision::{base64_decode, Provisioning};

/// Début de tout lien produit ici.
pub const PREFIXE: &str = "locaryn://connect?";

/// Version du format. Un téléphone plus ancien refuse proprement une autre.
const VERSION: &str = "1";

/// Le lien qui porte cette configuration.
///
/// Échoue si l'autorité n'est pas un PEM de certificat lisible : un lien sans
/// autorité mènerait à un écran de confirmation que l'enregistrement
/// refuserait ensuite.
pub fn vers_lien(p: &Provisioning) -> Result<String, String> {
    let mut parts: Vec<(&str, String)> = vec![("v", VERSION.to_string())];
    parts.push(("u", pourcent_encoder(&p.server_url)));
    if !p.organisation.is_empty() {
        parts.push(("n", pourcent_encoder(&p.organisation)));
    }
    if let Some(m) = p.access_mode.as_deref().filter(|m| !m.is_empty()) {
        parts.push(("m", pourcent_encoder(m)));
    }
    if p.login {
        parts.push(("l", "1".to_string()));
    }
    if let Some(lu) = p.local_url.as_deref().filter(|u| !u.is_empty()) {
        parts.push(("lu", pourcent_encoder(lu)));
    }
    if let Some(fp) = p
        .certificate_fingerprint
        .as_deref()
        .filter(|f| !f.is_empty())
    {
        parts.push(("fp", pourcent_encoder(fp)));
    }
    if !p.note.is_empty() {
        parts.push(("note", pourcent_encoder(&p.note)));
    }
    if let Some(pem) = p.authority_pem.as_deref().filter(|a| !a.trim().is_empty()) {
        let der = crate::mtls::pem_blocks(pem, "CERTIFICATE")
            .into_iter()
            .next()
            .ok_or_else(|| "autorité illisible : aucun certificat dans le PEM".to_string())?;
        parts.push(("a", base64url(&der)));
    }
    let requete = parts
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    Ok(format!("{PREFIXE}{requete}"))
}

/// Vrai pour un lien de ce format (`v=…` présent), quelle que soit sa version.
pub fn est_lien(texte: &str) -> bool {
    texte
        .trim()
        .strip_prefix(PREFIXE)
        .is_some_and(|q| q.split('&').any(|p| p.starts_with("v=")))
}

/// La configuration qu'un lien porte.
///
/// Les messages d'erreur s'affichent tels quels sur le téléphone.
pub fn depuis_lien(texte: &str) -> Result<Provisioning, String> {
    let requete = texte
        .trim()
        .strip_prefix(PREFIXE)
        .ok_or_else(|| "Ce lien n'est pas un code de connexion Locaryn.".to_string())?;
    // Un fragment éventuel (`#…`) n'a rien à faire ici ; on l'ignore.
    let requete = requete.split('#').next().unwrap_or_default();

    let mut p = Provisioning {
        server_url: String::new(),
        organisation: String::new(),
        certificate_fingerprint: None,
        authority_pem: None,
        access_mode: None,
        note: String::new(),
        login_code: None,
        login: false,
        local_url: None,
    };
    let mut version = None;
    for paire in requete.split('&').filter(|s| !s.is_empty()) {
        let (cle, valeur) = paire.split_once('=').unwrap_or((paire, ""));
        lire_champ(&mut p, &mut version, cle, valeur)?;
    }

    match version.as_deref() {
        Some(VERSION) => {}
        Some(autre) => {
            return Err(format!(
                "Ce code vient d'une version plus récente de Locaryn (format {autre}). \
                 Mettez l'application à jour."
            ))
        }
        None => return Err("Ce lien n'est pas un code de connexion Locaryn.".into()),
    }
    if p.server_url.trim().is_empty() {
        return Err("Ce code ne contient pas l'adresse du serveur.".into());
    }
    Ok(p)
}

/// Un champ du lien, rangé à sa place. Les clés inconnues sont ignorées : une
/// version future peut en ajouter sans rendre ses codes illisibles ici.
fn lire_champ(
    p: &mut Provisioning,
    version: &mut Option<String>,
    cle: &str,
    valeur: &str,
) -> Result<(), String> {
    match cle {
        "v" => *version = Some(pourcent_decoder(valeur)?),
        "u" => p.server_url = pourcent_decoder(valeur)?,
        "n" => p.organisation = pourcent_decoder(valeur)?,
        "m" => p.access_mode = Some(pourcent_decoder(valeur)?),
        "l" => p.login = valeur == "1",
        "lu" => p.local_url = Some(pourcent_decoder(valeur)?),
        "fp" => p.certificate_fingerprint = Some(pourcent_decoder(valeur)?),
        "note" => p.note = pourcent_decoder(valeur)?,
        "a" => {
            let der = base64_decode(valeur)
                .filter(|d| !d.is_empty())
                .ok_or_else(|| "L'autorité contenue dans ce code est illisible.".to_string())?;
            p.authority_pem = Some(der_vers_pem(&der));
        }
        _ => {}
    }
    Ok(())
}

/// Le certificat en PEM, lignes de 64 caractères comme partout ailleurs.
fn der_vers_pem(der: &[u8]) -> String {
    let b64 = locaryn_shared_types::base64_encode(der);
    let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
    for ligne in b64.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(ligne));
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    pem
}

/// Base64 alphabet URL, sans remplissage : passe tel quel dans une requête.
fn base64url(octets: &[u8]) -> String {
    locaryn_shared_types::base64_encode(octets)
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            autre => autre,
        })
        .collect()
}

/// Encodage pourcent des octets UTF-8 : le lien reste en ASCII, donc lu
/// identiquement par tous les scanners (voir `locaryn_travel::qr::ascii_seul`).
fn pourcent_encoder(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn pourcent_decoder(s: &str) -> Result<String, String> {
    let octets = s.as_bytes();
    let mut out = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        match octets[i] {
            b'%' => {
                let hex = octets
                    .get(i + 1..i + 3)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or_else(|| "Ce code est abîmé (encodage invalide).".to_string())?;
                out.push(hex);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "Ce code est abîmé (texte invalide).".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un vrai certificat d'autorité, produit comme le fait le service.
    fn autorite() -> String {
        let dir = std::env::temp_dir().join(format!(
            "locaryn-lien-connexion-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let ca = crate::mtls::authority(&dir).expect("autorité de test");
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            eprintln!("nettoyage de {} : {e}", dir.display());
        }
        ca.cert_pem
    }

    fn config(login: bool) -> Provisioning {
        Provisioning {
            server_url: "https://192.168.1.42:7474".into(),
            organisation: "PC-de-Téano".into(),
            certificate_fingerprint: None,
            authority_pem: Some(autorite()),
            access_mode: Some("public".into()),
            note: String::new(),
            login_code: None,
            login,
            local_url: Some("https://192.168.1.42:7474".into()),
        }
    }

    #[test]
    fn un_lien_rend_exactement_la_configuration_qu_il_porte() {
        let avant = config(true);
        let lien = vers_lien(&avant).unwrap();
        assert!(lien.starts_with("locaryn://connect?v=1&"));
        assert!(lien.is_ascii(), "le lien doit rester ASCII : {lien}");
        assert!(est_lien(&lien));

        let apres = depuis_lien(&lien).unwrap();
        assert_eq!(apres.server_url, avant.server_url);
        assert_eq!(apres.organisation, "PC-de-Téano");
        assert_eq!(apres.access_mode.as_deref(), Some("public"));
        assert!(apres.login);
        assert_eq!(apres.local_url, avant.local_url);
        // Le PEM peut différer d'un saut de ligne ; le certificat, jamais.
        let der = |pem: &str| crate::mtls::pem_blocks(pem, "CERTIFICATE");
        assert_eq!(
            der(apres.authority_pem.as_deref().unwrap()),
            der(avant.authority_pem.as_deref().unwrap())
        );
    }

    #[test]
    fn un_qr_d_appairage_n_ouvre_pas_l_ecran_du_code() {
        let p = depuis_lien(&vers_lien(&config(false)).unwrap()).unwrap();
        assert!(!p.login);
    }

    #[test]
    fn le_code_de_connexion_n_est_jamais_dans_le_lien() {
        let mut p = config(true);
        p.login_code = Some("ABC234".into());
        let lien = vers_lien(&p).unwrap();
        assert!(!lien.contains("ABC234"), "{lien}");
        assert!(depuis_lien(&lien).unwrap().login_code.is_none());
    }

    #[test]
    fn le_lien_reste_plus_court_que_le_json_qu_il_remplace() {
        let p = config(true);
        let json = serde_json::to_string(&p).unwrap();
        let lien = vers_lien(&p).unwrap();
        assert!(
            lien.len() < json.len(),
            "lien {} ≥ JSON {}",
            lien.len(),
            json.len()
        );
    }

    #[test]
    fn le_lien_d_un_autre_pc_ou_une_autre_version_n_est_pas_pris_pour_celui_ci() {
        let pc = "locaryn://connect?server=https%3A%2F%2Fa%3A7474&ca=https%3A%2F%2Fa%3A7474%2Fv1%2Fpairing%2Fca";
        assert!(!est_lien(pc));
        assert!(depuis_lien(pc).is_err());
        assert!(!est_lien("locaryn://travel?v=1&m=travel"));
        assert!(!est_lien("{\"serverUrl\":\"https://a\"}"));

        let futur = depuis_lien("locaryn://connect?v=2&u=https%3A%2F%2Fa").unwrap_err();
        assert!(futur.contains("plus récente"), "{futur}");
    }

    #[test]
    fn un_lien_abime_est_refuse_avec_un_message_lisible() {
        assert!(depuis_lien("locaryn://connect?v=1").is_err());
        assert!(depuis_lien("locaryn://connect?v=1&u=%ZZ").is_err());
        assert!(depuis_lien("locaryn://connect?v=1&u=https%3A%2F%2Fa&a=").is_err());
        // Clé inconnue : ignorée, pour qu'une version future reste lisible.
        let p = depuis_lien("locaryn://connect?v=1&u=https%3A%2F%2Fa&zz=1").unwrap();
        assert_eq!(p.server_url, "https://a");
    }
}
