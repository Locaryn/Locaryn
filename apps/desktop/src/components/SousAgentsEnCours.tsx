import { Icon, LoProgress } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type AppTask, useTasks } from "../lib/taskCenter";

/** Ce que fait le sous-agent en ce moment : son dernier outil, sinon la
 *  dernière ligne qu'il a écrite. */
function activite(t: AppTask): string {
  const outil = t.tools?.at(-1);
  if (outil) return `Outil : ${outil}`;
  const ligne = (t.transcript ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .at(-1);
  if (!ligne || ligne.startsWith("Tâche :")) return "Réfléchit…";
  return ligne.length > 140 ? `…${ligne.slice(-140)}` : ligne;
}

function enClair(secondes: number): string {
  const s = Math.max(0, Math.round(secondes));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")}`;
}

/** « 3 min 12 · reste 16 min » quand la limite est connue. */
function temps(t: AppTask, maintenant: number): string {
  const ecoule = (maintenant - t.createdAt) / 1000;
  if (!t.limiteS) return enClair(ecoule);
  const reste = Math.max(0, t.limiteS - ecoule);
  return `${enClair(ecoule)} · reste ${reste < 60 ? enClair(reste) : `${Math.ceil(reste / 60)} min`}`;
}

/** Un tour = un appel d'outil : le modèle a agi, puis relit le résultat. */
function tours(t: AppTask): string {
  const n = t.tools?.length ?? 0;
  return `${n} tour${n > 1 ? "s" : ""}`;
}

/** La part du temps accordé déjà passée, ou `null` sans limite connue. */
function avancement(t: AppTask, maintenant: number): number | null {
  if (!t.limiteS) return null;
  return Math.min(100, Math.round((maintenant - t.createdAt) / 10 / t.limiteS));
}

/**
 * Les sous-agents que cette conversation a lancés et qui travaillent encore.
 *
 * `lancer_sous_agent` rend la main tout de suite : sans cette tuile, rien dans
 * le fil ne disait qu'une tâche tournait — il fallait ouvrir l'onglet Tâches.
 * Elle disparaît à la fin ; le compte rendu arrive alors dans la conversation.
 */
export function SousAgentsEnCours({ sessionId }: { sessionId: string }) {
  const actifs = useTasks().filter(
    (t) => t.type === "subagent" && t.parent === sessionId && t.status === "running",
  );
  const [maintenant, setMaintenant] = useState(() => Date.now());
  const enCours = actifs.length > 0;
  useEffect(() => {
    if (!enCours) return;
    const id = window.setInterval(() => setMaintenant(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [enCours]);

  if (!enCours) return null;
  return (
    <div className="locaryn-sous-agents" aria-live="polite">
      {actifs.map((t) => (
        <div
          key={t.id}
          className="locaryn-sous-agent"
          title="Le fil complet est dans l'onglet Tâches"
        >
          <div className="locaryn-sous-agent-tete">
            <Icon name="robot" size={14} />
            <span className="locaryn-sous-agent-nom">{t.label}</span>
            {t.model && <span className="locaryn-sous-agent-modele">{t.model}</span>}
            <span className="locaryn-sous-agent-duree">
              {tours(t)} · {temps(t, maintenant)}
            </span>
          </div>
          <LoProgress value={avancement(t, maintenant)} />
          <div className="locaryn-sous-agent-activite">{activite(t)}</div>
        </div>
      ))}
    </div>
  );
}
