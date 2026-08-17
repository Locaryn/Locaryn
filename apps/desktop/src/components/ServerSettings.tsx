import { useCallback, useEffect, useState } from "react";
import { type ServerStatus, core } from "../lib/core";

/**
 * Share this machine's models with other people.
 *
 * The application does not serve HTTP itself — it starts the Locaryn service,
 * which already carries the accounts, the tokens and the encryption. What the
 * switch really does is expose that service on the network, and everything the
 * service guarantees comes with it: authentication becomes mandatory, traffic
 * is encrypted, and it refuses to run at all with no account.
 */
export function ServerSettings() {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await core.serverStatus());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    // The service can stop on its own; re-check so the switch never lies.
    const t = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(t);
  }, [refresh]);

  async function toggle(enabled: boolean) {
    setBusy(true);
    setError(null);
    try {
      setStatus(await core.setServerMode(enabled));
    } catch (e) {
      setError(String(e));
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  const blocked = Boolean(status?.blocker);

  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Partager cette machine</div>
      <p className="locaryn-field-hint">
        Rend les modèles de cet ordinateur utilisables depuis d'autres postes et depuis un
        téléphone. Utile quand une seule machine possède la carte graphique.
      </p>

      <div className="locaryn-srv-row">
        <label className="locaryn-srv-toggle">
          <input
            type="checkbox"
            checked={Boolean(status?.running)}
            disabled={busy || (blocked && !status?.running)}
            onChange={(e) => toggle(e.target.checked)}
          />
          <span>{busy ? "…" : status?.running ? "Serveur actif" : "Serveur arrêté"}</span>
        </label>
        {status?.running && <span className="locaryn-srv-live">en écoute</span>}
      </div>

      {status?.blocker && !status.running && (
        <p className="locaryn-vp-warn">
          {status.blocker}
          {status.accounts === 0 && (
            <>
              {" "}
              Depuis un terminal : <code>locaryn users add nom --admin</code>
            </>
          )}
        </p>
      )}

      {status?.running && (
        <>
          <div className="locaryn-kv-list" style={{ marginTop: 12 }}>
            <div className="locaryn-kv">
              <span className="locaryn-kv-key">Adresse à communiquer</span>
              <span className="locaryn-kv-val locaryn-kv-mono">{status.url}</span>
            </div>
            <div className="locaryn-kv">
              <span className="locaryn-kv-key">Comptes</span>
              <span className="locaryn-kv-val locaryn-kv-mono">{status.accounts}</span>
            </div>
          </div>

          <div className="locaryn-field-actions" style={{ marginTop: 10 }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(status.url);
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1500);
                } catch {
                  /* clipboard unavailable — the address is visible above */
                }
              }}
            >
              {copied ? "Adresse copiée" : "Copier l'adresse"}
            </button>
          </div>

          {status.fingerprint && (
            <>
              <div className="locaryn-field-label" style={{ marginTop: 20 }}>
                Empreinte du certificat
              </div>
              <p className="locaryn-field-hint">
                Le certificat est généré par cette machine, donc les postes clients afficheront un
                avertissement au premier contact. C'est attendu : cette empreinte est ce qui permet
                de vérifier qu'ils parlent bien à<em> cet</em> ordinateur et pas à un autre.
              </p>
              <div className="locaryn-srv-fingerprint">{status.fingerprint}</div>
            </>
          )}

          <p className="locaryn-field-hint" style={{ marginTop: 16 }}>
            Pour éviter à vos collègues toute configuration, générez un fichier de connexion depuis
            un terminal :{" "}
            <code>locaryn provision {status.url.replace(/^https?:\/\//, "").split(":")[0]}</code>.
            Il suffira ensuite de le déposer à côté de l'installeur.
          </p>
        </>
      )}

      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
