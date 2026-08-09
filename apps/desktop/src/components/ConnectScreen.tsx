import { useEffect, useState } from "react";
import { type CertificateStatus, type Provisioning, core } from "../lib/core";
import { pickAnyFile } from "../lib/dialog";

type Props = {
  provisioning: Provisioning;
  onConnected: () => void;
};

/**
 * Signing in to a server an administrator prepared.
 *
 * The employee should have nothing to configure: the address, the organisation
 * and the certificate to expect all come from the file dropped beside the
 * installer. What is left is a username and a password — and, when the server
 * demands one, installing the certificate they were sent.
 */
export function ConnectScreen({ provisioning, onConnected }: Props) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [cert, setCert] = useState<CertificateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    core
      .clientCertificateStatus()
      .then(setCert)
      .catch(() => {});
  }, []);

  async function installCertificate() {
    setError(null);
    setNotice(null);
    const picked = await pickAnyFile("Certificat", ["pem", "crt", "key"]);
    if (!picked) return;
    setBusy(true);
    try {
      const next = await core.installClientCertificate(picked);
      setCert(next);
      setNotice(
        next.issued_to ? `Certificat de « ${next.issued_to} » installé.` : "Certificat installé.",
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function connect() {
    setError(null);
    if (!username.trim() || !password) {
      setError("Renseignez votre identifiant et votre mot de passe.");
      return;
    }
    setBusy(true);
    try {
      await core.signIn(provisioning.serverUrl, username.trim(), password);
      // Never keep the password in component state after it has been used.
      setPassword("");
      onConnected();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-connect">
      <div className="locaryn-connect-card">
        <h2 className="locaryn-connect-title">{provisioning.organisation || "Connexion"}</h2>
        <p className="locaryn-connect-server">{provisioning.serverUrl}</p>

        <label htmlFor="connect-user" className="locaryn-field-label" style={{ marginTop: 20 }}>
          Identifiant
        </label>
        <input
          id="connect-user"
          className="locaryn-input"
          style={{ marginTop: 6 }}
          value={username}
          disabled={busy}
          onChange={(e) => setUsername(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void connect()}
        />

        <label htmlFor="connect-pass" className="locaryn-field-label" style={{ marginTop: 14 }}>
          Mot de passe
        </label>
        <input
          id="connect-pass"
          className="locaryn-input"
          style={{ marginTop: 6 }}
          type="password"
          value={password}
          disabled={busy}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void connect()}
        />

        <button
          type="button"
          className="locaryn-btn-primary"
          style={{ marginTop: 18, width: "100%" }}
          disabled={busy}
          onClick={connect}
        >
          {busy ? "Connexion…" : "Se connecter"}
        </button>

        {/* Certificate. Shown always: the server may start requiring one, and
            a user who cannot connect needs to see this without hunting. */}
        <div className="locaryn-connect-cert">
          {cert?.installed ? (
            <>
              <span className="locaryn-connect-cert-ok">✓</span>
              <span>
                Certificat installé
                {cert.issued_to ? ` — ${cert.issued_to}` : ""}
              </span>
              <button
                type="button"
                className="locaryn-btn-ghost"
                disabled={busy}
                onClick={async () => {
                  setCert(await core.removeClientCertificate());
                  setNotice("Certificat retiré.");
                }}
              >
                Retirer
              </button>
            </>
          ) : (
            <>
              <span>Certificat de connexion</span>
              <button
                type="button"
                className="locaryn-btn-ghost"
                disabled={busy}
                onClick={installCertificate}
              >
                Installer…
              </button>
            </>
          )}
        </div>
        <p className="locaryn-connect-hint">
          {cert?.installed
            ? "Ce certificat prouve à quelle machine appartient cette installation."
            : "Requis seulement si votre administrateur vous en a fourni un. Choisissez le fichier « .pem » qu'il vous a transmis."}
        </p>

        {provisioning.note && <p className="locaryn-connect-note">{provisioning.note}</p>}

        {error && <div className="locaryn-vp-error">{error}</div>}
        {notice && !error && <div className="locaryn-vp-notice">{notice}</div>}
      </div>
    </div>
  );
}
