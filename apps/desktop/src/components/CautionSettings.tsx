// Jusqu'où l'application accepte de remplir la mémoire avant de charger un
// modèle.
//
// Le réglage existe parce que la bonne réponse dépend de la machine et de ce
// qu'on en fait : quelqu'un qui code avec vingt onglets ouverts n'a pas la
// même tolérance que quelqu'un dont le poste ne sert qu'à ça. Le défaut est
// « intermédiaire » — refuser d'emblée ce qui tiendrait serait aussi pénible
// que de laisser la machine s'effondrer.

import { useEffect, useState } from "react";
import { core, CAUTION_LABELS, type CautionLevel } from "../lib/core";

const ORDER: CautionLevel[] = ["prudent", "equilibre", "risque"];

export function CautionSettings() {
  const [level, setLevel] = useState<CautionLevel | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        setLevel(await core.cautionLevel());
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    })();
  }, []);

  async function choose(next: CautionLevel) {
    const previous = level;
    setLevel(next); // optimiste : le réglage est local et instantané
    setSaving(true);
    setError(null);
    try {
      await core.setCautionLevel(next);
    } catch (e) {
      setLevel(previous); // un réglage qui n'a pas été enregistré ne doit pas
      setError(e instanceof Error ? e.message : String(e)); // paraître appliqué
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="locaryn-field">
      <label className="locaryn-field-label">Prudence au chargement d'un modèle</label>
      <p className="locaryn-field-hint">
        La mémoire libre est vérifiée avant chaque chargement. Ce réglage décide de la marge
        exigée — et de ce qui est refusé plutôt que tenté.
      </p>

      <div className="locaryn-caution-choices">
        {ORDER.map((id) => (
          <button
            key={id}
            type="button"
            className={`locaryn-caution-choice${level === id ? " locaryn-active" : ""}`}
            onClick={() => void choose(id)}
            disabled={saving || level === null}
            aria-pressed={level === id}
          >
            <span className="locaryn-caution-name">{CAUTION_LABELS[id].label}</span>
            <span className="locaryn-caution-desc">{CAUTION_LABELS[id].hint}</span>
          </button>
        ))}
      </div>

      {level === "risque" && (
        <p className="locaryn-caution-warning">
          En mode risqué, aucun chargement n'est refusé. Un modèle trop gros pour la mémoire fera
          compenser le système sur le disque : ralentissement sévère, et l'application peut être
          tuée par manque de mémoire.
        </p>
      )}

      {error && <p className="locaryn-caution-warning">{error}</p>}
    </div>
  );
}
