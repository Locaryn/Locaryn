//! Sonder les modèles de conversation d'un catalogue : répondent-ils vraiment ?
//!
//! Un catalogue compatible OpenAI publie tout ce que le fournisseur connaît,
//! pas ce que le compte peut appeler. Sur le palier gratuit de NVIDIA NIM,
//! des centaines d'entrées de `/v1/models` répondent 404 ou « function not
//! found » : le sélecteur en était plein (Locaryn/Locaryn#138). La commande
//! `available_models` d'une passerelle ne dit que quel fournisseur est
//! connecté ; seule une vraie requête dit si un modèle répond.
//!
//! Chaque modèle de conversation reçoit donc, en tâche de fond, une requête
//! minimale (`max_tokens: 1`). Le verdict est gardé par fournisseur et par
//! empreinte de clé, 24 heures : une autre clé, c'est un autre palier.
//! Seul un refus net masque un modèle ; un délai dépassé, une limite de
//! débit ou une panne passagère ne disent rien, et le modèle reste proposé.

use crate::catalog::CloudModel;
use crate::{stored_key, DeclaredProvider, Host};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// Combien de temps un verdict vaut.
pub const DUREE_DE_VIE_HEURES: i64 = 24;
/// Requêtes simultanées : le palier gratuit de NIM limite le débit.
const PARALLELISME: usize = 4;
/// Un modèle qui ne répond pas en ce temps-là n'est pas jugé, il est revu.
const DELAI: Duration = Duration::from_secs(25);
/// Attente maximale demandée par une limite de débit (429).
const ATTENTE_MAX: Duration = Duration::from_secs(60);
/// Verdicts enregistrés par lot : une sonde interrompue garde son travail.
const LOT: usize = 16;

/// Le verdict d'un modèle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Verdict {
    pub ok: bool,
    /// Pourquoi il a échoué ; vide quand il répond.
    #[serde(default)]
    pub raison: String,
    /// Quand (RFC 3339).
    pub le: String,
}

impl Verdict {
    pub fn maintenant(ok: bool, raison: &str) -> Self {
        Self {
            ok,
            raison: raison.to_string(),
            le: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn frais(&self, maintenant: chrono::DateTime<chrono::Utc>) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.le)
            .map(|t| {
                maintenant.signed_duration_since(t.with_timezone(&chrono::Utc))
                    < chrono::Duration::hours(DUREE_DE_VIE_HEURES)
            })
            .unwrap_or(false)
    }
}

/// Les verdicts d'un fournisseur, pour une clé.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Sondes {
    /// Empreinte de la clé qui a servi ; jamais la clé.
    pub empreinte: String,
    pub modeles: BTreeMap<String, Verdict>,
}

impl Sondes {
    pub fn path(data_dir: &Path, provider_id: &str) -> PathBuf {
        let safe: String = provider_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        data_dir.join("cloud").join(format!("{safe}.sondes.json"))
    }

    /// Les verdicts gardés pour cette clé ; vides si la clé a changé.
    pub fn charger(data_dir: &Path, provider_id: &str, empreinte: &str) -> Self {
        let lu = std::fs::read_to_string(Self::path(data_dir, provider_id))
            .ok()
            .and_then(|t| serde_json::from_str::<Self>(&t).ok());
        match lu {
            Some(s) if s.empreinte == empreinte => s,
            _ => Self {
                empreinte: empreinte.to_string(),
                modeles: BTreeMap::new(),
            },
        }
    }

    fn enregistrer(&self, data_dir: &Path, provider_id: &str) -> std::io::Result<()> {
        let path = Self::path(data_dir, provider_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let texte = serde_json::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, texte)
    }

    /// La raison de l'échec d'un modèle, si un verdict encore frais l'a jugé.
    pub fn echec(&self, id: &str, maintenant: chrono::DateTime<chrono::Utc>) -> Option<&str> {
        self.modeles
            .get(id)
            .filter(|v| !v.ok && v.frais(maintenant))
            .map(|v| v.raison.as_str())
    }

    /// Les modèles sans verdict frais, dans l'ordre donné.
    pub fn a_sonder(
        &self,
        ids: &[String],
        maintenant: chrono::DateTime<chrono::Utc>,
    ) -> Vec<String> {
        ids.iter()
            .filter(|id| !self.modeles.get(*id).is_some_and(|v| v.frais(maintenant)))
            .cloned()
            .collect()
    }
}

