//! L'aperçu d'une page HTML du dossier d'une conversation.
//!
//! La page s'ouvre dans le navigateur intégré, servie par le protocole
//! `apercu` : chaque dossier de page reçoit un jeton aléatoire, et
//! `http://apercu.localhost/<jeton>/…` ne sert que les fichiers de ce
//! dossier. La feuille de style, les scripts et les images relatifs se
//! chargent comme sur un vrai site ; rien hors du dossier n'est lisible, pas
//! même le reste du dossier de la conversation.

use locaryn_agent_runtime::tools::resolve_path;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::http::{header, Response, StatusCode};
use tauri::State;
use uuid::Uuid;

use crate::Core;

/// Le nom du protocole, enregistré au démarrage.
pub const SCHEMA: &str = "apercu";

/// Au-delà, les dossiers ouverts le plus tôt sont oubliés.
const DOSSIERS_MAX: usize = 64;

/// Les dossiers de pages ouverts depuis le lancement, avec leur jeton.
static DOSSIERS: Mutex<Vec<(String, PathBuf)>> = Mutex::new(Vec::new());

/// L'origine du protocole : `http://apercu.localhost` sous Windows et
/// Android (WebView2 et Android n'acceptent que http), `apercu://localhost`
/// ailleurs.
pub fn origine() -> String {
    if cfg!(any(windows, target_os = "android")) {
        format!("http://{SCHEMA}.localhost")
    } else {
        format!("{SCHEMA}://localhost")
    }
}

/// Le jeton d'un dossier : le même à chaque ouverture, pour que la page garde
/// son `localStorage` (une origine, des chemins stables).
fn jeton_pour(dossier: &Path) -> Result<String, String> {
    let mut dossiers = DOSSIERS
        .lock()
        .map_err(|e| format!("aperçu indisponible : {e}"))?;
    if let Some((jeton, _)) = dossiers.iter().find(|(_, d)| d == dossier) {
        return Ok(jeton.clone());
    }
    let jeton = Uuid::new_v4().simple().to_string();
    dossiers.push((jeton.clone(), dossier.to_path_buf()));
    if dossiers.len() > DOSSIERS_MAX {
        dossiers.remove(0);
    }
    Ok(jeton)
}

fn dossier_de(jeton: &str) -> Option<PathBuf> {
    match DOSSIERS.lock() {
        Ok(d) => d.iter().find(|(j, _)| j == jeton).map(|(_, p)| p.clone()),
        Err(e) => {
            tracing::warn!(erreur = %e, "dossiers d'aperçu illisibles");
            None
        }
    }
}

fn est_une_page(chemin: &Path) -> bool {
    chemin
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}

/// Un segment d'adresse : tout sauf les caractères sûrs est encodé.
fn encoder(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for octet in segment.bytes() {
        if octet.is_ascii_alphanumeric() || matches!(octet, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(octet));
        } else {
            out.push_str(&format!("%{octet:02X}"));
        }
    }
    out
}

