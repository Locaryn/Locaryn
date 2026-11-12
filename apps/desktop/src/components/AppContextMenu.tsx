import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ouvrirLien } from "../lib/apercuPage";
import { attachTerminalText } from "../lib/attachText";
import { isTauri } from "../lib/core";
import { focusPremiereEntree, navigationMenu, useMenuUnique } from "../lib/menuUnique";
import { ecrirePressePapiers } from "../lib/pressePapiers";

/**
 * Le clic droit de l'application.
 *
 * Le menu natif de la vue web (« Enregistrer sous », « Imprimer »,
 * « Actualiser », outils du navigateur) n'a rien à faire dans une application :
 * il imprimait une page vide ou rechargeait tout en perdant la conversation.
 * Il est remplacé ici par des actions qui dépendent de ce qu'on vise — du
 * texte sélectionné, un lien, un message — et par rien du tout ailleurs.
 *
 * Un champ de saisie garde le menu natif : c'est le seul qui connaît les
 * suggestions du correcteur d'orthographe, qu'aucune page ne peut lire. Le
 * menu de l'application les avait fait disparaître. Un composant qui pose son propre menu (les conversations, les
 * modèles installés) garde la main : il appelle `preventDefault` avant nous.
 */

type Action = { label: string; run: () => void | Promise<void> };
type Menu = { x: number; y: number; actions: Action[] };

const EDITABLE =
  'input:not([type=checkbox]):not([type=radio]):not([type=range]), textarea, [contenteditable=""], [contenteditable=true]';

/** Copier `texte` ; repli sur une zone cachée quand l'API refuse. */
async function copier(texte: string) {
  try {
    await ecrirePressePapiers(texte);
  } catch (e) {
    console.warn("presse-papier refusé, repli :", e);
    const zone = document.createElement("textarea");
    zone.value = texte;
    zone.style.position = "fixed";
    zone.style.opacity = "0";
    document.body.appendChild(zone);
    zone.select();
    document.execCommand("copy");
    zone.remove();
  }
}

function actionsPour(cible: HTMLElement): Action[] {
  const selection = window.getSelection()?.toString().trim() ?? "";
  const actions: Action[] = [];
  if (selection && cible.closest(".locaryn-term-scroll")) {
    actions.push({ label: "Joindre au message", run: () => attachTerminalText(selection) });
  }
  if (selection) {
    actions.push({ label: "Copier", run: () => copier(selection) });
    if (isTauri) {
      const court = selection.length > 32 ? `${selection.slice(0, 32)}…` : selection;
      actions.push({
        label: `Rechercher « ${court} »`,
        run: () => ouvrirLien(selection),
      });
    }
  }
  const lien = cible.closest<HTMLAnchorElement>("a[href]");
  if (lien && /^https?:/i.test(lien.href)) {
    if (isTauri) {
      actions.push({
        label: "Ouvrir dans le navigateur intégré",
        run: () => ouvrirLien(lien.href),
      });
    }
    actions.push({ label: "Copier l'adresse du lien", run: () => copier(lien.href) });
  }
  const message = cible.closest<HTMLElement>(".locaryn-msg");
  if (message && !selection) {
    const texte = message.innerText.trim();
    if (texte) actions.push({ label: "Copier le message", run: () => copier(texte) });
  }
  return actions;
}

export function AppContextMenu() {
  const { menu, ouvrir, fermer } = useMenuUnique<Menu>();

  useEffect(() => {
    function surClicDroit(e: MouseEvent) {
      // Un menu propre au composant visé a déjà été ouvert.
      if (e.defaultPrevented) return;
      const cible = e.target instanceof HTMLElement ? e.target : null;
      // Un champ : le menu natif et ses corrections d'orthographe.
      if (cible?.closest(EDITABLE)) return;
      e.preventDefault();
      const actions = cible ? actionsPour(cible) : [];
      if (actions.length > 0) ouvrir({ x: e.clientX, y: e.clientY, actions });
    }
    document.addEventListener("contextmenu", surClicDroit);
    return () => document.removeEventListener("contextmenu", surClicDroit);
  }, [ouvrir]);

  const boite = useRef<HTMLDivElement | null>(null);
  const [place, setPlace] = useState<{ x: number; y: number } | null>(null);
  // Rester dans la fenêtre : la place se calcule sur la taille réelle du menu,
  // une fois dessiné. L'estimation d'avant (largeur fixe, 40 px par ligne)
  // laissait déborder un menu aux libellés longs, coupé au bord.
  useLayoutEffect(() => {
    const el = boite.current;
    if (!menu || !el) {
      setPlace(null);
      return;
    }
    const marge = 8;
    const { width, height } = el.getBoundingClientRect();
    setPlace({
      x: Math.max(marge, Math.min(menu.x, window.innerWidth - width - marge)),
      y: Math.max(marge, Math.min(menu.y, window.innerHeight - height - marge)),
    });
  }, [menu]);

  if (!menu) return null;
  return (
    <div
      ref={(el) => {
        boite.current = el;
        focusPremiereEntree(el);
      }}
      className="locaryn-ctx locaryn-app-ctx"
      // Invisible le temps d'être mesuré : il ne saute pas sous les yeux.
      style={{
        top: place?.y ?? menu.y,
        left: place?.x ?? menu.x,
        visibility: place ? "visible" : "hidden",
      }}
      role="menu"
      tabIndex={-1}
      onKeyDown={navigationMenu}
    >
      {menu.actions.map((a) => (
        <button
          key={a.label}
          type="button"
          role="menuitem"
          className="locaryn-ctx-item"
          // mousedown : garder la sélection et le focus du champ jusqu'à l'action.
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            fermer();
            void a.run();
          }}
        >
          {a.label}
        </button>
      ))}
    </div>
  );
}