/// Une empreinte courte de la clé, pour savoir qu'elle a changé.
///
/// FNV-1a réduit à 32 bits : assez pour voir un changement, beaucoup trop peu
/// pour retrouver une clé de plusieurs dizaines de caractères aléatoires.
pub fn empreinte(cle: Option<&str>) -> String {
    let Some(cle) = cle.map(str::trim).filter(|c| !c.is_empty()) else {
        return "sans-cle".into();
    };
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in cle.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:08x}", (h >> 32) as u32 ^ (h as u32))
}

/// Toutes les écritures passent par ce verrou : la sonde de fond et un échec
/// constaté en cours d'usage ne s'écrasent pas.
fn verrou_d_ecriture() -> MutexGuard<'static, ()> {
    static VERROU: Mutex<()> = Mutex::new(());
    VERROU.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ajouter des verdicts à ceux qui sont sur le disque.
pub fn noter(
    data_dir: &Path,
    provider_id: &str,
    empreinte_: &str,
    verdicts: Vec<(String, Verdict)>,
) {
    if verdicts.is_empty() {
        return;
    }
    let _garde = verrou_d_ecriture();
    let mut sondes = Sondes::charger(data_dir, provider_id, empreinte_);
    sondes.modeles.extend(verdicts);
    if let Err(e) = sondes.enregistrer(data_dir, provider_id) {
        tracing::warn!(fournisseur = %provider_id, erreur = %e, "verdicts des sondes non enregistrés");
    }
}

/// Ce qu'une réponse dit du modèle.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    Repond,
    /// Refus net : le modèle n'est pas servi à ce compte, ou ne converse pas.
    Echoue(String),
    /// Le modèle refuse le paramètre `max_tokens` lui-même : il faut redemander
    /// autrement avant de juger.
    Parametre,
    /// Rien de concluant (limite de débit, panne, clé refusée).
    Indecis,
}

/// Les mots d'un refus définitif, quel que soit le statut — une passerelle
/// relaie souvent en 500 le 404 de son fournisseur.
const MARQUES_D_ECHEC: [&str; 10] = [
    "function not found",
    "not found for account",
    "model not found",
    "model_not_found",
    "does not exist",
    "no such model",
    "unknown model",
    "no endpoints found",
    "not a chat model",
    "not supported for chat",
];

/// Le message d'erreur d'un corps de réponse, ou son début.
fn message(corps: &str) -> String {
    let lu = serde_json::from_str::<serde_json::Value>(corps).ok();
    let texte = lu
        .as_ref()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("detail"))
                .or_else(|| v.get("message"))
                .or_else(|| v.get("error"))
        })
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| corps.trim().to_string());
    texte.chars().take(200).collect()
}

/// Juger une réponse à la requête minimale.
pub fn classer(statut: u16, corps: &str) -> Issue {
    let bas = corps.to_ascii_lowercase();
    let raison = || {
        let m = message(corps);
        if m.is_empty() {
            format!("HTTP {statut}")
        } else {
            format!("HTTP {statut} : {m}")
        }
    };
    if (200..300).contains(&statut) {
        let erreur = bas.contains("\"error\"") && !bas.contains("\"choices\"");
        return if erreur {
            Issue::Echoue(raison())
        } else {
            Issue::Repond
        };
    }
    if matches!(statut, 401 | 403 | 429) {
        return Issue::Indecis;
    }
    if bas.contains("max_tokens") || bas.contains("max_completion_tokens") {
        return Issue::Parametre;
    }
    if MARQUES_D_ECHEC.iter().any(|m| bas.contains(m)) {
        return Issue::Echoue(raison());
    }
    match statut {
        400 | 404 | 405 | 410 | 422 => Issue::Echoue(raison()),
        _ => Issue::Indecis,
    }
}

/// Un échec constaté en cours d'usage est-il un refus net du modèle ?
///
/// Plus prudent que [`classer`] : sans statut fiable, seuls les mots d'un
/// modèle introuvable comptent — une fenêtre pleine n'est pas la faute du
/// modèle.
pub fn echec_du_modele(raison: &str) -> bool {
    let bas = raison.to_ascii_lowercase();
    MARQUES_D_ECHEC.iter().any(|m| bas.contains(m))
        || bas.contains("http 404")
        || bas.contains("(404")
}

/// Ce qu'il faut pour sonder, sans rien emprunter : la tâche survit à l'appel.
#[derive(Clone)]
struct Cible {
    http: reqwest::Client,
    url: String,
    cle: Option<String>,
    entetes: Vec<(String, String)>,
}

