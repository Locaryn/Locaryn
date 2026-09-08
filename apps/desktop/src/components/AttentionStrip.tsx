import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import type { AttentionItem } from "../lib/core";

/**
 * La question du modèle, au-dessus du champ de saisie.
 *
 * **Ce n'est pas une fenêtre modale, et c'est le point.** La personne n'est
 * peut-être pas devant l'écran, ou lit un autre projet, ou attend qu'un travail
 * de fond finisse. Lui barrer l'application pour lui demander qui doit voir une
 * note serait disproportionné — et la pousserait à cliquer n'importe quoi pour
 * retrouver son écran, ce qui vide la question de son sens.
 *
 * Elle peut donc naviguer, changer de conversation, revenir plus tard : la
 * question l'attend, et une pastille sur la conversation le rappelle. Le prix
 * de ce choix, c'est qu'une question sans réponse ne s'oublie pas toute seule ;
 * d'où le bouton « Écarter », qui rend l'attente sans limite acceptable.
 *
 * Le focus n'est jamais volé. Une personne en train d'écrire son message doit
 * pouvoir finir sa phrase.
 *
 * L'appelant monte ce composant avec l'identifiant de la question pour clé :
 * une nouvelle question repart donc d'un champ vide, sans effet à écrire.
 * Garder le texte tapé pour une question précédente l'enverrait en réponse à
 * une autre.
 */

/** Le temps écoulé, en mots. L'horodatage est un nombre de secondes Unix. */
function depuis(secondes: string): string {
  const t = Number(secondes);
  if (!Number.isFinite(t) || t <= 0) return "";
  const ecart = Math.max(0, Math.floor(Date.now() / 1000 - t));
  if (ecart < 60) return "à l'instant";
  if (ecart < 3600) return `il y a ${Math.floor(ecart / 60)} min`;
  if (ecart < 86_400) return `il y a ${Math.floor(ecart / 3600)} h`;
  return `il y a ${Math.floor(ecart / 86_400)} j`;
}

type Props = {
  /** La plus ancienne question ou alerte à régler. `null` : rien n'attend. */
  item: AttentionItem | null;
  onAnswer: (id: string, answer: { choice?: string | null; text?: string | null }) => void;
  onDismiss: (id: string) => void;
};

export function AttentionStrip({ item, onAnswer, onDismiss }: Props) {
  const [libre, setLibre] = useState("");
  const [champOuvert, setChampOuvert] = useState(false);

  if (!item) return null;

  // Repris dans une constante : une declaration de fonction est hissee, donc
  // le compilateur n'y voit plus le retour anticipe ci-dessus.
  const fiche = item;
  const erreur = fiche.urgency === "erreur";
  const texte = libre.trim();

  function envoyerLibre() {
    if (!texte) return;
    onAnswer(fiche.id, { text: texte });
  }

  return (
    <div
      className={`locaryn-attention${erreur ? " locaryn-attention-erreur" : ""}`}
      // `region` et non `alertdialog` : rien n'est modal, et le lecteur
      // d'écran ne doit pas annoncer un piège de focus qui n'existe pas.
      role="region"
      aria-label={erreur ? "Quelque chose est cassé" : "Le modèle vous demande quelque chose"}
    >
      <div className="locaryn-attention-tete">
        <span className={`locaryn-attention-puce${erreur ? " locaryn-puce-erreur" : ""}`}>
          <Icon name={erreur ? "warning" : "question"} size={14} />
        </span>
        <p className="locaryn-attention-titre">{item.title}</p>
        <span className="locaryn-attention-quand">{depuis(item.asked_at)}</span>
        <button
          type="button"
          className="locaryn-icon-btn locaryn-attention-fermer"
          // Une alerte se ferme, une question s'écarte : le mot doit dire
          // laquelle des deux, sinon « fermer » laisserait croire qu'on
          // répondra plus tard à une question qu'on vient d'abandonner.
          title={
            item.blocking
              ? "Écarter sans répondre — le modèle continuera avec l'option la plus prudente"
              : "Fermer"
          }
          aria-label={item.blocking ? "Écarter la question" : "Fermer l'alerte"}
          onClick={() => onDismiss(item.id)}
        >
          <Icon name="close" size={14} />
        </button>
      </div>

      {item.detail && <p className="locaryn-attention-detail">{item.detail}</p>}

      {item.choices.length > 0 && (
        <div className="locaryn-attention-choix">
          {item.choices.map((c) => (
            <button
              key={c.id}
              type="button"
              className="locaryn-attention-option"
              title={c.hint ?? undefined}
              onClick={() => onAnswer(item.id, { choice: c.id })}
            >
              <span className="locaryn-attention-option-label">{c.label}</span>
              {c.hint && <span className="locaryn-attention-option-hint">{c.hint}</span>}
            </button>
          ))}
        </div>
      )}

      {item.free_text &&
        (champOuvert ? (
          <div className="locaryn-attention-libre">
            <input
              type="text"
              value={libre}
              placeholder={item.free_text}
              aria-label={item.free_text}
              onChange={(e) => setLibre(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  envoyerLibre();
                }
                // Échap referme le champ sans écarter la question : se
                // tromper de bouton ne doit pas coûter la question entière.
                if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                  setChampOuvert(false);
                }
              }}
              /* Le focus est pris parce que la personne vient de cliquer pour
                 ouvrir ce champ : c'est sa demande, pas une interruption. */
              autoFocus
            />
            <button
              type="button"
              className="locaryn-btn-primary"
              disabled={!texte}
              onClick={envoyerLibre}
            >
              Répondre
            </button>
          </div>
        ) : (
          <button
            type="button"
            className="locaryn-attention-autre"
            onClick={() => setChampOuvert(true)}
          >
            <Icon name="edit" size={13} /> {item.free_text}
          </button>
        ))}
    </div>
  );
}
