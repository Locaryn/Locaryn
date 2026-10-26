//! Le frein des connexions par mot de passe.
//!
//! `/v1/auth/login` est public par nécessité : c'est lui qui donne le premier
//! jeton. Argon2id rend chaque essai coûteux, mais rien n'empêchait d'essayer
//! des mots de passe en boucle depuis le réseau. Ici, après quelques échecs
//! rapprochés — pour une même adresse ou un même compte — la connexion est
//! refusée un moment, et ce moment double à chaque nouvel échec.
//!
//! En mémoire seulement : un redémarrage du service remet les compteurs à
//! zéro, ce qui ne rend à un attaquant qu'une poignée d'essais.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Échecs tolérés dans la fenêtre avant le premier blocage.
const ECHECS_TOLERES: u32 = 5;
/// La fenêtre dans laquelle les échecs se cumulent.
const FENETRE: Duration = Duration::from_secs(15 * 60);
/// Le premier blocage ; il double ensuite, jusqu'au plafond.
const PREMIER_BLOCAGE: Duration = Duration::from_secs(60);
const BLOCAGE_MAX: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy)]
struct Compte {
    echecs: u32,
    premier: Instant,
    bloque_jusqu_a: Option<Instant>,
}

/// Les compteurs, par clé (`ip:…` ou `compte:…`).
#[derive(Default)]
pub struct Frein {
    cles: Mutex<HashMap<String, Compte>>,
}

impl Frein {
    fn verrou(&self) -> std::sync::MutexGuard<'_, HashMap<String, Compte>> {
        match self.cles.lock() {
            Ok(g) => g,
            Err(empoisonne) => empoisonne.into_inner(),
        }
    }

    /// Combien de temps encore la connexion est refusée pour l'une de ces
    /// clés, s'il y a lieu.
    pub fn attente(&self, cles: &[String], maintenant: Instant) -> Option<Duration> {
        let carte = self.verrou();
        cles.iter()
            .filter_map(|c| carte.get(c)?.bloque_jusqu_a)
            .filter(|fin| *fin > maintenant)
            .map(|fin| fin - maintenant)
            .max()
    }

    /// Un échec : compté pour chaque clé, et blocage au-delà du seuil.
    pub fn echec(&self, cles: &[String], maintenant: Instant) {
        let mut carte = self.verrou();
        // Le ménage des fenêtres closes, pour que la table ne grossisse pas.
        carte.retain(|_, c| {
            maintenant.duration_since(c.premier) < FENETRE
                || c.bloque_jusqu_a.is_some_and(|f| f > maintenant)
        });
        for cle in cles {
            let c = carte.entry(cle.clone()).or_insert(Compte {
                echecs: 0,
                premier: maintenant,
                bloque_jusqu_a: None,
            });
            if maintenant.duration_since(c.premier) >= FENETRE && c.bloque_jusqu_a.is_none() {
                *c = Compte {
                    echecs: 0,
                    premier: maintenant,
                    bloque_jusqu_a: None,
                };
            }
            c.echecs += 1;
            if c.echecs >= ECHECS_TOLERES {
                let rang = (c.echecs - ECHECS_TOLERES).min(6);
                let duree = (PREMIER_BLOCAGE * 2u32.pow(rang)).min(BLOCAGE_MAX);
                c.bloque_jusqu_a = Some(maintenant + duree);
            }
        }
    }

    /// Une réussite efface les compteurs de ces clés.
    pub fn reussite(&self, cles: &[String]) {
        let mut carte = self.verrou();
        for cle in cles {
            carte.remove(cle);
        }
    }
}

/// Les clés d'une tentative : l'adresse d'où elle vient, et le compte visé.
pub fn cles(ip: std::net::IpAddr, compte: &str) -> Vec<String> {
    vec![
        format!("ip:{}", ip.to_canonical()),
        format!("compte:{}", compte.trim().to_lowercase()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cinq échecs passent, le sixième essai est refusé ; une réussite
    /// remet le compteur à zéro.
    #[test]
    fn les_echecs_repetes_bloquent_puis_une_reussite_libere() {
        let frein = Frein::default();
        let t0 = Instant::now();
        let k = cles("10.0.0.7".parse().unwrap(), "Admin");
        for _ in 0..4 {
            frein.echec(&k, t0);
            assert!(frein.attente(&k, t0).is_none());
        }
        frein.echec(&k, t0);
        let attente = frein.attente(&k, t0).expect("bloqué au cinquième échec");
        assert_eq!(attente, PREMIER_BLOCAGE);
        assert!(
            frein.attente(&k, t0 + PREMIER_BLOCAGE).is_none(),
            "le blocage expire"
        );
        frein.echec(&k, t0 + PREMIER_BLOCAGE);
        assert_eq!(
            frein.attente(&k, t0 + PREMIER_BLOCAGE),
            Some(PREMIER_BLOCAGE * 2),
            "il double"
        );
        frein.reussite(&k);
        assert!(frein.attente(&k, t0 + PREMIER_BLOCAGE).is_none());
    }

    /// Le compte visé compte aussi : changer d'adresse ne contourne pas le frein.
    #[test]
    fn changer_d_adresse_ne_libere_pas_le_compte() {
        let frein = Frein::default();
        let t0 = Instant::now();
        for i in 0..5 {
            frein.echec(&cles(format!("10.0.0.{i}").parse().unwrap(), "admin"), t0);
        }
        let autre = cles("10.0.0.99".parse().unwrap(), "ADMIN ");
        assert!(frein.attente(&autre, t0).is_some());
    }
}
