import { useEffect, useRef, useState } from "react";
import { attachTerminalText } from "../lib/attachText";
import { core, isTauri } from "../lib/core";
import { openTab } from "../lib/workspace";

/**
 * Le clic droit de l'application.
 *
 * Le menu natif de la vue web (« Enregistrer sous », « Imprimer »,
 * « Actualiser », outils du navigateur) n'a rien à faire dans une application :
 * il imprimait une page vide ou rechargeait tout en perdant la conversation.
 * Il est remplacé ici par des actions qui dépendent de ce qu'on vise — un
 * champ, du texte sélectionné, un lien, un message — et par rien du tout
 * ailleurs. Un composant qui pose son propre menu (les conversations, les
 * modèles installés) garde la main : il appelle `preventDefault` avant nous.
 */

type Action = { label: string; run: () => void | Promise<void> };
type Menu = { x: number; y: number; actions: Action[] };

const EDITABLE =
  'input:not([type=checkbox]):not([type=radio]):not([type=range]), textarea, [contenteditable=""], [contenteditable=true]';

/** Copier `texte` ; repli sur une zone cachée quand l'API refuse. */
async function copier(texte: string) {
  try {
    await navigator.clipboard.writeText(texte);
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

/** Ouvrir une adresse ou une recherche dans le navigateur intégré. */
function ouvrirDansLeNavigateur(cible: string) {
  openTab("browser");
  void core
    .browserNavigate(cible)
    .catch((e: unknown) => console.warn("navigateur intégré indisponible :", e));
}

function actionsPour(cible: HTMLElement): Action[] {
  const selection = window.getSelection()?.toString().trim() ?? "";
  const champ = cible.closest<HTMLElement>(EDITABLE);
  if (champ) {
    const lecture = champ instanceof HTMLInputElement && champ.readOnly;
    const actions: Action[] = [];
    if (!lecture) actions.push({ label: "Couper", run: () => void document.execCommand("cut") });
    actions.push({ label: "Copier", run: () => void document.execCommand("copy") });
    if (!lecture) {
      actions.push({
        label: "Coller",
        run: async () => {
          champ.focus();
          try {
            const texte = await navigator.clipboard.readText();
            document.execCommand("insertText", false, texte);
          } catch (e) {
            console.warn("lecture du presse-papier refusée :", e);
          }
        },
      });
    }
    actions.push({
      label: "Tout sélectionner",
      run: () => {
        champ.focus();
        if (champ instanceof HTMLInputElement || champ instanceof HTMLTextAreaElement) {
          champ.select();
        } else {
          document.execCommand("selectAll");
        }
      },
    });
    return actions;
  }

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
        run: () => ouvrirDansLeNavigateur(selection),
      });
    }
  }
  const lien = cible.closest<HTMLAnchorElement>("a[href]");
  if (lien && /^https?:/i.test(lien.href)) {
    if (isTauri) {
      actions.push({
        label: "Ouvrir dans le navigateur intégré",
        run: () => ouvrirDansLeNavigateur(lien.href),
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
  const [menu, setMenu] = useState<Menu | null>(null);
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    function surClicDroit(e: MouseEvent) {
      // Un menu propre au composant visé a déjà été ouvert.
      if (e.defaultPrevented) return;
      e.preventDefault();
      const cible = e.target instanceof HTMLElement ? e.target : null;
      const actions = cible ? actionsPour(cible) : [];
      // Rester dans la fenêtre : un menu coupé au bord n'est plus cliquable.
      const x = Math.min(e.clientX, window.innerWidth - 240);
      const y = Math.min(e.clientY, window.innerHeight - (actions.length * 36 + 16));
      setMenu(actions.length > 0 ? { x, y, actions } : null);
    }
    function fermer(e: Event) {
      if (e instanceof MouseEvent && ref.current?.contains(e.target as Node)) return;
      setMenu(null);
    }
    function touche(e: KeyboardEvent) {
      if (e.key === "Escape") setMenu(null);
    }
    document.addEventListener("contextmenu", surClicDroit);
    document.addEventListener("mousedown", fermer);
    window.addEventListener("blur", fermer);
    window.addEventListener("resize", fermer);
    document.addEventListener("keydown", touche);
    return () => {
      document.removeEventListener("contextmenu", surClicDroit);
      document.removeEventListener("mousedown", fermer);
      window.removeEventListener("blur", fermer);
      window.removeEventListener("resize", fermer);
      document.removeEventListener("keydown", touche);
    };
  }, []);

  if (!menu) return null;
  return (
    <div
      ref={ref}
      className="locaryn-ctx locaryn-app-ctx"
      style={{ top: menu.y, left: menu.x }}
      role="menu"
      tabIndex={-1}
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
            setMenu(null);
            void a.run();
          }}
        >
          {a.label}
        </button>
      ))}
    </div>
  );
}
