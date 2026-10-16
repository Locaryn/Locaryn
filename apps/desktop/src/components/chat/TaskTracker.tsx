import { Icon } from "@locaryn/ui-core";
import { useState } from "react";

const CLE_REPLI = "locaryn.suivi.replie";

/** Replié par défaut : une ligne, comme la liste de tâches de Freebuff ou de
 *  Claude Code. Déplié, il est retenu d'un plan à l'autre. */
function lireRepli(): boolean {
  try {
    return localStorage.getItem(CLE_REPLI) !== "0";
  } catch {
    return true;
  }
}

/** L'état d'une étape d'un plan, ou d'un tour d'objectif. */
export type EtatEtape = "attente" | "cours" | "fait" | "echec";

/** Un plan ou un objectif suivi dans la conversation. */
export type Suivi = {
  /** `plan` : des étapes connues d'avance. `objectif` : des tours jusqu'au but. */
  genre: "plan" | "objectif";
  titre: string;
  etapes: { texte: string; etat: EtatEtape }[];
  /** Plan : l'essai en cours quand la vérification fait reprendre. */
  essai: number;
  maxEssais: number;
  etat: "cours" | "fait" | "echec" | "arrete";
  /** Ce qui reste à faire (objectif), ou la raison d'un échec. */
  note?: string;
};

const LIBELLE_ETAT: Record<Suivi["etat"], string> = {
  cours: "En cours",
  fait: "Terminé",
  echec: "Non abouti",
  arrete: "Arrêté",
};

/**
 * Le suivi d'un plan (`/plan`, `/workflow`) ou d'un objectif (`/objectif`),
 * posé au-dessus de la zone de saisie, à la largeur de la conversation.
 *
 * Replié : une ligne — « Plan 2/5 », l'étape en cours, l'essai. Déplié : la
 * liste, les étapes faites cochées et barrées. Il remplace les lignes de
 * journal « Étape 2/5 » qui se perdaient entre les réponses, sans plus
 * cacher la conversation comme le grand cadre d'avant.
 */
export function TaskTracker({
  suivi,
  onArreter,
  onFermer,
}: {
  suivi: Suivi;
  onArreter?: () => void;
  onFermer?: () => void;
}) {
  const [replie, setReplie] = useState(lireRepli);
  const basculer = () => {
    setReplie((r) => {
      try {
        localStorage.setItem(CLE_REPLI, r ? "0" : "1");
      } catch {
        // Stockage indisponible : le choix vaut pour cette fenêtre.
      }
      return !r;
    });
  };
  const faites = suivi.etapes.filter((e) => e.etat === "fait").length;
  const courante =
    suivi.etapes.find((e) => e.etat === "cours") ?? suivi.etapes.find((e) => e.etat === "echec");
  const total = suivi.etapes.length;
  const enCours = suivi.etat === "cours";
  const etat = `${LIBELLE_ETAT[suivi.etat]}${
    suivi.genre === "plan" && suivi.maxEssais > 1 && suivi.essai > 1
      ? ` · essai ${suivi.essai}/${suivi.maxEssais}`
      : ""
  }`;
  return (
    <section
      className={`locaryn-todo is-${suivi.etat}${replie ? "" : " is-ouvert"}`}
      aria-label={suivi.genre === "plan" ? "Plan" : "Objectif"}
    >
      <div className="locaryn-todo-ligne">
        <button
          type="button"
          className="locaryn-todo-tete"
          onClick={basculer}
          aria-expanded={!replie}
          title={replie ? "Voir toutes les étapes" : "Replier"}
        >
          <Icon name={suivi.genre === "plan" ? "list-bullets" : "crosshair"} size={14} />
          <span className="locaryn-todo-libelle">
            {suivi.genre === "plan" ? "Plan" : "Objectif"}
          </span>
          <span className="locaryn-todo-compte" aria-live="polite">
            {suivi.genre === "plan" ? `${faites}/${total}` : `tour ${total}`}
          </span>
          <span className="locaryn-todo-courante">
            {replie ? (courante?.texte ?? suivi.titre) : suivi.titre}
          </span>
          <span className={`locaryn-todo-etat is-${suivi.etat}`}>{etat}</span>
          <Icon name={replie ? "caret-right" : "caret-down"} size={13} />
        </button>
        {enCours && onArreter && (
          <button type="button" className="locaryn-todo-arreter" onClick={onArreter}>
            Arrêter
          </button>
        )}
        {!enCours && onFermer && (
          <button
            type="button"
            className="locaryn-todo-fermer"
            onClick={onFermer}
            title="Fermer ce suivi"
            aria-label="Fermer ce suivi"
          >
            <Icon name="x" size={13} />
          </button>
        )}
      </div>

      {!replie && (
        <>
          <ol className="locaryn-todo-liste">
            {suivi.etapes.map((e, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: une étape n'a pas d'autre identité que son rang.
              <li key={i} className={`is-${e.etat}`}>
                <span className="locaryn-todo-puce" aria-hidden>
                  {e.etat === "fait" ? (
                    <Icon name="check" size={11} />
                  ) : e.etat === "echec" ? (
                    <Icon name="x" size={11} />
                  ) : e.etat === "cours" ? (
                    <span className="locaryn-todo-pouls" />
                  ) : null}
                </span>
                <span className="locaryn-todo-texte">{e.texte}</span>
              </li>
            ))}
          </ol>
          {suivi.note && <p className="locaryn-todo-note">{suivi.note}</p>}
        </>
      )}
    </section>
  );
}
