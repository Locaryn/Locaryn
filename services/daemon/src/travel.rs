//! Travel mode, from the daemon's side.
//!
//! Why the daemon and not the application: the tunnel has to outlive the
//! window. The person this exists for left their computer at home; closing the
//! interface before walking out must not take their access with it.
//!
//! ## What it is allowed to do, and what it is not
//!
//! Opening a tunnel publishes this server to the internet, so it is refused
//! unless authentication is actually in force — which, on any address other
//! than loopback, it always is.
//!
//! It stops short of demanding client certificates, where forwarding a router
//! port does demand them, and the difference is deliberate. A forwarded port
//! sits on a stable address that mass scanners sweep continuously. A relay
//! address is random, unlisted, and gone when the tunnel closes. Both are
//! exposure; only one is *found* without looking.

use locaryn_travel::{link, providers, qr, Provider};
use std::sync::Arc;
use tokio::sync::Mutex;

/// What the interface needs to show, and nothing more.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TravelStatus {
    pub active: bool,
    pub provider: Option<String>,
    /// The signed `locaryn://` link a phone reads. Not the server address —
    /// showing that would be showing the very configuration we are hiding.
    pub link: Option<String>,
    /// The same link, drawn.
    pub qr_svg: Option<String>,
    /// Why it is not running, when it is not.
    pub blocker: Option<String>,
}

#[derive(Default)]
pub struct TravelState {
    inner: Mutex<Option<Running>>,
    /// Last failure, kept so the interface can explain a switch that did not
    /// take rather than silently showing "off".
    blocker: Mutex<Option<String>>,
}

struct Running {
    tunnel: providers::Tunnel,
    provider: Provider,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TravelState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Open the tunnel and mint the first pairing link.
    ///
    /// `authenticated` mirrors whether the daemon demands a token; passing it
    /// in rather than reading configuration here keeps the refusal testable.
    pub async fn start(
        &self,
        provider: Provider,
        target: Option<&providers::SshTarget>,
        port: u16,
        data_dir: &std::path::Path,
        authenticated: bool,
    ) -> Result<TravelStatus, String> {
        if !authenticated {
            let msg = "Le mode Remote exposerait ce serveur à Internet alors qu'il \
                       n'exige aucune authentification. Écoutez sur une adresse réseau \
                       (0.0.0.0) plutôt qu'en local, ce qui rend l'authentification \
                       obligatoire, puis réessayez."
                .to_string();
            *self.blocker.lock().await = Some(msg.clone());
            return Err(msg);
        }

        // Close any previous one first: two relays pointing at the same port
        // means two addresses, and the phone would be holding the stale one.
        self.stop().await;

        let tunnel = match providers::start(provider, port, target).await {
            Ok(t) => t,
            Err(e) => {
                // Kept so the interface can explain a switch that did not
                // take, instead of showing "off" with no reason.
                let msg = e.to_string();
                *self.blocker.lock().await = Some(msg.clone());
                return Err(msg);
            }
        };

        let status = self.publish(data_dir, &tunnel.url, provider)?;
        *self.inner.lock().await = Some(Running { tunnel, provider });
        *self.blocker.lock().await = None;
        Ok(status)
    }

    fn publish(
        &self,
        data_dir: &std::path::Path,
        url: &str,
        provider: Provider,
    ) -> Result<TravelStatus, String> {
        let ca = locaryn_config::mtls::authority(data_dir)
            .map_err(|e| format!("autorité locale illisible : {e}"))?;
        let uri = link::sign(
            &ca.cert_pem,
            &ca.key_pem,
            link::Mode::Travel,
            url,
            now(),
            link::DEFAULT_TTL_SECONDS,
        )
        .map_err(|e| e.to_string())?;
        let svg = qr::svg(&uri).map_err(|e| e.to_string())?;
        Ok(TravelStatus {
            active: true,
            provider: Some(provider.id().to_string()),
            link: Some(uri),
            qr_svg: Some(svg),
            blocker: None,
        })
    }

    /// A fresh link for the tunnel already open.
    ///
    /// Links expire on purpose, so the screen has to be able to mint another
    /// without tearing the tunnel down and handing out a new address.
    ///
    /// Aucun appelant pour l'instant : la route HTTP qui l'exposera n'est pas
    /// écrite. La méthode reste parce que l'expiration des liens est déjà en
    /// place — sans elle, la seule issue serait de couper le tunnel, ce qui
    /// changerait l'adresse et casserait les appareils déjà appairés.
    #[allow(dead_code)]
    pub async fn refresh_link(&self, data_dir: &std::path::Path) -> Result<TravelStatus, String> {
        let guard = self.inner.lock().await;
        let running = guard.as_ref().ok_or("Le mode Remote n'est pas actif.")?;
        self.publish(data_dir, &running.tunnel.url, running.provider)
    }

