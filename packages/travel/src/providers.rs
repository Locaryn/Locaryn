//! Reaching a machine that sits behind a router nobody can configure.
//!
//! The tunnel is *outbound*: the computer calls a relay, the relay is what the
//! phone talks to. Nothing has to be opened on the box, which is the whole
//! point — the person this is for is in a hotel, not in front of their router.
//!
//! Three relays, because none of them suits everyone: Cloudflare needs no
//! account at all, ngrok is what many people already have, and Microsoft's dev
//! tunnels are the one an employer is most likely to permit.
//!
//! None of them ships with Lochor. Bundling somebody else's binary means
//! shipping their updates and their vulnerabilities; instead the tool is
//! detected, and if it is missing the user is told exactly how to get it.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

/// How long to wait for the relay to hand back a public address.
///
/// Generous: the first run of `cloudflared` on a slow connection genuinely
/// takes twenty seconds. Failing at five would look like a broken feature.
const URL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Cloudflare,
    Ngrok,
    DevTunnel,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Cloudflare, Provider::Ngrok, Provider::DevTunnel];

    pub fn id(&self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflare",
            Self::Ngrok => "ngrok",
            Self::DevTunnel => "devtunnel",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == s.trim().to_ascii_lowercase())
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Cloudflare => "Cloudflare",
            Self::Ngrok => "ngrok",
            Self::DevTunnel => "Tunnels Microsoft",
        }
    }

    /// The executable to look for.
    pub fn binary(&self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflared",
            Self::Ngrok => "ngrok",
            Self::DevTunnel => "devtunnel",
        }
    }

    /// What the user has to do to get it, in one sentence.
    pub fn install_hint(&self) -> &'static str {
        match self {
            Self::Cloudflare => {
                "Installez cloudflared (winget install Cloudflare.cloudflared, \
                 brew install cloudflared, ou le paquet .deb de Cloudflare). Aucun compte requis."
            }
            Self::Ngrok => {
                "Installez ngrok depuis ngrok.com, puis exécutez une fois \
                 « ngrok config add-authtoken <votre jeton> »."
            }
            Self::DevTunnel => {
                "Installez devtunnel (winget install Microsoft.devtunnel), puis \
                 exécutez une fois « devtunnel user login »."
            }
        }
    }

    /// Whether an account or a prior login is needed. Shown before the user
    /// picks, rather than discovered as a failure afterwards.
    pub fn needs_account(&self) -> bool {
        !matches!(self, Self::Cloudflare)
    }

    pub fn is_available(&self) -> bool {
        lochor_config::program_exists(self.binary())
    }

    fn args(&self, port: u16) -> Vec<String> {
        match self {
            // The daemon serves HTTPS with its own certificate, which no public
            // authority signed; without --no-tls-verify the relay refuses to
            // forward to it.
            Self::Cloudflare => vec![
                "tunnel".into(),
                "--url".into(),
                format!("https://127.0.0.1:{port}"),
                "--no-tls-verify".into(),
            ],
            Self::Ngrok => vec![
                "http".into(),
                format!("https://127.0.0.1:{port}"),
                "--log".into(),
                "stdout".into(),
                "--host-header".into(),
                "rewrite".into(),
            ],
            Self::DevTunnel => vec![
                "host".into(),
                "-p".into(),
                port.to_string(),
                "--protocol".into(),
                "https".into(),
                "--allow-anonymous".into(),
            ],
        }
    }

    /// The host suffix a genuine address from this relay ends with.
    fn domains(&self) -> &'static [&'static str] {
        match self {
            Self::Cloudflare => &["trycloudflare.com", "cfargotunnel.com"],
            Self::Ngrok => &["ngrok-free.app", "ngrok.app", "ngrok.io", "ngrok-free.dev"],
            Self::DevTunnel => &["devtunnels.ms"],
        }
    }
}

/// Pull the public address out of a line of the relay's own output.
///
/// Matching on the relay's domain rather than on "the first https:// we see"
/// matters: every one of these tools prints documentation links, update
/// notices and dashboard addresses on the way up, and any of those would be
/// happily accepted as the tunnel.
fn extract_url(provider: Provider, line: &str) -> Option<String> {
    let mut rest = line;
    while let Some(i) = rest.find("https://") {
        let candidate: String = rest[i..]
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | ',' | ')' | '\\'))
            .collect();
        let trimmed = candidate.trim_end_matches(['.', ':', ';']).to_string();
        let host = trimmed
            .trim_start_matches("https://")
            .split(['/', ':'])
            .next()
            .unwrap_or_default();
        // A *subdomain* of the relay, never the relay's own site: every one of
        // these tools prints "https://ngrok.com" or "https://trycloudflare.com"
        // in its banner, and either would be accepted as the tunnel.
        if provider
            .domains()
            .iter()
            .any(|d| host.ends_with(&format!(".{d}")))
        {
            return Some(trimmed);
        }
        rest = &rest[i + 8..];
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum TunnelError {
    #[error("{0} n'est pas installé. {1}")]
    NotInstalled(&'static str, &'static str),
    #[error("Impossible de lancer {0} : {1}")]
    Spawn(&'static str, String),
    #[error(
        "{0} n'a pas fourni d'adresse au bout d'une minute. Dernières lignes :\n{1}"
    )]
    NoUrl(&'static str, String),
    #[error("{0} s'est arrêté avant d'ouvrir le tunnel :\n{1}")]
    Exited(&'static str, String),
}

/// A running tunnel. Dropping it does *not* stop the relay — call
/// [`Tunnel::stop`] — because a tunnel torn down by an accidental drop is a
/// connection that dies mid-sentence with no explanation.
#[derive(Debug)]
pub struct Tunnel {
    pub provider: Provider,
    /// The address the phone will use.
    pub url: String,
    child: tokio::process::Child,
}

impl Tunnel {
    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
        tracing::info!(provider = self.provider.id(), "tunnel fermé");
    }
}

