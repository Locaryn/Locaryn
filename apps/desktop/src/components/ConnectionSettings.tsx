import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type CertificateStatus, type ServerSession, core } from "../lib/core";
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
  const [serverUrl, setServerUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  useEffect(() => {
    void (async () => {
      try {
        const [s, c] = await Promise.all([core.currentSession(), core.clientCertificateStatus()]);
        setSession(s);
        setCert(c);
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  async function handleSignIn() {
    setError(null);
    const cleanUrl = serverUrl.trim();
    const cleanUser = username.trim();
    if (!cleanUrl || !cleanUser || !password) {
      setError("Veuillez renseigner l'adresse du serveur, l'identifiant et le mot de passe.");
      return;
    }
    setBusy(true);
    try {
      await core.signIn(cleanUrl, cleanUser, password);
      setPassword("");
      window.location.reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

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
    <div className="locaryn-field" style={{ marginTop: 28 }}>
      <div className="locaryn-field-label">Se connecter à un serveur</div>
      <p className="locaryn-field-hint">
        Quand les modèles tournent sur une autre machine. La connexion est établie au démarrage ; ce
        qui suit permet de la changer.
      </p>

      {session ? (
        <>
          <div className="locaryn-kv-list" style={{ marginTop: 12 }}>
            <div className="locaryn-kv">
              <span className="locaryn-kv-key">Serveur</span>
              <span className="locaryn-kv-val locaryn-kv-mono">{session.server_url}</span>
            </div>
            <div className="locaryn-kv">
              <span className="locaryn-kv-key">Compte</span>
              <span className="locaryn-kv-val locaryn-kv-mono">{session.username}</span>
            </div>
          </div>
          <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                await core.signOut();
                window.location.reload();
              }}
            >
              Se déconnecter
            </button>
          </div>
        </>
      ) : (
        <div
          style={{
            marginTop: 12,
            padding: "12px",
            background: "var(--locaryn-bg-subtle, rgba(255,255,255,0.03))",
            borderRadius: 8,
          }}
        >
          <p className="locaryn-field-hint" style={{ margin: 0, marginBottom: 12 }}>
            Connectez cette application à un serveur ou supercalculateur IA distant (ex: DGX Spark).
          </p>
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <div>
              <label
                htmlFor="conn-server-url"
                className="locaryn-field-label"
                style={{ fontSize: "0.85rem" }}
              >
                Adresse du serveur (ex: https://192.168.1.50:7474)
              </label>
              <input
                id="conn-server-url"
                className="locaryn-input"
                style={{ width: "100%", marginTop: 4 }}
                placeholder="https://ip-ou-domaine:7474"
                value={serverUrl}
                disabled={busy}
                onChange={(e) => setServerUrl(e.target.value)}
              />
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
              <div>
                <label
                  htmlFor="conn-username"
                  className="locaryn-field-label"
                  style={{ fontSize: "0.85rem" }}
                >
                  Identifiant
                </label>
                <input
                  id="conn-username"
                  className="locaryn-input"
                  style={{ width: "100%", marginTop: 4 }}
                  placeholder="admin"
                  value={username}
                  disabled={busy}
                  onChange={(e) => setUsername(e.target.value)}
                />
              </div>
              <div>
                <label
                  htmlFor="conn-password"
                  className="locaryn-field-label"
                  style={{ fontSize: "0.85rem" }}
                >
                  Mot de passe
                </label>
                <input
                  id="conn-password"
                  type="password"
                  className="locaryn-input"
                  style={{ width: "100%", marginTop: 4 }}
                  value={password}
                  disabled={busy}
                  onChange={(e) => setPassword(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && void handleSignIn()}
                />
              </div>
            </div>
            <div style={{ marginTop: 4 }}>
              <button
                type="button"
                className="locaryn-btn-primary"
                disabled={busy || !serverUrl.trim() || !username.trim() || !password}
                onClick={handleSignIn}
              >
                {busy ? "Connexion en cours…" : "Se connecter au serveur"}
              </button>
            </div>
          </div>
        </div>
      )}

      <div className="locaryn-field-label" style={{ marginTop: 20 }}>
        Certificat de connexion
      </div>
      <p className="locaryn-field-hint">
        Certains serveurs n'acceptent que les postes qu'ils ont eux-mêmes autorisés. Le fichier «
        .pem » transmis par votre administrateur s'installe ici.
      </p>
      <div className="locaryn-connect-cert" style={{ marginTop: 10 }}>
        {cert?.installed ? (
          <>
            <Icon name="check" size={15} />
            <span>Installé{cert.issued_to ? ` — ${cert.issued_to}` : ""}</span>
            <button
              type="button"
              className="locaryn-btn-ghost"
              disabled={busy}
              onClick={async () => setCert(await core.removeClientCertificate())}
            >
              Retirer
            </button>
          </>
        ) : (
          <>
            <span>Aucun certificat</span>
            <button type="button" className="locaryn-btn-ghost" disabled={busy} onClick={install}>
              Installer…
            </button>
          </>
        )}
      </div>
      {cert?.installed && cert.path && (
        <p className="locaryn-connect-hint" style={{ marginTop: 8 }}>
          {cert.path}
        </p>
      )}

      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
