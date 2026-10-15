//! Chercher sur le web et lire une page, sans ouvrir de navigateur.
//!
//! Le modèle n'avait que le navigateur intégré pour une simple recherche :
//! ouvrir un onglet, taper, attendre le rendu, faire défiler, capturer —
//! une dizaine d'appels lourds là où une requête HTTP suffit. Ces deux outils
//! font le travail directement : `web_search` interroge un moteur (DuckDuckGo,
//! Brave, Bing, Google, Yandex) et rend titres, adresses et extraits ;
//! `web_fetch` rend le texte d'une page.
//!
//! Pour les images, `image_search` rend des adresses d'images directes
//! (Brave Images ; Wikimedia Commons et Openverse pour des images sous
//! licence libre ; Bing Images) et `image_download` en enregistre une dans le projet. Sans eux, un
//! modèle qui devait illustrer un site partait chercher Google Images dans un
//! navigateur, et finissait par prendre le contrôle du bureau.
//!
//! Le navigateur reste là pour ce qui en a vraiment besoin : une page qui se
//! construit en JavaScript, une connexion, un formulaire — ou quand la
//! personne le demande.

use crate::tools::{Risk, ToolResult, ToolSpec};
use std::time::Duration;

pub const WEB_SEARCH: &str = "web_search";
pub const WEB_FETCH: &str = "web_fetch";
pub const IMAGE_SEARCH: &str = "image_search";
pub const IMAGE_DOWNLOAD: &str = "image_download";

/// Taille maximale d'une image téléchargée.
const IMAGE_MAX_OCTETS: usize = 25 * 1024 * 1024;

/// Un navigateur ordinaire : certains moteurs servent une page vide ou un
/// défi à un client qui ne se présente pas.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

/// Les outils web, offerts dans toute conversation.
pub fn web_tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: WEB_SEARCH.into(),
            description: "Recherche sur le web et rend les meilleurs résultats (titre, adresse, \
                          extrait). À préférer au navigateur intégré pour toute recherche : \
                          c'est un seul appel, rapide. Enchaîne avec web_fetch pour lire une \
                          page trouvée."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Ce qu'il faut chercher." },
                    "engine": {
                        "type": "string",
                        "enum": ["auto", "duckduckgo", "brave", "bing", "google", "yandex"],
                        "description": "Le moteur. « auto » essaie DuckDuckGo, puis Brave, puis Bing."
                    },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 15 }
                },
                "required": ["query"]
            }),
            risk: Risk::Low,
            required_permissions: Vec::new(),
        },
        ToolSpec {
            name: WEB_FETCH.into(),
            description: "Télécharge une page web (http/https) et en rend le texte lisible, \
                          sans balises. Pour lire un article, une documentation, une page de \
                          résultats. Les pages qui exigent JavaScript ou une connexion \
                          demandent le navigateur intégré."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "L'adresse complète, en http ou https." },
                    "max_chars": { "type": "integer", "minimum": 500, "maximum": 60000 }
                },
                "required": ["url"]
            }),
            risk: Risk::Low,
            required_permissions: Vec::new(),
        },
        ToolSpec {
            name: IMAGE_SEARCH.into(),
            description: "Cherche des images sur le web et rend leurs adresses directes \
                          (utilisables dans <img src>), avec taille, titre et page d'origine. \
                          Un seul appel, rapide : à préférer à tout navigateur pour trouver des \
                          images. `free_license` : seulement des images sous licence libre \
                          (Wikimedia Commons, Openverse), avec licence et auteur — mots-clés en \
                          anglais. Enchaîne avec image_download pour les enregistrer dans le \
                          projet."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Ce que montre l'image (« forêt alpine au lever du soleil »)." },
                    "engine": {
                        "type": "string",
                        "enum": ["auto", "brave", "commons", "openverse", "bing"],
                        "description": "« auto » essaie Brave, puis Wikimedia Commons et Openverse."
                    },
                    "free_license": { "type": "boolean", "description": "Images sous licence libre seulement." },
                    "max_results": { "type": "integer", "minimum": 1, "maximum": 30 }
                },
                "required": ["query"]
            }),
            risk: Risk::Low,
            required_permissions: Vec::new(),
        },
        ToolSpec {
            name: IMAGE_DOWNLOAD.into(),
            description: "Télécharge une image (adresse http/https, par exemple rendue par \
                          image_search) et l'enregistre dans le projet, au chemin donné \
                          (« images/foret.jpg »). Rend le chemin et la taille, à mettre dans \
                          <img src>."
                .into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "L'adresse directe de l'image." },
                    "path": { "type": "string", "description": "Où l'enregistrer, relatif au projet. L'extension est ajoutée si elle manque." }
                },
                "required": ["url", "path"]
            }),
            risk: Risk::Medium,
            required_permissions: Vec::new(),
        },
    ]
}

pub fn is_web_tool(name: &str) -> bool {
    matches!(name, WEB_SEARCH | WEB_FETCH | IMAGE_SEARCH | IMAGE_DOWNLOAD)
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(6))
        .build()
        .unwrap_or_default()
}

fn erreur(msg: impl Into<String>) -> ToolResult {
    ToolResult {
        ok: false,
        output: msg.into(),
        artifact: None,
    }
}

pub async fn exec(tool: &str, args: &serde_json::Value, projet: &std::path::Path) -> ToolResult {
    match tool {
        WEB_SEARCH => exec_search(args).await,
        WEB_FETCH => exec_fetch(args).await,
        IMAGE_SEARCH => exec_image_search(args).await,
        IMAGE_DOWNLOAD => exec_image_download(args, projet).await,
        _ => erreur(format!("outil web inconnu : {tool}")),
    }
}

// ── Images ─────────────────────────────────────────────────────────────────

/// Une image trouvée.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Image {
    pub url: String,
    pub titre: String,
    pub page: String,
    pub largeur: u64,
    pub hauteur: u64,
    /// Licence et auteur, quand le moteur les donne (Openverse).
    pub licence: String,
}

