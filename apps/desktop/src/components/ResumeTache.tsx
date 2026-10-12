import { Icon, LoProgress } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type AppTask, estimationRestante, useTasks } from "../lib/taskCenter";
import { ModalShell } from "./ModalShell";

function duree(secondes: number): string {
  const s = Math.max(0, Math.round(secondes));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${String(s % 60).padStart(2, "0")} s`;
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")}`;
}

const LIBELLE: Record<AppTask["status"], string> = {
  running: "En cours",
  done: "Terminé",
  error: "Non abouti",
};

/**
 * Le résumé d'un plan, ouvert depuis le centre de tâches : où il en est,
 * depuis combien de temps, et combien de temps il reste à peu près.
 * Il suit la tâche en direct : la fenêtre peut rester ouverte.
 */
export function ResumeTache({ id, onClose }: { id: string; onClose: () => void }) {
  const t = useTasks().find((x) => x.id === id);
  const [maintenant, setMaintenant] = useState(() => Date.now());
  useEffect(() => {
    const minuterie = window.setInterval(() => setMaintenant(Date.now()), 1000);
    return () => window.clearInterval(minuterie);
  }, []);
  if (!t) return null;

  const etapes = t.steps ?? [];
  const total = etapes.length;
  const faites = Math.min(t.stepIndex ?? 0, total);
  const enCours = t.status === "running";
  const reste = estimationRestante(t, maintenant);
  const fin = enCours ? maintenant : t.updatedAt;

  return (
    <ModalShell label={`Résumé : ${t.label}`} onClose={onClose} style={{ maxWidth: 560 }}>
      <div className="lo-resume">
        <header className="lo-resume-tete">
          <h3 className="lo-resume-titre">{t.label}</h3>
          <span className={`lo-resume-etat is-${t.status}`}>{LIBELLE[t.status]}</span>
        </header>

        {total > 0 && (
          <>
            <LoProgress value={faites / total} on="surface-2" label={t.label} />
            <dl className="lo-resume-faits">
              <div>
                <dt>Étapes</dt>
                <dd>
                  {faites}/{total} ({Math.round((faites / total) * 100)} %)
                </dd>
              </div>
              <div>
                <dt>Depuis</dt>
                <dd>{duree((fin - t.createdAt) / 1000)}</dd>
              </div>
              {enCours && (
                <div>
                  <dt>Reste environ</dt>
                  <dd>{reste === null ? "estimé après la 1re étape" : duree(reste)}</dd>
                </div>
              )}
              {(t.attempt ?? 1) > 1 && (
                <div>
                  <dt>Essai</dt>
                  <dd>{t.attempt}</dd>
                </div>
              )}
            </dl>
            {enCours && faites < total && (
              <p className="lo-resume-courante">
                <strong>
                  Étape {faites + 1}/{total} :
                </strong>{" "}
                {etapes[faites]}
              </p>
            )}
            <ol className="locaryn-suivi-etapes">
              {etapes.map((texte, i) => {
                const etat =
                  i < faites
                    ? "fait"
                    : i === faites && enCours
                      ? "cours"
                      : i === faites && t.status === "error"
                        ? "echec"
                        : "attente";
                return (
                  // biome-ignore lint/suspicious/noArrayIndexKey: une étape n'a pas d'autre identité que son rang.
                  <li key={i} className={`is-${etat}`}>
                    <span className="locaryn-suivi-puce" aria-hidden>
                      {etat === "fait" ? (
                        <Icon name="check" size={12} />
                      ) : etat === "echec" ? (
                        <Icon name="close" size={12} />
                      ) : etat === "cours" ? (
                        <span className="locaryn-suivi-pouls" />
                      ) : null}
                    </span>
                    <span className="locaryn-suivi-texte">{texte}</span>
                  </li>
                );
              })}
            </ol>
          </>
        )}
        {total === 0 && (
          <p className="locaryn-field-hint">
            {enCours ? "Le plan est en cours d'écriture…" : (t.detail ?? "")}
          </p>
        )}
        {t.error && <div className="locaryn-vp-error">{t.error}</div>}
      </div>
    </ModalShell>
  );
}
