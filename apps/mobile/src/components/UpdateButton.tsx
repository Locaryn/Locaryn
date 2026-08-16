import { useEffect, useState } from "react";
import { type UpdateStatus, api, coreMode } from "../lib/core";

/**
 * Bouton de mise à jour de l'application Android.
 *
 * L'updater de Tauri ne couvre pas Android : sur un téléphone, une application
 * distribuée hors magasin se met à jour en ouvrant le nouvel APK, et c'est le
 * système qui installe, vérifie la signature et demande confirmation.
 *
 * Le bouton ne s'affiche que lorsqu'une version plus récente existe vraiment :
 * un bouton toujours présent qui répond « vous êtes à jour » est du bruit, et
 * un bouton qui prétend mettre à jour sans fichier à installer serait pire.
 */
export function UpdateButton() {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Hors téléphone (interface développée dans un navigateur), il n'y a rien
    // à installer.
    if (coreMode !== "tauri") return;
    let cancelled = false;
    void api
      .checkUpdate()
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {
        // Une vérification qui échoue ne doit pas s'imposer à l'écran : elle
        // se retentera au prochain lancement.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!status?.available || !status.download_url) return null;

  return (
    <button
      type="button"
      className="lo-bar-away"
      style={{ cursor: "pointer" }}
      disabled={busy}
      title={error ?? `Version ${status.latest} disponible (vous avez ${status.current})`}
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
      {error ? "Échec — réessayer" : `Mettre à jour (${status.latest})`}
    </button>
  );
}
