//! Les sous-agents : une tâche confiée par la conversation principale, qui
//! tourne en arrière-plan pendant que la conversation continue.
//!
//! La conversation choisit le modèle de chaque sous-agent (`lister_modeles`) :
//! celui chargé sur cette machine, ou un modèle d'un catalogue branché
//! (ceux qu'apportent les morphs). Le sous-agent travaille avec les mêmes outils et
//! les mêmes permissions que la conversation, sans pouvoir lancer à son tour
//! d'autres sous-agents. Son fil part vers l'interface (onglet « Tâches ») et,
//! à la fin, son compte rendu revient dans la conversation qui l'a lancé :
//! remis au modèle s'il travaille encore, sinon envoyé comme message pour qu'il
//! reprenne avec.

use crate::Core;
use futures::StreamExt;
use locaryn_agent_runtime::{Agent, AgentInput, OpenAiCompatAgent};
use locaryn_cloud_providers as cloud;
use locaryn_events::StreamEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Le nom réservé au modèle chargé sur cette machine.
pub const LOCAL: &str = "local";

/// Ce que l'interface reçoit d'un sous-agent, au fil de son travail.
#[derive(Debug, Clone, Serialize)]
pub struct EvenementSousAgent {
    pub id: String,
    /// La conversation qui l'a lancé.
    pub parent: String,
    pub nom: String,
    pub modele: String,
    /// `debut`, `jeton`, `outil`, `accord`, `fin`, `echec`.
    pub phase: &'static str,
    pub texte: String,
    /// Phase `accord` : la demande d'autorisation, telle que le fil l'émet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accord: Option<serde_json::Value>,
}

/// Où envoyer les requêtes d'un modèle choisi.
struct Cible {
    endpoint: String,
    modele: Option<String>,
    cle: Option<String>,
    libelle: String,
    /// Le client d'un autre PC Locaryn, qui croit son autorité.
    http: Option<reqwest::Client>,
}

fn hote(core: &Core) -> cloud::Host<'_> {
    cloud::Host {
        storage: &core.storage,
        data_dir: &core.data_dir,
        http: &core.http,
        keychain: core.keychain.as_ref(),
    }
}

/// Les modèles qu'un sous-agent peut utiliser, en texte pour le modèle.
pub async fn lister(core: &Core) -> String {
    let mut lignes = Vec::new();
    match core.storage.providers.active().await.ok().flatten() {
        Some(p) => {
            let nom = p
                .model
                .as_deref()
                .map(|m| m.rsplit(['/', '\\']).next().unwrap_or(m).to_string())
                .unwrap_or_else(|| "modèle actif".into());
            lignes.push(format!(
                "- `{LOCAL}` : {nom}, le modèle de cette machine (partagé avec la conversation)."
            ));
        }
        None => lignes.push(format!(
            "- `{LOCAL}` : aucun modèle actif sur cette machine."
        )),
    }
    let h = hote(core);
    for p in cloud::declared(&h).await {
        let local = p.manifest.local.is_some();
        if cloud::stored_key(&h, &p.id).is_none() && !local {
            continue;
        }
        // Une passerelle locale qui gère un compte le dit par `/auth/status` :
        // compte déconnecté, ses modèles échouent tous (« OAuth session
        // expired »). La conversation choisissait l'un d'eux pour un
        // sous-agent, qui rendait un échec au lieu d'un travail.
        if local && compte_deconnecte(&h, &p.id).await {
            lignes.push(format!(
                "- {} : compte non connecté, ses modèles ne répondront pas — ne pas le choisir.",
                p.label()
            ));
            continue;
        }
        let modeles = match cloud::models(&h, &p, false).await {
            Ok(m) => m,
            Err(e) => {
                lignes.push(format!("- {} : indisponible ({e}).", p.label()));
                continue;
            }
        };
        // Les plus utiles d'abord : ceux qui savent appeler des outils, puis
        // les gratuits. Trente suffisent pour choisir sans noyer la fenêtre.
        // Un modèle que la passerelle publie sans pouvoir le servir (son
        // fournisseur n'est pas connecté) échouerait : il n'est pas proposé.
        let mut tries: Vec<_> = modeles
            .iter()
            .filter(|m| m.available != Some(false))
            .collect();
        tries.sort_by_key(|m| {
            (
                !m.supports_tools,
                m.prompt_price_per_m.map(|x| x > 0.0).unwrap_or(true),
            )
        });
        lignes.push(format!("- {} (`{}:<modèle>`) :", p.label(), p.id));
        for m in tries.iter().take(30) {
            let mut traits = Vec::new();
            if m.supports_tools {
                traits.push("outils");
            }
            if m.prompt_price_per_m == Some(0.0) {
                traits.push("gratuit");
            }
            lignes.push(format!(
                "  - `{}:{}`{}",
                p.id,
                m.id,
                if traits.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", traits.join(", "))
                }
            ));
        }
    }
    lignes.join("\n")
}