async fn exec_image_search(args: &serde_json::Value) -> ToolResult {
    let Some(query) = args
        .get("query")
        .and_then(|q| q.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
    else {
        return erreur("il manque la recherche (« query »)");
    };
    let max = args
        .get("max_results")
        .and_then(|n| n.as_u64())
        .unwrap_or(10)
        .clamp(1, 30) as usize;
    let libre = args
        .get("free_license")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    let demande = args
        .get("engine")
        .and_then(|e| e.as_str())
        .unwrap_or("auto")
        .to_ascii_lowercase();
    let ordre: Vec<&str> = if libre {
        vec!["commons", "openverse"]
    } else {
        // Mesuré : Brave rend 200 images pertinentes en moins d'une seconde ;
        // Commons répond en 0,8 s, Openverse parfois en 20 s ; DuckDuckGo exige
        // un défi JavaScript, Google aussi, Bing ne sert qu'une image.
        match demande.as_str() {
            // Bing, seulement demandé : sans JavaScript, il rend des images
            // sans rapport avec la recherche.
            "commons" => vec!["commons", "brave", "openverse"],
            "openverse" => vec!["openverse", "brave", "commons"],
            "bing" => vec!["bing", "brave", "commons", "openverse"],
            _ => vec!["brave", "commons", "openverse"],
        }
    };
    let http = client();
    let mut echecs = Vec::new();
    for moteur in ordre {
        let r = match moteur {
            "brave" => images_brave(&http, query).await,
            "commons" => images_commons(&http, query).await,
            "bing" => images_bing(&http, query).await,
            _ => images_openverse(&http, query).await,
        };
        match r {
            Ok(mut images) if !images.is_empty() => {
                images.truncate(max);
                let mut out = format!("Images {moteur} pour « {query} » :\n");
                for (i, im) in images.iter().enumerate() {
                    out.push_str(&format!(
                        "\n{}. {}\n   image : {}\n",
                        i + 1,
                        im.titre,
                        im.url
                    ));
                    if im.largeur > 0 && im.hauteur > 0 {
                        out.push_str(&format!("   taille : {}×{}\n", im.largeur, im.hauteur));
                    }
                    if !im.page.is_empty() {
                        out.push_str(&format!("   page : {}\n", im.page));
                    }
                    if !im.licence.is_empty() {
                        out.push_str(&format!("   licence : {}\n", im.licence));
                    }
                }
                out.push_str(
                    "\nPour un site : image_download enregistre une image dans le projet \
                     (plus sûr qu'un lien vers un autre site, qui peut disparaître).",
                );
                return ToolResult {
                    ok: true,
                    output: out,
                    artifact: None,
                };
            }
            Ok(_) => echecs.push(format!("{moteur} : aucune image lisible")),
            Err(e) => echecs.push(format!("{moteur} : {e}")),
        }
    }
    erreur(format!(
        "aucun moteur d'images n'a répondu ({}). Réessaie avec d'autres mots.",
        echecs.join(" ; ")
    ))
}

/// Brave Images : la page porte ses résultats dans un objet JavaScript —
/// pour chacun, le titre, la page d'origine, l'image originale et sa taille.
async fn images_brave(http: &reqwest::Client, q: &str) -> Result<Vec<Image>, String> {
    let (finale, html) = telecharger(
        http,
        &format!("https://search.brave.com/images?q={}", encoder(q)),
    )
    .await?;
    if finale.contains("captcha") {
        return Err("le moteur demande une vérification humaine".into());
    }
    Ok(lire_images_brave(&html))
}

/// Une chaîne JavaScript : `\u002F` et `\/` redeviennent `/`.
fn chaine_js(s: &str) -> String {
    let re = regex::Regex::new(r"\\u([0-9a-fA-F]{4})").expect("motif valide");
    re.replace_all(s, |c: &regex::Captures| {
        u32::from_str_radix(&c[1], 16)
            .ok()
            .and_then(char::from_u32)
            .map(String::from)
            .unwrap_or_default()
    })
    .replace("\\/", "/")
}

pub fn lire_images_brave(html: &str) -> Vec<Image> {
    let original = regex::Regex::new(r#"original:"([^"]+)""#).expect("motif valide");
    let page = regex::Regex::new(r#"^\{?title:"[^"]*",url:"([^"]+)""#).expect("motif valide");
    let titre = regex::Regex::new(r#"^\{?title:"((?:[^"\\]|\\.)*)""#).expect("motif valide");
    let taille =
        regex::Regex::new(r"properties:\{[^}]*?height:(\d+),width:(\d+)").expect("motif valide");
    // L'image originale se lit aussi dans l'adresse des images semblables,
    // quand l'objet la désigne par une variable (`url:x`).
    let semblable = regex::Regex::new(r#"similar_images_api_url:"[^"]*?[?&]url=([^&"]+)"#)
        .expect("motif valide");
    let mut out: Vec<Image> = Vec::new();
    for morceau in html.split("{title:\"").skip(1) {
        let m = format!("title:\"{morceau}");
        let url = original
            .captures(&m)
            .map(|c| chaine_js(&c[1]))
            .filter(|u| u.starts_with("http"))
            .or_else(|| {
                semblable
                    .captures(&m)
                    .map(|c| decoder_pourcent(&c[1]))
                    .filter(|u| u.starts_with("http"))
            });
        let Some(url) = url else {
            continue;
        };
        if out.iter().any(|x| x.url == url) {
            continue;
        }
        let (largeur, hauteur) = taille
            .captures(&m)
            .map(|c| (c[2].parse().unwrap_or(0), c[1].parse().unwrap_or(0)))
            .unwrap_or((0, 0));
        out.push(Image {
            url,
            titre: couper(
                &entites(&chaine_js(
                    &titre
                        .captures(&m)
                        .map(|c| c[1].to_string())
                        .unwrap_or_default(),
                )),
                140,
            ),
            page: page
                .captures(&m)
                .map(|c| chaine_js(&c[1]))
                .unwrap_or_default(),
            largeur,
            hauteur,
            licence: String::new(),
        });
    }
    out
}

/// Bing Images : chaque vignette porte, dans son attribut `m`, le JSON de
/// l'image (`murl` l'image, `purl` sa page, `t` son titre).
async fn images_bing(http: &reqwest::Client, q: &str) -> Result<Vec<Image>, String> {
    let qe = encoder(q);
    let (finale, html) = telecharger(
        http,
        &format!("https://www.bing.com/images/search?q={qe}&form=HDRSC2&first=1"),
    )
    .await?;
    if finale.contains("captcha") {
        return Err("le moteur demande une vérification humaine".into());
    }
    Ok(lire_images_bing(&html))
}

pub fn lire_images_bing(html: &str) -> Vec<Image> {
    let re = regex::Regex::new(r#"\bm="(\{[^"]*\})""#).expect("motif valide");
    let mut out: Vec<Image> = Vec::new();
    for c in re.captures_iter(html) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&entites(&c[1])) else {
            continue;
        };
        let url = v["murl"].as_str().unwrap_or("").to_string();
        if !url.starts_with("http") || out.iter().any(|x| x.url == url) {
            continue;
        }
        out.push(Image {
            url,
            titre: couper(&texte(v["t"].as_str().unwrap_or("")), 140),
            page: v["purl"].as_str().unwrap_or("").to_string(),
            ..Image::default()
        });
    }
    out
}

/// Openverse : des images sous licence libre (Creative Commons, domaine
/// public), par une API ouverte et sans clé.
async fn images_openverse(http: &reqwest::Client, q: &str) -> Result<Vec<Image>, String> {
    let (_, corps) = telecharger_accept(
        http,
        &format!(
            "https://api.openverse.org/v1/images/?q={}&page_size=20",
            encoder(q)
        ),
        "application/json",
    )
    .await?;
    let v: serde_json::Value = serde_json::from_str(&corps).map_err(|e| e.to_string())?;
    Ok(lire_images_openverse(&v))
}

/// Wikimedia Commons : des millions de photos sous licence libre, par l'API
/// de MediaWiki. Une phrase entière ne trouve souvent rien (tous les mots
/// sont exigés) : on réessaie alors avec ses mots les plus longs.
async fn images_commons(http: &reqwest::Client, q: &str) -> Result<Vec<Image>, String> {
    let mut essais = vec![q.to_string()];
    let mut mots: Vec<&str> = q
        .split_whitespace()
        .filter(|m| m.chars().count() > 3)
        .collect();
    if mots.len() > 2 {
        mots.sort_by_key(|m| std::cmp::Reverse(m.chars().count()));
        essais.push(mots[..2].join(" "));
    }
    for essai in essais {
        let url = format!(
            "https://commons.wikimedia.org/w/api.php?action=query&format=json&generator=search\
             &gsrnamespace=6&gsrlimit=20&gsrsearch={}%20filetype%3Abitmap&prop=imageinfo\
             &iiprop=url%7Csize%7Cextmetadata&iiurlwidth=1600\
             &iiextmetadatafilter=LicenseShortName%7CArtist%7CObjectName",
            encoder(&essai).replace('+', "%20")
        );
        let (_, corps) = telecharger_accept(http, &url, "application/json").await?;
        let v: serde_json::Value = serde_json::from_str(&corps).map_err(|e| e.to_string())?;
        let images = lire_images_commons(&v);
        if !images.is_empty() {
            return Ok(images);
        }
    }
    Ok(Vec::new())
}

pub fn lire_images_commons(v: &serde_json::Value) -> Vec<Image> {
    let mut pages: Vec<&serde_json::Value> = v["query"]["pages"]
        .as_object()
        .map(|o| o.values().collect())
        .unwrap_or_default();
    pages.sort_by_key(|p| p["index"].as_u64().unwrap_or(u64::MAX));
    let mut out = Vec::new();
    for p in pages {
        let info = &p["imageinfo"][0];
        // La version réduite à 1 600 px quand l'original est plus grand : une
        // photo de 6 000 px pèse 15 Mo pour un site.
        let url = info["thumburl"]
            .as_str()
            .or_else(|| info["url"].as_str())
            .unwrap_or("")
            .to_string();
        if !url.starts_with("http") {
            continue;
        }
        let meta = &info["extmetadata"];
        let auteur = texte(meta["Artist"]["value"].as_str().unwrap_or(""));
        let licence = meta["LicenseShortName"]["value"].as_str().unwrap_or("");
        let titre = p["title"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches("File:")
            .to_string();
        out.push(Image {
            url,
            titre: couper(&titre, 140),
            page: info["descriptionurl"].as_str().unwrap_or("").to_string(),
            largeur: info["thumbwidth"]
                .as_u64()
                .or_else(|| info["width"].as_u64())
                .unwrap_or(0),
            hauteur: info["thumbheight"]
                .as_u64()
                .or_else(|| info["height"].as_u64())
                .unwrap_or(0),
            licence: if auteur.is_empty() {
                licence.to_string()
            } else {
                couper(&format!("{licence}, par {auteur}"), 120)
            },
        });
    }
    out
}

pub fn lire_images_openverse(v: &serde_json::Value) -> Vec<Image> {
    let mut out: Vec<Image> = Vec::new();
    for r in v["results"].as_array().into_iter().flatten() {
        let url = r["url"].as_str().unwrap_or("").to_string();
        if !url.starts_with("http") || out.iter().any(|x| x.url == url) {
            continue;
        }
        let licence = format!(
            "{} {}{}",
            r["license"].as_str().unwrap_or("").to_uppercase(),
            r["license_version"].as_str().unwrap_or(""),
            r["creator"]
                .as_str()
                .map(|c| format!(", par {c}"))
                .unwrap_or_default()
        );
        out.push(Image {
            url,
            titre: couper(r["title"].as_str().unwrap_or(""), 140),
            page: r["foreign_landing_url"].as_str().unwrap_or("").to_string(),
            largeur: r["width"].as_u64().unwrap_or(0),
            hauteur: r["height"].as_u64().unwrap_or(0),
            licence: licence.trim().to_string(),
        });
    }
    out
}

/// L'extension d'un type d'image, ou `None` si ce n'en est pas une.
fn extension_image(type_mime: &str, debut: &[u8]) -> Option<&'static str> {
    let t = type_mime.to_ascii_lowercase();
    let par_type = if t.contains("jpeg") || t.contains("jpg") {
        Some("jpg")
    } else if t.contains("png") {
        Some("png")
    } else if t.contains("webp") {
        Some("webp")
    } else if t.contains("gif") {
        Some("gif")
    } else if t.contains("svg") {
        Some("svg")
    } else if t.contains("avif") {
        Some("avif")
    } else {
        None
    };
    // Le contenu d'abord : bien des serveurs disent « octet-stream ».
    match debut {
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("webp"),
        _ => par_type,
    }
}

async fn exec_image_download(args: &serde_json::Value, projet: &std::path::Path) -> ToolResult {
    let url = args
        .get("url")
        .and_then(|u| u.as_str())
        .map(str::trim)
        .unwrap_or("");
    let chemin = args
        .get("path")
        .and_then(|u| u.as_str())
        .map(str::trim)
        .unwrap_or("");
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return erreur("il faut l'adresse http/https de l'image (« url »)");
    }
    if chemin.is_empty() {
        return erreur("il faut le chemin où l'enregistrer dans le projet (« path »)");
    }
    if projet.as_os_str().is_empty() {
        return erreur("aucun dossier de projet : rien où enregistrer l'image");
    }
    let mut cible = match crate::tools::resolve_path(projet, chemin) {
        Ok(p) => p,
        Err(e) => return erreur(e.to_string()),
    };
    let (type_mime, octets) = match image_par_curl(url).await {
        Ok(r) => r,
        Err(_) => match image_par_client(url).await {
            Ok(r) => r,
            Err(e) => return erreur(e),
        },
    };
    enregistrer_image(url, &type_mime, &octets, &mut cible, projet).await
}

/// L'image par curl : le type se lit à la fin de la sortie.
async fn image_par_curl(url: &str) -> Result<(String, Vec<u8>), String> {
    let mut commande = std::process::Command::new("curl");
    locaryn_config::hide_console(&mut commande);
    let sortie = tokio::process::Command::from(commande)
        .args([
            "-sL",
            "--max-time",
            "30",
            "--max-filesize",
            &IMAGE_MAX_OCTETS.to_string(),
            "-A",
            USER_AGENT,
            "-H",
            "Accept: image/avif,image/webp,image/*,*/*;q=0.8",
            "-w",
            &format!("{FIN_CURL}%{{content_type}}\u{1}%{{http_code}}"),
            url,
        ])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("curl absent : {e}"))?;
    let marque = FIN_CURL.as_bytes();
    let pos = sortie
        .stdout
        .windows(marque.len())
        .rposition(|w| w == marque)
        .ok_or("curl : sortie illisible")?;
    let fin = String::from_utf8_lossy(&sortie.stdout[pos + marque.len()..]).into_owned();
    let (type_mime, code) = fin.split_once('\u{1}').unwrap_or(("", "0"));
    let code: u16 = code.trim().parse().unwrap_or(0);
    if !(200..300).contains(&code) {
        return Err(format!(
            "HTTP {code} : le site refuse l'image, essaie une autre adresse de image_search"
        ));
    }
    let mut octets = sortie.stdout;
    octets.truncate(pos);
    Ok((type_mime.to_string(), octets))
}

async fn image_par_client(url: &str) -> Result<(String, Vec<u8>), String> {
    let http = client();
    let resp = match http
        .get(url)
        .header("Accept", "image/avif,image/webp,image/*,*/*;q=0.8")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Err(format!("réseau : {e}")),
    };
    if !resp.status().is_success() {
        return Err(format!(
            "HTTP {} : le site refuse l'image, essaie une autre adresse de image_search",
            resp.status().as_u16()
        ));
    }
    let type_mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let octets = resp
        .bytes()
        .await
        .map_err(|e| format!("téléchargement interrompu : {e}"))?;
    Ok((type_mime, octets.to_vec()))
}

async fn enregistrer_image(
    url: &str,
    type_mime: &str,
    octets: &[u8],
    cible: &mut std::path::PathBuf,
    projet: &std::path::Path,
) -> ToolResult {
    let _ = url;
    if octets.len() > IMAGE_MAX_OCTETS {
        return erreur("image trop lourde (plus de 25 Mo)");
    }
    let Some(ext) = extension_image(type_mime, &octets[..octets.len().min(16)]) else {
        return erreur(format!(
            "ce n'est pas une image (type « {type_mime} ») : c'est sans doute la page qui la \
             montre. Prends l'adresse « image : » de image_search."
        ));
    };
    if cible.extension().is_none() {
        cible.set_extension(ext);
    }
    if let Some(dossier) = cible.parent() {
        if let Err(e) = tokio::fs::create_dir_all(dossier).await {
            return erreur(format!("dossier impossible à créer : {e}"));
        }
    }
    if let Err(e) = tokio::fs::write(&*cible, octets).await {
        return erreur(format!("image non enregistrée : {e}"));
    }
    let relatif = cible
        .strip_prefix(projet)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| cible.to_string_lossy().into_owned());
    ToolResult {
        ok: true,
        output: format!(
            "Image enregistrée : {relatif} ({} Ko, {ext}). Dans une page à la racine du projet : \
             <img src=\"{relatif}\" alt=\"…\">",
            octets.len() / 1024
        ),
        artifact: None,
    }
}

