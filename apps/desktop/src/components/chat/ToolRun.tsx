import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import { ToolCard, estOutilCommande, estOutilWeb } from "./ToolCard";

export type ToolEntry = {
  id: string;
  tool: string;
  args: unknown;
  status: "running" | "ok" | "error";
  output: string;
  live?: string;
};

type Props = {
  entries: ToolEntry[];
};

/** « Recherche web en cours… », « Exécuté 4 commandes (1 échec) » — le texte
 *  de la ligne repliée. Une recherche web se dit comme telle : sur une
 *  réponse qui cherche sur Google ou Brave, « Exécute 1 commande… » ne
 *  dit pas ce qui se passe. */
function resume(entries: ToolEntry[]): string {
  const total = entries.length;
  const encours = entries.filter((e) => e.status === "running").length;
  const rates = entries.filter((e) => e.status === "error").length;
  const web = entries.filter((e) => estOutilWeb(e.tool)).length;
  const webEnCours = entries.filter((e) => e.status === "running" && estOutilWeb(e.tool)).length;
  const autresEnCours = encours - webEnCours;
  const echecs = rates === 0 ? "" : rates === 1 ? " (1 échec)" : ` (${rates} échecs)`;

  if (encours > 0) {
    // Uniquement des recherches : c'est le libellé qui doit le dire.
    if (autresEnCours === 0) {
      return webEnCours === 1
        ? "Recherche web en cours…"
        : `${webEnCours} recherches web en cours…`;
    }
    if (webEnCours > 0) {
      const nom = autresEnCours > 1 ? "commandes" : "commande";
      return `Exécute ${autresEnCours} ${nom}… et ${webEnCours} ${
        webEnCours > 1 ? "recherches" : "recherche"
      } web`;
    }
    return `Exécute ${total} ${total > 1 ? "commandes" : "commande"}…`;
  }
  if (web === total) {
    return web === 1
      ? `Recherche web terminée${echecs}`
      : `${web} recherches web terminées${echecs}`;
  }
  // Mélange fini : le globe à gauche de la ligne dit déjà la présence du web,
  // le détail reste sur les cartes dépliées.
  const nom = total > 1 ? "commandes" : "commande";
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
  // Déplié de lui-même tant qu'une commande tourne, pour qu'on suive sa sortie
  // en direct ; replié une fois fini. Un clic fige le choix et l'emporte.
  const [manuel, setManuel] = useState<boolean | null>(null);
  if (entries.length === 0) return null;

  const encours = entries.some((e) => e.status === "running");
  const rate = entries.some((e) => e.status === "error");
  const surLeWeb = entries.some((e) => estOutilWeb(e.tool));
  const commandeEnCours = entries.some((e) => e.status === "running" && estOutilCommande(e.tool));
  const ouvert = manuel ?? commandeEnCours;

  return (
    <div className={`locaryn-toolrun${encours ? " locaryn-toolrun-live" : ""}`}>
      <button
        type="button"
        className="locaryn-toolrun-head"
        onClick={() => setManuel(!ouvert)}
        aria-expanded={ouvert}
      >
        {surLeWeb && (
          <span className="locaryn-toolrun-web" aria-hidden="true">
            <Icon name="globe" size={13} />
          </span>
        )}
        <span className="locaryn-toolrun-label">{resume(entries)}</span>
        {rate && !encours && (
          <span className="locaryn-toolrun-warn" aria-hidden="true">
            <Icon name="close" size={12} />
          </span>
        )}
        <span className="locaryn-toolrun-caret" aria-hidden="true">
          <Icon name={ouvert ? "caret-down" : "caret-right"} size={12} />
        </span>
      </button>

      {ouvert && (
        <div className="locaryn-toolrun-body">
          {entries.map((e) => (
            <ToolCard
              key={e.id}
              tool={e.tool}
              args={e.args}
              status={e.status}
              output={e.output}
              live={e.live}
            />
          ))}
        </div>
      )}
    </div>
  );
}
