//! Le navigateur intégré : une vraie vue web posée dans la fenêtre, à la place
//! d'un onglet de l'espace de travail.
//!
//! La personne s'en sert comme d'un navigateur ; le modèle le pilote par les
//! outils `browser_*` (voir `app_tools`), sans extension ni morph. C'est la
//! même vue : ce que le modèle ouvre, la personne le voit, et inversement.
//!
//! La vue est un enfant natif de la fenêtre principale. Elle se dessine
//! par-dessus l'interface : l'interface lui donne sa place (`browser_show`,
//! en pixels logiques) et la cache dès qu'autre chose doit passer devant.
//!
//! Lire la page passe par `eval_with_callback`, qui rend le résultat d'un
//! script sur les trois systèmes. La page n'a aucun accès à l'application :
//! une adresse distante ne reçoit aucune capacité Tauri.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::Duration;
use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewUrl};

const LABEL: &str = "locaryn-browser";

/// L'état de la page, poussé à l'interface (barre d'adresse, titre d'onglet).
pub const EVENT: &str = "locaryn://browser";

/// Demande à l'interface d'ouvrir l'onglet du navigateur : le modèle s'en sert
/// et la personne doit voir ce qu'il fait.
pub const EVENT_OUVRIR: &str = "locaryn://workspace-open";

/// Moteur de recherche pour ce qui n'est pas une adresse.
const RECHERCHE: &str = "https://duckduckgo.com/?q=";

/// Délai maximal d'un chargement attendu par le modèle.
const ATTENTE_CHARGEMENT: Duration = Duration::from_secs(20);

/// Délai d'un script dans la page.
const ATTENTE_SCRIPT: Duration = Duration::from_secs(10);

/// Où poser la vue, en pixels logiques de la fenêtre.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Cadre {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EtatNavigateur {
    pub url: String,
    pub title: String,
    pub loading: bool,
    /// Chargements terminés depuis le lancement : le modèle attend que ce
    /// compteur avance, plutôt qu'un drapeau qu'il aurait pu lire trop tôt.
    #[serde(skip)]
    pub chargements: u64,
}

static ETAT: Mutex<EtatNavigateur> = Mutex::new(EtatNavigateur {
    url: String::new(),
    title: String::new(),
    loading: false,
    chargements: 0,
});

fn etat() -> EtatNavigateur {
    ETAT.lock().map(|e| e.clone()).unwrap_or_default()
}

fn modifier(app: &AppHandle, f: impl FnOnce(&mut EtatNavigateur)) {
    let copie = match ETAT.lock() {
        Ok(mut e) => {
            f(&mut e);
            e.clone()
        }
        Err(err) => {
            tracing::warn!(erreur = %err, "état du navigateur illisible");
            return;
        }
    };
    if let Err(e) = app.emit(EVENT, &copie) {
        tracing::warn!(erreur = %e, "état du navigateur non transmis");
    }
}

/// Ce que la personne ou le modèle a tapé, en adresse : `exemple.fr` devient
/// `https://exemple.fr`, le reste une recherche. Seuls http et https passent :
/// ni fichier local, ni `javascript:`, ni schéma de l'application.
pub fn normaliser(saisie: &str) -> Result<Url, String> {
    let s = saisie.trim();
    if s.is_empty() {
        return Err("Adresse vide.".into());
    }
    // « localhost:3000 » se lirait comme un schéma « localhost » : seul ce qui
    // porte « :// » ou un schéma connu est pris tel quel.
    let schema_explicite = s.contains("://")
        || ["javascript:", "file:", "data:", "vbscript:", "about:"]
            .iter()
            .any(|p| s.to_ascii_lowercase().starts_with(p));
    if schema_explicite {
        let url = Url::parse(s).map_err(|e| format!("Adresse illisible : {e}"))?;
        return match url.scheme() {
            "http" | "https" => Ok(url),
            autre => Err(format!(
                "Le navigateur intégré n'ouvre que des pages web (http, https), pas « {autre}: »."
            )),
        };
    }
    let ressemble_a_un_domaine = !s.contains(char::is_whitespace)
        && (s.contains('.') || s.starts_with("localhost"))
        && !s.starts_with('.');
    let texte = if ressemble_a_un_domaine {
        format!("https://{s}")
    } else {
        let mut recherche = Url::parse(RECHERCHE).map_err(|e| e.to_string())?;
        recherche.query_pairs_mut().clear().append_pair("q", s);
        return Ok(recherche);
    };
    Url::parse(&texte).map_err(|e| format!("Adresse illisible : {e}"))
}

