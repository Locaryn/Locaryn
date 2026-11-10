// Les terminaux ouverts, et de quoi leur écrire de l'extérieur.
//
// Le bouton « Exécuter » d'une réponse ne lance plus la commande dans un
// panneau à part : il l'écrit dans le terminal de l'espace de travail, celui
// que la personne regarde. L'onglet n'existe peut-être pas encore, ou n'a pas
// fini de s'ouvrir : ce qui est écrit attend alors que son shell soit prêt.

import { getWorkspace, openTab } from "./workspace";

/** Ce qu'un terminal monté expose. */
export interface TerminalOuvert {
  /** Écrire comme si la personne l'avait tapé. */
  ecrire: (texte: string) => void;
}

const ouverts = new Map<string, TerminalOuvert>();
const enAttente = new Map<string, string[]>();

/** Un terminal est prêt : ce qui l'attendait part. */
export function enregistrerTerminal(onglet: string, terminal: TerminalOuvert): () => void {
  ouverts.set(onglet, terminal);
  const attendu = enAttente.get(onglet);
  if (attendu) {
    enAttente.delete(onglet);
    for (const t of attendu) terminal.ecrire(t);
  }
  return () => {
    if (ouverts.get(onglet) === terminal) ouverts.delete(onglet);
  };
}

/** Un onglet Terminal se ferme : ce qui l'attendait n'a plus de destinataire. */
export function oublierTerminal(onglet: string): void {
  ouverts.delete(onglet);
  enAttente.delete(onglet);
}

/** L'onglet Terminal où écrire : l'actif s'il en est un, sinon le premier. */
function ongletCible(): string | null {
  const { tabs, active } = getWorkspace();
  const terminaux = tabs.filter((t) => t.kind === "terminal");
  if (terminaux.length === 0) return null;
  return terminaux.find((t) => t.id === active)?.id ?? terminaux[0].id;
}

/**
 * Taper `texte` dans le terminal de l'espace de travail, en l'ouvrant s'il le
 * faut. `entree` : valider la ligne (Entrée) après le texte.
 */
export function ecrireDansLeTerminal(texte: string, entree = true): void {
  openTab("terminal");
  const onglet = ongletCible();
  if (!onglet) return;
  // Chaque ligne se valide : un bloc de plusieurs commandes s'exécute en
  // entier, comme collé dans un shell.
  const envoye = `${texte.replace(/\r?\n/g, "\r")}${entree ? "\r" : ""}`;
  const terminal = ouverts.get(onglet);
  if (terminal) {
    terminal.ecrire(envoye);
    return;
  }
  enAttente.set(onglet, [...(enAttente.get(onglet) ?? []), envoye]);
}
