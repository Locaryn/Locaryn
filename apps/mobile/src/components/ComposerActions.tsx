import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { type ComposerAction, api } from "../lib/core";

type Props = {
  /** Ce que le champ contient — envoyé aux actions de type `tool`. */
  draft: string;
  /** Ce qu'il doit contenir ensuite. */
  onDraft: (texte: string) => void;
  /** Une erreur à montrer, plutôt que d'échouer en silence. */
  onError?: (message: string) => void;
};

/** Les icônes que le jeu partagé connaît. Une extension nomme, elle ne dessine pas. */
const CONNUES = new Set<string>([
  "mic",
  "sound",
  "music",
  "image",
  "video",
  "edit",
  "search",
  "translate",
  "chart",
  "models",
  "star",
  "plus",
  "cube",
  "target",
  "extensions",
]);

/**
 * Les boutons que les extensions posent à côté du champ de saisie.
 *
 * C'est le cas de la dictée : une extension de reconnaissance vocale ajoute un
 * micro, et il doit apparaître sur le téléphone — c'est même là qu'il sert le
 * plus. Rien n'est codé en dur pour elle : le manifeste décrit le bouton,
 * l'application le dessine.
 *
 * Deux comportements seulement. `insert` écrit un texte dans le champ ; `tool`
 * appelle un outil du serveur avec ce que le champ contient et met la réponse
 * à la place. Faire tourner du code d'extension dans l'interface reviendrait à
 * lui donner l'écran entier, ce qui n'arrivera pas.
 */
export function ComposerActions({ draft, onDraft, onError }: Props) {
  const [actions, setActions] = useState<ComposerAction[]>([]);
  const [occupe, setOccupe] = useState<string | null>(null);

  useEffect(() => {
    let annule = false;
    void (async () => {
      try {
        const exts = await api.listExtensions();
        if (annule) return;
        // Une extension éteinte n'apporte rien : le serveur vide déjà son
        // bloc `ui`, on ne fait que le refléter.
        setActions(exts.flatMap((e) => e.ui?.composer_actions ?? []));
      } catch {
        setActions([]);
      }
    })();
    const relire = () => {
      void api
        .listExtensions()
        .then((exts) => setActions(exts.flatMap((e) => e.ui?.composer_actions ?? [])))
        .catch(() => setActions([]));
    };
    window.addEventListener("locaryn:extensions-changed", relire);
    return () => {
      annule = true;
      window.removeEventListener("locaryn:extensions-changed", relire);
    };
  }, []);

  if (actions.length === 0) return null;

  async function agir(a: ComposerAction) {
    if (a.action === "insert") {
      onDraft(draft ? `${draft} ${a.value}` : a.value);
      return;
    }
    setOccupe(a.id);
    try {
      const texte = await api.runComposerTool(a.value, draft);
      if (texte) onDraft(texte);
    } catch (e) {
      onError?.(String(e));
    } finally {
      setOccupe(null);
    }
  }

  return (
    <div className="lo-compose-actions">
      {actions.map((a) => (
        <button
          key={a.id}
          type="button"
          className="lo-compose-action"
          disabled={occupe === a.id}
          onClick={() => void agir(a)}
          aria-label={a.label}
          title={a.hint ?? a.label}
        >
          <Icon
            name={(a.icon && CONNUES.has(a.icon) ? a.icon : "extensions") as IconName}
            size={18}
          />
        </button>
      ))}
    </div>
  );
}