/// Open a tunnel to `port` and wait until the relay announces the address.
pub async fn start(provider: Provider, port: u16) -> Result<Tunnel, TunnelError> {
    if !provider.is_available() {
        return Err(TunnelError::NotInstalled(
            provider.binary(),
            provider.install_hint(),
        ));
    }

    let mut child = tokio::process::Command::new(lochor_config::resolve_program(provider.binary()))
        .args(provider.args(port))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| TunnelError::Spawn(provider.binary(), e.to_string()))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // These tools disagree about which stream carries the address —
    // cloudflared uses stderr, ngrok stdout — so read both.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(64);
    if let Some(s) = stdout {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(s).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
    }
    if let Some(s) = stderr {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(s).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);

    let mut tail: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + URL_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            let _ = child.kill().await;
            return Err(TunnelError::NoUrl(provider.binary(), tail.join("\n")));
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(line)) => {
                if let Some(url) = extract_url(provider, &line) {
                    tracing::info!(provider = provider.id(), "tunnel ouvert");
                    return Ok(Tunnel { provider, url, child });
                }
                tail.push(line);
                // Keep only what would fit in an error message.
                if tail.len() > 12 {
                    tail.remove(0);
                }
            }
            // Both streams closed: the relay gave up.
            Ok(None) => {
                let _ = child.kill().await;
                return Err(TunnelError::Exited(provider.binary(), tail.join("\n")));
            }
            Err(_) => {
                let _ = child.kill().await;
                return Err(TunnelError::NoUrl(provider.binary(), tail.join("\n")));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cloudflare_address_is_found_in_its_real_output() {
        // Copied from an actual run: the address arrives inside a box drawn
        // with plus signs and vertical bars.
        let line = "2026-08-01T12:00:00Z INF |  https://petite-chose-abcd-1234.trycloudflare.com  |";
        assert_eq!(
            extract_url(Provider::Cloudflare, line).as_deref(),
            Some("https://petite-chose-abcd-1234.trycloudflare.com")
        );
    }

    #[test]
    fn documentation_links_are_not_mistaken_for_the_tunnel() {
        // Every one of these tools prints links on the way up. Taking the
        // first https:// would hand the user a documentation page as their
        // server address.
        for line in [
            "Thank you for trying Cloudflare Tunnel. Docs: https://developers.cloudflare.com/cloudflare-one/",
            "INF Cannot determine default origin certificate path. See https://developers.cloudflare.com/argo-tunnel",
            "Visit https://dash.cloudflare.com to configure",
        ] {
            assert_eq!(extract_url(Provider::Cloudflare, line), None, "faux positif : {line}");
        }
        assert_eq!(
            extract_url(Provider::Ngrok, "Sign up at https://ngrok.com to get a token"),
            None
        );
    }

    #[test]
    fn the_bare_domain_alone_is_not_an_address() {
        // "https://trycloudflare.com" is the marketing site, not a tunnel.
        assert_eq!(extract_url(Provider::Cloudflare, "see https://trycloudflare.com"), None);
        assert_eq!(extract_url(Provider::DevTunnel, "https://devtunnels.ms"), None);
    }

    #[test]
    fn the_ngrok_json_line_is_understood() {
        let line = r#"t=2026-08-01T12:00:00+0200 lvl=info msg="started tunnel" obj=tunnels name=command_line addr=https://127.0.0.1:7474 url=https://a1b2c3d4.ngrok-free.app"#;
        assert_eq!(
            extract_url(Provider::Ngrok, line).as_deref(),
            Some("https://a1b2c3d4.ngrok-free.app")
        );
    }

    #[test]
    fn the_microsoft_line_is_understood() {
        let line = "Connect via browser: https://abc123-7474.euw.devtunnels.ms";
        assert_eq!(
            extract_url(Provider::DevTunnel, line).as_deref(),
            Some("https://abc123-7474.euw.devtunnels.ms")
        );
    }

    #[test]
    fn one_relay_does_not_accept_another_ones_address() {
        // A mixed-up provider would produce an address that simply never
        // answers, with nothing pointing at the cause.
        let line = "url=https://a1b2c3d4.ngrok-free.app";
        assert_eq!(extract_url(Provider::Cloudflare, line), None);
    }

    #[tokio::test]
    async fn a_missing_tool_is_named_along_with_the_way_to_get_it() {
        // Every provider must answer "what do I install?" before anything is
        // spawned — the alternative is a raw "program not found".
        for p in Provider::ALL {
            let hint = p.install_hint();
            assert!(hint.len() > 40, "consigne trop vague pour {}", p.id());
            assert!(
                hint.contains("Installez") || hint.contains("installez"),
                "consigne sans action pour {}",
                p.id()
            );
        }
        // And the round trip through the identifier used in configuration.
        for p in Provider::ALL {
            assert_eq!(Provider::parse(p.id()), Some(p));
        }
        assert_eq!(Provider::parse("inconnu"), None);
    }
}