/// Un résultat de recherche.
#[derive(Debug, Clone, PartialEq)]
pub struct Resultat {
    pub titre: String,
    pub url: String,
    pub extrait: String,
}

async fn exec_search(args: &serde_json::Value) -> ToolResult {
    let Some(query) = args
        .get("query")
        .and_then(|q| q.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
    else {
        return erreur("il manque la recherche (« query »)");
    };
    let max = args
        .get("max_results")
        .and_then(|n| n.as_u64())
        .unwrap_or(8)
        .clamp(1, 15) as usize;
    let demande = args
        .get("engine")
        .and_then(|e| e.as_str())
        .unwrap_or("auto")
        .to_ascii_lowercase();
    let ordre: Vec<&str> = match demande.as_str() {
        // Bing termine chaque liste : mesuré, c'est celui qui refuse le moins
        // (Brave limite les rafales, Google exige JavaScript, Yandex un captcha).
        "duckduckgo" | "ddg" => vec!["duckduckgo", "bing", "brave"],
        "brave" => vec!["brave", "duckduckgo", "bing"],
        "bing" => vec!["bing", "duckduckgo", "brave"],
        "google" => vec!["google", "duckduckgo", "bing"],
        "yandex" => vec!["yandex", "duckduckgo", "bing"],
        _ => vec!["duckduckgo", "bing", "brave"],
    };
    let http = client();
    let mut echecs = Vec::new();
    for moteur in ordre {
        match chercher(&http, moteur, query).await {
            Ok(mut r) if !r.is_empty() => {
                r.truncate(max);
                let mut out = format!("Résultats {moteur} pour « {query} » :\n");
                for (i, x) in r.iter().enumerate() {
                    out.push_str(&format!("\n{}. {}\n   {}\n", i + 1, x.titre, x.url));
                    if !x.extrait.is_empty() {
                        out.push_str(&format!("   {}\n", x.extrait));
                    }
                }
                if moteur != demande && demande != "auto" {
                    out.push_str(&format!(
                        "\n(Le moteur demandé n'a rien rendu : {}.)",
                        echecs.join(" ; ")
                    ));
                }
                return ToolResult {
                    ok: true,
                    output: out,
                    artifact: None,
                };
            }
            Ok(_) => echecs.push(format!("{moteur} : aucun résultat lisible")),
            Err(e) => echecs.push(format!("{moteur} : {e}")),
        }
    }
    erreur(format!(
        "aucun moteur n'a répondu ({}). Réessaie avec d'autres mots, ou utilise le navigateur \
         intégré.",
        echecs.join(" ; ")
    ))
}

/// Marque qui sépare la page de ce que curl dit d'elle (adresse finale, code).
const FIN_CURL: &str = "\n\u{1}LOCARYN\u{1}";

/// Télécharger par le `curl` du système (présent sur Windows 10+, macOS,
/// Linux). Brave, Openverse et bien des sites derrière Cloudflare refusent
/// le client HTTP de l'application à sa signature TLS (429 à chaque requête,
/// ou 20 s d'attente) et répondent à curl en moins d'une seconde.
async fn telecharger_curl(url: &str, accept: &str) -> Result<(String, String), String> {
    let mut commande = std::process::Command::new("curl");
    locaryn_config::hide_console(&mut commande);
    let sortie = tokio::process::Command::from(commande)
        .args([
            "-sL",
            "--compressed",
            "--max-time",
            "20",
            "--max-redirs",
            "6",
            "-A",
            USER_AGENT,
            "-H",
            "Accept-Language: fr-FR,fr;q=0.9,en;q=0.8",
            "-H",
            &format!("Accept: {accept}"),
            "-w",
            &format!("{FIN_CURL}%{{url_effective}}\u{1}%{{http_code}}"),
            url,
        ])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("curl absent : {e}"))?;
    let texte = String::from_utf8_lossy(&sortie.stdout).into_owned();
    let (corps, fin) = texte
        .rsplit_once(FIN_CURL)
        .ok_or_else(|| format!("curl : {}", String::from_utf8_lossy(&sortie.stderr).trim()))?;
    let (finale, code) = fin.split_once('\u{1}').unwrap_or((url, "0"));
    let code: u16 = code.trim().parse().unwrap_or(0);
    if !(200..300).contains(&code) {
        return Err(format!("HTTP {code}"));
    }
    Ok((finale.to_string(), corps.to_string()))
}

