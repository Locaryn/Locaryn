//! Le coffre : les mots de passe que le modèle utilise sans jamais les voir.
//!
//! Un mot de passe sudo, un jeton d'API, une phrase de passe : l'agent en a
//! besoin pour agir, mais il n'a aucune raison de le lire — ni de le répéter
//! dans une réponse, un journal ou un fichier. Ici :
//!
//! - la valeur vit dans le **trousseau du système** (Gestionnaire
//!   d'identification Windows, Trousseau macOS, Secret Service sous Linux) ;
//!   seul un index (noms, libellés) est écrit sur le disque ;
//! - le modèle la demande avec `ask_secret` : la personne la tape dans un champ
//!   masqué, et le modèle n'apprend que le **nom** ;
//! - il s'en sert en écrivant `{{secret:nom}}` dans les arguments d'un outil
//!   d'extension : la valeur n'est mise qu'au dernier moment, après l'accord,
//!   juste avant l'envoi au morph ;
//! - toute réponse d'outil qui la contiendrait est masquée avant que le modèle
//!   ne la lise.
//!
//! Le coffre est natif pour que chaque morph s'en serve de la même façon ; ce
//! que chaque secret ouvre reste l'affaire du morph.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use locaryn_auth::Keychain;
use serde::{Deserialize, Serialize};

/// L'outil que le modèle appelle pour faire saisir un secret.
pub const OUTIL: &str = "ask_secret";

static TROUSSEAU: OnceLock<Arc<dyn Keychain>> = OnceLock::new();

/// Les valeurs déjà lues pendant cette exécution, pour les masquer partout.
static CONNUES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Brancher le trousseau du système. Appelé une fois par l'hôte (bureau,
/// service) au démarrage ; sans lui, le coffre dit qu'il n'est pas disponible.
pub fn installer(trousseau: Arc<dyn Keychain>) {
    let _ = TROUSSEAU.set(trousseau);
}

fn trousseau() -> Result<&'static Arc<dyn Keychain>, String> {
    TROUSSEAU
        .get()
        .ok_or_else(|| "le coffre n'est pas disponible dans cet hôte".to_string())
}

fn cle(nom: &str) -> String {
    format!("locaryn/secret/{nom}")
}

