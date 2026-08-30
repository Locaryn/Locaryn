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
//! None of them ships with Locaryn. Bundling somebody else's binary means
//! shipping their updates and their vulnerabilities; instead the tool is
//! detected, and if it is missing the user is told exactly how to get it.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

/// How long to wait for the relay to hand back a public address.
///
/// Generous: the first run of `cloudflared` on a slow connection genuinely
/// takes twenty seconds. Failing at five would look like a broken feature.
const URL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// Combien de temps observer un renvoi SSH avant de le declarer ouvert.
///
/// Assez pour que l'authentification aboutisse et que le serveur refuse le
/// port s'il est deja pris ; assez court pour que l'ecran ne paraisse pas fige.
const SSH_SETTLE: std::time::Duration = std::time::Duration::from_secs(4);

/// Le serveur vers lequel un renvoi SSH pousse le port local.
///
/// Ecrit « moi@serveur.fr:8443 » : le login tel que ssh l'attend, et le port
/// que le serveur ouvrira. Le port SSH lui-meme se precise avec « /2222 » a la
/// fin, parce qu'un serveur qui n'ecoute pas sur 22 est courant et qu'echouer
/// dessus sans pouvoir le dire serait absurde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    /// `utilisateur@hote`, passe tel quel a ssh.
    pub login: String,
    /// L'hote seul, pour construire l'adresse que lira le telephone.
    pub host: String,
    /// Le port que le serveur distant ouvrira, et par lequel on le joindra.
    pub remote_port: u16,
    /// Le port sur lequel le serveur ecoute en SSH.
    pub ssh_port: u16,
}

impl SshTarget {
    /// Lire « moi@serveur.fr:8443 », ou « moi@serveur.fr:8443/2222 ».
    ///
    /// Rendre une erreur en francais plutot qu'un `None` : la personne a tape
    /// quelque chose, et savoir *ce qui manque* lui evite de deviner.
    pub fn parse(brut: &str) -> Result<Self, String> {
        let brut = brut.trim();
        if brut.is_empty() {
            return Err("Indiquez le serveur, sous la forme « moi@serveur.fr:8443 ».".into());
        }
        let (avant, ssh_port) = match brut.rsplit_once('/') {
            Some((a, p)) => (
                a,
                p.parse::<u16>()
                    .map_err(|_| format!("Port SSH illisible : « {p} »."))?,
            ),
            None => (brut, 22),
        };
        let (login, remote) = avant.rsplit_once(':').ok_or_else(|| {
            "Il manque le port que le serveur ouvrira : « moi@serveur.fr:8443 ».".to_string()
        })?;
        let remote_port = remote
            .parse::<u16>()
            .map_err(|_| format!("Port distant illisible : « {remote} »."))?;
        if remote_port == 0 {
            return Err("Le port distant ne peut pas etre 0.".into());
        }
        let host = login.rsplit_once('@').map(|(_, h)| h).unwrap_or(login);
        if host.is_empty() || !login.contains('@') {
            return Err("Il manque l'utilisateur : « moi@serveur.fr:8443 ».".into());
        }
        Ok(Self {
            login: login.to_string(),
            host: host.to_string(),
            remote_port,
            ssh_port,
        })
    }