fn vue(app: &AppHandle) -> Option<Webview> {
    app.get_webview(LABEL)
}

/// Créer la vue, cachée : l'interface la montre à sa place.
fn creer(app: &AppHandle, url: Url, cadre: Option<Cadre>) -> Result<Webview, String> {
    let fenetre = app
        .get_window("main")
        .ok_or("fenêtre principale introuvable")?;
    let cadre = cadre.unwrap_or_else(|| {
        // Le côté droit de la fenêtre, le temps que l'interface donne la vraie place.
        let taille = fenetre
            .inner_size()
            .ok()
            .zip(fenetre.scale_factor().ok())
            .map(|(t, f)| t.to_logical::<f64>(f));
        let (l, h) = taille.map_or((1200.0, 800.0), |t| (t.width, t.height));
        Cadre {
            x: l * 0.55,
            y: 80.0,
            width: l * 0.45,
            height: (h - 120.0).max(200.0),
        }
    });

    let app_chargement = app.clone();
    let app_titre = app.clone();
    let app_fenetre = app.clone();
    let constructeur = WebviewBuilder::new(LABEL, WebviewUrl::External(url))
        // Une page ne mène qu'à d'autres pages web : un lien `file:` ou vers un
        // schéma d'application n'est pas suivi.
        .on_navigation(|url| matches!(url.scheme(), "http" | "https" | "about" | "blob" | "data"))
        .on_page_load(move |_vue, charge| {
            let url = charge.url().to_string();
            let fini = matches!(charge.event(), PageLoadEvent::Finished);
            modifier(&app_chargement, |e| {
                e.url = url;
                e.loading = !fini;
                if fini {
                    e.chargements += 1;
                }
            });
        })
        .on_document_title_changed(move |_vue, titre| {
            modifier(&app_titre, |e| e.title = titre);
        })
        // Un lien qui veut une nouvelle fenêtre s'ouvre ici : une fenêtre
        // flottante hors de l'espace de travail échapperait au modèle comme à
        // la personne.
        .on_new_window(move |url, _| {
            if let Some(v) = vue(&app_fenetre) {
                if let Err(e) = v.navigate(url) {
                    tracing::warn!(erreur = %e, "nouvelle fenêtre non ouverte dans le navigateur");
                }
            }
            NewWindowResponse::Deny
        });

    let v = fenetre
        .add_child(
            constructeur,
            LogicalPosition::new(cadre.x, cadre.y),
            LogicalSize::new(cadre.width.max(1.0), cadre.height.max(1.0)),
        )
        .map_err(|e| format!("navigateur impossible à créer : {e}"))?;
    v.hide().map_err(|e| e.to_string())?;
    Ok(v)
}

fn poser(v: &Webview, cadre: Cadre) -> Result<(), String> {
    v.set_position(LogicalPosition::new(cadre.x, cadre.y))
        .map_err(|e| e.to_string())?;
    v.set_size(LogicalSize::new(
        cadre.width.max(1.0),
        cadre.height.max(1.0),
    ))
    .map_err(|e| e.to_string())
}

/// Ouvrir `saisie` : dans la vue existante, ou dans une vue créée cachée.
fn aller(app: &AppHandle, saisie: &str) -> Result<(), String> {
    let url = normaliser(saisie)?;
    match vue(app) {
        Some(v) => v.navigate(url).map_err(|e| e.to_string()),
        None => creer(app, url, None).map(|_| ()),
    }
}

// ── Commandes de l'interface ───────────────────────────────────────────────

/// Montrer la vue à sa place. Elle est créée au premier appel, sur `url` ou une
/// page vide.
///
/// Toutes ces commandes sont asynchrones : synchrones, Tauri les exécute sur le
/// fil principal, et `add_child` attend justement ce fil pour créer la vue —
/// l'application se figeait au premier appel.
#[tauri::command]
pub async fn browser_show(
    app: AppHandle,
    cadre: Cadre,
    url: Option<String>,
) -> Result<EtatNavigateur, String> {
    let v = match vue(&app) {
        Some(v) => v,
        None => {
            let depart = match url.as_deref() {
                Some(u) if !u.trim().is_empty() => normaliser(u)?,
                _ => Url::parse("about:blank").map_err(|e| e.to_string())?,
            };
            creer(&app, depart, Some(cadre))?
        }
    };
    poser(&v, cadre)?;
    v.show().map_err(|e| e.to_string())?;
    Ok(etat())
}

