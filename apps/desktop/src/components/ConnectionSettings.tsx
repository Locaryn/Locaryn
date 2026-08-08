import { useEffect, useState } from "react";
import { core, type CertificateStatus, type ServerSession } from "../lib/core";
import { pickAnyFile } from "../lib/dialog";

/**
 * The other half of server mode: this machine as a *client*.
 *
 * The connection screen already covers signing in, but it is only shown when
 * there is no session — so without this panel there would be no way to sign
 * out, and no way to replace an expired certificate short of reinstalling.
 */
export function ConnectionSettings() {
  const [session, setSession] = useState<ServerSession | null>(null);
  const [cert, setCert] = useState<CertificateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [s, c] = await Promise.all([
          core.currentSession(),
          core.clientCertificateStatus(),
        ]);
        setSession(s);
        setCert(c);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  async function install() {
    setError(null);
    const picked = await pickAnyFile("Certificat", ["pem", "crt", "key"]);
    if (!picked) return;
    setBusy(true);
    try {
      setCert(await core.installClientCertificate(picked));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="lochor-field" style={{ marginTop: 28 }}>
      <label className="lochor-field-label">Se connecter à un serveur</label>
      <p className="lochor-field-hint">
        Quand les modèles tournent sur une autre machine. La connexion est établie
        au démarrage ; ce qui suit permet de la changer.
      </p>

      {session ? (
        <>
          <div className="lochor-kv-list" style={{ marginTop: 12 }}>
            <div className="lochor-kv">
              <span className="lochor-kv-key">Serveur</span>
              <span className="lochor-kv-val lochor-kv-mono">{session.server_url}</span>
            </div>
            <div className="lochor-kv">
              <span className="lochor-kv-key">Compte</span>
              <span className="lochor-kv-val lochor-kv-mono">{session.username}</span>
            </div>
          </div>
          <div className="lochor-field-actions" style={{ marginTop: 10 }}>
            <button
              type="button"
              className="lochor-btn-ghost"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                await core.signOut();
                // Reload rather than juggle state: the sign-in screen is
                // decided when the application starts, and this is exactly
                // the same situation as a fresh launch.
                window.location.reload();
              }}
            >
              Se déconnecter
            </button>
          </div>
        </>
      ) : (
        <p className="lochor-field-hint" style={{ marginTop: 10 }}>
          Aucune session. Cette installation utilise les modèles de cet ordinateur.
        </p>
      )}

      <label className="lochor-field-label" style={{ marginTop: 20 }}>
        Certificat de connexion
      </label>
      <p className="lochor-field-hint">
        Certains serveurs n'acceptent que les postes qu'ils ont eux-mêmes autorisés.
        Le fichier « .pem » transmis par votre administrateur s'installe ici.
      </p>
      <div className="lochor-connect-cert" style={{ marginTop: 10 }}>
        {cert?.installed ? (
          <>
            <span className="lochor-connect-cert-ok">✓</span>
            <span>Installé{cert.issued_to ? ` — ${cert.issued_to}` : ""}</span>
            <button
              type="button"
              className="lochor-btn-ghost"
              disabled={busy}
              onClick={async () => setCert(await core.removeClientCertificate())}
            >
              Retirer
            </button>
          </>
        ) : (
          <>
            <span>Aucun certificat</span>
            <button type="button" className="lochor-btn-ghost" disabled={busy} onClick={install}>
              Installer…
            </button>
          </>
        )}
      </div>
      {cert?.installed && cert.path && (
        <p className="lochor-connect-hint" style={{ marginTop: 8 }}>
          {cert.path}
        </p>
      )}

      {error && <div className="lochor-vp-error">{error}</div>}
    </div>
  );
}
