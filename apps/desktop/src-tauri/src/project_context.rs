//! Le contexte d'un projet, et ce que la portée exige.
//!
//! Le stockage accepte les trois portées sans discuter — c'est le serveur
//! lui-même qui écrit les fiches partagées, il ne peut pas s'interdire de le
//! faire. La règle vit donc ici, à l'endroit qui sait si un serveur répond.
//!
//! **Deux portées sur trois n'ont de sens qu'avec un serveur.** « Suivre la
//! personne entre ses appareils » suppose un point commun entre ces appareils ;
//! « visible de tous les collaborateurs » suppose des collaborateurs. Sans
//! serveur, écrire une fiche partagée produirait une fiche que personne ne
//! verra jamais, rangée dans une base locale — c'est-à-dire exactement une
//! fiche `machine`, mais avec une étiquette qui mentirait.
//!
//! On refuse donc, en disant quoi faire. C'est plus utile qu'un enregistrement
//! silencieux dont on découvrirait l'inutilité en changeant d'ordinateur.

use locaryn_storage::project_context::{ContextEntry, ContextScope};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::Core;

/// Ce que cette installation peut faire du contexte, et pourquoi.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ContextAvailability {
    /// Les portées utilisables ici, `machine` toujours comprise.
    pub scopes: Vec<String>,
    /// Un serveur répond — le sien, ou celui auquel on est connecté.
    pub server: bool,
    /// Ce qui manque pour les deux autres portées. `None` quand rien ne manque.
    pub blocker: Option<String>,
}

/// Un serveur répond-il, d'une façon ou d'une autre ?
///
/// Deux situations valent : cette machine sert (le mode serveur est allumé), ou
/// elle est cliente d'un serveur distant. Dans les deux cas il existe un
/// endroit commun où une fiche partagée a un sens. Les distinguer n'apporterait
/// rien à l'appelant.
async fn serveur_present(core: &Core) -> bool {
    if core.remote_client().is_some() {
        return true;
    }
    crate::server_mode::server_status()
        .await
        .map(|s| s.running)
        .unwrap_or(false)
}

#[tauri::command]
pub async fn context_availability(core: State<'_, Core>) -> Result<ContextAvailability, String> {
    let server = serveur_present(&core).await;
    Ok(ContextAvailability {
        scopes: if server {
            vec!["machine".into(), "compte".into(), "partage".into()]
        } else {
            vec!["machine".into()]
        },
        server,
        blocker: if server {
            None
        } else {
            Some(
                "Le contexte du compte et le contexte partagé passent par un serveur : \
                 activez le mode serveur, ou connectez-vous à un serveur existant. \
                 Sans lui, une fiche « partagée » resterait sur cet ordinateur."
                    .into(),
            )
        },
    })
}

/// Refuse une portée que cette installation ne peut pas honorer.
async fn verifier_portee(core: &Core, scope: ContextScope) -> Result<(), String> {
    if !scope.exige_serveur() {
        return Ok(());
    }
    if serveur_present(core).await {
        return Ok(());
    }
    Err(format!(
        "La portée « {} » exige un serveur : activez le mode serveur, ou connectez-vous à un \
         serveur existant. En attendant, rangez cette fiche en « machine » — elle ne quittera \
         pas cet ordinateur, mais au moins son étiquette dira la vérité.",
        scope.as_str()
    ))
}

/// Les fiches, telles qu'elles partent devant le modèle.
///
/// Pure : c'est ce qui permet de vérifier la mise en forme sans base ni
/// serveur, et c'est là que sont les décisions qui se discutent.
///
/// Les portées ne sont pas étiquetées dans le texte. Au moment de répondre,
/// « qui a le droit de le savoir » est déjà tranché : ce qui arrive ici est ce
/// que cette personne, sur cette machine, a le droit de voir. Les distinguer
/// n'ajouterait qu'une nuance dont le modèle ne peut rien faire.
///
/// `None` quand il n'y a rien à dire — une fiche vide comprise. Un en-tête
/// seul occuperait le contexte et laisserait croire au modèle qu'un contexte
/// existe et qu'il est vide, ce qui n'est pas la même chose que pas de
/// contexte du tout.
#[must_use]
pub fn rendre_contexte(fiches: &[ContextEntry]) -> Option<String> {
    let mut corps = String::new();
    for f in fiches {
        let titre = f.title.trim();
        if titre.is_empty() {
            continue;
        }
        // Les détails s'il y en a, le résumé sinon : c'est tout ce que la
        // fiche a. Une fiche sans l'un ni l'autre n'apprend rien et se saute.
        let mut lignes: Vec<&str> = f
            .details
            .iter()
            .map(|d| d.trim())
            .filter(|d| !d.is_empty())
            .collect();
        if lignes.is_empty() {
            let resume = f.summary.trim();
            if resume.is_empty() {
                continue;
            }
            lignes.push(resume);
        }
        corps.push_str(&format!("\n## {titre}\n"));
        for l in lignes {
            corps.push_str(&format!("- {l}\n"));
        }
    }
    if corps.is_empty() {
        return None;
    }
    Some(format!(
        "# Ce projet\n\nCe que les personnes qui travaillent sur ce projet ont noté. \
         Tenez-en compte sans le répéter, et n'inventez rien qui n'y soit pas :\n{corps}"
    ))
}

