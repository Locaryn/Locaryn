import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type TrustLevel, core } from "../lib/core";
import { TRUST_LEVELS, trustInfo } from "../lib/trust";

/**
 * Les permissions que portent les nouvelles conversations libres.
 *
 * Un projet ouvert a les siennes, choisies a sa creation. Ce reglage decide
 * pour les conversations qu'on ouvre pour poser une question — celles qui
 * n'appartiennent a personne d'autre qu'a la personne qui les ouvre.
 */
export function DefaultPermissionsSettings() {
  const [niveau, setNiveau] = useState<TrustLevel | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    core
      .defaultTrust()
      .then(setNiveau)
      .catch(() => setNiveau(null));
  }, []);

  async function choisir(v: TrustLevel) {
    setBusy(true);
    try {
      const suivant = await core.setDefaultTrust(v);
      setNiveau(suivant);
    } catch {
      // Le reglage reste tel quel ; rien a montrer de plus.
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="locaryn-model-preference-card">
      <div className="locaryn-model-preference-heading">
        <Icon name="private" size={17} />
        <div>
          <h4>Permissions des nouvelles conversations</h4>
          <p>
            Ce que le modele pourra faire dans chaque nouveau chat. Les conversations deja ouvertes
            gardent les leurs, et chacune reste modifiable depuis son panneau de reglages.
          </p>
        </div>
      </div>
      <div className="locaryn-segmented" role="group" aria-label="Permissions par defaut">
        {TRUST_LEVELS.map((n) => (
          <button
            key={n.value}
            type="button"
            disabled={busy}
            className={`locaryn-segment${niveau === n.value ? " locaryn-segment-on" : ""}`}
            aria-pressed={niveau === n.value}
            onClick={() => choisir(n.value)}
            title={n.hint}
          >
            {n.label}
          </button>
        ))}
      </div>
      <p className="locaryn-field-hint">{trustInfo(niveau ?? "untrusted").hint}</p>
    </section>
  );
}
