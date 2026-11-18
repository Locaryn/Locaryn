import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type DetailMessage, EVENEMENT_MESSAGE } from "../lib/messages";
import "../styles/messages.css";

interface MessageAffiche extends DetailMessage {
  id: number;
}

/** Combien de temps un message reste, en millisecondes. Une erreur se lit
 *  plus lentement qu'une confirmation. */
const DUREE_MS = { info: 4000, success: 4000, warning: 7000, error: 8000 } as const;
const MAX_VISIBLES = 4;

let prochainId = 0;

/** La pile des messages passagers, en bas à droite. */
export function Messages() {
  const [file, setFile] = useState<MessageAffiche[]>([]);

  useEffect(() => {
    const minuteurs = new Set<number>();
    const recevoir = (ev: Event) => {
      const detail = (ev as CustomEvent<DetailMessage>).detail;
      if (!detail?.message) return;
      const id = ++prochainId;
      const type = detail.type ?? "info";
      setFile((f) => [...f, { id, message: detail.message, type }].slice(-MAX_VISIBLES));
      const minuteur = window.setTimeout(() => {
        minuteurs.delete(minuteur);
        setFile((f) => f.filter((m) => m.id !== id));
      }, DUREE_MS[type]);
      minuteurs.add(minuteur);
    };
    window.addEventListener(EVENEMENT_MESSAGE, recevoir);
    return () => {
      window.removeEventListener(EVENEMENT_MESSAGE, recevoir);
      for (const m of minuteurs) window.clearTimeout(m);
    };
  }, []);

  if (file.length === 0) return null;
  return (
    <div className="locaryn-messages" role="status" aria-live="polite">
      {file.map((m) => (
        <div key={m.id} className={`locaryn-message is-${m.type}`}>
          <span className="locaryn-message-texte">{m.message}</span>
          <button
            type="button"
            className="locaryn-message-fermer"
            aria-label="Fermer le message"
            onClick={() => setFile((f) => f.filter((x) => x.id !== m.id))}
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
