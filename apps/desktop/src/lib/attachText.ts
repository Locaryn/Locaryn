/**
 * Joindre un texte au message en cours d'écriture, comme un fichier.
 *
 * Le terminal (onglet de l'espace de travail ou panneau du bas) s'en sert pour
 * renvoyer au modèle la sortie d'une commande qu'il a proposée : la sélection
 * arrive dans le composeur en pièce jointe, et la personne écrit son
 * commentaire à côté.
 */

export const ATTACH_TEXT_EVENT = "locaryn:attach-text";

export interface AttachedText {
  name: string;
  text: string;
}

/** Joindre `text` au message, sous un nom lisible (« Terminal · 12 lignes »). */
export function attachTerminalText(text: string) {
  const propre = text.replace(/\s+$/, "");
  if (!propre) return;
  const lignes = propre.split("\n").length;
  const detail: AttachedText = {
    name: `Terminal · ${lignes} ligne${lignes > 1 ? "s" : ""}`,
    text: propre,
  };
  window.dispatchEvent(new CustomEvent<AttachedText>(ATTACH_TEXT_EVENT, { detail }));
}

/** La sélection courante si elle est entièrement dans `conteneur`. */
export function selectionIn(conteneur: HTMLElement | null): string | null {
  const sel = window.getSelection();
  if (!conteneur || !sel || sel.isCollapsed || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!conteneur.contains(range.commonAncestorContainer)) return null;
  const texte = sel.toString();
  return texte.trim() ? texte : null;
}
