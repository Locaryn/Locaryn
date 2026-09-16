//! `/v1/pairing` — le code à photographier pour qu'un téléphone connaisse ce
//! serveur.
//!
//! Trois façons de joindre une machine, donc trois codes. Ils ne diffèrent que
//! par l'adresse qu'ils portent, et ce qu'ils portent d'autre est identique :
//! l'autorité du déploiement. C'est elle qui permet ensuite au téléphone de
//! vérifier un certificat renouvelé et de reconnaître un lien de mode Remote —
//! une adresse tapée à la main ne l'apporte pas, et l'interface le dit.

use crate::DaemonState;
use axum::{
    extract::{ConnectInfo, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::net::SocketAddr;
use std::sync::Arc;

/// L'appelant est-il sur cette machine ?
///
/// C'est la frontiere qui protege le code de confirmation. Le QR ne porte rien
/// de secret — une adresse et une autorite publique — et se donne a qui le
/// demande. Le code, lui, est le second facteur : sa raison d'etre est d'etre
/// *vu sur l'ecran de l'hote*. Le servir a un appelant du reseau le viderait
/// de son sens, puisque `/v1/auth/pair/confirm` est lui aussi ouvert : qui
/// pourrait lire le code n'aurait plus qu'a le renvoyer.
///
/// `to_canonical` avant `is_loopback` : une adresse IPv4 mappee en IPv6
/// (`::ffff:127.0.0.1`) n'est pas reconnue comme boucle locale telle quelle, et
/// c'est sous cette forme qu'arrive une connexion IPv4 sur une ecoute IPv6.
///
/// Ne vaut que parce que ce service est joint directement. Derriere un proxy
/// inverse, le pair serait le proxy, et cette fonction dirait « local » pour
/// tout le monde : ce deploiement-la devrait couper l'appairage par code.
fn sur_cette_machine(pair: SocketAddr) -> bool {
    pair.ip().to_canonical().is_loopback()
}

/// Un code d'appairage en attente de consommation.
///
/// Le QR porte l'adresse ; le code porte la preuve que celui qui scanne a
/// l'écran de l'hôte sous les yeux. Deux minutes, un seul essai : c'est le
/// second facteur de l'appairage.
pub struct PendingPairing {
    pub code: String,
    pub created_at: std::time::Instant,
    pub attempts: u32,
    /// L'appareil qui a scanne et joint ce serveur, quand il l'a fait.
    ///
    /// Tant que c'est `None`, personne n'essaie : le code n'a donc aucune
    /// raison d'etre a l'ecran. C'est ce qui permet de ne l'afficher qu'au
    /// moment ou quelqu'un en a besoin, et de nommer qui.
    pub annonce: Option<Annonce>,
}

/// Un appareil qui vient de scanner le QR et qui joint ce serveur.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Annonce {
    /// Le nom que l'appareil se donne. Non verifiable : c'est une etiquette
    /// pour reconnaitre son propre telephone, pas une preuve d'identite.
    pub device: String,
    /// L'adresse d'ou vient la demande. Elle, on l'a constatee.
    pub ip: String,
    /// Quand l'annonce est arrivee. Sert a dire « il y a 40 s » plutot que
    /// de laisser croire que le telephone vient de frapper.
    #[serde(skip)]
    pub at: std::time::Instant,
}

/// TTL et plafond d'essais. Six chiffres, deux minutes, cinq essais : le
/// brute-force perd avant d'avoir vu la moitié de l'espace.
pub const PAIRING_TTL: std::time::Duration = std::time::Duration::from_secs(120);
pub const PAIRING_MAX_ATTEMPTS: u32 = 5;

/// Génère un code à 6 chiffres uniformément (000000–999999).
///
/// Le code vient du CSPRNG de l'OS (le même que les tokens) : un code
/// prévisible annulerait le second facteur.
fn generer_code() -> String {
    use rand::RngCore;
    // Réjection : 10^6 ne divise pas 2^32, on tire jusqu'à tomber dans
    // [0, 1_000_000) pour éviter le biais modulo.
    const LIMITE: u32 = 1_000_000;
    const ZONE: u32 = u32::MAX - (u32::MAX % LIMITE);
    loop {
        let mut b = [0u8; 4];
        rand::rngs::OsRng.fill_bytes(&mut b);
        let n = u32::from_le_bytes(b);
        if n < ZONE {
            return format!("{:06}", n % LIMITE);
        }
    }
}

#[derive(serde::Deserialize)]
pub struct QrQuery {
    /// `local` (défaut), `public` ou `tunnel`.
    #[serde(default)]
    pub mode: Option<String>,
    /// Pour `public` : l'adresse par laquelle on joint la machine de dehors.
    #[serde(default)]
    pub url: Option<String>,
}

/// GET /v1/pairing — l'adresse, la configuration, et le code qui la porte.
pub async fn qr(
    State(s): State<Arc<DaemonState>>,
    ConnectInfo(pair): ConnectInfo<SocketAddr>,
    Query(q): Query<QrQuery>,
) -> Response {
    let mode = q.mode.as_deref().unwrap_or("local");

    let url = match mode {
        "local" => {
            // Le service n'écoute sur le réseau que si le mode serveur est
            // actif. Produire un code portant l'adresse locale sans cela
            // donnerait un carré parfaitement valide menant à une adresse qui
            // ne répond à personne — un échec que le téléphone constaterait
            // sans pouvoir l'expliquer.
            if !s.auth_required {
                return erreur(
                    StatusCode::CONFLICT,
                    "Cette machine n'écoute que sur elle-même. Activez le mode serveur \
                     dans les réglages pour qu'un téléphone du réseau local puisse la joindre."
                        .into(),
                );
            }
            s.local_url.clone()
        }
        "tunnel" => {
            let Some(u) = s.travel.tunnel_url().await.filter(|u| !u.is_empty()) else {
                return erreur(
                    StatusCode::CONFLICT,
                    "Le mode Remote n'est pas actif : il n'y a pas encore d'adresse extérieure \
                     à mettre dans un code."
                        .into(),
                );
            };
            u
        }
        "public" => {
            let Some(u) = q.url.filter(|u| !u.trim().is_empty()) else {
                return erreur(
                    StatusCode::BAD_REQUEST,
                    "Indiquez l'adresse publique par laquelle on joint cette machine.".into(),
                );
            };
            normaliser(&u)
        }
        autre => {
            return erreur(
                StatusCode::BAD_REQUEST,
                format!("Mode inconnu : « {autre} » (local, public ou tunnel)."),
            );
        }
    };

    let ca = match locaryn_config::mtls::authority(&s.data_dir) {
        Ok(ca) => ca,
        Err(e) => {
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("autorité locale illisible : {e}"),
            );
        }
    };

    // Le mode Remote avec le code : sans lui, le téléphone ne sait pas s'il
    // reçoit une adresse de réseau local, un port ouvert, ou un tunnel dont
    // l'adresse expirera. Les trois se comportent différemment, et la
    // différence se voit le jour où ça ne marche plus.
    //
    // Construit le vrai type plutôt qu'un objet écrit à la main : celui-ci
    // porte `#[serde(rename_all = "camelCase")]`, et un `json!({"server_url":
    // ...})` composé indépendamment produisait des clés que le téléphone ne
    // reconnaissait pas — chaque code scanné échouait au décodage, en silence
    // jusqu'à ce qu'une personne essaie vraiment de s'appairer.
    let provisioning = locaryn_config::provision::Provisioning {
        server_url: url.clone(),
        organisation: nom_du_serveur(),
        certificate_fingerprint: None,
        authority_pem: Some(ca.cert_pem),
        access_mode: Some(mode.to_string()),
        note: String::new(),
    };
    let charge = match serde_json::to_string(&provisioning) {
        Ok(c) => c,
        Err(e) => {
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("configuration illisible ({e})"),
            );
        }
    };

    // La charge passe en ASCII avant d'entrer dans le code. Un QR n'annonce
    // pas son jeu de caracteres : sans cela, le nom d'une machine francaise
    // ressortait en cyrillique sur l'ecran du telephone. Le meme texte est
    // renvoye dans la reponse, pour que le QR et le champ HTTP restent
    // identiques au caractere pres.
    let charge = locaryn_travel::qr::ascii_seul(&charge);

    let svg = match locaryn_travel::qr::svg(&charge) {
        Ok(svg) => svg,
        Err(e) => {
            // Un PEM d'autorité tient dans un code, mais pas dans n'importe
            // lequel : le dire est plus utile qu'un carré vide.
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("code impossible à produire ({e}) — l'autorité est trop longue."),
            );
        }
    };

    // Le code à usage unique : généré à chaque affichage du QR, consommé au
    // premier confirm valide. Tant qu'il n'est pas confirmé, il ne sert à
    // personne — c'est pourquoi il n'est pas dans le QR lui-même.
    let code = generer_code();
    {
        let mut pending = s.pairing_pending.lock().expect("verrou pairing");
        *pending = Some(PendingPairing {
            code: code.clone(),
            created_at: std::time::Instant::now(),
            attempts: 0,
            annonce: None,
        });
    }

    Json(serde_json::json!({
        "mode": mode,
        "url": url,
        "provisioning": charge,
        "qr_svg": svg,
        // Le code ne part qu'a l'hote. Un appelant du reseau recoit le QR,
        // qui ne contient rien de secret, et rien d'autre : autrement le
        // second facteur se donnerait a qui sait demander.
        "pairing_code": if sur_cette_machine(pair) { code } else { String::new() },
        "pairing_ttl_seconds": PAIRING_TTL.as_secs(),
    }))
    .into_response()
}

