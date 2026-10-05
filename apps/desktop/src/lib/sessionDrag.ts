/**
 * Prendre une conversation de la barre latérale et la déposer ailleurs.
 *
 * Le glisser-déposer HTML5 ne passe pas dans l'application : sous Windows,
 * Tauri garde pour lui les dépôts du webview (ceux de fichiers), et l'événement
 * `drop` n'arrivait jamais. On prenait la ligne, rien ne la recevait. Le geste
 * est donc suivi au pointeur : la ligne émet le départ, les déplacements et le
 * lâcher ; la barre latérale dessine le fantôme et décide où ça tombe, d'après
 * l'élément sous le curseur (`data-drop`).
 */

export type SessionDragStart = { id: string; label: string; x: number; y: number };
export type SessionDragPoint = { x: number; y: number };

export const SESSION_DRAG_START = "locaryn:session-drag-start";
export const SESSION_DRAG_MOVE = "locaryn:session-drag-move";
export const SESSION_DRAG_END = "locaryn:session-drag-end";
export const SESSION_DRAG_CANCEL = "locaryn:session-drag-cancel";

/** Distance sous laquelle un appui reste un clic : la main tremble un peu. */
const SEUIL_PX = 6;

let clicAvale = false;

function emettre<T>(type: string, detail: T): void {
  window.dispatchEvent(new CustomEvent<T>(type, { detail }));
}

/**
 * Suivre un appui sur une ligne. Tant que le pointeur n'a pas bougé de quelques
 * pixels, rien ne se passe et le clic ouvre la conversation comme avant.
 */
export function suivreAppui(
  e: { clientX: number; clientY: number },
  id: string,
  label: string,
): void {
  const x0 = e.clientX;
  const y0 = e.clientY;
  let parti = false;

  const bouger = (ev: PointerEvent) => {
    if (!parti) {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < SEUIL_PX) return;
      parti = true;
      document.documentElement.classList.add("locaryn-session-dragging");
      emettre<SessionDragStart>(SESSION_DRAG_START, { id, label, x: ev.clientX, y: ev.clientY });
    }
    emettre<SessionDragPoint>(SESSION_DRAG_MOVE, { x: ev.clientX, y: ev.clientY });
  };

  const finir = (ev: PointerEvent, annule: boolean) => {
    window.removeEventListener("pointermove", bouger);
    window.removeEventListener("pointerup", lacher);
    window.removeEventListener("pointercancel", annuler);
    window.removeEventListener("keydown", echap);
    if (!parti) return;
    document.documentElement.classList.remove("locaryn-session-dragging");
    // Le clic qui suit le lâcher ne doit pas ouvrir la conversation déplacée.
    clicAvale = true;
    window.setTimeout(() => {
      clicAvale = false;
    }, 400);
    if (annule) emettre(SESSION_DRAG_CANCEL, null);
    else emettre<SessionDragPoint>(SESSION_DRAG_END, { x: ev.clientX, y: ev.clientY });
  };
  const lacher = (ev: PointerEvent) => finir(ev, false);
  const annuler = (ev: PointerEvent) => finir(ev, true);
  // Échap rend la conversation à sa place, comme un lâcher dans le vide.
  const echap = (ev: KeyboardEvent) => {
    if (ev.key !== "Escape" || !parti) return;
    finir(new PointerEvent("pointercancel"), true);
  };

  window.addEventListener("pointermove", bouger);
  window.addEventListener("pointerup", lacher);
  window.addEventListener("pointercancel", annuler);
  window.addEventListener("keydown", echap);
}

/** Vrai une fois, juste après un glisser : le clic qui le termine est avalé. */
export function clicApresGlisser(): boolean {
  const v = clicAvale;
  clicAvale = false;
  return v;
}

export type CibleDepot =
  | { kind: "archive" }
  | { kind: "project"; id: string }
  | { kind: "merge"; id: string };

/** Ce qui se trouve sous le curseur et accepte une conversation. */
export function cibleSous(x: number, y: number): CibleDepot | null {
  const el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
  if (!el) return null;
  const kind = el.dataset.drop;
  const id = el.dataset.dropId ?? "";
  if (kind === "archive") return { kind: "archive" };
  if (kind === "project" && id) return { kind: "project", id };
  if (kind === "merge" && id) return { kind: "merge", id };
  return null;
}

export function memeCible(a: CibleDepot | null, b: CibleDepot | null): boolean {
  if (a === null || b === null) return a === b;
  if (a.kind !== b.kind) return false;
  return a.kind === "archive" || (b.kind !== "archive" && a.id === b.id);
}