/// Une requête minimale ; `None` quand le réseau n'a rien rendu.
async fn requete(c: &Cible, modele: &str, alternative: bool) -> Option<(u16, String, Option<u64>)> {
    let mut corps = serde_json::json!({
        "model": modele,
        "messages": [{ "role": "user", "content": "ok" }],
        "stream": false,
    });
    if alternative {
        corps["max_completion_tokens"] = 16.into();
    } else {
        corps["max_tokens"] = 1.into();
    }
    let mut req = c.http.post(&c.url).timeout(DELAI).json(&corps);
    if let Some(k) = &c.cle {
        req = req.bearer_auth(k);
    }
    for (n, v) in &c.entetes {
        req = req.header(n.as_str(), v.as_str());
    }
    let r = req.send().await.ok()?;
    let statut = r.status().as_u16();
    let attente = r
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    let texte = r.text().await.unwrap_or_default();
    Some((statut, texte, attente))
}

/// Sonder un modèle : une seconde requête si le paramètre est refusé, deux
/// reprises après une limite de débit.
async fn sonder_un(c: &Cible, modele: &str) -> Issue {
    let mut alternative = false;
    for _ in 0..4 {
        let Some((statut, corps, attente)) = requete(c, modele, alternative).await else {
            return Issue::Indecis;
        };
        match (classer(statut, &corps), statut) {
            (Issue::Parametre, _) if !alternative => alternative = true,
            (Issue::Parametre, _) => return Issue::Indecis,
            (Issue::Indecis, 429) => {
                let pause = Duration::from_secs(attente.unwrap_or(15)).min(ATTENTE_MAX);
                tokio::time::sleep(pause).await;
            }
            (issue, _) => return issue,
        }
    }
    Issue::Indecis
}

/// Les fournisseurs en cours de sonde : une seule tâche à la fois chacun.
fn en_cours() -> MutexGuard<'static, HashSet<String>> {
    static ENSEMBLE: std::sync::OnceLock<Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    ENSEMBLE
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Retire le fournisseur de l'ensemble à la fin de la tâche, même en panique.
struct Inscription(String);

impl Drop for Inscription {
    fn drop(&mut self) {
        en_cours().remove(&self.0);
    }
}

/// Faut-il sonder ce fournisseur ? Pas un autre PC Locaryn : chaque requête y
/// chargerait un modèle local.
fn sondable(p: &DeclaredProvider) -> bool {
    !p.extension_id.is_nil() && !p.manifest.api_url.trim().is_empty()
}

/// Les modèles à sonder : ceux qui conversent et ne sont pas payants — une
/// requête de sonde sur un modèle facturé coûterait à la personne.
fn candidats(modeles: &[CloudModel]) -> Vec<String> {
    let mut vus = HashSet::new();
    modeles
        .iter()
        .filter(|m| m.pour_le_chat())
        .filter(|m| {
            let paye = |p: Option<f64>| p.is_some_and(|x| x > 0.0);
            !paye(m.prompt_price_per_m) && !paye(m.completion_price_per_m)
        })
        .filter(|m| vus.insert(m.id.clone()))
        .map(|m| m.id.clone())
        .collect()
}

/// Lancer, en tâche de fond, la sonde des modèles sans verdict frais.
/// `tout` revoit aussi ceux qui en ont un (« Vérifier »).
pub fn lancer(host: &Host<'_>, p: &DeclaredProvider, modeles: &[CloudModel], tout: bool) {
    if !sondable(p) {
        return;
    }
    let cle = stored_key(host, &p.id);
    let empreinte_ = empreinte(cle.as_deref());
    let ids = candidats(modeles);
    let ids = if tout {
        ids
    } else {
        Sondes::charger(host.data_dir, &p.id, &empreinte_).a_sonder(&ids, chrono::Utc::now())
    };
    let runtime = match tokio::runtime::Handle::try_current() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(fournisseur = %p.id, erreur = %e, "sonde impossible hors d'un runtime");
            return;
        }
    };
    if ids.is_empty() || !en_cours().insert(p.id.clone()) {
        return;
    }
    let cible = Cible {
        http: crate::http_for(host, p),
        url: format!(
            "{}/v1/chat/completions",
            p.manifest.api_url.trim_end_matches('/')
        ),
        cle,
        entetes: p
            .manifest
            .headers
            .iter()
            .map(|(n, v)| (n.clone(), v.clone()))
            .collect(),
    };
    let inscription = Inscription(p.id.clone());
    let data_dir = host.data_dir.to_path_buf();
    runtime.spawn(async move {
        sonder_tous(cible, ids, &data_dir, &inscription.0, &empreinte_).await;
        drop(inscription);
    });
}