/// Cacher la vue : un autre onglet, une autre page, une fenêtre par-dessus.
#[tauri::command]
pub async fn browser_hide(app: AppHandle) -> Result<(), String> {
    match vue(&app) {
        Some(v) => v.hide().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

/// Suivre la place de l'onglet (redimensionnement du panneau, de la fenêtre).
#[tauri::command]
pub async fn browser_bounds(app: AppHandle, cadre: Cadre) -> Result<(), String> {
    match vue(&app) {
        Some(v) => poser(&v, cadre),
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    aller(&app, &url)
}

/// `back`, `forward` ou `reload`.
#[tauri::command]
pub async fn browser_history(app: AppHandle, action: String) -> Result<(), String> {
    let Some(v) = vue(&app) else {
        return Ok(());
    };
    match action.as_str() {
        "back" => v.eval("history.back()").map_err(|e| e.to_string()),
        "forward" => v.eval("history.forward()").map_err(|e| e.to_string()),
        "reload" => v.reload().map_err(|e| e.to_string()),
        autre => Err(format!("action inconnue : {autre}")),
    }
}

#[tauri::command]
pub async fn browser_state() -> EtatNavigateur {
    etat()
}

// ── Ce dont le modèle se sert ──────────────────────────────────────────────

/// Exécuter `script` dans la page et rendre sa valeur.
async fn evaluer(app: &AppHandle, script: &str) -> Result<serde_json::Value, String> {
    let v = vue(app).ok_or("Aucune page ouverte : commencez par browser_open.")?;
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let tx = Mutex::new(Some(tx));
    v.eval_with_callback(script, move |json| {
        if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
            // Le récepteur a pu renoncer (délai dépassé) : rien à faire de plus.
            let _ = tx.send(json);
        }
    })
    .map_err(|e| e.to_string())?;
    let json = tokio::time::timeout(ATTENTE_SCRIPT, rx)
        .await
        .map_err(|_| "La page n'a pas répondu à temps.".to_string())?
        .map_err(|_| "La page s'est fermée pendant la lecture.".to_string())?;
    let valeur: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("réponse de la page illisible : {e}"))?;
    match valeur.get("error").and_then(|e| e.as_str()) {
        Some(err) => Err(err.to_string()),
        None if valeur.is_null() => Err("La page a refusé le script.".into()),
        None => Ok(valeur),
    }
}

/// Attendre la fin d'un chargement commencé après `avant` chargements.
async fn attendre_chargement(avant: u64, delai: Duration) {
    let debut = tokio::time::Instant::now();
    while tokio::time::Instant::now() - debut < delai {
        let e = etat();
        if e.chargements > avant && !e.loading {
            // Laisser une application de page dessiner ce qu'elle charge.
            tokio::time::sleep(Duration::from_millis(400)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Le script qui lit la page : texte visible et éléments sur lesquels agir,
/// numérotés (`data-locaryn-ref`) pour `browser_click` et `browser_type`.
const SCRIPT_LECTURE: &str = r#"(() => { try {
  const visible = (e) => { const r = e.getBoundingClientRect(); const s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none"; };
  document.querySelectorAll("[data-locaryn-ref]").forEach((e) => e.removeAttribute("data-locaryn-ref"));
  const choix = 'a[href],button,input:not([type=hidden]),select,textarea,[role=button],[role=link],[role=tab],[role=menuitem],[role=checkbox],[contenteditable=true],summary';
  const elements = []; let n = 0;
  for (const e of document.querySelectorAll(choix)) {
    if (n >= MAX_ELEMENTS) break;
    if (!visible(e)) continue;
    n += 1; e.setAttribute("data-locaryn-ref", String(n));
    const tag = e.tagName.toLowerCase();
    const libelle = (e.getAttribute("aria-label") || e.innerText || e.value || e.getAttribute("placeholder")
      || e.getAttribute("title") || e.getAttribute("name") || "").trim().replace(/\s+/g, " ").slice(0, 80);
    elements.push({ ref: n, tag, type: e.getAttribute("type") || e.getAttribute("role") || "",
      label: libelle, href: tag === "a" ? String(e.href).slice(0, 160) : "" });
  }
  const tout = (document.body && document.body.innerText) || "";
  return { url: location.href, title: document.title, text: tout.replace(/\n{3,}/g, "\n\n").slice(0, MAX_TEXTE),
    truncated: tout.length > MAX_TEXTE, elements };
} catch (err) { return { error: String(err) }; } })()"#;

/// La page, mise en texte pour le modèle.
pub async fn lire(app: &AppHandle, max_texte: usize) -> Result<String, String> {
    let script = SCRIPT_LECTURE
        .replace("MAX_ELEMENTS", "150")
        .replace("MAX_TEXTE", &max_texte.to_string());
    let page = evaluer(app, &script).await?;
    let s = |k: &str| {
        page.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let mut out = format!("Page : {}\nAdresse : {}\n\n", s("title"), s("url"));
    out.push_str(&s("text"));
    if page.get("truncated").and_then(|v| v.as_bool()) == Some(true) {
        out.push_str("\n[… texte coupé …]");
    }
    let elements = page
        .get("elements")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if !elements.is_empty() {
        out.push_str("\n\nÉléments (ref — type — libellé) :\n");
        for e in elements {
            let g = |k: &str| e.get(k).and_then(|v| v.as_str()).unwrap_or_default();
            let genre = match (g("tag"), g("type")) {
                ("a", _) => "lien".to_string(),
                ("button", _) => "bouton".to_string(),
                ("select", _) => "liste".to_string(),
                ("textarea", _) => "zone de texte".to_string(),
                ("input", "") => "champ".to_string(),
                ("input", t) => format!("champ {t}"),
                (tag, "") => tag.to_string(),
                (_, role) => role.to_string(),
            };
            let reference = e.get("ref").and_then(|v| v.as_u64()).unwrap_or_default();
            out.push_str(&format!("[{reference}] {genre} — {}", g("label")));
            if !g("href").is_empty() {
                out.push_str(&format!(" → {}", g("href")));
            }
            out.push('\n');
        }
    }
    Ok(out)
}

/// Ouvrir une adresse, attendre la page, la lire.
pub async fn ouvrir(app: &AppHandle, saisie: &str) -> Result<String, String> {
    if let Err(e) = app.emit(EVENT_OUVRIR, serde_json::json!({ "tab": "browser" })) {
        tracing::warn!(erreur = %e, "onglet du navigateur non ouvert dans l'interface");
    }
    let avant = etat().chargements;
    aller(app, saisie)?;
    attendre_chargement(avant, ATTENTE_CHARGEMENT).await;
    lire(app, 12_000).await
}

/// Après une action : si elle a lancé un chargement, l'attendre, puis relire.
async fn apres_action(app: &AppHandle, avant: u64) -> Result<String, String> {
    tokio::time::sleep(Duration::from_millis(700)).await;
    if etat().loading || etat().chargements > avant {
        attendre_chargement(avant, ATTENTE_CHARGEMENT).await;
    }
    lire(app, 8_000).await
}

pub async fn cliquer(app: &AppHandle, reference: u64) -> Result<String, String> {
    let avant = etat().chargements;
    let script = format!(
        r#"(() => {{ try {{
  const e = document.querySelector('[data-locaryn-ref="{reference}"]');
  if (!e) return {{ error: "Élément {reference} introuvable : relisez la page (browser_read), les numéros changent à chaque lecture." }};
  e.scrollIntoView({{ block: "center" }}); if (e.focus) e.focus(); e.click();
  return {{ ok: true }};
}} catch (err) {{ return {{ error: String(err) }}; }} }})()"#
    );
    evaluer(app, &script).await?;
    apres_action(app, avant).await
}