/// POST /v1/auth/pair/announce — « j'ai scanne, je suis la ».
///
/// Le telephone le dit avant de pouvoir confirmer quoi que ce soit : il n'a
/// pas encore le code, et c'est justement cette annonce qui le fait
/// apparaitre a l'ecran de l'hote. L'ordre compte — un code affiche avant que
/// personne n'essaie reste expose pour rien, et n'apprend a l'hote ni qui
/// arrive ni d'ou.
///
/// Ouvert comme `confirm` : un appareil qui s'appaire n'a pas de jeton.
/// L'annonce ne donne rien — ni code, ni jeton — elle demande seulement a
/// etre vue.
pub async fn announce(
    State(s): State<Arc<DaemonState>>,
    ConnectInfo(pair): ConnectInfo<SocketAddr>,
    Json(body): Json<PairAnnounceBody>,
) -> Response {
    let mut pending = s.pairing_pending.lock().expect("verrou pairing");
    match pending.as_mut() {
        None => erreur(
            StatusCode::CONFLICT,
            "Aucun appairage en cours sur ce serveur. Affichez le QR sur l'hote, puis \
             rescannez."
                .into(),
        ),
        Some(p) if p.created_at.elapsed() > PAIRING_TTL => {
            *pending = None;
            erreur(
                StatusCode::GONE,
                "Ce QR a expire (2 minutes). Affichez-en un nouveau sur l'hote.".into(),
            )
        }
        Some(p) => {
            p.annonce = Some(Annonce {
                device: label_appareil(&body.device_label),
                ip: pair.ip().to_canonical().to_string(),
                at: std::time::Instant::now(),
            });
            Json(serde_json::json!({
                "waiting": true,
                "server": nom_du_serveur(),
                "ttl_seconds": PAIRING_TTL
                    .as_secs()
                    .saturating_sub(p.created_at.elapsed().as_secs()),
            }))
            .into_response()
        }
    }
}

