// Jusqu'où l'application accepte de remplir la mémoire avant de charger un
// modèle.
//
// Le réglage existe parce que la bonne réponse dépend de la machine et de ce
// qu'on en fait : quelqu'un qui code avec vingt onglets ouverts n'a pas la
// même tolérance que quelqu'un dont le poste ne sert qu'à ça. Le défaut est
// « intermédiaire » — refuser d'emblée ce qui tiendrait serait aussi pénible
// que de laisser la machine s'effondrer.

import { useEffect, useState } from "react";
import { CAUTION_LABELS, type CautionLevel, type CautionReserves, core } from "../lib/core";

/** « 1 Go », « 0,5 Go » : une décimale au plus. */
function go(n: number): string {
  return `${n.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} Go`;
}

const ORDER: CautionLevel[] = ["prudent", "equilibre", "risque"];

export function CautionSettings() {
  const [level, setLevel] = useState<CautionLevel | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reserves, setReserves] = useState<CautionReserves | null>(null);

  useEffect(() => {
    core
      .cautionReserves()
      .then(setReserves)
      .catch(() => {
        // Sans mesure de la machine, les niveaux restent décrits en mots.
      });
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
      {/* Un groupe de boutons n'est pas un champ : `label` ne désignerait rien.
          `fieldset` + `legend` est la forme que les lecteurs d'écran annoncent
          avant d'égrener les options — obtenir le même effet avec un `div` et
          `role="group"` marcherait, mais réinventerait un élément qui existe. */}
      <fieldset className="locaryn-caution-fieldset">
        <legend className="locaryn-field-label">Prudence au chargement d'un modèle</legend>
        <p className="locaryn-field-hint">
          Au moment du chargement, Locaryn regarde la VRAM libre à cet instant — pas la VRAM totale
          de la carte. Le modèle doit y tenir en laissant libre la réserve du niveau choisi ; sinon
          il n'est pas chargé, à tous les niveaux, et une fenêtre explique pourquoi (un jeu ouvert,
          par exemple). Le contexte s'ajuste ensuite à la VRAM qui reste.
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
              {(() => {
                const r = reserves?.levels.find((l) => l.level === id);
                if (!r || !reserves) return null;
                return (
                  <span className="locaryn-caution-reserve">
                    {reserves.total_vram_gb > 0 && !reserves.unified_memory
                      ? `Garde ${go(r.vram_gb)} de VRAM libres pour le système (carte de ${go(reserves.total_vram_gb)})`
                      : `Laisse ${go(r.ram_gb)} de mémoire libres sur ${go(reserves.total_ram_gb)}`}
                    {reserves.total_vram_gb > 0 && !reserves.unified_memory
                      ? ` · ${go(r.ram_gb)} de RAM`
                      : ""}
                  </span>
                );
              })()}
            </button>
          ))}
        </div>
      </fieldset>

      {level === "risque" && (
        <p className="locaryn-caution-warning">
          En mode risqué, si une autre application réclame de la VRAM pendant que le modèle tourne,
          Windows déplace une partie du modèle en mémoire système : les réponses ralentissent
          fortement.
        </p>
      )}

      {error && <p className="locaryn-caution-warning">{error}</p>}
    </div>
  );
}