/// Vrai seulement si la passerelle répond qu'elle n'est pas connectée. Une
/// passerelle sans compte (pas de `/auth/status`) ou injoignable n'est pas
/// jugée ici : sa liste de modèles dira si elle répond.
async fn compte_deconnecte(h: &cloud::Host<'_>, fournisseur: &str) -> bool {
    match cloud::account(h, fournisseur, "status", None).await {
        Ok(etat) => etat.get("logged_in").and_then(serde_json::Value::as_bool) == Some(false),
        Err(_) => false,
    }
}

/// Lire le fil d'un sous-agent jusqu'au bout, et dire comment il a fini.
async fn suivre<F>(mut flux: locaryn_agent_runtime::EventStream, emettre: &F)
where
    F: Fn(&'static str, String, Option<serde_json::Value>),
{
    let mut texte = String::new();
    let mut erreur: Option<String> = None;
    while let Some(ev) = flux.next().await {
        match ev {
            StreamEvent::Token { text } => {
                texte.push_str(&text);
                emettre("jeton", text, None);
            }
            StreamEvent::ToolCall { tool, .. } => emettre("outil", tool, None),
            StreamEvent::ToolApproval { .. } => {
                emettre("accord", String::new(), serde_json::to_value(&ev).ok())
            }
            StreamEvent::Log {
                level: locaryn_events::LogLevel::Error,
                msg,
                ..
            } => erreur = Some(msg),
            _ => {}
        }
    }
    let compte_rendu = locaryn_agent_runtime::compaction::sans_marqueurs(&texte);
    if compte_rendu.trim().is_empty() {
        emettre("echec", raison_du_vide(erreur, &texte), None);
    } else {
        emettre("fin", compte_rendu, None);
    }
}

/// Pourquoi un sous-agent n'a rien rendu. « N'a rien rendu », seul, ne
/// disait pas qu'un modèle de raisonnement avait épuisé son budget à penser.
fn raison_du_vide(erreur: Option<String>, texte_brut: &str) -> String {
    if let Some(e) = erreur {
        return e;
    }
    if texte_brut.trim().is_empty() {
        "le sous-agent n'a rien rendu : le modèle n'a écrit aucun texte".into()
    } else {
        "le sous-agent n'a rendu que son raisonnement, sans réponse : le modèle a sans doute          épuisé son budget de jetons à réfléchir — confiez la tâche à un autre modèle"
            .into()
    }
}

/// Un sous-agent qui travaille, et de quoi l'arrêter.
struct EnCours {
    id: String,
    nom: String,
    modele: String,
    arret: tokio::task::AbortHandle,
}

type Registre = std::sync::Mutex<std::collections::HashMap<uuid::Uuid, Vec<EnCours>>>;

/// Les sous-agents en cours, par conversation qui les a lancés.
fn registre() -> &'static Registre {
    static REGISTRE: std::sync::OnceLock<Registre> = std::sync::OnceLock::new();
    REGISTRE.get_or_init(Default::default)
}

fn verrou() -> std::sync::MutexGuard<'static, std::collections::HashMap<uuid::Uuid, Vec<EnCours>>> {
    match registre().lock() {
        Ok(g) => g,
        Err(empoisonne) => empoisonne.into_inner(),
    }
}

fn inscrire(parent: uuid::Uuid, agent: EnCours) {
    let mut carte = verrou();
    let liste = carte.entry(parent).or_default();
    liste.retain(|a| !a.arret.is_finished());
    liste.push(agent);
}

fn retirer(parent: uuid::Uuid, id: &str) {
    let mut carte = verrou();
    if let Some(liste) = carte.get_mut(&parent) {
        liste.retain(|a| a.id != id);
        if liste.is_empty() {
            carte.remove(&parent);
        }
    }
}

/// Arrêter les sous-agents d'une conversation — ce que « Stop » doit faire :
/// ils continuaient après l'arrêt de la conversation qui les avait lancés.
pub fn arreter(app: &AppHandle, parent: uuid::Uuid) -> usize {
    let agents = verrou().remove(&parent).unwrap_or_default();
    let mut arretes = 0;
    for a in agents.into_iter().filter(|a| !a.arret.is_finished()) {
        a.arret.abort();
        arretes += 1;
        let ev = EvenementSousAgent {
            id: a.id,
            parent: parent.to_string(),
            nom: a.nom,
            modele: a.modele,
            phase: "echec",
            texte: "arrêté avec la conversation".into(),
            accord: None,
        };
        if let Err(e) = app.emit("sous-agent", ev) {
            tracing::warn!(error = %e, "arrêt de sous-agent non transmis");
        }
    }
    arretes
}

