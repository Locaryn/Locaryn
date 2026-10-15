import { useCallback, useEffect, useRef, useState } from "react";
import { type CloudUsage, core } from "../../lib/core";

/** « dans 2 h 13 », « dans 4 j » : quand la fenêtre repart de zéro. */
function avant(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  const s = Math.max(0, Math.round((t - Date.now()) / 1000));
  if (s < 60) return "dans un instant";
  const min = Math.round(s / 60);
  if (min < 60) return `dans ${min} min`;
  const h = Math.floor(min / 60);
  if (h < 48) return `dans ${h} h ${String(min % 60).padStart(2, "0")}`;
  return `dans ${Math.round(h / 24)} j`;
}

function niveau(pct: number): "ok" | "warn" | "danger" {
  return pct >= 90 ? "danger" : pct >= 70 ? "warn" : "ok";
}

/**
 * La jauge des limites d'un abonnement (Claude, Codex…).
 *
 * Elle n'existe que pour un modèle payant dont le morph publie ses quotas :
 * un modèle local n'a pas de limite à suivre, et rien ne s'affiche. Un
 * anneau montre la fenêtre la plus entamée ; le clic ouvre le détail de
 * chaque fenêtre (5 heures, semaine…) et quand elle repart de zéro.
 */
export function UsageGauge({ generating }: { generating: boolean }) {
  const [usage, setUsage] = useState<CloudUsage | null>(null);
  const [ouvert, setOuvert] = useState(false);
  const boite = useRef<HTMLDivElement>(null);

  const lire = useCallback(() => {
    core
      .cloudUsage()
      .then(setUsage)
      .catch(() => setUsage(null));
  }, []);

  useEffect(() => {
    lire();
    const t = window.setInterval(lire, 60_000);
    const surModele = () => lire();
    window.addEventListener("locaryn:active-model-changed", surModele);
    return () => {
      window.clearInterval(t);
      window.removeEventListener("locaryn:active-model-changed", surModele);
    };
  }, [lire]);

  // Une réponse finie a consommé du quota : on relit.
  const avantGen = useRef(generating);
  useEffect(() => {
    if (avantGen.current && !generating) lire();
    avantGen.current = generating;
  }, [generating, lire]);

  useEffect(() => {
    if (!ouvert) return;
    const dehors = (e: MouseEvent) => {
      if (boite.current && !boite.current.contains(e.target as Node)) setOuvert(false);
    };
    document.addEventListener("mousedown", dehors);
    return () => document.removeEventListener("mousedown", dehors);
  }, [ouvert]);

  const fenetres = (usage?.windows ?? []).filter((w) => typeof w.used_percent === "number");
  if (!usage || (fenetres.length === 0 && !usage.credits)) return null;

  const pire = fenetres.reduce((m, w) => Math.max(m, w.used_percent ?? 0), 0);
  const r = 7;
  const c = 2 * Math.PI * r;

  return (
    <div className="locaryn-usage" ref={boite}>
      <button
        type="button"
        className={`locaryn-usage-pill is-${niveau(pire)}`}
        onClick={() => setOuvert((o) => !o)}
        aria-expanded={ouvert}
        title={`Limites de ${usage.provider ?? "l'abonnement"} : ${Math.round(pire)} % utilisés`}
      >
        <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">
          <circle cx="9" cy="9" r={r} className="locaryn-usage-rail" />
          <circle
            cx="9"
            cy="9"
            r={r}
            className="locaryn-usage-arc"
            strokeDasharray={`${(Math.min(pire, 100) / 100) * c} ${c}`}
            transform="rotate(-90 9 9)"
          />
        </svg>
        <span>{Math.round(pire)} %</span>
      </button>
      {ouvert && (
        <div className="locaryn-usage-pop" role="dialog" aria-label="Limites d'utilisation">
          <div className="locaryn-usage-pop-head">
            <strong>{usage.provider ?? "Abonnement"}</strong>
            {usage.plan && <span>{usage.plan}</span>}
          </div>
          {fenetres.map((w) => (
            <div key={w.id ?? w.label} className="locaryn-usage-row">
              <div className="locaryn-usage-row-head">
                <span>{w.label}</span>
                <span className="locaryn-usage-pct">{Math.round(w.used_percent ?? 0)} %</span>
              </div>
              <div className={`locaryn-usage-bar is-${niveau(w.used_percent ?? 0)}`}>
                <span style={{ width: `${Math.min(100, w.used_percent ?? 0)}%` }} />
              </div>
              {avant(w.resets_at) && (
                <div className="locaryn-usage-reset">Repart de zéro {avant(w.resets_at)}</div>
              )}
            </div>
          ))}
          {usage.credits && (
            <div className="locaryn-usage-row">
              <div className="locaryn-usage-row-head">
                <span>{usage.credits.label ?? "Crédits"}</span>
                <span className="locaryn-usage-pct">
                  {usage.credits.remaining}
                  {usage.credits.unit ? ` ${usage.credits.unit}` : ""}
                </span>
              </div>
            </div>
          )}
          {usage.note && <p className="locaryn-usage-note">{usage.note}</p>}
        </div>
      )}
    </div>
  );
}
