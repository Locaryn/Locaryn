//! Géolocalisation approximative d'une adresse IP, entièrement locale.
//!
//! La base vient de DB-IP (« IP to City Lite », licence CC BY 4.0, sans clé)
//! et n'est téléchargée que si la personne le demande. Aucune adresse ne
//! quitte la machine : la recherche se fait dans ce fichier. Sans base, les
//! adresses de la boucle locale et du réseau privé restent reconnues.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Le nom de la base dans le dossier de données.
const FICHIER: &str = "dbip-city-lite.mmdb";

/// La mention que la licence de DB-IP demande à côté de toute donnée affichée.
pub const ATTRIBUTION: &str = "IP Geolocation by DB-IP (https://db-ip.com)";

/// Où la base vit : `<données>/geoip/dbip-city-lite.mmdb`.
pub fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join("geoip").join(FICHIER)
}

/// Une base ouverte, gardée tant que le fichier ne change pas : la relire à
/// chaque adresse coûterait une centaine de mégaoctets de lecture.
type Ouverte = (PathBuf, Option<SystemTime>, Arc<maxminddb::Reader<Vec<u8>>>);

fn cache() -> &'static Mutex<Option<Ouverte>> {
    static CACHE: std::sync::OnceLock<Mutex<Option<Ouverte>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn lecteur(chemin: &Path) -> Option<Arc<maxminddb::Reader<Vec<u8>>>> {
    let modifie = std::fs::metadata(chemin).ok()?.modified().ok();
    let mut garde = cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, m, r)) = garde.as_ref() {
        if p == chemin && *m == modifie {
            return Some(r.clone());
        }
    }
    match maxminddb::Reader::open_readfile(chemin) {
        Ok(r) => {
            let r = Arc::new(r);
            *garde = Some((chemin.to_path_buf(), modifie, r.clone()));
            Some(r)
        }
        Err(e) => {
            tracing::warn!(error = %e, chemin = %chemin.display(), "base de géolocalisation illisible");
            None
        }
    }
}

/// Une adresse qui ne sort pas de la machine ou du réseau local.
fn locale(ip: IpAddr) -> Option<&'static str> {
    let ip = match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        autre => autre,
    };
    if ip.is_loopback() {
        return Some("Cette machine");
    }
    let privee = match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_link_local()
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
        }
        IpAddr::V6(v6) => {
            (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    };
    privee.then_some("Réseau local")
}

/// « Lyon, Auvergne-Rhône-Alpes, France », « Réseau local », ou rien quand la
/// base manque ou ne connaît pas l'adresse.
pub fn locate(ip_texte: &str, base: &Path) -> Option<String> {
    let ip: IpAddr = ip_texte.trim().parse().ok()?;
    if let Some(l) = locale(ip) {
        return Some(l.to_string());
    }
    let lecteur = lecteur(base)?;
    let trouve = match lecteur.lookup(ip) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!(error = %e, "recherche de géolocalisation impossible");
            return None;
        }
    };
    let ville: maxminddb::geoip2::City = match trouve.decode() {
        Ok(Some(v)) => v,
        Ok(None) => return None,
        Err(e) => {
            tracing::debug!(error = %e, "fiche de géolocalisation illisible");
            return None;
        }
    };
    let nom = |fr: Option<&str>, en: Option<&str>| fr.or(en).map(str::to_string);
    let parties: Vec<String> = [
        nom(ville.city.names.french, ville.city.names.english),
        ville
            .subdivisions
            .first()
            .and_then(|s| nom(s.names.french, s.names.english)),
        nom(ville.country.names.french, ville.country.names.english),
    ]
    .into_iter()
    .flatten()
    .filter(|p| !p.trim().is_empty())
    .collect();
    (!parties.is_empty()).then(|| parties.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_adresses_locales_se_reconnaissent_sans_base() {
        let rien = Path::new("absent.mmdb");
        assert_eq!(locate("127.0.0.1", rien).as_deref(), Some("Cette machine"));
        assert_eq!(locate("::1", rien).as_deref(), Some("Cette machine"));
        assert_eq!(
            locate("192.168.1.20", rien).as_deref(),
            Some("Réseau local")
        );
        assert_eq!(
            locate("::ffff:10.0.0.4", rien).as_deref(),
            Some("Réseau local")
        );
        assert_eq!(locate("100.72.3.1", rien).as_deref(), Some("Réseau local"));
        assert_eq!(locate("8.8.8.8", rien), None);
        assert_eq!(locate("pas une adresse", rien), None);
    }
}