/// `/equipe` : la consigne qui fait travailler la demande par plusieurs
/// sous-agents en même temps, chacun sur le modèle qui convient.
///
/// La liste des modèles est jointe : la conversation choisit d'emblée, sans
/// un tour de `lister_modeles`, et une consigne de la personne (« le cœur avec
/// tel modèle ») se respecte contre ce qui existe vraiment.
pub async fn consigne_equipe(core: &Core, demande: &str) -> String {
    let modeles = lister(core).await;
    format!(
        "[Équipe] {demande}\n\n\
         La personne a demandé que ce travail soit réparti entre plusieurs sous-agents qui \
         tournent en même temps. Ne fais pas tout toi-même :\n\
         1. Découpe la demande en 2 à 6 sous-tâches indépendantes : chacune doit pouvoir être \
         faite sans le résultat des autres.\n\
         2. Lance un sous-agent par sous-tâche avec `lancer_sous_agent`, tous dans ce même tour, \
         pour qu'ils travaillent en parallèle. Chaque tâche est complète : le sous-agent ne voit \
         pas cette conversation — donne-lui le dossier, les fichiers, les contraintes et ce qu'il \
         doit rendre.\n\
         3. Choisis le modèle de chacun dans la liste ci-dessous : les plus capables pour le cœur \
         et les décisions difficiles, les plus rapides pour le reste. Plusieurs sous-agents sur \
         `{LOCAL}` se partagent la même carte graphique : au-delà de deux, préfère un autre \
         modèle de la liste s'il y en a. Si la demande impose un modèle pour une partie, \
         respecte-le à la lettre ; s'il n'est pas dans la liste, dis-le au lieu d'en prendre un \
         autre en silence.\n\
         4. Garde pour toi l'assemblage : à l'arrivée des comptes rendus, relis-les, vérifie le \
         travail (fichiers, tests), corrige ce qui ne va pas, puis rends un bilan.\n\n\
         Modèles disponibles :\n{modeles}"
    )
}

/// La cible d'un nom de modèle : `local`, ou `<catalogue>:<modèle>`.
async fn cible(core: &Core, modele: &str) -> Result<Cible, String> {
    let modele = modele.trim();
    if modele.is_empty() || modele == LOCAL {
        let p = core
            .storage
            .providers
            .active()
            .await
            .ok()
            .flatten()
            .ok_or("aucun modèle actif sur cette machine")?;
        if crate::moteur_supervise(&p.engine) {
            if let Err(e) = core.supervisor.ensure_running(&p.engine).await {
                return Err(format!("le moteur local n'a pas démarré : {e}"));
            }
        }
        let cle = crate::cloud_providers::key_for_active_provider(core, &p);
        let http = crate::cloud_providers::http_for_active_provider(core, &p).await;
        return Ok(Cible {
            http,
            endpoint: p.endpoint.clone(),
            libelle: p
                .model
                .as_deref()
                .map(|m| m.rsplit(['/', '\\']).next().unwrap_or(m).to_string())
                .unwrap_or_else(|| LOCAL.into()),
            modele: p.model,
            cle,
        });
    }
    let (fournisseur, id) = modele.split_once(':').ok_or_else(|| {
        format!("« {modele} » : écrivez `local` ou `<catalogue>:<modèle>` (voir lister_modeles).")
    })?;
    let h = hote(core);
    let p = cloud::find(&h, fournisseur).await?;
    Ok(Cible {
        endpoint: p.manifest.api_url.clone(),
        modele: Some(id.to_string()),
        cle: cloud::stored_key(&h, &p.id),
        libelle: format!("{} · {id}", p.label()),
        // Un autre PC Locaryn : le client qui croit son autorité.
        http: p.autorite.is_some().then(|| cloud::http_for(&h, &p)),
    })
}

