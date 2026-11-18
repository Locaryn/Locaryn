/**
 * Les messages passagers de l'application (« toasts »).
 *
 * Un seul canal, l'événement `locaryn:toast` : l'application l'émet par
 * {@link afficherMessage}, les morphs par `api.ui.showToast`, et le composant
 * `Messages` l'affiche. Avant lui, l'événement partait sans que personne
 * l'écoute.
 */

export type GenreMessage = "info" | "success" | "warning" | "error";

export interface DetailMessage {
  message: string;
  type: GenreMessage;
}

export const EVENEMENT_MESSAGE = "locaryn:toast";

export function afficherMessage(message: string, type: GenreMessage = "info"): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent<DetailMessage>(EVENEMENT_MESSAGE, { detail: { message, type } }),
  );
}

function texteErreur(e: unknown): string {
  const brut = e instanceof Error ? e.message : String(e);
  return brut.replace(/^Error:\s*/, "");
}

/**
 * Pour un geste de l'utilisateur : l'échec se voit. `action` dit ce qui a
 * raté (« Ouverture du dossier »), l'erreur suit.
 */
export function echecVisible(action: string): (e: unknown) => null {
  return (e: unknown) => {
    console.warn(`${action} :`, e);
    afficherMessage(`${action} impossible : ${texteErreur(e)}`, "error");
    return null;
  };
}

/**
 * Pour un travail de fond : l'échec n'interrompt rien, mais il laisse une
 * trace avec son contexte. Rend `null`, ce qui sert de valeur de repli.
 */
export function echecJournalise(contexte: string): (e: unknown) => null {
  return (e: unknown) => {
    console.warn(`${contexte} :`, e);
    return null;
  };
}