/// `%20` → espace ; `None` pour un encodage invalide.
fn decoder(texte: &str) -> Option<String> {
    let octets = texte.as_bytes();
    let mut out = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' {
            let hex = std::str::from_utf8(octets.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(octets[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// L'adresse d'aperçu d'une page du dossier de la conversation.
#[tauri::command]
pub async fn apercu_page(
    core: State<'_, Core>,
    session_id: Uuid,
    chemin: String,
) -> Result<String, String> {
    let racine = crate::workspace::racine(&core, session_id).await?;
    let cible = resolve_path(&racine, &chemin).map_err(|e| e.to_string())?;
    let cible = tokio::fs::canonicalize(&cible)
        .await
        .map_err(|e| format!("{chemin} : {e}"))?;
    if !est_une_page(&cible) {
        return Err(format!("{chemin} n'est pas une page HTML."));
    }
    let (Some(dossier), Some(nom)) = (cible.parent(), cible.file_name()) else {
        return Err(format!("{chemin} : chemin illisible."));
    };
    let jeton = jeton_pour(dossier)?;
    Ok(format!(
        "{}/{jeton}/{}",
        origine(),
        encoder(&nom.to_string_lossy())
    ))
}

/// Au-delà, les extraits les plus anciens sont effacés.
const EXTRAITS_MAX: usize = 20;

/// Effacer les extraits les plus anciens pour n'en garder que `garder`.
fn purger_extraits(base: &Path, garder: usize) {
    let Ok(lecture) = std::fs::read_dir(base) else {
        return;
    };
    let mut dossiers: Vec<(std::time::SystemTime, PathBuf)> = lecture
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let date = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (date, e.path())
        })
        .collect();
    dossiers.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, ancien) in dossiers.into_iter().skip(garder) {
        if let Err(e) = std::fs::remove_dir_all(&ancien) {
            tracing::warn!(erreur = %e, dossier = %ancien.display(), "extrait d'aperçu non effacé");
        }
    }
}

/// L'aperçu d'un bloc HTML ou SVG d'une réponse : écrit dans un dossier à lui,
/// puis servi comme une page. Rendu dans un cadre `srcDoc`, il héritait de la
/// politique de sécurité de l'application, qui refuse les scripts en ligne.
#[tauri::command]
pub async fn apercu_extrait(
    core: State<'_, Core>,
    contenu: String,
    svg: bool,
) -> Result<String, String> {
    let base = core.data_dir.join("apercus");
    purger_extraits(&base, EXTRAITS_MAX - 1);
    let dossier = base.join(Uuid::new_v4().simple().to_string());
    tokio::fs::create_dir_all(&dossier)
        .await
        .map_err(|e| format!("Dossier d'aperçu impossible : {e}"))?;
    let nom = if svg { "image.svg" } else { "index.html" };
    tokio::fs::write(dossier.join(nom), contenu)
        .await
        .map_err(|e| format!("Aperçu impossible à écrire : {e}"))?;
    let dossier = tokio::fs::canonicalize(&dossier)
        .await
        .map_err(|e| format!("Dossier d'aperçu illisible : {e}"))?;
    let jeton = jeton_pour(&dossier)?;
    Ok(format!("{}/{jeton}/{nom}", origine()))
}

/// Le fichier demandé par `/<jeton>/<chemin>`, s'il est bien dans le dossier
/// de sa page — liens symboliques compris, puisque tout est résolu avant la
/// comparaison.
fn fichier_demande(chemin_url: &str) -> Result<PathBuf, StatusCode> {
    let mut morceaux = chemin_url.trim_start_matches('/').splitn(2, '/');
    let racine = dossier_de(morceaux.next().unwrap_or("")).ok_or(StatusCode::NOT_FOUND)?;
    let reste = decoder(morceaux.next().unwrap_or("")).ok_or(StatusCode::BAD_REQUEST)?;
    let relatif = Path::new(&reste);
    // Ni `..`, ni lecteur, ni racine : seulement des noms.
    if relatif
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut cible = racine.join(relatif);
    if cible.is_dir() {
        cible = cible.join("index.html");
    }
    let cible = cible.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    if !cible.starts_with(&racine) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !cible.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(cible)
}

fn type_mime(chemin: &Path) -> &'static str {
    let ext = chemin
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" | "cjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "txt" | "md" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn reponse(statut: StatusCode, mime: &str, corps: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(statut)
        .header(header::CONTENT_TYPE, mime)
        // Le modèle réécrit la page : recharger montre toujours la dernière version.
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(corps)
        .unwrap_or_else(|e| {
            tracing::warn!(erreur = %e, "réponse d'aperçu impossible à construire");
            Response::new(Vec::new())
        })
}

/// La réponse du protocole à une requête de chemin `chemin_url`.
pub fn repondre(chemin_url: &str) -> Response<Vec<u8>> {
    let fichier = match fichier_demande(chemin_url) {
        Ok(f) => f,
        Err(statut) => {
            let texte = statut.canonical_reason().unwrap_or("Erreur");
            return reponse(
                statut,
                "text/plain; charset=utf-8",
                texte.as_bytes().to_vec(),
            );
        }
    };
    match std::fs::read(&fichier) {
        Ok(octets) => reponse(StatusCode::OK, type_mime(&fichier), octets),
        Err(e) => {
            tracing::warn!(erreur = %e, fichier = %fichier.display(), "fichier d'aperçu illisible");
            reponse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "text/plain; charset=utf-8",
                b"Fichier illisible".to_vec(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier_de_test() -> PathBuf {
        let d = std::env::temp_dir().join(format!("locaryn-apercu-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(d.join("page/img")).expect("dossier de test");
        std::fs::write(d.join("page/index.html"), "<link href=style.css>").expect("index");
        std::fs::write(d.join("page/style.css"), "body{}").expect("css");
        std::fs::write(d.join("page/img/a b.png"), [1u8, 2, 3]).expect("image");
        std::fs::write(d.join("secret.txt"), "non").expect("secret");
        d.canonicalize().expect("canonique")
    }

    #[test]
    fn encode_et_decode() {
        assert_eq!(encoder("a b/é.html"), "a%20b%2F%C3%A9.html");
        assert_eq!(
            decoder("a%20b%2F%C3%A9.html").as_deref(),
            Some("a b/é.html")
        );
        assert_eq!(decoder("%zz"), None);
        assert_eq!(decoder("%4"), None);
    }

    #[test]
    fn sert_le_dossier_de_la_page_et_rien_d_autre() {
        let d = dossier_de_test();
        let jeton = jeton_pour(&d.join("page")).expect("jeton");
        assert_eq!(jeton_pour(&d.join("page")).expect("jeton"), jeton);

        let ok = |c: &str| fichier_demande(&format!("/{jeton}/{c}"));
        assert_eq!(ok("style.css"), Ok(d.join("page/style.css")));
        assert_eq!(ok(""), Ok(d.join("page/index.html")));
        assert_eq!(ok("img/a%20b.png"), Ok(d.join("page/img/a b.png")));
        assert_eq!(ok("../secret.txt"), Err(StatusCode::FORBIDDEN));
        assert_eq!(ok("img/..%2F..%2Fsecret.txt"), Err(StatusCode::FORBIDDEN));
        assert!(ok("..%5Csecret.txt").is_err());
        assert_eq!(ok("absent.js"), Err(StatusCode::NOT_FOUND));
        assert_eq!(
            fichier_demande("/inconnu/style.css"),
            Err(StatusCode::NOT_FOUND)
        );

        let r = repondre(&format!("/{jeton}/style.css"));
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(
            r.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(r.body().as_slice(), b"body{}");
        std::fs::remove_dir_all(&d).expect("nettoyage");
    }
}