/// GET /v1/pairing/state — ce que l'hote doit montrer, et a qui.
///
/// Reserve a cette machine : c'est ici que le code se lit.
pub async fn state(
    State(s): State<Arc<DaemonState>>,
    ConnectInfo(pair): ConnectInfo<SocketAddr>,
) -> Response {
    if !sur_cette_machine(pair) {
        return erreur(
            StatusCode::FORBIDDEN,
            "L'etat d'un appairage ne se lit que sur la machine qui l'affiche.".into(),
        );
    }
    let mut pending = s.pairing_pending.lock().expect("verrou pairing");
    // Un code expire n'est pas un code : on le retire plutot que de laisser
    // l'ecran promettre une saisie qui echouera.
    if pending
        .as_ref()
        .is_some_and(|p| p.created_at.elapsed() > PAIRING_TTL)
    {
        *pending = None;
    }
    let corps = match pending.as_ref() {
        None => serde_json::json!({ "pending": false }),
        Some(p) => serde_json::json!({
            "pending": true,
            "attempts": p.attempts,
            "ttl_seconds": PAIRING_TTL
                .as_secs()
                .saturating_sub(p.created_at.elapsed().as_secs()),
            "announced": p.annonce.is_some(),
            // Depuis combien de temps il frappe : « il y a 40 s » se lit
            // autrement qu'« a l'instant » quand on hesite a accepter.
            "announced_seconds_ago": p.annonce.as_ref().map(|a| a.at.elapsed().as_secs()),
            "device": p.annonce.as_ref().map(|a| a.device.clone()),
            "ip": p.annonce.as_ref().map(|a| a.ip.clone()),
            // Le code n'apparait qu'une fois quelqu'un annonce : avant, il
            // n'a personne a servir.
            "pairing_code": p.annonce.as_ref().map(|_| p.code.clone()),
        }),
    };
    Json(corps).into_response()
}