    /// The link that puts a phone back on the local address.
    ///
    /// Signed the same way, for the same reason: leaving travel mode must not
    /// be something a passer-by can trigger either.
    pub fn home_link(
        data_dir: &std::path::Path,
        local_url: &str,
    ) -> Result<(String, String), String> {
        let ca = locaryn_config::mtls::authority(data_dir)
            .map_err(|e| format!("autorité locale illisible : {e}"))?;
        let uri = link::sign(
            &ca.cert_pem,
            &ca.key_pem,
            link::Mode::Home,
            local_url,
            now(),
            link::DEFAULT_TTL_SECONDS,
        )
        .map_err(|e| e.to_string())?;
        let svg = qr::svg(&uri).map_err(|e| e.to_string())?;
        Ok((uri, svg))
    }

    /// L'adresse du tunnel en cours, s'il y en a un.
    ///
    /// Elle n'apparaît pas dans `TravelStatus` : afficher l'adresse serait
    /// afficher la configuration qu'on cherche justement à ne montrer à
    /// personne. Un code d'appairage, lui, doit bien la contenir — c'est par
    /// elle que le téléphone joindra la machine.
    pub async fn tunnel_url(&self) -> Option<String> {
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|r| r.tunnel.url.clone())
    }

    pub async fn stop(&self) {
        if let Some(r) = self.inner.lock().await.take() {
            r.tunnel.stop().await;
        }
    }

    pub async fn status(&self, data_dir: &std::path::Path) -> TravelStatus {
        let guard = self.inner.lock().await;
        match guard.as_ref() {
            Some(r) => self
                .publish(data_dir, &r.tunnel.url, r.provider)
                .unwrap_or_else(|e| TravelStatus {
                    active: true,
                    provider: Some(r.provider.id().to_string()),
                    blocker: Some(e),
                    ..Default::default()
                }),
            None => TravelStatus {
                blocker: self.blocker.lock().await.clone(),
                ..Default::default()
            },
        }
    }

    pub async fn record_blocker(&self, msg: String) {
        *self.blocker.lock().await = Some(msg);
    }
}

/// What a server with no interface prints: the code itself, in the terminal.
///
/// The address is deliberately absent. Someone reading a log over someone's
/// shoulder gains nothing, and the person who needs it points a camera at it.
pub fn announce(uri: &str, provider: Provider) {
    match qr::terminal(uri) {
        Ok(code) => {
            println!();
            println!("  Mode Remote actif via {}.", provider.label());
            println!("  Scannez ce code avec l'appareil photo du téléphone :");
            println!();
            for line in code.lines() {
                println!("  {line}");
            }
            println!();
            println!(
                "  Ce code expire dans {} minutes.",
                link::DEFAULT_TTL_SECONDS / 60
            );
            println!("  Pour en afficher un nouveau :  locaryn travel qr");
            println!();
        }
        Err(e) => tracing::warn!("code non affichable : {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_server_without_authentication_is_refused_a_tunnel() {
        // The guard has to fire before anything is spawned: publishing an
        // unauthenticated server to the internet is not a recoverable mistake.
        let state = TravelState::new();
        let err = state
            .start(
                Provider::Cloudflare,
                None,
                7474,
                std::path::Path::new("."),
                false,
            )
            .await
            .unwrap_err();
        assert!(
            err.contains("authentification"),
            "message peu clair : {err}"
        );
        // And it must say what to do about it.
        assert!(err.contains("0.0.0.0"), "message sans issue : {err}");
        assert!(!state.status(std::path::Path::new(".")).await.active);
    }

    #[tokio::test]
    async fn un_renvoi_ssh_sans_serveur_est_refuse_avant_de_lancer_quoi_que_ce_soit() {
        // Sans cible, `ssh -R` n'a rien vers quoi renvoyer. Lancer le
        // processus pour le voir echouer donnerait une erreur de ssh la ou une
        // phrase suffit.
        let state = TravelState::new();
        let err = state
            .start(Provider::Ssh, None, 7474, std::path::Path::new("."), true)
            .await
            .unwrap_err();
        assert!(err.contains("serveur"), "message peu clair : {err}");
    }

    #[test]
    fn the_home_link_round_trips_through_verification() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_travel_home_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (uri, svg) = TravelState::home_link(&dir, "https://192.168.1.10:7474").unwrap();

        let ca = locaryn_config::mtls::authority(&dir).unwrap();
        let kid = link::key_id(&ca.cert_pem).unwrap();
        let parsed =
            link::verify(&uri, &|k| (k == kid).then(|| ca.cert_pem.clone()), now()).unwrap();
        assert_eq!(parsed.mode, link::Mode::Home);
        assert_eq!(parsed.url, "https://192.168.1.10:7474");
        assert!(svg.starts_with("<svg"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn what_is_shown_never_contains_the_address_itself() {
        // The point of the whole flow is that nobody reads an IP off a screen.
        let dir = std::env::temp_dir().join(format!(
            "locaryn_travel_priv_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (uri, svg) = TravelState::home_link(&dir, "https://192.168.1.10:7474").unwrap();
        assert!(
            !uri.contains("192.168"),
            "adresse en clair dans le lien : {uri}"
        );
        assert!(!svg.contains("192.168"), "adresse en clair dans l'image");
        std::fs::remove_dir_all(&dir).ok();
    }
}
