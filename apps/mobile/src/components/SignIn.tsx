import { useState } from "react";
import { api, type MobileStatus } from "../lib/core";

type Props = {
  status: MobileStatus;
  onSignedIn: (s: MobileStatus) => void;
  onScan: () => void;
};

/**
 * Signing in on a phone.
 *
 * Two fields. The server was registered once from the file an administrator
 * produced, and after that the address follows whatever code gets scanned —
 * so there is nothing here about networks.
 */
export function SignIn({ status, onSignedIn, onScan }: Props) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    if (!username.trim() || !password) {
      setError("Renseignez votre identifiant et votre mot de passe.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const next = await api.signIn(username.trim(), password);
      setPassword("");
      onSignedIn(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="lo-screen">
      <div className="lo-center">
        <h1 className="lo-title">{status.server_name ?? "Lochor"}</h1>
        <p className="lo-sub">
          {status.travelling
            ? "Connexion depuis l'extérieur."
            : "Connexion sur le réseau local."}
        </p>

        <div>
          <label className="lo-label" htmlFor="u">
            Identifiant
          </label>
          <input
            id="u"
            className="lo-input"
            autoCapitalize="none"
            autoCorrect="off"
            value={username}
            disabled={busy}
            onChange={(e) => setUsername(e.target.value)}
          />
        </div>

        <div>
          <label className="lo-label" htmlFor="p">
            Mot de passe
          </label>
          <input
            id="p"
            className="lo-input"
            type="password"
            value={password}
            disabled={busy}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void submit()}
          />
        </div>

        <button type="button" className="lo-btn" disabled={busy} onClick={submit}>
          {busy ? "Connexion…" : "Se connecter"}
        </button>

        {error && <p className="lo-error">{error}</p>}

        <button type="button" className="lo-btn-ghost" onClick={onScan}>
          Scanner un code
        </button>
      </div>
    </div>
  );
}
