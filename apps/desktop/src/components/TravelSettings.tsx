import { useCallback, useEffect, useState } from "react";
import { type RelayChoice, type TravelStatus, core } from "../lib/core";

/**
 * Travel mode.
 *
 * One switch and a square to photograph. No address, no port, no relay
 * hostname — the whole point is that nobody has to read a network setting off
 * a screen, let alone type one into a phone.
 */
export function TravelSettings() {
  const [status, setStatus] = useState<TravelStatus | null>(null);
  const [relays, setRelays] = useState<RelayChoice[]>([]);
  const [choice, setChoice] = useState<string>("cloudflare");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [homeCode, setHomeCode] = useState<TravelStatus | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await core.travelStatus());
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
        // Default to something that will actually work on this machine, and
        // prefer the one needing no account.
        const usable = r.find((x) => x.installed && !x.needs_account) ?? r.find((x) => x.installed);
        if (usable) setChoice(usable.id);
      })
      .catch(() => {});
  }, [refresh]);

  // While it is on, the link expires; asking again mints a fresh one, so a
  // window left open never shows a code that has quietly stopped working.
  useEffect(() => {
    if (!status?.active) return;
    const t = window.setInterval(() => void refresh(), 4 * 60 * 1000);
    return () => window.clearInterval(t);
  }, [status?.active, refresh]);

  const selected = relays.find((r) => r.id === choice);

  async function toggle(on: boolean) {
    setBusy(true);
    setError(null);
    setHomeCode(null);
    try {
      setStatus(await core.setTravelMode(on ? choice : null));
      if (!on) setHomeCode(await core.travelHomeCode());
    } catch (e) {
      setError(String(e));
      await refresh();
    } finally {
      setBusy(false);
    }
  }

  const code = status?.active ? status : homeCode;

  return (
    <div className="locaryn-field" style={{ marginTop: 28 }}>
      <label className="locaryn-field-label">Mode voyage</label>
      <p className="locaryn-field-hint">
        Utiliser cette machine depuis n'importe où, sans rien ouvrir sur la box. L'ordinateur
        appelle un relais ; c'est le relais que le téléphone contacte.
      </p>

      <div className="locaryn-srv-row">
        <label className="locaryn-srv-toggle">
          <input
            type="checkbox"
            checked={Boolean(status?.active)}
            disabled={busy || (!status?.active && !selected?.installed)}
            onChange={(e) => toggle(e.target.checked)}
          />
          <span>{busy ? "…" : status?.active ? "Actif" : "Inactif"}</span>
        </label>
        {!status?.active && (
          <select
            className="locaryn-select locaryn-select-sm"
            value={choice}
            disabled={busy}
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
      </div>

      {!status?.active && selected && !selected.installed && (
        <p className="locaryn-vp-warn">{selected.install_hint}</p>
      )}

      {code?.qr_svg && (
        <div className="locaryn-travel-code">
          <div
            className="locaryn-travel-qr"
            /* The image comes from our own daemon and contains no script. */
            dangerouslySetInnerHTML={{ __html: code.qr_svg }}
          />
          <div className="locaryn-travel-say">
            <p className="locaryn-travel-title">
              {status?.active
                ? "Scannez avec l'appareil photo du téléphone"
                : "Scannez pour revenir au réseau local"}
            </p>
            <p className="locaryn-travel-sub">
              {status?.active
                ? "Le téléphone se reconfigure tout seul. Ce code expire au bout de dix minutes ; celui affiché ici reste à jour."
                : "À faire une fois rentré. Rien d'autre à changer."}
            </p>
            {code.link && (
              <button
                type="button"
                className="locaryn-btn-ghost"
                onClick={() => void navigator.clipboard.writeText(code.link!).catch(() => {})}
              >
                Copier le lien
              </button>
            )}
          </div>
        </div>
      )}

      {status?.blocker && !status.active && !error && (
        <p className="locaryn-vp-warn">{status.blocker}</p>
      )}
      {error && <div className="locaryn-vp-error">{error}</div>}
    </div>
  );
}
