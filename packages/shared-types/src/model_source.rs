//! D'où vient un modèle qu'on demande à installer.
//!
//! Trois façons de nommer la même chose circulent dans l'application : une
//! étiquette du registre Ollama (`llama3.2:3b`), un identifiant HuggingFace
//! (`unsloth/Qwen3-4B-GGUF`), ou une adresse complète. Les distinguer est la
//! première décision de toute installation, et elle doit tomber pareil partout.
//!
//! Ce module existe parce qu'elle ne tombait pas pareil. L'application de
//! bureau et le service en portaient chacun leur copie, à la virgule près ;
//! l'une testait l'étiquette Ollama avant de réécrire l'adresse, l'autre après.
//! La seconde préfixait donc `https://huggingface.co/` à `ollama/llama3.2:3b`
//! et cherchait un dépôt qui n'existe pas — le préfixe documenté par
//! l'interface ne pouvait aboutir dans l'application, tout en marchant dans le
//! service. Une seule copie ne peut plus diverger.

/// Reconnaître une étiquette du registre Ollama.
///
/// Accepte `llama3.2:3b`, `qwen3:8b-instruct`, le nom nu (`qwen3` devient
/// alors `qwen3:latest`), et la forme préfixée `ollama/…` que pose le
/// marketplace. Retourne l'étiquette normalisée `nom:version`.
///
/// Renvoie `None` pour tout ce qui n'en est pas : un identifiant HuggingFace,
/// une URL, un chemin local. Sans le préfixe explicite, une barre oblique sans
/// version est un identifiant HuggingFace — c'est ce qui garde
/// `stablediffusionapi/deliberate-v2` du bon côté.
///
/// À appeler sur l'entrée **brute**, avant toute réécriture d'adresse : une
/// normalisation qui préfixe HuggingFace dès qu'elle voit une barre oblique
/// détruit la forme `ollama/…` avant qu'on ait pu la reconnaître.
pub fn ollama_registry_tag(model: &str) -> Option<String> {
    let raw = model.trim();
    let had_prefix = raw.starts_with("ollama/");
    let raw = raw.strip_prefix("ollama/").unwrap_or(raw);
    if !had_prefix && raw.contains('/') && !raw.contains(':') {
        return None;
    }
    if raw.len() < 3 || raw.contains('\\') || raw.contains(' ') || raw.starts_with('/') {
        return None;
    }
    let (name, version) = match raw.split_once(':') {
        Some((n, v)) if n.len() >= 2 && !v.is_empty() => (n, v),
        Some(_) => return None,
        None => (raw, "latest"),
    };
    let ok = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    };
    if name.split('/').any(|part| part.is_empty() || !ok(part)) || !ok(version) {
        return None;
    }
    Some(format!("{name}:{version}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_etiquettes_ollama_sont_reconnues() {
        assert_eq!(
            ollama_registry_tag("llama3.2:3b"),
            Some("llama3.2:3b".into())
        );
        assert_eq!(
            ollama_registry_tag("qwen3:8b-instruct"),
            Some("qwen3:8b-instruct".into())
        );
        // Un nom nu vise la version courante.
        assert_eq!(ollama_registry_tag("qwen3"), Some("qwen3:latest".into()));
        assert_eq!(
            ollama_registry_tag("  mistral:7b  "),
            Some("mistral:7b".into())
        );
    }

    /// Le préfixe que pose le marketplace. C'est le cas qui échouait : réécrit
    /// en adresse HuggingFace, il ne désignait plus rien.
    #[test]
    fn le_prefixe_du_marketplace_est_accepte() {
        assert_eq!(
            ollama_registry_tag("ollama/llama3.2:3b"),
            Some("llama3.2:3b".into())
        );
        assert_eq!(
            ollama_registry_tag("ollama/deliberate-v2"),
            Some("deliberate-v2:latest".into())
        );
    }

    /// Et l'inverse : ce qui appartient à HuggingFace doit y rester.
    #[test]
    fn un_identifiant_huggingface_nest_jamais_pris_pour_une_etiquette() {
        for hf in [
            "stablediffusionapi/deliberate-v2",
            "unsloth/Qwen3-4B-GGUF",
            "hf.co/user/depot",
            "https://huggingface.co/user/depot",
            "https://huggingface.co/user/depot/resolve/main/x.gguf",
        ] {
            assert_eq!(ollama_registry_tag(hf), None, "{hf} nest pas une etiquette");
        }
    }

    #[test]
    fn les_entrees_douteuses_sont_refusees() {
        for mauvais in [
            "",
            "ab",
            "a:b",
            "qwen3:",
            ":3b",
            "/etc/passwd",
            "C:\\modeles\\x.gguf",
            "deux mots",
            "qwen3:8b!",
        ] {
            assert_eq!(
                ollama_registry_tag(mauvais),
                None,
                "{mauvais} devait etre refuse"
            );
        }
    }
}
