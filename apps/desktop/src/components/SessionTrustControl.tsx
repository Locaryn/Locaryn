import { useCallback, useEffect, useState } from "react";
import { type TrustLevel, core } from "../lib/core";
import { TRUST_LEVELS, trustInfo } from "../lib/trust";

/**
 * Les permissions d'une conversation, telles que le modèle les reçoit.
 *
 * Une exception posée ici ne concerne que cette conversation : les autres et
 * le projet qui la porte ne bougent pas. Sans exception, la conversation suit
 * le projet — ou, pour un chat libre, le réglage des nouvelles conversations.
 * Le même contrôle sert à la fenêtre « Paramètres du chat » et au panneau du
 * modèle, pour qu'un chat sans projet se règle aussi facilement qu'un autre.
 */
export function SessionTrustControl({ sessionId }: { sessionId: string }) {
  const [effective, setEffective] = useState<TrustLevel | null>(null);
  const [override, setOverride] = useState<TrustLevel | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setEffective(null);
    setOverride(null);
    setError(null);
    let cancelled = false;
    core
      .sessionTrust(sessionId)
      .then((t) => {
        if (cancelled) return;
        setEffective(t.effective);
        setOverride(t.override_value);
      })
      .catch((e) => {
        if (!cancelled) setError(`Permissions illisibles : ${String(e).replace(/^Error:\s*/, "")}`);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  const change = useCallback(
    async (level: TrustLevel | null) => {
      setBusy(true);
      setError(null);
      try {
        const next = await core.setSessionTrust(sessionId, level);
        setEffective(next);
        setOverride(level);
      } catch (e) {
        setError(`Changement impossible : ${String(e).replace(/^Error:\s*/, "")}`);
      } finally {
        setBusy(false);
      }
    },
    [sessionId],
  );

  return (
    <div className="lmc-field">
      <div className="locaryn-segmented" role="group" aria-label="Permissions de la conversation">
        {TRUST_LEVELS.map((n) => (
          <button
            key={n.value}
            type="button"
            disabled={busy || effective === null}
            className={`locaryn-segment${effective === n.value ? " locaryn-segment-on" : ""}`}
            aria-pressed={effective === n.value}
            title={n.hint}
            onClick={() => change(n.value)}
          >
            {n.label}
          </button>
        ))}
      </div>
      {effective && <p className="lmc-ctx-cap">{trustInfo(effective).hint}</p>}
      <p className="lmc-ctx-cap">
        {override
          ? "Exception posée sur cette conversation ; les autres chats ne bougent pas."
          : "Aucune exception : cette conversation suit le réglage du projet, ou celui des nouvelles conversations."}
      </p>
      {override && (
        <button
          type="button"
          className="locaryn-btn-ghost"
          disabled={busy}
          onClick={() => change(null)}
        >
          Rétablir le réglage par défaut
        </button>
      )}
      {error && (
        <p className="lmc-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