pub async fn saisir(
    app: &AppHandle,
    reference: u64,
    texte: &str,
    valider: bool,
) -> Result<String, String> {
    let avant = etat().chargements;
    let texte_js = serde_json::to_string(texte).map_err(|e| e.to_string())?;
    let script = format!(
        r#"(() => {{ try {{
  const e = document.querySelector('[data-locaryn-ref="{reference}"]');
  if (!e) return {{ error: "Élément {reference} introuvable : relisez la page (browser_read), les numéros changent à chaque lecture." }};
  const texte = {texte_js};
  e.scrollIntoView({{ block: "center" }}); if (e.focus) e.focus();
  if (e.isContentEditable) {{
    e.textContent = texte; e.dispatchEvent(new InputEvent("input", {{ bubbles: true }}));
  }} else {{
    const proto = e instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype
      : e instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    const champ = Object.getOwnPropertyDescriptor(proto, "value");
    if (!champ || !champ.set) return {{ error: "Cet élément ne se remplit pas." }};
    champ.set.call(e, texte);
    e.dispatchEvent(new Event("input", {{ bubbles: true }}));
    e.dispatchEvent(new Event("change", {{ bubbles: true }}));
  }}
  if ({valider}) {{
    if (e.form && e.form.requestSubmit) e.form.requestSubmit();
    else for (const t of ["keydown", "keypress", "keyup"])
      e.dispatchEvent(new KeyboardEvent(t, {{ key: "Enter", code: "Enter", keyCode: 13, which: 13, bubbles: true }}));
  }}
  return {{ ok: true }};
}} catch (err) {{ return {{ error: String(err) }}; }} }})()"#
    );
    evaluer(app, &script).await?;
    apres_action(app, avant).await
}

