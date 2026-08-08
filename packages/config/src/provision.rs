//! Pre-configured client enrolment.
//!
//! Asking an employee to type a server address, a port and a certificate
//! fingerprint is where a rollout dies — it looks like network administration,
//! and both the user and the IT department back away from it.
//!
//! So the administrator produces the settings once and the client picks them
//! up on its own. What the employee does is: run the installer, open the app,
//! type the username and password they were given.
//!
//! The file is *not* a credential. It says where the server is and which
//! certificate to expect; it grants nothing on its own, so it can travel by
//! email or sit on a shared drive without being a secret to protect.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Settings an administrator hands to their users.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provisioning {
    /// Full base URL, e.g. `https://192.168.1.10:7474`.
    pub server_url: String,
    /// Shown in the sign-in screen so people know which server they reached.
    #[serde(default)]
    pub organisation: String,
    /// SHA-256 of the server certificate, colon-separated.
    ///
    /// With a self-signed certificate this is what distinguishes the real
    /// server from anything else answering on that address — without it the
    /// client can only choose between refusing every connection and trusting
    /// any of them.
    #[serde(default)]
    pub certificate_fingerprint: Option<String>,
    /// The deployment's own certificate authority, in PEM.
    ///
    /// Public by nature — it vouches for others, it does not authorise
    /// anything by itself. Carrying it means a client can keep verifying the
    /// server after a certificate renewal, and it is what lets a phone check
    /// that a scanned pairing code really came from this deployment.
    #[serde(default)]
    pub authority_pem: Option<String>,
    /// Optional note displayed under the sign-in form.
    #[serde(default)]
    pub note: String,
}

/// Standard file name. Recognisable next to an installer.
pub const PROVISION_FILE: &str = "locaryn-connect.json";

/// Where a client looks for its settings, most specific first.
///
/// Beside the executable comes first so an administrator can drop the file
/// into the install directory; the machine-wide location covers a deployment
/// pushed by software distribution.
pub fn search_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(PROVISION_FILE));
        }
    }
    if let Ok(dir) = std::env::var("PROGRAMDATA") {
        out.push(PathBuf::from(dir).join("Locaryn").join(PROVISION_FILE));
    }
    #[cfg(not(windows))]
    out.push(PathBuf::from("/etc/locaryn").join(PROVISION_FILE));
    out.push(global_dir().join(PROVISION_FILE));
    out
}

use crate::global_dir;

/// Load the provisioning file, if the machine has one.
///
/// A malformed file is reported rather than ignored: silently falling back to
/// "no server configured" would look to the user like the deployment failed
/// for no reason.
pub fn load() -> Result<Option<Provisioning>, String> {
    for path in search_paths() {
        if !path.is_file() {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("lecture de {} : {e}", path.display()))?;
        let p: Provisioning = serde_json::from_str(&raw)
            .map_err(|e| format!("{} est illisible : {e}", path.display()))?;
        if p.server_url.trim().is_empty() {
            return Err(format!(
                "{} ne contient pas d'adresse de serveur",
                path.display()
            ));
        }
        tracing::info!(path = %path.display(), server = %p.server_url, "configuration de déploiement trouvée");
        return Ok(Some(p));
    }
    Ok(None)
}

/// Write a provisioning file for distribution.
pub fn write(dir: &Path, p: &Provisioning) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("création de {} : {e}", dir.display()))?;
    let path = dir.join(PROVISION_FILE);
    let json = serde_json::to_string_pretty(p).map_err(|e| format!("sérialisation : {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("écriture de {} : {e}", path.display()))?;
    Ok(path)
}

/// Normalise what an administrator typed into a usable base URL.
///
/// People write `192.168.1.10`, `192.168.1.10:7474` or a full URL. All three
/// have to work, and the scheme must default to HTTPS — an exposed daemon
/// always serves TLS, so defaulting to HTTP would produce a file that cannot
/// connect.
pub fn normalise_url(input: &str, default_port: u16) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("adresse vide".into());
    }
    // Split the scheme *before* stripping trailing slashes: doing it the other
    // way turns "https://" into "https:", which then looks like a bare host and
    // yields the nonsense "https://https:".
    let (scheme, rest) = match s.split_once("://") {
        Some((sc, r)) => (sc.to_ascii_lowercase(), r),
        None => ("https".to_string(), s),
    };
    let rest = rest.trim_end_matches('/');
    if scheme != "http" && scheme != "https" {
        return Err(format!("schéma non pris en charge : {scheme}"));
    }
    if rest.is_empty() {
        return Err("adresse vide".into());
    }
    // A port is only absent if the host part has no colon — bracketed IPv6
    // literals carry their own.
    let has_port = if rest.starts_with('[') {
        rest.rsplit_once(']')
            .map(|(_, t)| t.starts_with(':'))
            .unwrap_or(false)
    } else {
        rest.contains(':')
    };
    Ok(if has_port {
        format!("{scheme}://{rest}")
    } else {
        format!("{scheme}://{rest}:{default_port}")
    })
}

