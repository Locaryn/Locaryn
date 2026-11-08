import { type KeyboardEvent as ReactKeyboardEvent, useCallback, useEffect, useState } from "react";

/**
 * Un seul menu contextuel ouvert à la fois dans toute l'application.
 *
 * Chaque composant garde son propre contenu de menu, mais l'ouverture passe par
 * ce module : ouvrir un menu ferme celui qui était ouvert ailleurs, et un clic
 * dehors, Échap, un défilement, un clic droit ailleurs ou la perte du focus de
 * la fenêtre les ferment tous.
 */

const DANS_UN_MENU = '[role="menu"], [data-menu-trigger]';

let fermeurCourant: (() => void) | null = null;
let ecouteursPoses = false;

/** Ferme le menu ouvert, où qu'il soit. */
export function fermerTousLesMenus(): void {
  const fermeur = fermeurCourant;
  fermeurCourant = null;
  fermeur?.();
}

function estDansUnMenu(cible: EventTarget | null): boolean {
  return cible instanceof Element && cible.closest(DANS_UN_MENU) !== null;
}

function poserEcouteurs(): void {
  if (ecouteursPoses) return;
  ecouteursPoses = true;
  // Phase de capture : un nouveau clic droit ferme l'ancien menu avant que le
  // composant visé n'ouvre le sien.
  window.addEventListener(
    "mousedown",
    (e) => {
      if (!estDansUnMenu(e.target)) fermerTousLesMenus();
    },
    true,
  );
  window.addEventListener("contextmenu", () => fermerTousLesMenus(), true);
  window.addEventListener(
    "scroll",
    (e) => {
      if (!estDansUnMenu(e.target)) fermerTousLesMenus();
    },
    true,
  );
  window.addEventListener(
    "keydown",
    (e) => {
      if (e.key === "Escape") fermerTousLesMenus();
    },
    true,
  );
  window.addEventListener("blur", fermerTousLesMenus);
  window.addEventListener("resize", fermerTousLesMenus);
}

/** Les entrées actives d'un menu, dans l'ordre d'affichage. */
function entrees(menu: HTMLElement): HTMLElement[] {
  return Array.from(menu.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)'));
}

/** Flèches, Début et Fin déplacent le focus d'une entrée à l'autre. */
export function navigationMenu(e: ReactKeyboardEvent<HTMLElement>): void {
  const liste = entrees(e.currentTarget);
  if (liste.length === 0) return;
  const index = liste.indexOf(document.activeElement as HTMLElement);
  let cible: number | null = null;
  if (e.key === "ArrowDown") cible = (index + 1) % liste.length;
  else if (e.key === "ArrowUp") cible = (index <= 0 ? liste.length : index) - 1;
  else if (e.key === "Home") cible = 0;
  else if (e.key === "End") cible = liste.length - 1;
  if (cible === null) return;
  e.preventDefault();
  liste[cible]?.focus();
}

/** À poser en `ref` du menu : donne le focus à la première entrée à l'ouverture. */
export function focusPremiereEntree(menu: HTMLElement | null): void {
  if (menu) entrees(menu)[0]?.focus();
}

/**
 * État d'un menu contextuel participant à la règle « un seul ouvert ».
 * `ouvrir` ferme d'abord celui d'un autre composant.
 */
export function useMenuUnique<T>(): {
  menu: T | null;
  ouvrir: (valeur: T) => void;
  fermer: () => void;
} {
  const [menu, setMenu] = useState<T | null>(null);

  useEffect(() => {
    poserEcouteurs();
  }, []);

  const fermer = useCallback(() => setMenu(null), []);

  const ouvrir = useCallback(
    (valeur: T) => {
      fermerTousLesMenus();
      fermeurCourant = fermer;
      setMenu(valeur);
    },
    [fermer],
  );

  // Un menu fermé par le composant lui-même ne doit plus être « le courant ».
  useEffect(() => {
    if (menu === null && fermeurCourant === fermer) fermeurCourant = null;
  }, [menu, fermer]);

  return { menu, ouvrir, fermer };
}
