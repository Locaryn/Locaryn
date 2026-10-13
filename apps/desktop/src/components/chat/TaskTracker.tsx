import { Icon } from "@locaryn/ui-core";

/** L'état d'une étape d'un plan, ou d'une tâche d'objectif. */
export type EtatEtape = "attente" | "cours" | "fait" | "echec";

/** Un plan ou un objectif suivi dans la conversation. */
export type Suivi = {
  /** `plan` : des étapes connues d'avance. `objectif` : les tâches de son plan. */
  genre: "plan" | "objectif";
  titre: string;
  etapes: { texte: string; etat: EtatEtape }[];
  /** L'essai en cours quand une vérification fait reprendre. */
  essai: number;
  maxEssais: number;
  etat: "cours" | "fait" | "echec" | "arrete";
  /** La raison d'un échec, ou la tâche qui a été reprise. */
  note?: string;
};

const LIBELLE_ETAT: Record<Suivi["etat"], string> = {
  cours: "En cours",
  fait: "Terminé",
  echec: "Non abouti",
  arrete: "Arrêté",
};

/**
 * Le suivi d'un plan (`/plan`, `/workflow`) ou d'un objectif (`/objectif`) :
 * combien de tâches, lesquelles sont faites, laquelle tourne. Il remplace les
 * lignes de journal « Étape 2/5 » qui se perdaient entre les réponses.
 */
export function TaskTracker({ suivi, onArreter }: { suivi: Suivi; onArreter?: () => void }) {
  const faites = suivi.etapes.filter((e) => e.etat === "fait").length;
  const total = suivi.etapes.length;
  const enCours = suivi.etat === "cours";
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
          {total > 0 ? `${faites}/${total}` : "…"}
        </span>
      </header>

      {total > 0 && (
        <div className="locaryn-suivi-barre" aria-hidden>
          {suivi.etapes.map((e, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: une étape n'a pas d'autre identité que son rang.
            <span key={i} className={`is-${e.etat}`} />
          ))}
        </div>
      )}

      <ol className="locaryn-suivi-etapes">
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
              ) : (
                i + 1
              )}
            </span>
            <span className="locaryn-suivi-texte">{e.texte}</span>
          </li>
        ))}
      </ol>

      <footer className="locaryn-suivi-pied">
        <span className={`locaryn-suivi-etat is-${suivi.etat}`}>
          {LIBELLE_ETAT[suivi.etat]}
          {suivi.maxEssais > 1 && ` · essai ${suivi.essai}/${suivi.maxEssais}`}
        </span>
        {suivi.note && <span className="locaryn-suivi-note">{suivi.note}</span>}
        {enCours && onArreter && (
          <button type="button" className="locaryn-suivi-arreter" onClick={onArreter}>
            Arrêter
          </button>
        )}
      </footer>
    </section>
  );
}