async fn telecharger(http: &reqwest::Client, url: &str) -> Result<(String, String), String> {
    telecharger_accept(
        http,
        url,
        "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.5",
    )
    .await
}

/// Par curl d'abord, par le client de l'application s'il manque ou échoue.
async fn telecharger_accept(
    http: &reqwest::Client,
    url: &str,
    accept: &str,
) -> Result<(String, String), String> {
    match telecharger_curl(url, accept).await {
        Ok(r) => return Ok(r),
        Err(e) => tracing::debug!(erreur = %e, %url, "curl n'a pas abouti — client intégré"),
    }
    let resp = http
        .get(url)
        .header("Accept-Language", "fr-FR,fr;q=0.9,en;q=0.8")
        .header("Accept", accept)
        .send()
        .await
        .map_err(|e| format!("réseau : {e}"))?;
    let finale = resp.url().to_string();
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let texte = resp.text().await.map_err(|e| e.to_string())?;
    Ok((finale, texte))
}

async fn chercher(http: &reqwest::Client, moteur: &str, q: &str) -> Result<Vec<Resultat>, String> {
    let qe = encoder(q);
    let url = match moteur {
        "duckduckgo" => format!("https://html.duckduckgo.com/html/?q={qe}"),
        "brave" => format!("https://search.brave.com/search?q={qe}&source=web"),
        "bing" => format!("https://www.bing.com/search?q={qe}"),
        "google" => format!("https://www.google.com/search?q={qe}&hl=fr&gbv=1&num=12"),
        "yandex" => format!("https://yandex.com/search/?text={qe}"),
        _ => return Err("moteur inconnu".into()),
    };
    let (finale, html) = telecharger(http, &url).await?;
    if finale.contains("captcha") || finale.contains("consent.") || html.contains("showcaptcha") {
        return Err("le moteur demande une vérification humaine".into());
    }
    Ok(match moteur {
        "duckduckgo" => lire_duckduckgo(&html),
        "brave" => lire_brave(&html),
        "bing" => lire_bing(&html),
        "google" => lire_google(&html),
        _ => lire_yandex(&html),
    })
}

