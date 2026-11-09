/**
 * Ce qui passe par-dessus une vue web native.
 *
 * La vue du navigateur intégré se dessine au-dessus de tout le DOM : un menu,
 * une infobulle, une notification ou une fenêtre qui empiète sur sa place
 * serait caché derrière la page. L'interface la retire donc dès que quelque
 * chose recouvre la zone qui lui est réservée.
 */

/** Une fenêtre modale : la page s'efface, où qu'elle soit. */
const MODALES = 'dialog[open], [role="dialog"], [role="alertdialog"], [aria-modal="true"]';

/** Ce qui flotte au-dessus de l'interface : compte seulement s'il empiète. */
const FLOTTANTS =
  '[role="menu"], [role="listbox"], [role="tooltip"], [role="status"], [role="alert"], [popover]';

/** Le quadrillage sondé dans la zone : de quoi attraper un menu de 30 px. */
const COLONNES = 9;
const LIGNES = 12;

type Rect = { left: number; top: number; right: number; bottom: number };

function seCroisent(a: Rect, b: Rect): boolean {
  return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
}

function estVisible(el: Element): boolean {
  const r = el.getBoundingClientRect();
  if (r.width < 1 || r.height < 1) return false;
  const style = getComputedStyle(el);
  return style.visibility !== "hidden" && Number(style.opacity) > 0.05;
}

/** Un élément flottant connu (menu, notification…) qui empiète sur la zone. */
function flottantSurLaZone(zone: HTMLElement, cadre: Rect): boolean {
  for (const el of document.querySelectorAll(FLOTTANTS)) {
    if (zone.contains(el) || el.contains(zone)) continue;
    if (seCroisent(el.getBoundingClientRect(), cadre) && estVisible(el)) return true;
  }
  return false;
}

/** N'importe quel élément posé sur la zone, quel que soit son rôle : le DOM
 *  ignore la vue native, le plus haut élément d'un point doit être la zone. */
function quelqueChoseSurLaZone(zone: HTMLElement, cadre: Rect): boolean {
  const largeur = cadre.right - cadre.left;
  const hauteur = cadre.bottom - cadre.top;
  for (let c = 0; c < COLONNES; c++) {
    const x = cadre.left + 2 + ((largeur - 4) * c) / (COLONNES - 1);
    for (let l = 0; l < LIGNES; l++) {
      const y = cadre.top + 2 + ((hauteur - 4) * l) / (LIGNES - 1);
      const haut = document.elementFromPoint(x, y);
      if (haut && !zone.contains(haut)) return true;
    }
  }
  return false;
}

/** Faut-il retirer la vue native posée sur `zone` ? */
export function zoneRecouverte(zone: HTMLElement | null): boolean {
  if (document.body.classList.contains("locaryn-resizing")) return true;
  if (document.querySelector(MODALES) !== null) return true;
  if (!zone) return false;
  const r = zone.getBoundingClientRect();
  if (r.width < 4 || r.height < 4) return false;
  const cadre: Rect = { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  return flottantSurLaZone(zone, cadre) || quelqueChoseSurLaZone(zone, cadre);
}

/**
 * Suivre `zoneRecouverte` : à chaque changement du DOM, fin d'animation ou
 * redimensionnement, au plus une fois par image. Rend de quoi arrêter.
 */
export function suivreRecouvrement(
  zone: () => HTMLElement | null,
  rappel: (recouverte: boolean) => void,
): () => void {
  let image = 0;
  const verifier = () => {
    image = 0;
    rappel(zoneRecouverte(zone()));
  };
  const planifier = () => {
    if (image === 0) image = requestAnimationFrame(verifier);
  };
  verifier();
  const obs = new MutationObserver(planifier);
  obs.observe(document.body, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["open", "role", "aria-modal", "class", "style", "hidden", "popover"],
  });
  document.addEventListener("transitionend", planifier, true);
  document.addEventListener("animationend", planifier, true);
  document.addEventListener("toggle", planifier, true);
  window.addEventListener("resize", planifier);
  return () => {
    if (image !== 0) cancelAnimationFrame(image);
    obs.disconnect();
    document.removeEventListener("transitionend", planifier, true);
    document.removeEventListener("animationend", planifier, true);
    document.removeEventListener("toggle", planifier, true);
    window.removeEventListener("resize", planifier);
  };
}
