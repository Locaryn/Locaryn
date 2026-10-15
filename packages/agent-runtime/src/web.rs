//! Chercher sur le web et lire une page, sans ouvrir de navigateur.
//!
//! Le modèle n'avait que le navigateur intégré pour une simple recherche :
//! ouvrir un onglet, taper, attendre le rendu, faire défiler, capturer —
//! une dizaine d'appels lourds là où une requête HTTP suffit. Ces deux outils
//! font le travail directement : `web_search` interroge un moteur (DuckDuckGo,
//! Brave, Bing, Google, Yandex) et rend titres, adresses et extraits ;
//! `web_fetch` rend le texte d'une page.
//!
//! Le navigateur reste là pour ce qui en a vraiment besoin : une page qui se
//! construit en JavaScript, une connexion, un formulaire — ou quand la
//! personne le demande.

use crate::tools::{Risk, ToolResult, ToolSpec};
use std::time::Duration;

pub const WEB_SEARCH: &str = "web_search";
pub const WEB_FETCH: &str = "web_fetch";

/// Un navigateur ordinaire : certains moteurs servent une page vide ou un
/// défi à un client qui ne se présente pas.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

/// Les deux outils web, offerts dans toute conversation.
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
    ]
}

pub fn is_web_tool(name: &str) -> bool {
    name == WEB_SEARCH || name == WEB_FETCH
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

pub async fn exec(tool: &str, args: &serde_json::Value) -> ToolResult {
    match tool {
        WEB_SEARCH => exec_search(args).await,
        WEB_FETCH => exec_fetch(args).await,
        _ => erreur(format!("outil web inconnu : {tool}")),
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

async fn telecharger(http: &reqwest::Client, url: &str) -> Result<(String, String), String> {
    let resp = http
        .get(url)
        .header("Accept-Language", "fr-FR,fr;q=0.9,en;q=0.8")
        .header(
            "Accept",
            "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.5",
        )
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
mod reel {
    /// Contre les vrais moteurs : `cargo test -p locaryn-agent-runtime web::reel -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn chaque_moteur_repond() {
        for moteur in ["duckduckgo", "brave", "bing", "google", "yandex"] {
            let r = super::exec(
                super::WEB_SEARCH,
                &serde_json::json!({"query": "nvidia rtx spark", "engine": moteur, "max_results": 3}),
            )
            .await;
            println!("=== {moteur} ok={}\n{}\n", r.ok, r.output);
        }
        let p = super::exec(
            super::WEB_FETCH,
            &serde_json::json!({"url": "https://en.wikipedia.org/wiki/Nvidia_RTX_Spark", "max_chars": 600}),
        )
        .await;
        println!("=== fetch ok={}\n{}", p.ok, p.output);
    }
}
