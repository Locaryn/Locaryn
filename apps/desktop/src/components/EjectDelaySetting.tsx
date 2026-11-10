import { useEffect, useState } from "react";
import { core } from "../lib/core";

/** Les durées proposées, en secondes ; 0 : jamais. */
const CHOIX: { secs: number; libelle: string }[] = [
  { secs: 5 * 60, libelle: "5 minutes" },
  { secs: 15 * 60, libelle: "15 minutes" },
  { secs: 30 * 60, libelle: "30 minutes (par défaut)" },
  { secs: 60 * 60, libelle: "1 heure" },
  { secs: 2 * 60 * 60, libelle: "2 heures" },
  { secs: 0, libelle: "Jamais" },
];

/** Combien de temps un modèle inutilisé reste en mémoire avant d'être éjecté. */
export function EjectDelaySetting() {
  const [secs, setSecs] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    core
      .ejectDelay()
      .then(setSecs)
      .catch((e) => setError(String(e)));
  }, []);

  async function choisir(valeur: number) {
    setBusy(true);
    setError(null);
    try {
      setSecs(await core.setEjectDelay(valeur));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // Un délai réglé à la main hors de la liste reste affiché tel quel.
  const horsListe = secs !== null && !CHOIX.some((c) => c.secs === secs);

  return (
    <div className="locaryn-field">
      <div className="locaryn-field-label">Éjection automatique du modèle</div>
      <p className="locaryn-field-hint">
        Sans conversation pendant ce temps, le modèle quitte la mémoire.
      </p>
      <select
        className="locaryn-select"
        value={secs ?? ""}
        disabled={busy || secs === null}
        onChange={(e) => void choisir(Number(e.target.value))}
      >
        {horsListe && <option value={secs ?? 0}>{Math.round((secs ?? 0) / 60)} minutes</option>}
        {CHOIX.map((c) => (
          <option key={c.secs} value={c.secs}>
            {c.libelle}
          </option>
        ))}
      </select>
      {secs === 0 && (
        <p className="locaryn-field-hint">
          Le modèle garde sa mémoire vidéo jusqu'à ce que vous l'éjectiez.
        </p>
      )}
      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