/// Lancer un sous-agent. Rend tout de suite le texte que lit le modèle.
pub async fn lancer(
    app: &AppHandle,
    parent: uuid::Uuid,
    tache: String,
    modele: String,
    nom: String,
) -> Result<String, String> {
    let core = app.state::<Core>();
    let cible = cible(&core, &modele).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let nom = if nom.trim().is_empty() {
        tache.chars().take(40).collect()
    } else {
        nom.trim().to_string()
    };

    // Le dossier et les permissions de la conversation qui délègue.
    let (project_id, project_path, trust) = contexte_du_parent(&core, parent).await;
    let input = AgentInput {
        system_override: None,
        session_id: uuid::Uuid::new_v4(),
        message: format!(
            "Tâche confiée par la conversation principale :\n{tache}\n\nTravaille seul jusqu'au \
             bout avec tes outils, puis termine par un compte rendu : ce que tu as fait, ce que \
             tu as trouvé, ce qui reste."
        ),
        mode: core.mode,
        model: cible.modele.clone(),
        agent: None,
        project_id,
        project_path,
        trust,
        images: Vec::new(),
        params: None,
        history: Vec::new(),
        mcp_state: Some(core.mcp.clone()),
        extra_system: None,
        git_contributeur: None,
        project_context: None,
        capabilities: crate::extensions::active_capabilities(&core).await,
        tools: None,
        // Les demandes d'accord passent par la même porte que la conversation :
        // l'interface les montre là où la personne regarde.
        approval: Some(locaryn_agent_runtime::approval::ApprovalHandle(
            std::sync::Arc::new(core.approval_gate.clone()),
        )),
        // Aucune question à la personne : elle n'a pas lancé ce travail, et un
        // sous-agent resté sur `ask_user` attendait une réponse sans fin.
        question: None,
        // Pas d'outils de l'application : un sous-agent ne relance pas
        // d'autres sous-agents, ni n'installe d'extension.
        host_tools: None,
        trim_tools: false,
        forced_tools: Vec::new(),
        require_tool_call: false,
        trust_source: Some(locaryn_agent_runtime::trust_source::TrustSourceHandle::new(
            crate::trust_source::SessionTrustSource::new(app.clone(), parent),
        )),
        mailbox: None,
        gpu: Some(crate::gpu_arbiter::handle(app)),
        bearer_token: cible.cle.clone(),
        native_chat_api: false,
    };
    let mut agent =
        OpenAiCompatAgent::with_defaults(Some(&cible.endpoint), cible.modele.as_deref());
    if let Some(http) = cible.http.clone() {
        agent = agent.with_client(http);
    }
    let flux = agent.run(input).await.map_err(|e| e.to_string())?;

    let emettre = {
        let app = app.clone();
        let (id, parent, nom, modele) = (
            id.clone(),
            parent.to_string(),
            nom.clone(),
            cible.libelle.clone(),
        );
        move |phase: &'static str, texte: String, accord: Option<serde_json::Value>| {
            let ev = EvenementSousAgent {
                id: id.clone(),
                parent: parent.clone(),
                nom: nom.clone(),
                modele: modele.clone(),
                phase,
                texte,
                accord,
            };
            if let Err(e) = app.emit("sous-agent", ev) {
                tracing::warn!(error = %e, "événement de sous-agent non transmis");
            }
        }
    };
    emettre("debut", tache.clone(), None);
    let libelle = cible.libelle.clone();
    let (id_fin, parent_fin) = (id.clone(), parent);
    let tache_de_fond = tokio::spawn(async move {
        suivre(flux, &emettre).await;
        retirer(parent_fin, &id_fin);
    });
    inscrire(
        parent,
        EnCours {
            id: id.clone(),
            nom: nom.clone(),
            modele: libelle.clone(),
            arret: tache_de_fond.abort_handle(),
        },
    );

    Ok(format!(
        "Sous-agent « {nom} » lancé (id {id}) avec {libelle}. Il travaille en arrière-plan ; son \
         compte rendu arrivera dans cette conversation. Continue en attendant."
    ))
}

/// Le dossier, le projet et les permissions de la conversation qui délègue.
async fn contexte_du_parent(
    core: &Core,
    parent: uuid::Uuid,
) -> (
    Option<uuid::Uuid>,
    Option<std::path::PathBuf>,
    Option<locaryn_shared_types::TrustLevel>,
) {
    let Ok(session) = core.storage.sessions.get(parent).await else {
        return (None, None, None);
    };
    let Ok(projet) = core.storage.projects.get(session.project_id).await else {
        return (None, None, None);
    };
    let dossier = if projet.path == crate::FREE_CHAT_PROJECT_PATH {
        locaryn_agent_runtime::free_chat_dir::resolve(
            &locaryn_config::free_chats_dir(),
            session.id,
            session.created_at,
        )
    } else {
        std::path::PathBuf::from(&projet.path)
    };
    (
        Some(session.project_id),
        Some(dossier),
        Some(session.trust_override.unwrap_or(projet.trust_level)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une erreur dit pourquoi ; sans elle, un texte fait de raisonnement seul
    /// ne se confond pas avec un silence.
    #[test]
    fn la_raison_d_un_compte_rendu_vide_est_dite() {
        assert_eq!(raison_du_vide(Some("HTTP 504".into()), "x"), "HTTP 504");
        assert!(raison_du_vide(None, "").contains("aucun texte"));
        assert!(raison_du_vide(None, "<think>je réfléchis</think>").contains("raisonnement"));
    }
}