/// Sonder la liste, `PARALLELISME` à la fois, en notant par lots.
async fn sonder_tous(
    cible: Cible,
    ids: Vec<String>,
    data_dir: &Path,
    provider_id: &str,
    empreinte_: &str,
) {
    let total = ids.len();
    tracing::info!(fournisseur = %provider_id, modeles = total, "sonde des modèles de conversation");
    let limite = Arc::new(tokio::sync::Semaphore::new(PARALLELISME));
    let mut taches = tokio::task::JoinSet::new();
    for id in ids {
        let (c, limite) = (cible.clone(), limite.clone());
        taches.spawn(async move {
            let _jeton = limite.acquire_owned().await.ok();
            let issue = sonder_un(&c, &id).await;
            (id, issue)
        });
    }
    let (mut lot, mut masques) = (Vec::new(), 0usize);
    while let Some(fini) = taches.join_next().await {
        let (id, issue) = match fini {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(fournisseur = %provider_id, erreur = %e, "sonde interrompue");
                continue;
            }
        };
        match issue {
            Issue::Repond => lot.push((id, Verdict::maintenant(true, ""))),
            Issue::Echoue(raison) => {
                masques += 1;
                lot.push((id, Verdict::maintenant(false, &raison)));
            }
            Issue::Parametre | Issue::Indecis => {}
        }
        if lot.len() >= LOT {
            noter(data_dir, provider_id, empreinte_, std::mem::take(&mut lot));
        }
    }
    noter(data_dir, provider_id, empreinte_, lot);
    tracing::info!(fournisseur = %provider_id, modeles = total, masques, "sonde terminée");
}

/// Poser sur chaque modèle la raison de son échec, s'il a échoué.
pub fn appliquer(host: &Host<'_>, provider_id: &str, modeles: &mut [CloudModel]) {
    let empreinte_ = empreinte(stored_key(host, provider_id).as_deref());
    let sondes = Sondes::charger(host.data_dir, provider_id, &empreinte_);
    appliquer_verdicts(&sondes, modeles, chrono::Utc::now());
}

/// La partie sans disque d'[`appliquer`].
pub fn appliquer_verdicts(
    sondes: &Sondes,
    modeles: &mut [CloudModel],
    maintenant: chrono::DateTime<chrono::Utc>,
) {
    for m in modeles {
        m.unavailable_reason = sondes.echec(&m.id, maintenant).map(str::to_string);
    }
}

