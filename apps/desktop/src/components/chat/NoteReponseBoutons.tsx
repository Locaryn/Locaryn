import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { useI18n } from "../../lib/i18n";
import type { GraviteNote, NoteReponse } from "../../lib/rapports";
import "../../styles/rapports.css";

type Etat =
  | { sorte: "repos" }
  | { sorte: "question" }
  | { sorte: "envoi"; note: NoteReponse }
  | { sorte: "envoye"; note: NoteReponse }
  | { sorte: "erreur"; message: string };

type Noter = (note: NoteReponse, gravite: GraviteNote) => Promise<void>;

/** Après un pouce baissé : bug critique, ou réponse simplement mauvaise ? */
function QuestionGravite(props: { onChoix: (g: GraviteNote) => void; onFermer: () => void }) {
  const { t } = useI18n();
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    ref.current?.querySelector("button")?.focus();
    const touche = (e: KeyboardEvent) => e.key === "Escape" && props.onFermer();
    const clic = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) props.onFermer();
    };
    document.addEventListener("keydown", touche);
    document.addEventListener("mousedown", clic);
    return () => {
      document.removeEventListener("keydown", touche);
      document.removeEventListener("mousedown", clic);
    };
  }, [props.onFermer]);

  return (
    <div
      ref={ref}
      className="locaryn-note-question"
      role="dialog"
      aria-label={t("Qu'est-ce qui ne va pas ?")}
    >
      <span className="locaryn-note-question-titre">{t("Qu'est-ce qui ne va pas ?")}</span>
      <button
        type="button"
        className="locaryn-note-choix"
        onClick={() => props.onChoix("critique")}
      >
        <Icon name="bug" size={16} />
        <span>
          <strong>{t("Bug critique")}</strong>
          <small>{t("Plantage, outil cassé, rien ne marche")}</small>
        </span>
      </button>
      <button type="button" className="locaryn-note-choix" onClick={() => props.onChoix("info")}>
        <Icon name="thumbs-down" size={16} />
        <span>
          <strong>{t("Réponse juste mauvaise")}</strong>
          <small>{t("Fausse, à côté ou inutile")}</small>
        </span>
      </button>
      <span className="locaryn-note-question-note">
        {t("Envoi anonyme : cet échange et les journaux, nettoyés.")}
      </span>
    </div>
  );
}

/**
 * Pouce levé / baissé à côté de « Copier » : la note part avec l'échange et
 * les journaux, nettoyés, pour savoir ce qui a aidé ou non. Un pouce baissé
 * demande d'abord s'il s'agit d'un bug critique : une erreur ne se voit pas
 * toujours dans les journaux.
 */
export function NoteReponseBoutons({ onRate }: { onRate: Noter }) {
  const { t } = useI18n();
  const [etat, setEtat] = useState<Etat>({ sorte: "repos" });
  const choisie = etat.sorte === "envoi" || etat.sorte === "envoye" ? etat.note : null;
  const fermer = useRef(() => setEtat({ sorte: "repos" })).current;

  async function noter(note: NoteReponse, gravite: GraviteNote) {
    setEtat({ sorte: "envoi", note });
    try {
      await onRate(note, gravite);
      setEtat({ sorte: "envoye", note });
    } catch (e) {
      console.warn("[Note] envoi impossible :", e);
      setEtat({ sorte: "erreur", message: e instanceof Error ? e.message : String(e) });
    }
  }

  function cliquer(note: NoteReponse) {
    if (etat.sorte === "envoi" || (etat.sorte === "envoye" && etat.note === note)) return;
    if (note === "up") void noter("up", "info");
    else setEtat({ sorte: "question" });
  }

  const bouton = (note: NoteReponse) => {
    const libelle = note === "up" ? t("Bonne réponse") : t("Mauvaise réponse");
    return (
      <button
        type="button"
        className="locaryn-msg-action locaryn-note-btn"
        data-done={choisie === note || (note === "down" && etat.sorte === "question")}
        data-busy={etat.sorte === "envoi" && etat.note === note}
        aria-pressed={choisie === note}
        aria-label={libelle}
        title={libelle}
        disabled={etat.sorte === "envoi"}
        onClick={() => cliquer(note)}
      >
        <Icon name={note === "up" ? "thumbs-up" : "thumbs-down"} size={16} />
      </button>
    );
  };

  return (
    <span className="locaryn-note" data-noted={choisie !== null || etat.sorte === "question"}>
      {bouton("up")}
      {bouton("down")}
      {etat.sorte === "question" && (
        <QuestionGravite onChoix={(g) => void noter("down", g)} onFermer={fermer} />
      )}
      <span className="locaryn-note-statut" aria-live="polite">
        {etat.sorte === "envoye" && t("Merci, note envoyée")}
        {etat.sorte === "erreur" && (
          <span className="locaryn-note-erreur" title={etat.message}>
            {t("Non envoyée")}
          </span>
        )}
      </span>
    </span>
  );
}
