import { useCallback, useEffect, useState } from "react";
import { type UpdateStatus, api, coreMode } from "../lib/core";

/**
 * La version installée, et s'il en existe une plus récente.
 *
 * Cet écran doit être atteignable **avant** toute connexion. Le cas est
 * concret : le serveur passe à une version que le téléphone ne sait pas
 * encore parler, la connexion échoue, et si la mise à jour n'était accessible
 * qu'une fois connecté, il n'y aurait aucune façon d'en sortir depuis
 * l'application.
 *
 * Rien ici ne demande de serveur : la vérification interroge directement le
 * manifeste publié, et l'installation est confiée à Android.
 */
export function VersionSection() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const verifier = useCallback(async () => {
    if (coreMode !== "tauri") return;
    setBusy(true);
    try {
      setStatus(await api.checkUpdate());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void verifier();
  }, [verifier]);

  const etat = (() => {
    if (busy && !status) return "Vérification…";
    if (!status) return "Version inconnue";
    if (status.error) return status.error;
    if (status.available) return `Version ${status.latest} disponible`;
    if (status.latest) return "À jour";
    return "Impossible de joindre le serveur de mises à jour";
  })();

  return (
    <section className="lo-section">
      <h2 className="lo-section-title">Version</h2>

      <div className="lo-card">
        <div className="lo-card-text">
          <span className="lo-card-title">Locaryn {status?.current ?? ""}</span>
          <span className="lo-hint">{etat}</span>
        </div>
        <div className="lo-card-actions">
          <button type="button" className="lo-btn-small" disabled={busy} onClick={verifier}>
            {busy ? "…" : "Vérifier"}
          </button>
        </div>
      </div>

      {status?.available && status.download_url && (
        <button
          type="button"
          className="lo-btn"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            setError(null);
            try {
              await api.openUpdate(status.download_url as string);
            } catch (e) {
              setError(String(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          Installer la version {status.latest}
        </button>
      )}

      {status?.available && (
        <p className="lo-hint">
          Android affiche son propre écran d'installation et vérifie la signature. L'application ne
          s'installe jamais elle-même.
        </p>
      )}

      {error && <p className="lo-error">{error}</p>}
    </section>
  );
}
