import { useEffect, useState } from "react";
import { type MicroModel, core } from "../lib/core";

/** Ce qu'on enregistre pour dire « celui de la conversation », côté socle
 *  `locaryn_config::MICRO_MODEL_ACTIF`. */
const MODELE_ACTIF = "@actif";

/**
 * Le modèle des micro-tâches.
 *
 * Nommer une conversation à partir de son sujet, ranger, résumer : des travaux
 * de quelques mots, qui n'ont pas besoin du modèle qui tient la conversation et
 * ne doivent pas lui prendre son tour. Un petit modèle suffit, et rien n'est
 * choisi par défaut : tant que personne n'en désigne un, ces services ne
 * tournent pas et une conversation garde le titre tiré de sa première phrase.
 */
export function MicroModelSetting() {
  const [state, setState] = useState<MicroModel | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    core
      .microModel()
      .then(setState)
      .catch((e) => setError(String(e)));
  }, []);

  async function choose(model: string | null) {
    setBusy(true);
    setError(null);
    try {
      setState(await core.setMicroModel(model));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-field locaryn-micro-model-setting">
      <div className="locaryn-field-label">Modèle des petites tâches</div>
      <p className="locaryn-field-hint">
        Il nomme les conversations d'après leur sujet, pour qu'une liste se lise. Sans modèle
        désigné, rien de tout cela ne tourne, et le titre reste la première phrase.
      </p>

      <select
        className="locaryn-select"
        value={state?.model ?? ""}
        disabled={busy || !state}
        onChange={(e) => void choose(e.target.value || null)}
      >
        <option value="">Aucun — ne rien nommer automatiquement</option>
        <option value={MODELE_ACTIF}>Celui déjà chargé — le modèle de la conversation</option>
        {state?.available.map((m) => (
          <option key={m} value={m}>
            {m}
          </option>
        ))}
      </select>

      <p className="locaryn-field-hint">
        {state?.model === MODELE_ACTIF
          ? "Rien n'est chargé ni déchargé : la petite tâche passe par le modèle déjà en mémoire. C'est le choix le plus rapide, et le seul qui ne prenne pas de VRAM."
          : "Un modèle dédié — un Qwen 1,5 à 3 milliards, un Gemma compact — répond mieux à ces questions courtes, mais le moteur n'en tient qu'un à la fois : chaque titre sort le modèle de conversation de la mémoire, charge celui-ci, puis recharge l'autre."}
      </p>

      <p className="locaryn-field-hint">
        Un titre que vous écrivez vous-même n'est jamais remplacé.
      </p>

      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