fn encoder(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn decoder_pourcent(s: &str) -> String {
    let octets = s.as_bytes();
    let mut out = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        match octets[i] {
            b'%' if i + 2 < octets.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                }
                Err(_) => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Les entités HTML courantes. Le reste passe tel quel : un extrait lisible
/// suffit, pas une restitution exacte.
fn entites(s: &str) -> String {
    let mut t = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&#0183;", "·")
        .replace("&#32;", " ");
    // &#NNN; et &#xHH;
    let re = regex::Regex::new(r"&#(x?)([0-9a-fA-F]+);").expect("motif valide");
    t = re
        .replace_all(&t, |c: &regex::Captures| {
            let radix = if &c[1] == "x" { 16 } else { 10 };
            u32::from_str_radix(&c[2], radix)
                .ok()
                .and_then(char::from_u32)
                .map(String::from)
                .unwrap_or_default()
        })
        .into_owned();
    t
}

/// Le texte d'un fragment HTML : balises retirées, blancs resserrés.
fn texte(fragment: &str) -> String {
    let sans = regex::Regex::new(r"(?s)<[^>]*>")
        .expect("motif valide")
        .replace_all(fragment, " ");
    entites(&sans)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn couper(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n).collect();
    out.push('…');
    out
}

fn ajouter(out: &mut Vec<Resultat>, titre: String, url: String, extrait: String) {
    let url = url.trim().to_string();
    if titre.is_empty() || !url.starts_with("http") || out.iter().any(|r| r.url == url) {
        return;
    }
    out.push(Resultat {
        titre: couper(&titre, 160),
        url,
        extrait: couper(&extrait, 320),
    });
}

pub fn lire_duckduckgo(html: &str) -> Vec<Resultat> {
    let lien = regex::Regex::new(r#"(?s)class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
        .expect("motif valide");
    let lien2 = regex::Regex::new(r#"(?s)href="([^"]+)"[^>]*class="result__a"[^>]*>(.*?)</a>"#)
        .expect("motif valide");
    let extrait =
        regex::Regex::new(r#"(?s)class="result__snippet"[^>]*>(.*?)</a>"#).expect("motif valide");
    let mut out = Vec::new();
    // Chaque résultat est un bloc « result » ; on les lit un par un pour
    // garder l'extrait avec son titre.
    for bloc in html.split("class=\"result ").skip(1) {
        let Some(c) = lien.captures(bloc).or_else(|| lien2.captures(bloc)) else {
            continue;
        };
        let href = entites(&c[1]);
        let url = match href.split("uddg=").nth(1) {
            Some(enc) => decoder_pourcent(enc.split('&').next().unwrap_or(enc)),
            None if href.starts_with("//") => format!("https:{href}"),
            None => href,
        };
        // Les annonces passent par `duckduckgo.com/y.js` : pas des résultats.
        if url.contains("duckduckgo.com/y.js") || url.contains("ad_domain=") {
            continue;
        }
        let ex = extrait
            .captures(bloc)
            .map(|e| texte(&e[1]))
            .unwrap_or_default();
        ajouter(&mut out, texte(&c[2]), url, ex);
    }
    out
}

pub fn lire_bing(html: &str) -> Vec<Resultat> {
    let titre = regex::Regex::new(r#"(?s)<h2[^>]*>\s*<a[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
        .expect("motif valide");
    let para = regex::Regex::new(r#"(?s)<p[^>]*>(.*?)</p>"#).expect("motif valide");
    let mut out = Vec::new();
    for bloc in html.split("<li class=\"b_algo\"").skip(1) {
        let bloc = bloc.split("</li>").next().unwrap_or(bloc);
        let Some(c) = titre.captures(bloc) else {
            continue;
        };
        let href = entites(&c[1]);
        let url = bing_cible(&href).unwrap_or(href);
        let ex = para
            .captures(bloc)
            .map(|p| texte(&p[1]))
            .unwrap_or_default();
        ajouter(&mut out, texte(&c[2]), url, ex);
    }
    out
}

/// Bing enveloppe ses liens (`/ck/a?…&u=a1<base64>`) : la vraie adresse est
/// dans `u`, en base64 « url-safe » précédé de `a1`.
fn bing_cible(href: &str) -> Option<String> {
    use base64::Engine as _;
    let u = href.split("&u=").nth(1)?.split('&').next()?;
    let b64 = u.strip_prefix("a1")?;
    let octets = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(b64.trim_end_matches('='))
        .ok()?;
    String::from_utf8(octets).ok()
}

pub fn lire_brave(html: &str) -> Vec<Resultat> {
    let href = regex::Regex::new(r#"href="(https?://[^"]+)""#).expect("motif valide");
    let titre = regex::Regex::new(r#"(?s)class="[^"]*search-snippet-title[^"]*"[^>]*>(.*?)</div>"#)
        .expect("motif valide");
    let svg = regex::Regex::new(r"(?s)<svg.*?</svg>").expect("motif valide");
    let mut out = Vec::new();
    for bloc in html.split("data-type=\"web\"").skip(1) {
        let bloc = &bloc[..bloc.len().min(12_000)];
        let bloc = svg.replace_all(bloc, "");
        let Some(u) = href.captures(&bloc) else {
            continue;
        };
        let Some(t) = titre.captures(&bloc) else {
            continue;
        };
        let t_txt = texte(&t[1]);
        // L'extrait : le texte qui suit le titre, sans l'adresse affichée.
        let apres = &bloc[t.get(0).map(|m| m.end()).unwrap_or(0)..];
        let ex = texte(apres);
        let ex = ex.split_whitespace().take(60).collect::<Vec<_>>().join(" ");
        ajouter(&mut out, t_txt, entites(&u[1]), ex);
    }
    out
}

pub fn lire_google(html: &str) -> Vec<Resultat> {
    let lien = regex::Regex::new(r#"(?s)<a href="/url\?q=([^"&]+)[^"]*"[^>]*>(.*?)</a>"#)
        .expect("motif valide");
    let h3 = regex::Regex::new(r"(?s)<h3[^>]*>(.*?)</h3>").expect("motif valide");
    let mut out = Vec::new();
    for c in lien.captures_iter(html) {
        let url = decoder_pourcent(&c[1]);
        let t = h3
            .captures(&c[2])
            .map(|h| texte(&h[1]))
            .unwrap_or_else(|| texte(&c[2]));
        if url.contains("google.com/") {
            continue;
        }
        ajouter(&mut out, t, url, String::new());
    }
    out
}

pub fn lire_yandex(html: &str) -> Vec<Resultat> {
    let lien = regex::Regex::new(
        r#"(?s)<a[^>]*class="[^"]*(?:OrganicTitle-Link|organic__url)[^"]*"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#,
    )
    .expect("motif valide");
    let mut out = Vec::new();
    for c in lien.captures_iter(html) {
        ajouter(&mut out, texte(&c[2]), entites(&c[1]), String::new());
    }
    out
}

async fn exec_fetch(args: &serde_json::Value) -> ToolResult {
    let Some(url) = args
        .get("url")
        .and_then(|u| u.as_str())
        .map(str::trim)
        .filter(|u| !u.is_empty())
    else {
        return erreur("il manque l'adresse (« url »)");
    };
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return erreur("seules les adresses http et https se lisent ici");
    }
    let max = args
        .get("max_chars")
        .and_then(|n| n.as_u64())
        .unwrap_or(20_000)
        .clamp(500, 60_000) as usize;
    let http = client();
    let (finale, corps) = match telecharger(&http, url).await {
        Ok(r) => r,
        Err(e) => return erreur(format!("page illisible : {e}")),
    };
    let lisible = if corps.trim_start().starts_with('<') {
        page_en_texte(&corps)
    } else {
        corps
    };
    if lisible.trim().is_empty() {
        return erreur(
            "la page est vide sans JavaScript : ouvre-la avec le navigateur intégré si elle est \
             indispensable.",
        );
    }
    let total = lisible.chars().count();
    let mut out = format!("Page : {finale}\n\n{}", couper(&lisible, max));
    if total > max {
        out.push_str(&format!(
            "\n\n[{total} caractères au total : relance avec un max_chars plus grand pour la suite.]"
        ));
    }
    ToolResult {
        ok: true,
        output: out,
        artifact: None,
    }
}

/// Le texte d'une page : scripts, styles et navigation retirés, titres et
/// paragraphes séparés par des lignes.
pub fn page_en_texte(html: &str) -> String {
    // Le contenu principal quand la page le désigne : sans lui, une page de
    // Wikipédia commençait par cinquante lignes de menus.
    let principal = ["<main", "<article", "role=\"main\""]
        .iter()
        .find_map(|debut| {
            let i = html.find(debut)?;
            let fin = ["</main>", "</article>"]
                .iter()
                .filter_map(|f| html.rfind(f))
                .max()
                .filter(|f| *f > i)
                .unwrap_or(html.len());
            Some(&html[i..fin])
        })
        .unwrap_or(html);
    // Le moteur de motifs n'a pas de référence arrière : une passe par balise.
    let mut sans = principal.to_string();
    for balise in [
        "script", "style", "noscript", "svg", "template", "iframe", "head", "nav", "footer",
        "aside", "form",
    ] {
        let re = regex::Regex::new(&format!(r"(?is)<{balise}\b[^>]*>.*?</{balise}>"))
            .expect("motif valide");
        sans = re.replace_all(&sans, " ").into_owned();
    }
    let blocs =
        regex::Regex::new(r"(?i)</?(p|div|br|li|h[1-6]|tr|section|article|pre|ul|ol|table)[^>]*>")
            .expect("motif valide");
    let avec_lignes = blocs.replace_all(&sans, "\n");
    let balises = regex::Regex::new(r"(?s)<[^>]*>").expect("motif valide");
    let brut = entites(&balises.replace_all(&avec_lignes, " "));
    let mut out = String::new();
    let mut vides = 0;
    for ligne in brut.lines() {
        let l = ligne.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            vides += 1;
            if vides == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        vides = 0;
        out.push_str(&l);
        out.push('\n');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duckduckgo_ignore_les_annonces() {
        let html = r#"<div class="result results_links"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fduckduckgo.com%2Fy.js%3Fad_domain%3Damazon.fr&amp;rut=1">Pub</a></div>
<div class="result results_links"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.nvidia.com%2Ffr%2Dfr%2Fproducts%2Frtx%2Dspark%2F&amp;rut=2">NVIDIA RTX Spark — Slim</a><a class="result__snippet" href="x">Le <b>superchip</b> &amp; plus</a></div>"#;
        let r = lire_duckduckgo(html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://www.nvidia.com/fr-fr/products/rtx-spark/");
        assert_eq!(r[0].titre, "NVIDIA RTX Spark — Slim");
        assert_eq!(r[0].extrait, "Le superchip & plus");
    }

    #[test]
    fn bing_rend_la_vraie_adresse() {
        let html = r#"<li class="b_algo" data-id><h2><a target="_blank" href="https://www.bing.com/ck/a?!&amp;&amp;p=1&amp;u=a1aHR0cHM6Ly9wZXJzcGVjdGl2ZXMubnZpZGlhLmNvbS9ydHgtc3Bhcmsv&amp;ntb=1">RTX <strong>Spark</strong></a></h2><p class="b_lineclamp2">Un extrait.</p></li>"#;
        let r = lire_bing(html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://perspectives.nvidia.com/rtx-spark/");
        assert_eq!(r[0].titre, "RTX Spark");
        assert_eq!(r[0].extrait, "Un extrait.");
    }

    #[test]
    fn la_page_perd_ses_scripts() {
        let t = page_en_texte(
            "<html><head><title>x</title></head><body><script>var a=1</script><h1>Titre</h1><p>Un &amp; deux</p></body></html>",
        );
        assert_eq!(t, "Titre\n\nUn & deux");
    }

    #[test]
    fn encodage_de_la_requete() {
        assert_eq!(encoder("rtx spark é"), "rtx+spark+%C3%A9");
        assert_eq!(decoder_pourcent("a%2Fb+c"), "a/b c");
    }
}

#[cfg(test)]
mod images_hors_ligne {
    #[test]
    fn bing_rend_l_adresse_de_l_image() {
        let html = r#"<a class="iusc" m="{&quot;purl&quot;:&quot;https://exemple.fr/page&quot;,&quot;murl&quot;:&quot;https://exemple.fr/foret.jpg&quot;,&quot;t&quot;:&quot;For&#234;t &lt;b&gt;alpine&lt;/b&gt;&quot;}" href="/images">x</a>"#;
        let r = super::lire_images_bing(html);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://exemple.fr/foret.jpg");
        assert_eq!(r[0].page, "https://exemple.fr/page");
        assert_eq!(r[0].titre, "Forêt alpine");
    }

    #[test]
    fn brave_rend_l_original_sa_page_et_sa_taille() {
        let html = r#"x,{title:"Forêt \u002F alpine",url:"https://exemple.fr/page",is_source_local:false,thumbnail:{src:"https://imgs.search.brave.com/a",height:250,width:250,original:"https://exemple.fr/foret.webp",resized:null},properties:{url:"https://exemple.fr/foret.webp",resized:"r",placeholder:"p",height:1112,width:960,format:null},meta_url:{}},{title:"Deux",url:"https://b.fr",thumbnail:{src:"t"},properties:{url:x,height:10,width:20},similar_images_api_url:"/api/images/similar?q=a&url=https%3A%2F%2Fb.fr%2Fdeux.jpg&safesearch=moderate"},{title:"Sans image",url:"https://c.fr"}"#;
        let r = super::lire_images_brave(html);
        assert_eq!(r.len(), 2, "{r:?}");
        assert_eq!(r[0].url, "https://exemple.fr/foret.webp");
        assert_eq!(r[0].page, "https://exemple.fr/page");
        assert_eq!(r[0].titre, "Forêt / alpine");
        assert_eq!((r[0].largeur, r[0].hauteur), (960, 1112));
        assert_eq!(r[1].url, "https://b.fr/deux.jpg");
    }

    #[test]
    fn commons_rend_la_version_reduite_et_la_licence() {
        let v = serde_json::json!({"query": {"pages": {
            "2": {"index": 2, "title": "File:B.jpg", "imageinfo": [{"url": "https://u.org/b.jpg", "width": 800, "height": 600, "descriptionurl": "https://c.org/B"}]},
            "1": {"index": 1, "title": "File:A.jpg", "imageinfo": [{"url": "https://u.org/a.jpg", "thumburl": "https://u.org/1600px-a.jpg", "thumbwidth": 1600, "thumbheight": 900, "width": 6000,
                  "extmetadata": {"LicenseShortName": {"value": "CC BY-SA 4.0"}, "Artist": {"value": "<a href=\"x\">Ana</a>"}}}]}
        }}});
        let r = super::lire_images_commons(&v);
        assert_eq!(r[0].url, "https://u.org/1600px-a.jpg");
        assert_eq!(r[0].titre, "A.jpg");
        assert_eq!((r[0].largeur, r[0].hauteur), (1600, 900));
        assert_eq!(r[0].licence, "CC BY-SA 4.0, par Ana");
        assert_eq!(r[1].url, "https://u.org/b.jpg");
    }

    #[test]
    fn openverse_rend_la_licence() {
        let v = serde_json::json!({"results": [
            {"url": "https://o.org/a.jpg", "title": "A", "license": "by", "license_version": "4.0", "creator": "Ana", "foreign_landing_url": "https://flickr.com/a"}
        ]});
        let r = super::lire_images_openverse(&v);
        assert_eq!(r[0].licence, "BY 4.0, par Ana");
    }

    #[test]
    fn le_contenu_dit_si_c_est_une_image() {
        assert_eq!(
            super::extension_image("application/octet-stream", &[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("jpg")
        );
        assert_eq!(
            super::extension_image("image/webp", b"RIFF1234WEBPVP8"),
            Some("webp")
        );
        assert_eq!(
            super::extension_image("text/html", b"<!doctype html>"),
            None
        );
    }
}

#[cfg(test)]
mod reel {
    /// Contre les vrais moteurs : `cargo test -p locaryn-agent-runtime web::reel -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn chaque_moteur_repond() {
        for moteur in ["duckduckgo", "brave", "bing", "google", "yandex"] {
            let r = super::exec(
                super::WEB_SEARCH,
                &serde_json::json!({"query": "nvidia rtx spark", "engine": moteur, "max_results": 3}),
                std::path::Path::new(""),
            )
            .await;
            println!("=== {moteur} ok={}\n{}\n", r.ok, r.output);
        }
        let p = super::exec(
            super::WEB_FETCH,
            &serde_json::json!({"url": "https://en.wikipedia.org/wiki/Nvidia_RTX_Spark", "max_chars": 600}),
            std::path::Path::new(""),
        )
        .await;
        println!("=== fetch ok={}\n{}", p.ok, p.output);
    }

    /// `cargo test -p locaryn-agent-runtime web::reel::images -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn images() {
        let dossier = std::env::temp_dir().join("locaryn-images-essai");
        let _ = std::fs::create_dir_all(&dossier);
        for moteur in ["brave", "commons", "openverse", "bing"] {
            let debut = std::time::Instant::now();
            let r = super::exec(
                super::IMAGE_SEARCH,
                &serde_json::json!({"query": "forêt alpine lever du soleil", "engine": moteur, "max_results": 3}),
                &dossier,
            )
            .await;
            println!(
                "=== {moteur} ok={} en {:?}\n{}\n",
                r.ok,
                debut.elapsed(),
                r.output
            );
            if moteur != "brave" {
                continue;
            }
            let url = r
                .output
                .lines()
                .find_map(|l| l.trim().strip_prefix("image : "))
                .unwrap_or("")
                .to_string();
            let d = super::exec(
                super::IMAGE_DOWNLOAD,
                &serde_json::json!({"url": url, "path": "images/foret"}),
                &dossier,
            )
            .await;
            println!("=== téléchargement ok={}\n{}", d.ok, d.output);
        }
    }
}
