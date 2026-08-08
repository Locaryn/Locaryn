//! Installing the client certificate an administrator issued.
//!
//! "Installing" here means registering it with Locaryn, not adding it to the
//! Windows certificate store. Two reasons: the system store would let any
//! program on the machine use the credential, and importing into it needs
//! privileges an employee usually does not have — which is exactly the
//! friction this whole flow exists to remove.
//!
//! The file is copied into the application's own directory, so it keeps
//! working after the user empties their Downloads folder.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CertificateStatus {
    pub installed: bool,
    /// Name the certificate was issued to, read from the file itself.
    pub issued_to: Option<String>,
    pub path: Option<String>,
    /// True once an authority certificate is also present, which is what lets
    /// the client verify the server in return.
    pub authority_installed: bool,
}

fn cert_dir() -> PathBuf {
    locaryn_config::default_data_dir().join("client-tls")
}

fn cert_path() -> PathBuf {
    cert_dir().join("client.pem")
}

fn ca_path() -> PathBuf {
    cert_dir().join("authority.pem")
}

/// Common name inside the first certificate of a PEM bundle.
///
/// Shown so the user can confirm they installed *their* certificate and not a
/// colleague's — the files look identical otherwise.
fn common_name(pem: &str) -> Option<String> {
    let der = locaryn_config::provision::base64_decode(
        &pem.lines()
            .skip_while(|l| !l.starts_with("-----BEGIN CERTIFICATE"))
            .skip(1)
            .take_while(|l| !l.starts_with("-----END"))
            .collect::<Vec<_>>()
            .join(""),
    )?;
    // Find the commonName OID (2.5.4.3 → 55 04 03) and read the string after
    // it. A full X.509 parser would be a heavy dependency for one label.
    //
    // Take the *last* occurrence: in a DER certificate the issuer name comes
    // before the subject, so the first match is the authority. Reading it
    // showed "Locaryn local CA" for everyone's certificate, which defeats the
    // point of displaying a name at all.
    let needle = [0x55u8, 0x04, 0x03];
    let pos = der.windows(3).rposition(|w| w == needle)?;
    let rest = der.get(pos + 3..)?;
    let len = *rest.get(1)? as usize;
    let bytes = rest.get(2..2 + len)?;
    let name = String::from_utf8_lossy(bytes).trim().to_string();
    (!name.is_empty() && name.chars().all(|c| !c.is_control())).then_some(name)
}

#[tauri::command]
pub fn client_certificate_status() -> Result<CertificateStatus, String> {
    let path = cert_path();
    let installed = path.is_file();
    Ok(CertificateStatus {
        issued_to: installed
            .then(|| std::fs::read_to_string(&path).ok())
            .flatten()
            .as_deref()
            .and_then(common_name),
        path: installed.then(|| path.to_string_lossy().to_string()),
        installed,
        authority_installed: ca_path().is_file(),
    })
}

/// Copy a certificate the administrator provided into the application.
///
/// `source` is the `.pem` bundle; `authority` is the CA file, optional only
/// because a company using a public certificate authority does not need one.
#[tauri::command]
pub fn install_client_certificate(
    source: String,
    authority: Option<String>,
) -> Result<CertificateStatus, String> {
    let src = PathBuf::from(source.trim());
    if !src.is_file() {
        return Err(format!("Fichier introuvable : {}", src.display()));
    }
    let pem = std::fs::read_to_string(&src).map_err(|e| format!("lecture : {e}"))?;

    // Reject early and clearly. A certificate without its key cannot
    // authenticate anything, and the two files look alike to a non-specialist.
    if !pem.contains("BEGIN CERTIFICATE") {
        return Err(
            "Ce fichier ne contient pas de certificat. Utilisez le fichier « .pem » \
             fourni par votre administrateur."
                .into(),
        );
    }
    if !pem.contains("PRIVATE KEY") {
        return Err(
            "Ce fichier contient un certificat mais pas sa clé privée : il ne peut pas \
             servir à vous identifier. Demandez le fichier complet à votre administrateur."
                .into(),
        );
    }

    std::fs::create_dir_all(cert_dir()).map_err(|e| format!("création du dossier : {e}"))?;
    std::fs::write(cert_path(), &pem).map_err(|e| format!("copie : {e}"))?;
    restrict(&cert_path());

    if let Some(auth) = authority
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let ap = Path::new(auth);
        if ap.is_file() {
            let ca = std::fs::read_to_string(ap).map_err(|e| format!("lecture autorité : {e}"))?;
            if ca.contains("BEGIN CERTIFICATE") {
                std::fs::write(ca_path(), ca).map_err(|e| format!("copie autorité : {e}"))?;
            }
        }
    }

    tracing::info!("certificat client installé");
    client_certificate_status()
}

#[tauri::command]
pub fn remove_client_certificate() -> Result<CertificateStatus, String> {
    let _ = std::fs::remove_file(cert_path());
    let _ = std::fs::remove_file(ca_path());
    client_certificate_status()
}

fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        if let Ok(md) = std::fs::metadata(path) {
            let mut p = md.permissions();
            p.set_readonly(true);
            let _ = std::fs::set_permissions(path, p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "locaryn_cc_{tag}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_name_inside_a_real_certificate_is_read_back() {
        let dir = scratch("name");
        // A genuine certificate, issued the way the server issues them.
        let cred = locaryn_config::mtls::issue_client(&dir, "marie", 30).unwrap();
        assert_eq!(common_name(&cred.bundle_pem).as_deref(), Some("marie"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_certificate_without_its_key_is_refused_with_a_usable_message() {
        let dir = scratch("nokey");
        let cred = locaryn_config::mtls::issue_client(&dir, "paul", 30).unwrap();
        // Strip the key: the classic mistake of sending only the certificate.
        let cert_only: String = cred
            .bundle_pem
            .lines()
            .take_while(|l| !l.contains("PRIVATE KEY"))
            .collect::<Vec<_>>()
            .join("\n");
        let p = dir.join("cert-only.pem");
        std::fs::write(&p, cert_only).unwrap();

        let err = install_client_certificate(p.to_string_lossy().to_string(), None).unwrap_err();
        assert!(err.contains("clé privée"), "message peu clair : {err}");
        assert!(err.contains("administrateur"), "message sans issue : {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_file_that_is_not_a_certificate_is_refused() {
        let dir = scratch("junk");
        let p = dir.join("notes.txt");
        std::fs::write(&p, "bonjour").unwrap();
        let err = install_client_certificate(p.to_string_lossy().to_string(), None).unwrap_err();
        assert!(err.contains("pas de certificat"), "got: {err}");

        let missing = install_client_certificate("D:/nope/absent.pem".into(), None).unwrap_err();
        assert!(missing.contains("introuvable"), "got: {missing}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn garbage_does_not_produce_a_fake_name() {
        assert_eq!(common_name("pas du pem"), None);
        assert_eq!(common_name(""), None);
    }
}

/// Where the session token is kept between launches.
fn token_path() -> PathBuf {
    locaryn_config::default_data_dir().join("session-token.json")
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Session {
    pub server_url: String,
    pub username: String,
    pub token: String,
}

/// Exchange credentials for a token and remember it.
///
/// The password is used once and never written anywhere: only the token it
/// produced is stored, and that token can be revoked from the server without
/// the user having to change their password.
#[tauri::command]
pub async fn sign_in(
    server_url: String,
    username: String,
    password: String,
) -> Result<Session, String> {
    let url = format!("{}/v1/auth/login", server_url.trim_end_matches('/'));

    // The fingerprint is read here rather than taken from the caller: it is the
    // one thing that decides whether this really is the company's server, and a
    // value arriving from the interface could have come from anywhere.
    let fingerprint = locaryn_config::provision::load()
        .ok()
        .flatten()
        .and_then(|p| p.certificate_fingerprint);

    let client = crate::secure_client::build(
        std::fs::read_to_string(cert_path()).ok().as_deref(),
        std::fs::read_to_string(ca_path()).ok().as_deref(),
        fingerprint.as_deref(),
        std::time::Duration::from_secs(20),
    )?;

    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "username": username, "password": password }))
        .send()
        .await
        .map_err(|e| {
            // A refused certificate is not a network problem, and telling the
            // user to check their cable would send them the wrong way.
            let chain = {
                let mut s = String::new();
                let mut cur: Option<&dyn std::error::Error> = Some(&e);
                while let Some(c) = cur {
                    s.push_str(&c.to_string());
                    s.push(' ');
                    cur = c.source();
                }
                s
            };
            if chain.contains("empreinte") {
                "Le certificat présenté ne correspond pas à l'empreinte fournie par votre \
                 administrateur. Ne saisissez pas votre mot de passe : signalez-le d'abord."
                    .to_string()
            } else if chain.contains("certificate required")
                || chain.contains("CertificateRequired")
            {
                "Ce serveur exige un certificat client. Installez celui que votre \
                 administrateur vous a transmis, puis réessayez."
                    .to_string()
            } else {
                format!(
                    "Serveur injoignable ({e}). Vérifiez qu'il est allumé et que vous êtes \
                     sur le bon réseau."
                )
            }
        })?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("Identifiant ou mot de passe incorrect.".into());
    }
    if !resp.status().is_success() {
        return Err(format!(
            "Le serveur a refusé la connexion ({}).",
            resp.status()
        ));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("réponse illisible : {e}"))?;
    let token = body
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or("Le serveur n'a pas renvoyé de jeton.")?
        .to_string();

    let session = Session {
        server_url: server_url.trim_end_matches('/').to_string(),
        username,
        token,
    };
    let json =
        serde_json::to_string_pretty(&session).map_err(|e| format!("sérialisation : {e}"))?;
    std::fs::create_dir_all(locaryn_config::default_data_dir())
        .map_err(|e| format!("dossier de données : {e}"))?;
    std::fs::write(token_path(), json).map_err(|e| format!("écriture du jeton : {e}"))?;
    restrict(&token_path());
    Ok(session)
}

/// The stored session, if the user already signed in.
#[tauri::command]
pub fn current_session() -> Result<Option<Session>, String> {
    let path = token_path();
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("lecture : {e}"))?;
    Ok(serde_json::from_str(&raw).ok())
}

#[tauri::command]
pub fn sign_out() -> Result<(), String> {
    let _ = std::fs::remove_file(token_path());
    Ok(())
}
