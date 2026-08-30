import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import { ToolCard } from "./ToolCard";

export type ToolEntry = {
  id: string;
  tool: string;
  args: unknown;
  status: "running" | "ok" | "error";
  output: string;
};

type Props = {
  entries: ToolEntry[];
};

/** « Exécuté 4 commandes (1 échec) » — le texte de la ligne repliée. */
function resume(entries: ToolEntry[]): string {
  const total = entries.length;
  const encours = entries.filter((e) => e.status === "running").length;
  const rates = entries.filter((e) => e.status === "error").length;

  const nom = total > 1 ? "commandes" : "commande";
  if (encours > 0) {
    return `Exécute ${total} ${nom}…`;
  }
  const echecs = rates === 0 ? "" : rates === 1 ? " (1 échec)" : ` (${rates} échecs)`;
  return `Exécuté ${total} ${nom}${echecs}`;
}

/**
 * Une suite d'appels d'outils, repliée en une ligne.
 *
 * Une carte par appel noyait la reponse : sur une reponse qui lit six fichiers
 * et lance deux commandes, huit cartes separaient chaque phrase de la
 * suivante. Le detail reste a un clic — c'est la place qu'il occupe par defaut
 * qui change, pas sa disponibilite.
 */
export function ToolRun({ entries }: Props) {
  const [ouvert, setOuvert] = useState(false);
  if (entries.length === 0) return null;

  const encours = entries.some((e) => e.status === "running");
  const rate = entries.some((e) => e.status === "error");

  return (
    <div className={`locaryn-toolrun${encours ? " locaryn-toolrun-live" : ""}`}>
      <button
        type="button"
        className="locaryn-toolrun-head"
        onClick={() => setOuvert((o) => !o)}
        aria-expanded={ouvert}
      >
        <span className="locaryn-toolrun-label">{resume(entries)}</span>
        {rate && !encours && (
          <span className="locaryn-toolrun-warn" aria-hidden="true">
            <Icon name="close" size={12} />
          </span>
        )}
        <span className="locaryn-toolrun-caret" aria-hidden="true">
          {ouvert ? "▾" : "›"}
        </span>
      </button>

      {ouvert && (
        <div className="locaryn-toolrun-body">
          {entries.map((e) => (
            <ToolCard key={e.id} tool={e.tool} args={e.args} status={e.status} output={e.output} />
          ))}
        </div>
      )}
    </div>
  );
}
