import { useEffect, useState } from "react";
import { type CloudUsage, type CloudUsageWindow, core } from "../../lib/core";
import { avant, estHebdo, etatSansPourcentage, niveau } from "../chat/UsageGauge";

/** Relire les limites d'un fournisseur toutes les minutes. */
export function useProviderUsage(providerId: string): CloudUsage | null {
  const [usage, setUsage] = useState<CloudUsage | null>(null);
  useEffect(() => {
    let annule = false;
    const lire = () => {
      core
        .cloudUsageOf(providerId)
        .then((u) => {
          if (!annule) setUsage(u);
        })
        .catch((e: unknown) => {
          console.warn(`[Modèles] limites de ${providerId} illisibles :`, e);
          if (!annule) setUsage(null);
        });
    };
    lire();
    const t = window.setInterval(lire, 60_000);
    return () => {
      annule = true;
      window.clearInterval(t);
    };
  }, [providerId]);
  return usage;
}

function court(w: CloudUsageWindow): string {
  if (estHebdo(w)) return "Semaine";
  return w.label.replace(/^5 heures$/, "5 h");
}

function valeur(w: CloudUsageWindow): string {
  return typeof w.used_percent === "number"
    ? `reste ${Math.max(0, 100 - Math.round(w.used_percent))} %`
    : etatSansPourcentage(w);
}

function connues(usage: CloudUsage | null): CloudUsageWindow[] {
  return (usage?.windows ?? []).filter((w) => typeof w.used_percent === "number" || w.status);
}

/** Une ligne pour la tuile du dossier : « 5 h reste 34 % · Semaine reste 57 % ». */
export function UsageResume({ usage }: { usage: CloudUsage | null }) {
  const fenetres = connues(usage);
  if (fenetres.length === 0 && !usage?.credits) return null;
  return (
    <div className="locaryn-provider-usage-resume">
      {fenetres.map((w) => (
        <span
          key={w.id ?? w.label}
          className={`locaryn-provider-usage-chip is-${
            w.status === "rejected" ? "danger" : niveau(w.used_percent ?? 0)
          }`}
        >
          {court(w)} {valeur(w)}
        </span>
      ))}
      {usage?.credits && (
        <span className="locaryn-provider-usage-chip is-ok">
          {usage.credits.label ?? "Crédits"} {usage.credits.remaining}
          {usage.credits.unit ? ` ${usage.credits.unit}` : ""}
        </span>
      )}
    </div>
  );
}

/** Le détail, en haut du dossier ouvert. */
export function UsageCarte({ usage }: { usage: CloudUsage | null }) {
  const fenetres = connues(usage);
  if (!usage || (fenetres.length === 0 && !usage.credits)) return null;
  return (
    <section className="locaryn-provider-usage" aria-label="Limites d'utilisation">
      <div className="locaryn-provider-usage-head">
        <strong>Ce qu'il vous reste</strong>
        {usage.plan && <span>{usage.plan}</span>}
      </div>
      <div className="locaryn-provider-usage-grille">
        {fenetres.map((w) => {
          const pct = w.used_percent;
          return (
            <div key={w.id ?? w.label} className="locaryn-usage-row">
              <div className="locaryn-usage-row-head">
                <span>{w.label}</span>
                <span className="locaryn-usage-pct">
                  {typeof pct === "number"
                    ? `${Math.max(0, 100 - Math.round(pct))} % restants`
                    : etatSansPourcentage(w)}
                </span>
              </div>
              {typeof pct === "number" && (
                <div className={`locaryn-usage-bar is-${niveau(pct)}`}>
                  <span style={{ width: `${Math.min(100, pct)}%` }} />
                </div>
              )}
              {avant(w.resets_at) && (
                <div className="locaryn-usage-reset">Repart de zéro {avant(w.resets_at)}</div>
              )}
            </div>
          );
        })}
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
      </div>
      {usage.note && <p className="locaryn-usage-note">{usage.note}</p>}
    </section>
  );
}