/// Un modèle a échoué en cours d'usage : le marquer indisponible, s'il
/// s'agit bien d'un refus du modèle.
pub fn signaler_echec(host: &Host<'_>, provider_id: &str, modele: &str, raison: &str) {
    if modele.trim().is_empty() || !echec_du_modele(raison) {
        return;
    }
    let empreinte_ = empreinte(stored_key(host, provider_id).as_deref());
    let raison: String = raison.chars().take(200).collect();
    tracing::info!(fournisseur = %provider_id, modele, "modèle marqué indisponible après un échec");
    noter(
        host.data_dir,
        provider_id,
        &empreinte_,
        vec![(modele.to_string(), Verdict::maintenant(false, &raison))],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("locaryn-sonde-{nom}-{}", std::process::id()));
        if let Err(e) = std::fs::remove_dir_all(&d) {
            assert_eq!(e.kind(), std::io::ErrorKind::NotFound, "nettoyage : {e}");
        }
        d
    }

    /// Les refus de NIM se reconnaissent, quel que soit le statut relayé ; la
    /// limite de débit, la clé refusée et la panne ne jugent rien.
    #[test]
    fn une_reponse_se_classe() {
        assert_eq!(
            classer(200, r#"{"choices":[{"message":{}}]}"#),
            Issue::Repond
        );
        assert!(matches!(
            classer(
                404,
                r#"{"error":{"message":"Function 'abc': Not found for account 'x'"}}"#
            ),
            Issue::Echoue(_)
        ));
        assert!(matches!(
            classer(
                500,
                r#"{"error":{"message":"[nvidia] function not found"}}"#
            ),
            Issue::Echoue(_)
        ));
        assert!(matches!(
            classer(400, "model is not a chat model"),
            Issue::Echoue(_)
        ));
        assert!(matches!(
            classer(200, r#"{"error":"upstream"}"#),
            Issue::Echoue(_)
        ));
        assert_eq!(classer(429, "rate limited"), Issue::Indecis);
        assert_eq!(classer(401, "bad key"), Issue::Indecis);
        assert_eq!(classer(503, "inference error"), Issue::Indecis);
        assert_eq!(
            classer(400, "Unsupported parameter: 'max_tokens'"),
            Issue::Parametre
        );
    }

    /// La raison garde le message du fournisseur, pas tout le corps.
    #[test]
    fn la_raison_porte_le_message() {
        let Issue::Echoue(r) = classer(404, r#"{"error":{"message":"model not found"}}"#) else {
            panic!("refus attendu");
        };
        assert_eq!(r, "HTTP 404 : model not found");
    }

    /// Une fenêtre pleine n'est pas la faute du modèle ; un 404 l'est.
    #[test]
    fn un_echec_d_usage_ne_masque_que_les_refus_du_modele() {
        assert!(echec_du_modele(
            "Le moteur a refusé la requête (HTTP 404) : not found"
        ));
        assert!(echec_du_modele("Function 'x' not found for account 'y'"));
        assert!(!echec_du_modele(
            "Le moteur a refusé la requête (HTTP 400) : context size exceeded"
        ));
        assert!(!echec_du_modele("Le moteur a refusé la requête (HTTP 429)"));
    }

    /// Le verdict vaut 24 heures, puis le modèle est revu.
    #[test]
    fn un_verdict_vieillit() {
        let maintenant = chrono::Utc::now();
        let mut s = Sondes::default();
        let vieux = (maintenant - chrono::Duration::hours(DUREE_DE_VIE_HEURES + 1)).to_rfc3339();
        s.modeles
            .insert("frais".into(), Verdict::maintenant(false, "HTTP 404"));
        s.modeles.insert(
            "vieux".into(),
            Verdict {
                ok: false,
                raison: "HTTP 404".into(),
                le: vieux,
            },
        );
        s.modeles
            .insert("bon".into(), Verdict::maintenant(true, ""));
        assert_eq!(s.echec("frais", maintenant), Some("HTTP 404"));
        assert_eq!(s.echec("vieux", maintenant), None);
        assert_eq!(s.echec("bon", maintenant), None);
        let ids: Vec<String> = ["frais", "vieux", "bon", "neuf"].map(String::from).into();
        assert_eq!(s.a_sonder(&ids, maintenant), ["vieux", "neuf"]);
    }

    /// Les verdicts se gardent par clé : une autre clé repart de zéro, et
    /// l'empreinte ne contient jamais la clé.
    #[test]
    fn le_cache_suit_la_cle() {
        let d = dossier("cle");
        let a = empreinte(Some("nvapi-cle-secrete-a"));
        let b = empreinte(Some("nvapi-cle-secrete-b"));
        assert_ne!(a, b);
        assert!(!a.contains("nvapi"));
        assert_eq!(empreinte(None), empreinte(Some("  ")));

        noter(
            &d,
            "nim",
            &a,
            vec![("m".into(), Verdict::maintenant(false, "HTTP 404"))],
        );
        noter(
            &d,
            "nim",
            &a,
            vec![("n".into(), Verdict::maintenant(true, ""))],
        );
        let lu = Sondes::charger(&d, "nim", &a);
        assert_eq!(lu.modeles.len(), 2, "les lots s'ajoutent");
        assert!(Sondes::charger(&d, "nim", &b).modeles.is_empty());
        let texte = std::fs::read_to_string(Sondes::path(&d, "nim")).unwrap_or_default();
        assert!(!texte.contains("secrete"));
    }

    /// Un modèle en échec porte sa raison ; les autres non.
    #[test]
    fn les_verdicts_se_posent_sur_les_modeles() {
        let mut s = Sondes::default();
        s.modeles
            .insert("mort".into(), Verdict::maintenant(false, "HTTP 404"));
        s.modeles
            .insert("vif".into(), Verdict::maintenant(true, ""));
        let m = |id: &str| CloudModel {
            id: id.into(),
            kind: "chat".into(),
            ..Default::default()
        };
        let mut liste = vec![m("mort"), m("vif"), m("inconnu")];
        appliquer_verdicts(&s, &mut liste, chrono::Utc::now());
        assert_eq!(liste[0].unavailable_reason.as_deref(), Some("HTTP 404"));
        assert!(!liste[0].pour_le_chat());
        assert!(liste[1].pour_le_chat() && liste[2].pour_le_chat());
    }

    /// Seuls les modèles de conversation gratuits ou sans prix sont sondés,
    /// une fois chacun.
    #[test]
    fn les_candidats_a_la_sonde() {
        let m = |id: &str, kind: &str, prix: Option<f64>| CloudModel {
            id: id.into(),
            kind: kind.into(),
            prompt_price_per_m: prix,
            ..Default::default()
        };
        let liste = vec![
            m("libre", "chat", None),
            m("gratuit", "chat", Some(0.0)),
            m("paye", "chat", Some(3.0)),
            m("embed", "embedding", None),
            m("libre", "chat", None),
        ];
        assert_eq!(candidats(&liste), ["libre", "gratuit"]);
    }
}