/// Un nom de secret : lettres, chiffres, `.`, `_` et `-`, 64 au plus.
pub fn nom_valide(nom: &str) -> bool {
    !nom.is_empty()
        && nom.len() <= 64
        && nom
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Ce que l'on sait d'un secret sans le lire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretInfo {
    pub nom: String,
    pub libelle: String,
    pub cree_le: String,
}

fn chemin_index() -> PathBuf {
    locaryn_config::default_data_dir().join("secrets.json")
}

fn lire_index() -> Vec<SecretInfo> {
    std::fs::read_to_string(chemin_index())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn ecrire_index(index: &[SecretInfo]) -> Result<(), String> {
    let chemin = chemin_index();
    if let Some(parent) = chemin.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &chemin,
        serde_json::to_string_pretty(index).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}

/// Les secrets enregistrés : noms et libellés, jamais les valeurs.
pub fn lister() -> Vec<SecretInfo> {
    let mut index = lire_index();
    index.sort_by(|a, b| a.nom.cmp(&b.nom));
    index
}

pub fn existe(nom: &str) -> bool {
    lire_index().iter().any(|s| s.nom == nom)
}

/// Enregistrer (ou remplacer) un secret.
pub fn definir(nom: &str, valeur: &str, libelle: &str) -> Result<(), String> {
    if !nom_valide(nom) {
        return Err(format!(
            "nom de secret invalide « {nom} » : lettres, chiffres, « . », « _ » et « - » seulement"
        ));
    }
    if valeur.is_empty() {
        return Err("valeur vide".into());
    }
    trousseau()?
        .put(&cle(nom), valeur)
        .map_err(|e| format!("trousseau du système : {e}"))?;
    let mut index = lire_index();
    index.retain(|s| s.nom != nom);
    index.push(SecretInfo {
        nom: nom.to_string(),
        libelle: libelle.trim().to_string(),
        cree_le: chrono::Utc::now().to_rfc3339(),
    });
    ecrire_index(&index)?;
    retenir(nom, valeur);
    Ok(())
}

/// Effacer un secret, du trousseau et de l'index.
pub fn effacer(nom: &str) -> Result<(), String> {
    if let Ok(t) = trousseau() {
        t.delete(&cle(nom))
            .map_err(|e| format!("trousseau du système : {e}"))?;
    }
    let mut index = lire_index();
    index.retain(|s| s.nom != nom);
    ecrire_index(&index)?;
    if let Ok(mut c) = CONNUES.lock() {
        c.retain(|(n, _)| n != nom);
    }
    Ok(())
}

fn retenir(nom: &str, valeur: &str) {
    if let Ok(mut c) = CONNUES.lock() {
        c.retain(|(n, _)| n != nom);
        c.push((nom.to_string(), valeur.to_string()));
    }
}

fn lire(nom: &str) -> Result<String, String> {
    let valeur = trousseau()?.get(&cle(nom)).map_err(|_| {
        format!(
            "secret « {nom} » inconnu : demandez-le d'abord à l'utilisateur avec {OUTIL} (name=\"{nom}\")"
        )
    })?;
    retenir(nom, &valeur);
    Ok(valeur)
}

fn motif() -> &'static regex::Regex {
    static MOTIF: OnceLock<regex::Regex> = OnceLock::new();
    MOTIF.get_or_init(|| {
        regex::Regex::new(r"\{\{\s*secret:([A-Za-z0-9._-]{1,64})\s*\}\}").expect("motif valide")
    })
}

/// Les arguments contiennent-ils une référence `{{secret:…}}` ?
pub fn reference_dans(args: &serde_json::Value) -> bool {
    match args {
        serde_json::Value::String(s) => motif().is_match(s),
        serde_json::Value::Array(a) => a.iter().any(reference_dans),
        serde_json::Value::Object(o) => o.values().any(reference_dans),
        _ => false,
    }
}

/// Remplacer chaque `{{secret:nom}}` par sa valeur. Un nom inconnu est une
/// erreur dite au modèle : envoyer la référence telle quelle ferait taper
/// « {{secret:x}} » comme mot de passe.
pub fn substituer(args: &serde_json::Value) -> Result<serde_json::Value, String> {
    Ok(match args {
        serde_json::Value::String(s) if motif().is_match(s) => {
            let mut erreur = None;
            let texte = motif().replace_all(s, |c: &regex::Captures| match lire(&c[1]) {
                Ok(v) => v,
                Err(e) => {
                    erreur.get_or_insert(e);
                    String::new()
                }
            });
            if let Some(e) = erreur {
                return Err(e);
            }
            serde_json::Value::String(texte.into_owned())
        }
        serde_json::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(substituer).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(o) => serde_json::Value::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), substituer(v)?)))
                .collect::<Result<_, String>>()?,
        ),
        autre => autre.clone(),
    })
}

/// Masquer les valeurs connues dans un texte rendu au modèle ou affiché.
pub fn masquer(texte: &str) -> String {
    let Ok(connues) = CONNUES.lock() else {
        return texte.to_string();
    };
    let mut sortie = texte.to_string();
    for (nom, valeur) in connues.iter() {
        // Une valeur d'un ou deux caractères masquerait n'importe quoi.
        if valeur.chars().count() >= 3 && sortie.contains(valeur.as_str()) {
            sortie = sortie.replace(valeur.as_str(), &format!("[secret:{nom}]"));
        }
    }
    sortie
}

/// L'outil `ask_secret`, offert seulement quand un outil d'extension dit
/// accepter `{{secret:…}}` : sans lui, demander un mot de passe n'aurait
/// aucun usage, et chaque outil offert coûte du contexte.
pub fn outil_si_utile(outils: &[crate::tools::ToolSpec]) -> Option<crate::tools::ToolSpec> {
    let utile = outils.iter().any(|t| {
        t.description.contains("{{secret:") || t.input_schema.to_string().contains("{{secret:")
    });
    utile.then(outil)
}

