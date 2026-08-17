import { useEffect, useState } from "react";
import { type UpdateStatus, api, coreMode } from "../lib/core";

/**
 * Le raccourci vers la mise à jour, dans la barre du chat.
 *
 * Il ne s'affiche que lorsqu'une version plus récente existe vraiment : un
 * bouton toujours présent qui répond « vous êtes à jour » est du bruit.
 *
 * Il n'installe rien lui-même — il mène à l'écran des réglages, où l'on voit
 * d'où l'on part, où l'on va, et ce que la version apporte avant de décider.
 */
export function UpdateButton({ onOpen }: { onOpen: () => void }) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);

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
      title={`Version ${status.latest} disponible (vous avez ${status.current})`}
      onClick={onOpen}
    >
      {`Mettre à jour (${status.latest})`}
    </button>
  );
}
