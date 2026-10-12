import { Icon } from "@locaryn/ui-core";
import { useState } from "react";

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
  /** `validation` : le plan est écrit, il attend l'accord de la personne. */
  etat: "validation" | "cours" | "fait" | "echec" | "arrete";
  /** Ce qui reste à faire (objectif), ou la raison d'un échec. */
  note?: string;
};

const LIBELLE_ETAT: Record<Suivi["etat"], string> = {
  validation: "En attente de votre accord",
  cours: "En cours",
  fait: "Terminé",
  echec: "Non abouti",
  arrete: "Arrêté",
};

/**
 * Le suivi d'un plan (`/plan`, `/workflow`) ou d'un objectif (`/objectif`) :
 * combien d'étapes, lesquelles sont faites, laquelle tourne. Il remplace les
 * lignes de journal « Étape 2/5 » qui se perdaient entre les réponses.
 */
export function TaskTracker({
  suivi,
  onArreter,
  onDecider,
}: {
  suivi: Suivi;
  onArreter?: () => void;
  /** Exécuter (`true`) ou abandonner (`false`) un plan en attente d'accord. */
  onDecider?: (executer: boolean) => void;
}) {
  const faites = suivi.etapes.filter((e) => e.etat === "fait").length;
  const total = suivi.etapes.length;
  const enCours = suivi.etat === "cours";
  // Repliée, la carte garde son compte et sa barre : on voit où en est le
  // travail sans que la liste pousse la conversation vers le haut.
  const [replie, setReplie] = useState(false);
  const courante = suivi.etapes.findIndex((e) => e.etat === "cours");
  return (
    <section
      className={`locaryn-suivi is-${suivi.etat}`}
      aria-label={suivi.genre === "plan" ? "Plan" : "Objectif"}
    >
      <header className="locaryn-suivi-tete">
        <span className="locaryn-suivi-icone" aria-hidden>
          <Icon name={suivi.genre === "plan" ? "list-bullets" : "crosshair"} size={16} />
        </span>
        <span className="locaryn-suivi-titre">
          <strong>{suivi.genre === "plan" ? "Plan" : "Objectif"}</strong>
          <span>{suivi.titre}</span>
        </span>
        <span className="locaryn-suivi-compte" aria-live="polite">
          {suivi.genre === "plan" ? `${faites}/${total}` : `tour ${total}`}
        </span>
        {total > 0 && (
          <button
            type="button"
            className={`locaryn-suivi-replier${replie ? " is-replie" : ""}`}
            onClick={() => setReplie((r) => !r)}
            aria-expanded={!replie}
            aria-label={replie ? "Afficher les étapes" : "Masquer les étapes"}
            title={replie ? "Afficher les étapes" : "Masquer les étapes"}
          >
            <Icon name="caret-down" size={14} />
          </button>
        )}
      </header>

      {suivi.genre === "plan" && total > 0 && (
        <div className="locaryn-suivi-barre" aria-hidden>
          {suivi.etapes.map((e, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: une étape n'a pas d'autre identité que son rang.
            <span key={i} className={`is-${e.etat}`} />
          ))}
        </div>
      )}

      {replie && courante >= 0 && (
        <p className="locaryn-suivi-courante">
          Étape {courante + 1}/{total} : {suivi.etapes[courante].texte}
        </p>
      )}

      <ol className="locaryn-suivi-etapes" hidden={replie}>
        {suivi.etapes.map((e, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: une étape n'a pas d'autre identité que son rang.
          <li key={i} className={`is-${e.etat}`}>
            <span className="locaryn-suivi-puce" aria-hidden>
              {e.etat === "fait" ? (
                <Icon name="check" size={12} />
              ) : e.etat === "echec" ? (
                <Icon name="close" size={12} />
              ) : e.etat === "cours" ? (
                <span className="locaryn-suivi-pouls" />
              ) : null}
            </span>
            <span className="locaryn-suivi-texte">{e.texte}</span>
          </li>
        ))}
      </ol>

      <footer className="locaryn-suivi-pied">
        <span className={`locaryn-suivi-etat is-${suivi.etat}`}>
          {LIBELLE_ETAT[suivi.etat]}
          {suivi.genre === "plan" &&
            suivi.maxEssais > 1 &&
            ` · essai ${suivi.essai}/${suivi.maxEssais}`}
        </span>
        {suivi.note && <span className="locaryn-suivi-note">{suivi.note}</span>}
        {suivi.etat === "validation" && onDecider && (
          <span className="locaryn-suivi-decision">
            <button type="button" className="locaryn-btn-ghost" onClick={() => onDecider(false)}>
              Annuler
            </button>
            <button type="button" className="locaryn-btn-primary" onClick={() => onDecider(true)}>
              Exécuter le plan
            </button>
          </span>
        )}
        {enCours && onArreter && (
          <button type="button" className="locaryn-suivi-arreter" onClick={onArreter}>
            Arrêter
          </button>
        )}
      </footer>
    </section>
  );
}