/// Le contexte du projet ouvert, prêt à rejoindre le message système.
///
/// L'écran promet que Locaryn relit ces fiches avant de répondre. Sans cette
/// fonction la promesse serait fausse : les fiches existeraient, personne ne
/// les lirait, et la personne les écrirait pour rien.
pub async fn contexte_pour_le_modele(core: &Core, project_id: uuid::Uuid) -> Option<String> {
    let scopes: Vec<ContextScope> = if serveur_present(core).await {
        vec![
            ContextScope::Machine,
            ContextScope::Compte,
            ContextScope::Partage,
        ]
    } else {
        vec![ContextScope::Machine]
    };
    let fiches = core
        .storage
        .project_context
        .list(&project_id.to_string(), &scopes)
        .await
        // Un contexte illisible ne doit pas empêcher de répondre : on répond
        // sans lui plutôt que de faire échouer le tour.
        .ok()?;
    rendre_contexte(&fiches)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberContextArgs {
    pub project_id: String,
    /// `machine`, `compte` ou `partage`. Un mot inconnu retombe sur `machine`.
    pub scope: String,
    pub title: String,
    pub detail: String,
    /// `utilisateur` ou `assistant`.
    #[serde(default)]
    pub source: Option<String>,
}

#[tauri::command]
pub async fn remember_context(
    core: State<'_, Core>,
    args: RememberContextArgs,
) -> Result<ContextEntry, String> {
    let scope = ContextScope::depuis(&args.scope);
    verifier_portee(&core, scope).await?;
    // L'auteur n'est renseigné qu'en mode serveur, où plusieurs personnes
    // écrivent : sur un poste seul, il n'apprendrait rien à personne.
    let author = crate::client_cert::current_session()
        .ok()
        .flatten()
        .map(|s| s.username);
    core.storage
        .project_context
        .remember(
            &args.project_id,
            scope,
            author.as_deref(),
            &args.title,
            &args.detail,
            args.source.as_deref().unwrap_or("utilisateur"),
        )
        .await
        .map_err(|e| e.to_string())
}

/// Les fiches d'un projet.
///
/// Sans serveur, seules les fiches `machine` sont rendues : annoncer un
/// contexte partagé qu'on ne peut pas relire ferait croire à une synchronisation
/// qui n'a pas lieu.
#[tauri::command]
pub async fn list_context(
    core: State<'_, Core>,
    project_id: String,
) -> Result<Vec<ContextEntry>, String> {
    let scopes: Vec<ContextScope> = if serveur_present(&core).await {
        vec![
            ContextScope::Machine,
            ContextScope::Compte,
            ContextScope::Partage,
        ]
    } else {
        vec![ContextScope::Machine]
    };
    core.storage
        .project_context
        .list(&project_id, &scopes)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetContextScopeArgs {
    pub id: String,
    pub scope: String,
}

/// Change la portée d'une fiche.
///
/// C'est ce que la question posée à l'écran déclenche : une fiche écrite en
/// personnel se révèle concerner tout le projet, ou l'inverse.
#[tauri::command]
pub async fn set_context_scope(
    core: State<'_, Core>,
    args: SetContextScopeArgs,
) -> Result<ContextEntry, String> {
    let scope = ContextScope::depuis(&args.scope);
    verifier_portee(&core, scope).await?;
    core.storage
        .project_context
        .set_scope(&args.id, scope)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_context_summary(
    core: State<'_, Core>,
    id: String,
    summary: String,
) -> Result<(), String> {
    core.storage
        .project_context
        .set_summary(&id, &summary)
        .await
        .map_err(|e| e.to_string())
}

/// Retire un détail d'une fiche, en gardant les autres.
#[tauri::command]
pub async fn remove_context_detail(
    core: State<'_, Core>,
    id: String,
    detail: String,
) -> Result<ContextEntry, String> {
    let fiche = core
        .storage
        .project_context
        .find(&id)
        .await
        .map_err(|e| e.to_string())?;
    let restants: Vec<String> = fiche.details.into_iter().filter(|d| d != &detail).collect();
    core.storage
        .project_context
        .set_details(&id, &restants)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn forget_context(core: State<'_, Core>, id: String) -> Result<(), String> {
    core.storage
        .project_context
        .forget(&id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fiche(titre: &str, resume: &str, details: &[&str]) -> ContextEntry {
        ContextEntry {
            id: "x".into(),
            project_id: "p".into(),
            scope: ContextScope::Machine,
            author: None,
            title: titre.into(),
            summary: resume.into(),
            details: details.iter().map(|d| (*d).to_string()).collect(),
            source: "utilisateur".into(),
            created_at: "2026-09-08T00:00:00Z".into(),
            updated_at: "2026-09-08T00:00:00Z".into(),
        }
    }

    /// Sans fiche, pas d'en-tête. Un en-tête seul laisserait croire au modèle
    /// qu'un contexte existe et qu'il est vide — ce qui n'est pas la même
    /// chose que pas de contexte du tout.
    #[test]
    fn aucune_fiche_ne_pose_rien_devant_le_modele() {
        assert!(rendre_contexte(&[]).is_none());
    }

    /// Rien ne suppose du code : le format d'un rendu s'y range comme une
    /// commande de test.
    #[test]
    fn une_fiche_sans_domaine_arrive_telle_quelle() {
        let t = rendre_contexte(&[fiche(
            "Format de rendu",
            "A2",
            &["Le rendu final est en A2 sur papier grain torchon."],
        )])
        .expect("une fiche donne un contexte");
        assert!(t.contains("## Format de rendu"));
        assert!(t.contains("- Le rendu final est en A2 sur papier grain torchon."));
        assert!(
            t.contains("n'inventez rien"),
            "le modèle doit savoir quoi faire de ces lignes"
        );
    }

    /// Les détails passent devant le résumé : c'est ce qu'on a appris, et le
    /// résumé n'en est qu'une abréviation.
    #[test]
    fn les_details_passent_devant_le_resume() {
        let t = rendre_contexte(&[fiche(
            "Mesures",
            "à 20 °C",
            &[
                "Les mesures se font à 20 °C.",
                "Sinon la dilatation fausse tout.",
            ],
        )])
        .unwrap();
        assert!(t.contains("- Les mesures se font à 20 °C."));
        assert!(t.contains("- Sinon la dilatation fausse tout."));
        assert!(
            !t.contains("- à 20 °C"),
            "le résumé ne double pas les détails"
        );
    }

    /// Une fiche sans détail garde son résumé : c'est tout ce qu'elle a.
    #[test]
    fn une_fiche_sans_detail_garde_son_resume() {
        let t =
            rendre_contexte(&[fiche("Relecture", "On me relit avant d'envoyer.", &[])]).unwrap();
        assert!(t.contains("- On me relit avant d'envoyer."));
    }

    /// Une fiche qui n'apprend rien se saute, plutôt que de poser un titre nu
    /// que le modèle prendrait pour une consigne vide.
    #[test]
    fn une_fiche_muette_est_sautee() {
        assert!(rendre_contexte(&[fiche("Vide", "  ", &["  ", ""])]).is_none());
        let t = rendre_contexte(&[fiche("Vide", "", &[]), fiche("Utile", "Compte tenu.", &[])])
            .unwrap();
        assert!(!t.contains("## Vide"));
        assert!(t.contains("## Utile"));
    }

    /// Un titre vide ne fait pas un titre. Sans ce filtre, `## ` seul
    /// apparaissait dans le message système.
    #[test]
    fn un_titre_vide_ne_fait_pas_un_titre() {
        assert!(rendre_contexte(&[fiche("   ", "Quelque chose.", &[])]).is_none());
    }
}