/// Faire défiler la page : `down`, `up` (un écran), `top` ou `bottom`. Rend la
/// position et le texte maintenant à l'écran — la lecture complète s'arrête
/// au début d'une longue page, c'est ce qui manquait pour en voir la fin.
pub async fn defiler(app: &AppHandle, direction: &str) -> Result<String, String> {
    let geste = match direction {
        "down" => "window.scrollBy(0, window.innerHeight * 0.9)",
        "up" => "window.scrollBy(0, -window.innerHeight * 0.9)",
        "top" => "window.scrollTo(0, 0)",
        "bottom" => "window.scrollTo(0, document.documentElement.scrollHeight)",
        autre => {
            return Err(format!(
                "Direction « {autre} » inconnue : down, up, top ou bottom."
            ))
        }
    };
    let script = format!(
        r#"(() => {{ try {{
  {geste};
  const vus = []; let taille = 0;
  const marche = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  while (marche.nextNode() && taille < 6000) {{
    const t = marche.currentNode.textContent.trim();
    const parent = marche.currentNode.parentElement;
    if (!t || !parent) continue;
    const r = parent.getBoundingClientRect();
    if (r.bottom < 0 || r.top > window.innerHeight || r.width === 0) continue;
    vus.push(t); taille += t.length;
  }}
  const max = Math.max(1, document.documentElement.scrollHeight - window.innerHeight);
  return {{ position: Math.round(100 * window.scrollY / max), texte: vus.join(" ").replace(/\s+/g, " ").slice(0, 6000) }};
}} catch (err) {{ return {{ error: String(err) }}; }} }})()"#
    );
    let premier = evaluer(app, &script).await?;
    let position = premier
        .get("position")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let texte = premier
        .get("texte")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    Ok(format!(
        "Position : {position} % de la page.\n\nÀ l'écran :\n{texte}\n\n(Les numéros d'éléments de la dernière lecture restent valables ; browser_read les renumérote.)"
    ))
}

pub async fn revenir(app: &AppHandle) -> Result<String, String> {
    let avant = etat().chargements;
    browser_history(app.clone(), "back".into()).await?;
    apres_action(app, avant).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_saisie_devient_une_adresse_ou_une_recherche() {
        assert_eq!(
            normaliser("exemple.fr").unwrap().as_str(),
            "https://exemple.fr/"
        );
        assert_eq!(
            normaliser("http://localhost:3000/a").unwrap().as_str(),
            "http://localhost:3000/a"
        );
        let r = normaliser("météo à Lyon").unwrap();
        assert!(r.as_str().starts_with(RECHERCHE));
        assert!(r.query().unwrap().contains("m%C3%A9t%C3%A9o"));
    }

    #[test]
    fn seules_les_pages_web_s_ouvrent() {
        assert!(normaliser("file:///C:/Windows/win.ini").is_err());
        assert!(normaliser("javascript:alert(1)").is_err());
        assert!(normaliser("locaryn://install?src=x").is_err());
        assert!(normaliser("  ").is_err());
        assert_eq!(
            normaliser("localhost:3000").unwrap().as_str(),
            "https://localhost:3000/"
        );
    }
}
