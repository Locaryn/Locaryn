//! Ce que le modèle actif sait recevoir, et ce qu'il sait faire.
//!
//! L'interface proposait de joindre une image et d'ouvrir un artefact quel que
//! soit le modèle chargé. Un modèle sans projecteur multimodal ne reçoit
//! pourtant jamais l'image : elle part, il n'en voit rien, et il répond à côté
//! sans que rien ne dise pourquoi. Même chose pour les outils — un modèle dont
//! le gabarit ne les mentionne pas répond en prose au lieu d'appeler, et
//! l'artefact reste vide.
//!
//! Ce module répond donc à deux questions, avec les mêmes sources que le
//! lancement, pour que l'interface ne promette rien que le moteur ne tienne.
//!
//! Ce module ne dit **pas** quels fichiers sont joignables : tous le sont. Ce
//! que l'utilisateur veut joindre, il le joint — c'est à la lecture qu'on
//! constate si le fichier porte du texte, et on le lui dit quand ce n'est pas
//! le cas. Une liste blanche d'extensions écartait des fichiers parfaitement
//! lisibles pour la seule raison qu'ils n'y figuraient pas.
//!
//! **Les images.** Le superviseur passe `--mmproj` si et seulement si un
//! projecteur accompagne les poids. C'est la même fonction qui répond ici, donc
//! la même réponse — et comme elle ne lit que le dossier, elle répond aussi
//! quand rien n'est chargé, ce qui est précisément le moment où l'utilisateur
//! ouvre la fenêtre de sélection.
//!
//! **Les outils.** La réponse est dans le gabarit de conversation. Deux
//! sources, dans cet ordre : `/props` quand le moteur tourne — c'est le gabarit
//! *effectif*, celui que llama.cpp a retenu, repli intégré compris — puis
//! l'en-tête GGUF. Aucune des deux ne répond ? On dit « inconnu » plutôt que
//! « non » : beaucoup de fichiers ne déclarent aucun gabarit, et annoncer une
//! incapacité qu'on n'a pas vérifiée est un mensonge de plus, dans l'autre sens.

use crate::Core;
use locaryn_llmfit as llmfit;
use serde::Serialize;
use tauri::State;

/// Ce qu'on sait d'une capacité — parce qu'on ne le sait pas toujours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Certitude {
    Oui,
    Non,
    /// Ni le moteur ni le fichier ne permettent de trancher.
    Inconnu,
}

impl From<Option<bool>> for Certitude {
    fn from(v: Option<bool>) -> Self {
        match v {
            Some(true) => Certitude::Oui,
            Some(false) => Certitude::Non,
            None => Certitude::Inconnu,
        }
    }
}

/// Ce que l'interface a besoin de savoir pour ne rien promettre à faux.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ModelAbilities {
    /// Le modèle actif, tel qu'enregistré. `None` si aucun n'est choisi.
    pub model: Option<String>,
    /// Le modèle accepte des images en entrée.
    pub vision: bool,
    /// Le projecteur trouvé, pour pouvoir le nommer à l'utilisateur.
    pub projector: Option<String>,
    /// Le modèle sait appeler des outils.
    pub tools: Certitude,
    /// D'où vient la réponse sur les outils : `props`, `entete`, ou `aucune`.
    pub tools_source: String,
}

/// Ce que le modèle actif accepte et sait faire.
#[tauri::command]
pub async fn model_abilities(core: State<'_, Core>) -> Result<ModelAbilities, String> {
    let active = core.storage.providers.active().await.ok().flatten();
    let model = active.as_ref().and_then(|p| p.model.clone());
    let chemin = model.as_ref().map(|m| locaryn_config::models_dir().join(m));

    let projector = chemin
        .as_deref()
        .and_then(locaryn_provider_supervisor::find_mmproj_for)
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        });

    let (tools, tools_source) = capacite_outils(&core, active.as_ref(), chemin.as_deref()).await;

    Ok(ModelAbilities {
        model,
        vision: projector.is_some(),
        projector,
        tools,
        tools_source,
    })
}

/// Le gabarit effectif d'abord, le fichier ensuite, l'aveu en dernier.
async fn capacite_outils(
    core: &Core,
    active: Option<&locaryn_shared_types::Provider>,
    chemin: Option<&std::path::Path>,
) -> (Certitude, String) {
    if let Some(p) = active {
        if let Some(gabarit) = gabarit_du_moteur(core, &p.endpoint).await {
            let geres = gabarit.contains("tools");
            return (Certitude::from(Some(geres)), "props".to_string());
        }
    }
    if let Some(path) = chemin {
        if let Ok(resume) = llmfit::gguf::read_summary(path) {
            if let Some(geres) = resume.supports_tools() {
                return (Certitude::from(Some(geres)), "entete".to_string());
            }
        }
    }
    (Certitude::Inconnu, "aucune".to_string())
}

/// Le gabarit que le moteur a réellement retenu, s'il tourne.
///
/// llama.cpp complète un fichier sans gabarit par un repli choisi sur son
/// architecture : c'est donc cette réponse-là, et non l'en-tête, qui dit ce que
/// le modèle recevra vraiment.
async fn gabarit_du_moteur(core: &Core, endpoint: &str) -> Option<String> {
    let url = format!("{}/props", endpoint.trim_end_matches('/'));
    let res = core.http.get(&url).send().await.ok()?;
    if !res.status().is_success() {
        return None;
    }
    let val: serde_json::Value = res.json().await.ok()?;
    let gabarit = val.get("chat_template").and_then(|v| v.as_str())?;
    (!gabarit.trim().is_empty()).then(|| gabarit.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_capacite_inconnue_ne_se_lit_pas_comme_un_refus() {
        assert_eq!(Certitude::from(None), Certitude::Inconnu);
        assert_eq!(Certitude::from(Some(false)), Certitude::Non);
        assert_eq!(Certitude::from(Some(true)), Certitude::Oui);
    }

    /// « inconnu » et « non » doivent rester distinguables cote interface :
    /// l'un invite a essayer, l'autre annonce que ca ne marchera pas.
    #[test]
    fn les_trois_etats_sont_distincts_en_json() {
        let rendu = |c: Certitude| serde_json::to_string(&c).unwrap();
        assert_eq!(rendu(Certitude::Oui), "\"oui\"");
        assert_eq!(rendu(Certitude::Non), "\"non\"");
        assert_eq!(rendu(Certitude::Inconnu), "\"inconnu\"");
    }
}
