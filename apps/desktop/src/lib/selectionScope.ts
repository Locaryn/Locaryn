/**
 * Ctrl+A ne sélectionne que le contenu sur lequel on vient de cliquer.
 *
 * Dans un webview, Ctrl+A prend toute la fenêtre : la conversation, la barre
 * latérale, les boutons. Après un clic dans un fichier ouvert (panneau
 * Fichiers) ou dans un bloc de code d'une réponse, l'utilisateur veut le texte
 * de ce fichier ou de ce bloc, et rien d'autre — comme dans un éditeur.
 *
 * Une zone se déclare avec l'attribut `data-select-scope` ; les blocs de code
 * du rendu Markdown (`pre.md-code`) en sont une d'office, sans leur barre
 * Copier/Exécuter. Les champs de saisie gardent leur Ctrl+A natif.
 */

const ZONE = "[data-select-scope], pre.md-code";

let derniereZone: HTMLElement | null = null;

function contenuDe(zone: HTMLElement): HTMLElement {
  // Un bloc de code : le <code>, pas la barre d'outils ajoutée devant.
  if (zone.matches("pre.md-code")) return zone.querySelector("code") ?? zone;
  return zone.querySelector<HTMLElement>("[data-select-content]") ?? zone;
}

function estUnChamp(el: Element | null): boolean {
  if (!el) return false;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return true;
  if (el instanceof HTMLElement && el.isContentEditable) return true;
  return el.closest(".xterm, .monaco-editor") !== null;
}

window.addEventListener(
  "pointerdown",
  (e) => {
    const cible = e.target instanceof Element ? e.target : null;
    derniereZone = cible?.closest<HTMLElement>(ZONE) ?? null;
  },
  { capture: true },
);

window.addEventListener(
  "keydown",
  (e) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== "a") return;
    if (estUnChamp(document.activeElement)) return;
    const zone = derniereZone;
    if (!zone || !zone.isConnected) return;
    e.preventDefault();
    const selection = window.getSelection();
    if (!selection) return;
    const plage = document.createRange();
    plage.selectNodeContents(contenuDe(zone));
    selection.removeAllRanges();
    selection.addRange(plage);
  },
  { capture: true },
);
