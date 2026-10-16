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
        let modeles = match cloud::models(&h, &p, false).await {
            Ok(m) => m,
            Err(e) => {
                lignes.push(format!("- {} : indisponible ({e}).", p.label()));
                continue;
            }
        };
        // Les plus utiles d'abord : ceux qui savent appeler des outils, puis
        // les gratuits. Trente suffisent pour choisir sans noyer la fenêtre.
        let mut tries: Vec<_> = modeles.iter().collect();
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
        question: Some(locaryn_agent_runtime::question::QuestionHandle(
            std::sync::Arc::new(core.attention.clone()),
        )),
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
    tauri::async_runtime::spawn(async move {
        let mut flux = flux;
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
            emettre(
                "echec",
                erreur.unwrap_or_else(|| "le sous-agent n'a rien rendu".into()),
                None,
            );
        } else {
            emettre("fin", compte_rendu, None);
        }
    });

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
