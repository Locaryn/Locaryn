import { useCallback, useEffect, useState } from "react";
import { type RelayChoice, type TravelStatus, core } from "../lib/core";

/**
 * Configuration du plugin Remote.
 *
 * Les réglages du relais restent dans la colonne de configuration. Les codes
 * QR, eux, sont tous regroupés dans le panneau d'appairage à droite pour que
 * l'utilisateur sache immédiatement lequel faire scanner.
 */
export function TravelSettings() {
  const [status, setStatus] = useState<TravelStatus | null>(null);
  const [relays, setRelays] = useState<RelayChoice[]>([]);
  const [choice, setChoice] = useState("cloudflare");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await core.travelStatus());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    core
      .travelRelays()
      .then((r) => {
        setRelays(r);
        const usable = r.find((x) => x.installed && !x.needs_account) ?? r.find((x) => x.installed);
        if (usable) setChoice(usable.id);
      })
      .catch((e) => setError(String(e)));
  }, [refresh]);

  useEffect(() => {
    if (!status?.active) return;
    const timer = window.setInterval(() => void refresh(), 4000);
    return () => window.clearInterval(timer);
  }, [status?.active, refresh]);

  const selected = relays.find((r) => r.id === choice);

  async function toggle(on: boolean) {
    setBusy(true);
    setError(null);
    try {
      setStatus(await core.setTravelMode(on ? choice : null));
    } catch (e) {
      setError(String(e));
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="locaryn-field locaryn-remote-settings">
      <div className="locaryn-field-label">Remote</div>
      <p className="locaryn-field-hint">
        Utilisez cette machine depuis n'importe où. Le plugin Remote ouvre un tunnel sortant vers un
        relais : rien n'est ouvert sur votre box.
      </p>

      <div className="locaryn-srv-row">
        <label className="locaryn-srv-toggle">
          <input
            type="checkbox"
            checked={Boolean(status?.active)}
            disabled={busy || (!status?.active && !selected?.installed)}
            onChange={(e) => void toggle(e.target.checked)}
          />
          <span>{busy ? "…" : status?.active ? "Tunnel actif" : "Tunnel arrêté"}</span>
        </label>
        {!status?.active && (
          <select
            className="locaryn-select locaryn-select-sm"
            value={choice}
            disabled={busy}
            aria-label="Relais Remote"
            onChange={(e) => setChoice(e.target.value)}
          >
            {relays.map((r) => (
              <option key={r.id} value={r.id}>
                {r.label}
                {r.installed ? "" : " — non installé"}
                {r.needs_account && r.installed ? " — compte requis" : ""}
              </option>
            ))}
          </select>
        )}
        {status?.active && <span className="locaryn-srv-live">en ligne</span>}
      </div>

      {!status?.active && selected && !selected.installed && (
        <p className="locaryn-vp-warn">{selected.install_hint}</p>
      )}

      {status?.active && status.link && (
        <div className="locaryn-remote-link">
          <div>
            <strong>Relais actif</strong>
            <span>Le code « Tunnel sortant » est disponible dans le panneau à droite.</span>
          </div>
          <button
            type="button"
            className="locaryn-btn-ghost"
            onClick={() =>
              void navigator.clipboard
                .writeText(status.link!)
                .then(() => {
                  setCopied(true);
                  window.setTimeout(() => setCopied(false), 1500);
                })
                .catch(() => {})
            }
          >
            {copied ? "Lien copié" : "Copier le lien"}
          </button>
        </div>
      )}

      {status?.blocker && !status.active && !error && (
        <p className="locaryn-vp-warn">{status.blocker}</p>
      )}
      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