/// POST /v1/pairing/reject — « ce n'est pas moi ».
///
/// Efface l'appairage en attente. Le `confirm` du telephone echouera alors,
/// meme s'il a lu le code : c'est le refus, pas un simple masquage.
pub async fn reject(
    State(s): State<Arc<DaemonState>>,
    ConnectInfo(pair): ConnectInfo<SocketAddr>,
) -> Response {
    if !sur_cette_machine(pair) {
        return erreur(
            StatusCode::FORBIDDEN,
            "Un appairage ne se refuse que depuis la machine qui l'affiche.".into(),
        );
    }
    let mut pending = s.pairing_pending.lock().expect("verrou pairing");
    let avait = pending.is_some();
    *pending = None;
    Json(serde_json::json!({ "rejected": avait })).into_response()
}

#[derive(serde::Deserialize)]
pub struct PairAnnounceBody {
    /// Le nom que l'appareil se donne, pour que l'hote sache qui frappe.
    #[serde(default)]
    pub device_label: Option<String>,
}

/// POST /v1/auth/pair/confirm — le client qui a scanné le QR renvoie le code
/// affiché à l'écran. Valide une fois, dans les deux minutes : le serveur
/// délivre alors un token de session dédié à l'appareil.
pub async fn confirm(
    State(s): State<Arc<DaemonState>>,
    Json(body): Json<PairConfirmBody>,
) -> Response {
    let code_saisi = body.pairing_code.trim();
    if code_saisi.len() != 6 || !code_saisi.chars().all(|c| c.is_ascii_digit()) {
        return erreur(
            StatusCode::BAD_REQUEST,
            "Le code d'appairage attendu compte six chiffres.".into(),
        );
    }

    let Some(admin_id) = s.pairing_admin_user_id else {
        return erreur(
            StatusCode::CONFLICT,
            "Aucun compte administrateur sur ce serveur : l'appairage par code              exige un compte à appairer."
                .into(),
        );
    };

    let verdict = {
        let mut pending = s.pairing_pending.lock().expect("verrou pairing");
        match pending.as_mut() {
            None => Err("Aucun code d'appairage en attente. Affichez le QR sur                          l'hôte, puis réessayez."
                .to_string()),
            Some(p) if p.created_at.elapsed() > PAIRING_TTL => {
                *pending = None;
                Err("Code d'appairage expiré (2 minutes). Affichez un nouveau QR.".into())
            }
            Some(p) if p.attempts >= PAIRING_MAX_ATTEMPTS => {
                *pending = None;
                Err("Trop d'essais. Un nouveau QR génère un nouveau code.".into())
            }
            Some(p) => {
                if constant_time_eq(p.code.as_bytes(), code_saisi.as_bytes()) {
                    // Consommé à la première réussite : jamais rejouable.
                    *pending = None;
                    Ok(())
                } else {
                    p.attempts += 1;
                    Err(format!(
                        "Code incorrect ({} essai{} restant{}).",
                        PAIRING_MAX_ATTEMPTS - p.attempts,
                        if PAIRING_MAX_ATTEMPTS - p.attempts > 1 { "s" } else { "" },
                        if PAIRING_MAX_ATTEMPTS - p.attempts > 1 { "s" } else { "" },
                    ))
                }
            }
        }
    };

    if let Err(message) = verdict {
        return erreur(StatusCode::UNAUTHORIZED, message);
    }

    // Un appareil appairé est une session de longue durée : 180 jours,
    // renouvelable en re-scannant. Le kind reste 'session' — c'est un
    // appareil, pas une clé développeur.
    match s
        .users
        .issue_token(admin_id, Some(&label_appareil(&body.device_label)), 180)
        .await
    {
        Ok(tok) => {
            tracing::info!("appairage confirmé, token de session appareil émis");
            Json(serde_json::json!({
                "token": tok.plaintext,
                "expires_at": tok.expires_at,
                "device_label": label_appareil(&body.device_label),
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "émission du token d'appairage impossible");
            erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("token d'appairage impossible ({e})"),
            )
        }
    }
}

/// Comparaison à temps constant : la longueur est publique (6 chiffres),
/// mais prendre l'habitude ne coûte rien.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Une étiquette d'appareil lisible : tronquée, vide par défaut.
fn label_appareil(brut: &Option<String>) -> String {
    let l = brut
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Appareil appairé");
    l.chars().take(40).collect()
}

#[derive(serde::Deserialize)]
pub struct PairConfirmBody {
    pub pairing_code: String,
    #[serde(default)]
    pub device_label: Option<String>,
}

/// Une réponse 401 d'un téléchargement protégé — le même corps que `login`.
fn unauthorized_response(detail: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [("WWW-Authenticate", "Basic realm=\"Locaryn pairing\"")],
        Json(serde_json::json!({ "error": "unauthorized", "detail": detail })),
    )
        .into_response()
}