fn outil() -> crate::tools::ToolSpec {
    crate::tools::ToolSpec {
        name: OUTIL.into(),
        description: "Have the user type a password or secret (sudo password, API token…) into a masked field. You never see the value: it goes to the system keychain, and you only learn its name. Then pass `{{secret:NAME}}` in the argument of an extension tool that needs it — the real value is put there at the last moment. If the secret is already stored, this returns at once without asking. Never ask the user to type a password in the chat.".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Stable name: letters, digits, . _ - (for example wsl-sudo-Ubuntu)."
                },
                "label": {
                    "type": "string",
                    "description": "What it is, for the user, in their language (for example « Mot de passe sudo d'Ubuntu »)."
                },
                "reason": {
                    "type": "string",
                    "description": "Why it is needed now, in one sentence."
                },
                "replace": {
                    "type": "boolean",
                    "description": "true to ask again when the stored one is wrong."
                }
            },
            "required": ["name", "label"]
        }),
        // Rien n'est exécuté : c'est une question, posée à l'écran.
        risk: crate::tools::Risk::Low,
        required_permissions: Vec::new(),
    }
}

/// La demande lue dans l'appel du modèle.
#[derive(Debug, Clone)]
pub struct Demande {
    pub nom: String,
    pub libelle: String,
    pub raison: Option<String>,
    pub remplacer: bool,
}

pub fn lire_demande(args: &serde_json::Value) -> Result<Demande, String> {
    let texte = |c: &str| {
        args.get(c)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let nom = texte("name").ok_or("`name` est obligatoire")?;
    if !nom_valide(&nom) {
        return Err(format!(
            "nom invalide « {nom} » : lettres, chiffres, « . », « _ » et « - » seulement"
        ));
    }
    Ok(Demande {
        libelle: texte("label").unwrap_or_else(|| nom.clone()),
        raison: texte("reason"),
        remplacer: args.get("replace").and_then(|v| v.as_bool()) == Some(true),
        nom,
    })
}

/// Ce que le modèle lit une fois le secret enregistré.
pub fn pret(nom: &str) -> String {
    format!(
        "Le secret « {nom} » est dans le trousseau. Écrivez {{{{secret:{nom}}}}} dans l'argument de l'outil qui en a besoin : sa valeur y sera mise sans que vous la voyiez. Ne la demandez pas dans la conversation."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use locaryn_auth::KeychainError;
    use std::collections::HashMap;

    struct Memoire(Mutex<HashMap<String, String>>);
    impl Keychain for Memoire {
        fn put(&self, k: &str, v: &str) -> Result<(), KeychainError> {
            self.0.lock().unwrap().insert(k.into(), v.into());
            Ok(())
        }
        fn get(&self, k: &str) -> Result<String, KeychainError> {
            self.0
                .lock()
                .unwrap()
                .get(k)
                .cloned()
                .ok_or_else(|| KeychainError::NotFound(k.into()))
        }
        fn delete(&self, k: &str) -> Result<(), KeychainError> {
            self.0.lock().unwrap().remove(k);
            Ok(())
        }
    }

    #[test]
    fn le_modele_ecrit_une_reference_et_ne_lit_jamais_la_valeur() {
        installer(Arc::new(Memoire(Mutex::new(HashMap::from([(
            cle("wsl-sudo-Ubuntu"),
            "motdepasse-42".to_string(),
        )])))));
        let args = serde_json::json!({
            "command": "sudo apt update",
            "sudo_password": "{{secret:wsl-sudo-Ubuntu}}",
            "liste": ["{{ secret:wsl-sudo-Ubuntu }}", 3]
        });
        assert!(reference_dans(&args));
        let pleins = substituer(&args).unwrap();
        assert_eq!(pleins["sudo_password"], "motdepasse-42");
        assert_eq!(pleins["liste"][0], "motdepasse-42");
        assert_eq!(pleins["command"], "sudo apt update");
        // Une sortie qui la répète est masquée.
        assert_eq!(
            masquer("[sudo] motdepasse-42 refusé"),
            "[sudo] [secret:wsl-sudo-Ubuntu] refusé"
        );
        // Un nom inconnu n'envoie rien.
        let e = substituer(&serde_json::json!({ "p": "{{secret:absent}}" })).unwrap_err();
        assert!(e.contains(OUTIL));
    }

    #[test]
    fn les_noms_restent_simples() {
        assert!(nom_valide("wsl-sudo-Ubuntu-22.04"));
        assert!(!nom_valide(""));
        assert!(!nom_valide("a b"));
        assert!(!nom_valide("../x/y"));
        assert!(lire_demande(&serde_json::json!({ "name": "x y", "label": "l" })).is_err());
    }
}
