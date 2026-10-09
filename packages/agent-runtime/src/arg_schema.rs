//! Vérifier les arguments d'un appel d'outil contre son schéma JSON.
//!
//! Un appel faux vers un morph qui charge son propre modèle coûte cher : le
//! modèle de conversation a déjà été déchargé pour lui faire de la place, et il
//! faut le recharger juste pour lire « paramètre inconnu ». On vérifie donc
//! avant, pendant que le modèle est encore là pour se corriger.
//!
//! Le sous-ensemble couvert est celui qu'écrivent les serveurs MCP :
//! `type`, `required`, `properties`, `enum`, `minimum`/`maximum`, `items`. En
//! mode strict, une clé absente des `properties` est refusée avec le nom le
//! plus proche — `widht` n'est plus ignoré en silence.

use serde_json::Value;

/// Les arguments respectent-ils `schema` ? `Err` porte un message destiné au
/// modèle : ce qui ne va pas, et comment le corriger.
pub fn verifier(schema: &Value, args: &Value, strict: bool) -> Result<(), String> {
    let mut erreurs = Vec::new();
    valeur(schema, args, "", strict, &mut erreurs);
    if erreurs.is_empty() {
        Ok(())
    } else {
        Err(format!("arguments invalides : {}", erreurs.join(" ; ")))
    }
}

fn nom_du_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn accepte(attendu: &str, v: &Value) -> bool {
    match attendu {
        "integer" => v.as_f64().is_some_and(|f| f.fract() == 0.0),
        "number" => v.is_number(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        "null" => v.is_null(),
        _ => true,
    }
}

fn chemin(parent: &str, cle: &str) -> String {
    if parent.is_empty() {
        cle.to_string()
    } else {
        format!("{parent}.{cle}")
    }
}

fn valeur(schema: &Value, v: &Value, ou: &str, strict: bool, erreurs: &mut Vec<String>) {
    let ici = if ou.is_empty() { "l'appel" } else { ou };
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !types.is_empty() && !types.iter().any(|t| accepte(t, v)) {
        erreurs.push(format!(
            "« {ici} » doit être de type {} (reçu : {})",
            types.join(" ou "),
            nom_du_type(v)
        ));
        return;
    }
    if let Some(choix) = schema.get("enum").and_then(Value::as_array) {
        if !choix.contains(v) {
            let permis: Vec<String> = choix.iter().map(Value::to_string).collect();
            erreurs.push(format!(
                "« {ici} » doit valoir {} (reçu : {v})",
                permis.join(", ")
            ));
        }
    }
    if let Some(n) = v.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
            if n < min {
                erreurs.push(format!("« {ici} » vaut au moins {min} (reçu : {n})"));
            }
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
            if n > max {
                erreurs.push(format!("« {ici} » vaut au plus {max} (reçu : {n})"));
            }
        }
    }
    if let (Some(items), Some(liste)) = (schema.get("items"), v.as_array()) {
        for (i, element) in liste.iter().enumerate() {
            valeur(items, element, &format!("{ici}[{i}]"), strict, erreurs);
        }
    }
    let Some(objet) = v.as_object() else {
        return;
    };
    let proprietes = schema.get("properties").and_then(Value::as_object);
    if let Some(requis) = schema.get("required").and_then(Value::as_array) {
        for cle in requis.iter().filter_map(Value::as_str) {
            if objet.get(cle).is_none_or(Value::is_null) {
                erreurs.push(format!("« {} » est obligatoire", chemin(ou, cle)));
            }
        }
    }
    let Some(proprietes) = proprietes else {
        return;
    };
    for (cle, contenu) in objet {
        match proprietes.get(cle) {
            Some(sous) => {
                if !contenu.is_null() {
                    valeur(sous, contenu, &chemin(ou, cle), strict, erreurs);
                }
            }
            None if strict && !cle.starts_with("__") => {
                let proche = plus_proche(cle, proprietes.keys().map(String::as_str));
                erreurs.push(match proche {
                    Some(p) => format!(
                        "« {} » n'existe pas — vouliez-vous « {p} » ?",
                        chemin(ou, cle)
                    ),
                    None => format!(
                        "« {} » n'existe pas (paramètres possibles : {})",
                        chemin(ou, cle),
                        proprietes.keys().cloned().collect::<Vec<_>>().join(", ")
                    ),
                });
            }
            None => {}
        }
    }
}

/// Le nom connu le plus proche, s'il l'est assez pour être une faute de
/// frappe (distance d'édition au plus 2, ou l'un contient l'autre).
fn plus_proche<'a>(cle: &str, connus: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let cle_min = cle.to_lowercase();
    connus
        .map(|c| (c, distance(&cle_min, &c.to_lowercase())))
        .filter(|(c, d)| {
            *d <= 2 || c.to_lowercase().contains(&cle_min) || cle_min.contains(&c.to_lowercase())
        })
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prec: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut ligne = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cout = usize::from(ca != *cb);
            ligne.push((prec[j] + cout).min(prec[j + 1] + 1).min(ligne[j] + 1));
        }
        prec = ligne;
    }
    prec[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": { "type": "string" },
                "width": { "type": "integer", "minimum": 64, "maximum": 2048 },
                "mode": { "type": "string", "enum": ["recolor", "replace"] },
                "variants": { "type": "integer", "minimum": 1, "maximum": 8 }
            }
        })
    }

    #[test]
    fn un_appel_correct_passe() {
        assert!(verifier(&schema(), &json!({ "prompt": "a cat", "width": 512 }), true).is_ok());
        // Un entier écrit 512.0 reste un entier.
        assert!(verifier(
            &schema(),
            &json!({ "prompt": "a cat", "width": 512.0 }),
            true
        )
        .is_ok());
    }

    #[test]
    fn une_faute_de_frappe_est_nommee_avec_sa_correction() {
        let e = verifier(&schema(), &json!({ "prompt": "a cat", "widht": 512 }), true).unwrap_err();
        assert!(
            e.contains("« widht » n'existe pas — vouliez-vous « width » ?"),
            "{e}"
        );
        // Hors mode strict, une clé de plus ne gêne pas.
        assert!(verifier(
            &schema(),
            &json!({ "prompt": "a cat", "widht": 512 }),
            false
        )
        .is_ok());
    }

    #[test]
    fn requis_types_bornes_et_choix_sont_verifies() {
        let e = verifier(
            &schema(),
            &json!({ "width": "grand", "variants": 12, "mode": "repaint" }),
            true,
        )
        .unwrap_err();
        assert!(e.contains("« prompt » est obligatoire"), "{e}");
        assert!(e.contains("« width » doit être de type integer"), "{e}");
        assert!(e.contains("« variants » vaut au plus 8"), "{e}");
        assert!(e.contains("« mode » doit valoir"), "{e}");
    }

    #[test]
    fn les_cles_reservees_de_l_hote_ne_sont_pas_des_fautes() {
        assert!(verifier(
            &schema(),
            &json!({ "prompt": "x", "__locaryn_preflight": true }),
            true
        )
        .is_ok());
    }
}