/// SHA-256 of a byte string.
///
/// Lives here so the daemon that prints a certificate fingerprint, the command
/// that writes it into a provisioning file, and the client that verifies it all
/// compute the same value. Three copies would eventually disagree, and a
/// fingerprint that disagrees is worse than none.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());

    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (i, v) in [a, b, c, d, e, f, g, hh].iter().enumerate() {
            h[i] = h[i].wrapping_add(*v);
        }
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// Fingerprint of a PEM certificate, in the colon-separated form browsers show.
pub fn certificate_fingerprint(pem: &str) -> Option<String> {
    let b64: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    let der = base64_decode(&b64)?;
    Some(
        sha256(&der)
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// Base64 decoder, enough for PEM bodies.
///
/// Accepts the URL-safe alphabet as well: `-` and `_` never occur in standard
/// base64, so treating them as aliases cannot misread a certificate, and it
/// means one decoder serves both PEM and the pairing links.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        let c = match c {
            b'-' => b'+',
            b'_' => b'/',
            other => other,
        };
        let v = T.iter().position(|&t| t == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_typed_any_of_the_usual_ways_becomes_a_usable_url() {
        // Bare host: HTTPS, because an exposed daemon always serves TLS.
        assert_eq!(
            normalise_url("192.168.1.10", 7474).unwrap(),
            "https://192.168.1.10:7474"
        );
        assert_eq!(
            normalise_url("192.168.1.10:9000", 7474).unwrap(),
            "https://192.168.1.10:9000"
        );
        assert_eq!(
            normalise_url("https://serveur.local:7474/", 7474).unwrap(),
            "https://serveur.local:7474"
        );
        assert_eq!(
            normalise_url("  serveur.local  ", 7474).unwrap(),
            "https://serveur.local:7474"
        );
        // Plain HTTP stays if explicitly asked for — a reverse proxy may
        // terminate TLS ahead of the daemon.
        assert_eq!(
            normalise_url("http://10.0.0.5", 7474).unwrap(),
            "http://10.0.0.5:7474"
        );
    }

    #[test]
    fn ipv6_literals_keep_their_own_colons() {
        assert_eq!(normalise_url("[::1]", 7474).unwrap(), "https://[::1]:7474");
        assert_eq!(
            normalise_url("[::1]:9000", 7474).unwrap(),
            "https://[::1]:9000"
        );
    }

    #[test]
    fn nonsense_is_refused_rather_than_turned_into_a_broken_url() {
        assert!(normalise_url("", 7474).is_err());
        assert!(normalise_url("   ", 7474).is_err());
        assert!(normalise_url("ftp://serveur", 7474).is_err());
        assert!(normalise_url("https://", 7474).is_err());
    }

    #[test]
    fn a_written_file_reads_back_identically() {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_prov_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let p = Provisioning {
            server_url: "https://192.168.1.10:7474".into(),
            organisation: "Atelier Durand".into(),
            certificate_fingerprint: Some("AB:CD:EF".into()),
            note: "Identifiants fournis par le service informatique.".into(),
            authority_pem: None,
        };
        let path = write(&dir, &p).expect("écriture");
        let back: Provisioning =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back.server_url, p.server_url);
        assert_eq!(back.organisation, p.organisation);
        assert_eq!(back.certificate_fingerprint, p.certificate_fingerprint);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_older_file_without_the_newer_fields_still_loads() {
        // Files written by an earlier version must not break a client update.
        let minimal = r#"{"serverUrl":"https://10.0.0.2:7474"}"#;
        let p: Provisioning = serde_json::from_str(minimal).expect("doit se charger");
        assert_eq!(p.server_url, "https://10.0.0.2:7474");
        assert!(p.certificate_fingerprint.is_none());
        assert_eq!(p.organisation, "");
    }
}