    /// L'adresse que portera le code d'appairage.
    ///
    /// Deduite, jamais lue dans la sortie de ssh : `ssh -N` n'annonce rien
    /// quand tout va bien, et c'est le serveur qu'on a nomme qui repond.
    pub fn url(&self) -> String {
        format!("https://{}:{}", self.host, self.remote_port)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Cloudflare,
    Ngrok,
    DevTunnel,
    /// Un serveur qui vous appartient, joint par un renvoi de port SSH.
    ///
    /// Le seul relais sans tiers : rien ne transite par une entreprise, il n'y
    /// a pas de compte, pas de quota, et l'adresse est la vôtre. En échange il
    /// faut un serveur — c'est la seule entrée qui demande une cible.
    Ssh,
}

impl Provider {
    pub const ALL: [Provider; 4] = [
        Provider::Cloudflare,
        Provider::Ngrok,
        Provider::DevTunnel,
        Provider::Ssh,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflare",
            Self::Ngrok => "ngrok",
            Self::DevTunnel => "devtunnel",
            Self::Ssh => "ssh",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.id() == s.trim().to_ascii_lowercase())
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Cloudflare => "Cloudflare",
            Self::Ngrok => "ngrok",
            Self::DevTunnel => "Tunnels Microsoft",
            Self::Ssh => "Votre serveur (SSH)",
        }
    }

    /// The executable to look for.
    pub fn binary(&self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflared",
            Self::Ngrok => "ngrok",
            Self::DevTunnel => "devtunnel",
            Self::Ssh => "ssh",
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
            Self::Ssh => {
                "Un client SSH suffit — il est déjà là sur Windows 10+, macOS et Linux. \
                 Indiquez un serveur qui vous appartient, sous la forme « moi@serveur.fr:8443 ». \
                 La clé est celle de votre agent SSH : Locaryn n'en manipule aucune."
            }
        }
    }

    /// Ce relais a-t-il besoin d'une cible fournie par la personne ?
    ///
    /// Les trois autres annoncent eux-mêmes une adresse. Un renvoi SSH, non :
    /// c'est votre serveur, et personne d'autre que vous ne sait lequel.
    pub fn needs_target(&self) -> bool {
        matches!(self, Self::Ssh)
    }

    /// Whether an account or a prior login is needed. Shown before the user
    /// picks, rather than discovered as a failure afterwards.
    pub fn needs_account(&self) -> bool {
        !matches!(self, Self::Cloudflare | Self::Ssh)
    }

    pub fn is_available(&self) -> bool {
        locaryn_config::program_exists(self.binary())
    }

    fn args(&self, port: u16, target: Option<&SshTarget>) -> Vec<String> {
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
            // Un renvoi de port, rien de plus : le serveur distant ouvre son
            // port et pousse ce qu'il recoit dans la connexion deja etablie.
            //
            // `ExitOnForwardFailure` est ce qui rend l'echec lisible : sans
            // lui, un port deja pris cote serveur laisse ssh tourner avec un
            // tunnel qui ne transporte rien, et l'ecran annoncerait un succes.
            //
            // Le trafic reste chiffre de bout en bout : c'est notre TLS qui
            // traverse, pas celui d'un tiers. Le telephone verifie donc la
            // meme empreinte que sur le reseau local.
            Self::Ssh => match target {
                Some(t) => vec![
                    "-N".into(),
                    "-o".into(),
                    "ExitOnForwardFailure=yes".into(),
                    "-o".into(),
                    "ServerAliveInterval=30".into(),
                    "-o".into(),
                    "StrictHostKeyChecking=accept-new".into(),
                    "-p".into(),
                    t.ssh_port.to_string(),
                    "-R".into(),
                    format!("{}:127.0.0.1:{port}", t.remote_port),
                    t.login.clone(),
                ],
                None => Vec::new(),
            },
        }
    }

    /// The host suffix a genuine address from this relay ends with.
    fn domains(&self) -> &'static [&'static str] {
        match self {
            Self::Cloudflare => &["trycloudflare.com", "cfargotunnel.com"],
            Self::Ngrok => &["ngrok-free.app", "ngrok.app", "ngrok.io", "ngrok-free.dev"],
            Self::DevTunnel => &["devtunnels.ms"],
            // L'adresse d'un renvoi SSH n'est pas annoncee : elle se deduit du
            // serveur qu'on a nomme. Rien a reconnaitre dans la sortie.
            Self::Ssh => &[],
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
    #[error("{0} n'a pas fourni d'adresse au bout d'une minute. Dernières lignes :\n{1}")]
    NoUrl(&'static str, String),
    #[error("{0} s'est arrêté avant d'ouvrir le tunnel :\n{1}")]
    Exited(&'static str, String),
    #[error("{0} a besoin du serveur vers lequel renvoyer le port.")]
    NoTarget(&'static str),
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
///
/// `target` n'est lu que pour un renvoi SSH, ou l'adresse n'est pas annoncee
/// mais deduite du serveur qu'on a nomme.
pub async fn start(
    provider: Provider,
    port: u16,
    target: Option<&SshTarget>,
) -> Result<Tunnel, TunnelError> {
    if !provider.is_available() {
        return Err(TunnelError::NotInstalled(
            provider.binary(),
            provider.install_hint(),
        ));
    }
    if provider.needs_target() && target.is_none() {
        return Err(TunnelError::NoTarget(provider.binary()));
    }

    let mut child =
        tokio::process::Command::new(locaryn_config::resolve_program(provider.binary()))
            .args(provider.args(port, target))
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

    // ── Le renvoi SSH ne dit rien quand il marche ──
    //
    // `ssh -N` reste muet en cas de succes : il n'y a pas d'adresse a lire, et
    // attendre une ligne qui ne viendra jamais ferait echouer un tunnel qui
    // fonctionne. L'adresse se deduit du serveur nomme ; ce qu'on verifie,
    // c'est que ssh n'est pas mort — `ExitOnForwardFailure` garantit qu'il
    // meurt si le port distant est pris ou si l'authentification echoue.
    if let (Provider::Ssh, Some(t)) = (provider, target) {
        let mut tail: Vec<String> = Vec::new();
        let butoir = tokio::time::Instant::now() + SSH_SETTLE;
        loop {
            let reste = butoir.saturating_duration_since(tokio::time::Instant::now());
            if reste.is_zero() {
                tracing::info!(provider = provider.id(), "renvoi SSH ouvert");
                return Ok(Tunnel {
                    provider,
                    url: t.url(),
                    child,
                });
            }
            match tokio::time::timeout(reste, rx.recv()).await {
                // ssh ne parle que pour se plaindre : on garde tout.
                Ok(Some(line)) => {
                    tail.push(line);
                    if tail.len() > 12 {
                        tail.remove(0);
                    }
                }
                Ok(None) => {
                    let _ = child.kill().await;
                    return Err(TunnelError::Exited(
                        provider.binary(),
                        tail.join(
                            "
",
                        ),
                    ));
                }
                Err(_) => {}
            }
            if matches!(child.try_wait(), Ok(Some(_))) {
                return Err(TunnelError::Exited(
                    provider.binary(),
                    tail.join(
                        "
",
                    ),
                ));
            }
        }
    }

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
                    return Ok(Tunnel {
                        provider,
                        url,
                        child,
                    });
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
    fn une_cible_ssh_se_lit_avec_ou_sans_port_ssh() {
        let t = SshTarget::parse("moi@serveur.fr:8443").expect("cible simple");
        assert_eq!(t.login, "moi@serveur.fr");
        assert_eq!(t.host, "serveur.fr");
        assert_eq!(t.remote_port, 8443);
        assert_eq!(t.ssh_port, 22);
        assert_eq!(t.url(), "https://serveur.fr:8443");

        let t = SshTarget::parse(" moi@serveur.fr:8443/2222 ").expect("port ssh explicite");
        assert_eq!(t.ssh_port, 2222);
        assert_eq!(t.remote_port, 8443);
    }

    #[test]
    fn une_cible_incomplete_dit_ce_qui_manque() {
        // Un `None` laisserait deviner ; la personne a tape quelque chose.
        for (brut, attendu) in [
            ("", "moi@serveur.fr:8443"),
            ("moi@serveur.fr", "port que le serveur ouvrira"),
            ("serveur.fr:8443", "utilisateur"),
            ("moi@serveur.fr:zero", "Port distant illisible"),
        ] {
            let err = SshTarget::parse(brut).unwrap_err();
            assert!(err.contains(attendu), "« {brut} » a donne : {err}");
        }
    }

    #[test]
    fn le_renvoi_ssh_pousse_le_port_local_vers_le_port_distant() {
        let t = SshTarget::parse("moi@serveur.fr:8443/2222").unwrap();
        let args = Provider::Ssh.args(7474, Some(&t));
        // Le sens compte : `-R distant:127.0.0.1:local`. Inverse, ssh ouvrirait
        // un tunnel qui ne mene nulle part et n'en dirait rien.
        assert!(
            args.contains(&"8443:127.0.0.1:7474".to_string()),
            "{args:?}"
        );
        assert!(args.contains(&"ExitOnForwardFailure=yes".to_string()));
        assert_eq!(args.last().unwrap(), "moi@serveur.fr");
    }

    #[test]
    fn seul_le_renvoi_ssh_reclame_une_cible() {
        let avec: Vec<_> = Provider::ALL
            .into_iter()
            .filter(|p| p.needs_target())
            .collect();
        assert_eq!(avec, vec![Provider::Ssh]);
    }

    #[test]
    fn the_cloudflare_address_is_found_in_its_real_output() {
        // Copied from an actual run: the address arrives inside a box drawn
        // with plus signs and vertical bars.
        let line =
            "2026-08-01T12:00:00Z INF |  https://petite-chose-abcd-1234.trycloudflare.com  |";
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
            extract_url(
                Provider::Ngrok,
                "Sign up at https://ngrok.com to get a token"
            ),
            None
        );
    }

    #[test]
    fn the_bare_domain_alone_is_not_an_address() {
        // "https://trycloudflare.com" is the marketing site, not a tunnel.
        assert_eq!(
            extract_url(Provider::Cloudflare, "see https://trycloudflare.com"),
            None
        );
        assert_eq!(
            extract_url(Provider::DevTunnel, "https://devtunnels.ms"),
            None
        );
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
        // Every provider must answer "what do I do?" before anything is
        // spawned — the alternative is a raw "program not found".
        //
        // L'action n'est pas toujours « installer » : le client SSH est deja
        // la partout, et ce qui manque alors est le serveur a nommer. Ce que
        // le test tient, c'est qu'il y ait une action — pas laquelle.
        const ACTIONS: [&str; 2] = ["nstallez", "ndiquez"];
        for p in Provider::ALL {
            let hint = p.install_hint();
            assert!(hint.len() > 40, "consigne trop vague pour {}", p.id());
            assert!(
                ACTIONS.iter().any(|a| hint.contains(a)),
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