/// Les deux facteurs du téléchargement protégé, dans l'ordre Basic (RFC 7617).
/// Le décodeur est celui du dépôt (`locaryn_config`), pas une dépendance de
/// plus pour dix lignes.
fn basic_credentials(req: &axum::extract::Request) -> Option<(String, String)> {
    use axum::http::header::AUTHORIZATION;
    let raw = req.headers().get(AUTHORIZATION)?.to_str().ok()?;
    let rest = raw
        .strip_prefix("Basic ")
        .or_else(|| raw.strip_prefix("basic "))?;
    let decoded = locaryn_config::provision::base64_decode(rest.trim())?;
    let s = String::from_utf8(decoded).ok()?;
    let (user, pass) = s.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// GET /v1/pairing/ca — l'autorité du déploiement, en clair.
///
/// Public, et c'est exactement sa place : le QR d'appairage porte déjà cette
/// même autorité dans sa charge (`Provisioning.authority_pem`), et une
/// autorité publique n'authentifie personne par elle-même. Ce qui rend ce GET
/// défendable, c'est la modale de consentement côté client : personne ne
/// devient un client reconnu pour avoir lu cette URL.
///
/// HTTPS strict : la commande Rust de l'hôte refuse de télécharger autre
/// chose, donc servir ce corps en clair n'arriverait jamais à destination.
pub async fn get_ca(State(s): State<Arc<DaemonState>>) -> Response {
    match locaryn_config::mtls::authority(&s.data_dir) {
        Ok(authority) => (
            [
                ("Content-Type", "application/x-pem-file"),
                (
                    "Content-Disposition",
                    "attachment; filename=\"locaryn-ca.pem\"",
                ),
            ],
            authority.cert_pem,
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "lecture de l'autorité impossible");
            erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("autorité indisponible ({e})"),
            )
        }
    }
}

fn nom_du_serveur() -> String {
    hostname().unwrap_or_else(|| "Locaryn".to_string())
}

fn hostname() -> Option<String> {
    // Rien de critique n'en dépend : c'est une étiquette. Les variables
    // couvrent Windows et les systèmes Unix sans dépendance de plus.
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|h| !h.trim().is_empty())
}

/// Une adresse publique tapée par un humain, ramenée à une URL.
fn normaliser(brut: &str) -> String {
    let brut = brut.trim().trim_end_matches('/');
    if brut.starts_with("http://") || brut.starts_with("https://") {
        return brut.to_string();
    }
    // Hors du réseau local, le chiffrement n'est pas optionnel.
    format!("https://{brut}")
}

fn erreur(code: StatusCode, message: String) -> Response {
    (
        code,
        Json(serde_json::json!({ "error": { "code": "pairing", "message": message } })),
    )
        .into_response()
}

/// GET /v1/pairing/cert — le paquet certificat client + clé, émis à la volée.
///
/// Ce fichier est un secret : avec lui, TLS reconnaît l'appelant comme un
/// client légitime. Il ne se sert donc pas à qui sait demander — la route
/// exige des identifiants valides sur ce serveur, en Basic (RFC 7617). C'est
/// le paramètre `user`/`password` du lien `locaryn://connect` qui les porte :
/// le lien qui demande la connexion est celui qui prouve le droit de la
/// télécharger, et la modale de consentement reste le lieu où la personne les
/// a fournis.
///
/// Un utilisateur sans certificat existant en reçoit un neuf, signé par
/// l'autorité locale — c'est l'appairage. Une Basic correcte a toujours un
/// temps de réponse identique, qu'on réémette ou qu'on refuse.
pub async fn get_client_cert(
    State(s): State<Arc<DaemonState>>,
    req: axum::extract::Request,
) -> Response {
    let Some((username, password)) = basic_credentials(&req) else {
        return unauthorized_response(
            "Ce paquet est protégé. Envoyez les identifiants du serveur en \
             Authorization: Basic — ce sont ceux du lien de connexion.",
        );
    };

    // Le même verdict que /v1/auth/login, sans émettre de jeton : le bundle
    // lui-même devient la preuve d'identité. Le même message pour un mot de
    // passe faux et un compte inconnu, comme à la connexion — ne pas aider à
    // énumérer les comptes.
    match s.users.authenticate(&username, &password).await {
        Ok(Some(_user)) => {}
        Ok(None) => {
            tracing::info!(user = %username, "téléchargement de certificat refusé");
            return unauthorized_response("Identifiants incorrects.");
        }
        Err(e) => {
            tracing::error!(error = %e, "authentification impossible");
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                "authentification indisponible".into(),
            );
        }
    }

    let credential = match locaryn_config::mtls::issue_client(&s.data_dir, &username, 365) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "émission du certificat client impossible");
            return erreur(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("émission du certificat impossible ({e})"),
            );
        }
    };

    (
        [
            ("Content-Type", "application/x-pem-file"),
            (
                "Content-Disposition",
                "attachment; filename=\"locaryn-client.pem\"",
            ),
        ],
        credential.bundle_pem,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{constant_time_eq, generer_code, normaliser, sur_cette_machine};

    /// La frontiere qui protege le code de confirmation. Une IPv4 mappee en
    /// IPv6 est la forme sous laquelle arrive une connexion IPv4 sur une
    /// ecoute IPv6 : la manquer aurait ferme la porte a l'hote lui-meme.
    #[test]
    fn seule_cette_machine_est_locale() {
        let a = |t: &str| sur_cette_machine(t.parse().expect("adresse"));
        assert!(a("127.0.0.1:9000"));
        assert!(a("[::1]:9000"));
        assert!(a("[::ffff:127.0.0.1]:9000"), "IPv4 mappee en IPv6");
        assert!(a("127.4.5.6:9000"), "tout 127.0.0.0/8 est la boucle locale");
        assert!(!a("192.168.1.20:9000"));
        assert!(!a("[::ffff:192.168.1.20]:9000"));
        assert!(!a("88.120.4.3:9000"));
    }

    /// Six chiffres, et tout l'espace atteignable. Un code biaise ou trop
    /// court affaiblirait le second facteur sans que rien ne le signale.
    #[test]
    fn le_code_fait_six_chiffres() {
        for _ in 0..200 {
            let c = generer_code();
            assert_eq!(c.len(), 6, "{c}");
            assert!(c.chars().all(|d| d.is_ascii_digit()), "{c}");
        }
    }

    #[test]
    fn la_comparaison_a_temps_constant_est_exacte() {
        assert!(constant_time_eq(b"123456", b"123456"));
        assert!(!constant_time_eq(b"123456", b"123457"));
        assert!(!constant_time_eq(b"123456", b"12345"));
        assert!(!constant_time_eq(b"", b"123456"));
    }

    #[test]
    fn une_adresse_publique_passe_en_https() {
        assert_eq!(
            normaliser("maison.exemple:7474"),
            "https://maison.exemple:7474"
        );
        assert_eq!(normaliser(" 88.120.4.3:7474/ "), "https://88.120.4.3:7474");
    }

    #[test]
    fn un_schema_deja_ecrit_est_respecte() {
        assert_eq!(
            normaliser("http://192.168.1.20:7474"),
            "http://192.168.1.20:7474"
        );
        assert_eq!(normaliser("https://a.b:1/"), "https://a.b:1");
    }
}
