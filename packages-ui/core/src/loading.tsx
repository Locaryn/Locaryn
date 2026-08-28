/**
 * Les quatre formes de chargement du système visuel Locaryn.
 *
 * Il n'y en a que quatre, et on n'en invente pas d'autres :
 *
 *  1. `LoProgress` — progression ondulée déterminée : tout chargement dont on
 *     connaît la fin (téléchargement de modèle, quantification, fine-tuning,
 *     indexation, import, mise à jour).
 *  2. `LoMorph`    — onde courte : le modèle réfléchit avant le premier jeton,
 *     une commande courte s'exécute, une transcription démarre.
 *  3. `LoSkeleton` — squelette : listes en attente.
 *  4. `LoSpinner`  — rotateur : action courte sans progression connue.
 *
 * La matière (couleurs, animations) vit dans `packages-ui/tokens/loading.css`.
 */

import { useEffect, useRef } from "react";

/* ═══════════════════════════════════════════════════════════
   Tracé d'onde partagé
   ═══════════════════════════════════════════════════════════ */

/** Une longueur d'onde du tracé — `lo-wave` défile exactement de ça. */
const WAVELENGTH = 24;

/**
 * Le chemin d'une onde continue, en demi-arcs alternés. La largeur doit
 * dépasser largement le conteneur pour que le défilement soit continu.
 */
function wavePath(width: number, mid: number, amplitude: number): string {
  const half = WAVELENGTH / 2;
  const steps = Math.ceil(width / half);
  let d = `M0 ${mid} q${half / 2} -${amplitude} ${half} 0`;
  for (let i = 1; i < steps; i++) d += ` t${half} 0`;
  return d;
}

/* ═══════════════════════════════════════════════════════════
   1. Progression ondulée déterminée
   ═══════════════════════════════════════════════════════════ */

export interface LoProgressProps {
  /**
   * Avancement, de 0 à 1. Hors bornes, il est ramené dedans.
   *
   * `null` quand la fin n'est pas connue : l'onde balaie alors le rail au lieu
   * de faire grandir une part accomplie. Un pourcentage inventé serait pire
   * qu'une absence de pourcentage.
   */
  value: number | null;
  /** La surface sous la barre, pour que le cerclage du point s'y fonde. */
  on?: "surface" | "surface-2" | "bg";
  /** Étiquette lue par les lecteurs d'écran. */
  label?: string;
}

/**
 * Deux copies du même tracé : le rail complet en `--surface-3`, la part faite
 * en `--accent` découpée à la valeur de progression. Les deux ondulent
 * ensemble ; un point marque la frontière.
 */
export function LoProgress({ value, on = "surface", label }: LoProgressProps) {
  const sweep = value === null || !Number.isFinite(value);
  const pct = sweep ? 0 : Math.max(0, Math.min(1, value as number)) * 100;
  const d = wavePath(900, 12, 7);
  return (
    <div
      className={sweep ? "lo-progress lo-progress-sweep" : "lo-progress"}
      data-on={on}
      role="progressbar"
      aria-label={label}
      aria-valuenow={sweep ? undefined : Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="lo-progress-rail">
        <svg width={900} height={24} aria-hidden="true">
          <path d={d} stroke="var(--surface-3)" strokeWidth={4} strokeLinecap="round" fill="none" />
        </svg>
      </div>
      <div className="lo-progress-done" style={sweep ? undefined : { width: `${pct}%` }}>
        <svg width={900} height={24} aria-hidden="true">
          <path d={d} stroke="var(--accent)" strokeWidth={4} strokeLinecap="round" fill="none" />
        </svg>
      </div>
      <span className="lo-progress-head" style={{ left: `${pct}%` }} />
    </div>
  );
}

/* ═══════════════════════════════════════════════════════════
   2. Onde courte
   ═══════════════════════════════════════════════════════════ */

/** Géométrie du dessin, en unités de la viewBox. */
const BOX = { w: 112, h: 40, mid: 20, amp: 5, railEnd: 108 };
/** La coche : angle vif, deux segments droits. */
const CHECK: Array<[number, number]> = [
  [34, 20],
  [46, 32],
  [72, 8],
];
/** Nombre de points du ruban. */
const RIBBON = 33;
/** Durée d'un cycle complet, en millisecondes. */
const CYCLE = 4000;
/** Bornes des quatre temps du cycle. */
const PHASE = { land: 0.45, hold: 0.78, back: 0.95 };

type Pt = [number, number];

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

/** Un point sur une courbe de Bézier cubique. */
function bezier(p0: Pt, c0: Pt, c1: Pt, p1: Pt, t: number): Pt {
  const u = 1 - t;
  const [a, b, c, d] = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
  return [
    a * p0[0] + b * c0[0] + c * c1[0] + d * p1[0],
    a * p0[1] + b * c0[1] + c * c1[1] + d * p1[1],
  ];
}

/**
 * Le chemin guide, échantillonné : le rail ondulé, l'élan vers le haut, le
 * retour en arrière par une courbe, puis la coche. Le ruban ne fait que s'y
 * déplacer — c'est ce qui donne le mouvement de ficelle.
 *
 * `phase` fait onduler le rail, `damp` éteint l'ondulation à l'atterrissage.
 */
function guide(phase: number, damp: number): Pt[] {
  const pts: Pt[] = [];
  for (let x = 0; x <= BOX.railEnd; x += 2) {
    pts.push([x, BOX.mid + BOX.amp * damp * Math.sin((x / WAVELENGTH) * Math.PI * 2 + phase)]);
  }
  const launchFrom: Pt = [BOX.railEnd, BOX.mid];
  for (let i = 1; i <= 24; i++) {
    pts.push(bezier(launchFrom, [140, -6], [118, -2], [92, 4], i / 24));
  }
  for (let i = 1; i <= 24; i++) {
    pts.push(bezier([92, 4], [70, -4], [38, 6], CHECK[0], i / 24));
  }
  for (let i = 1; i <= 12; i++) pts.push(segment(CHECK[0], CHECK[1], i / 12));
  for (let i = 1; i <= 20; i++) pts.push(segment(CHECK[1], CHECK[2], i / 20));
  return pts;
}

function segment(a: Pt, b: Pt, t: number): Pt {
  return [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
}

/** Les longueurs cumulées le long d'un chemin échantillonné. */
function arcLengths(pts: Pt[]): number[] {
  const out = [0];
  for (let i = 1; i < pts.length; i++) {
    out.push(out[i - 1] + Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]));
  }
  return out;
}

/** Le point situé à `s` unités du début du chemin. */
function at(pts: Pt[], lens: number[], s: number): Pt {
  const total = lens[lens.length - 1];
  const target = Math.max(0, Math.min(total, s));
  let i = 1;
  while (i < lens.length - 1 && lens[i] < target) i++;
  const span = lens[i] - lens[i - 1] || 1;
  return segment(pts[i - 1], pts[i], (target - lens[i - 1]) / span);
}

/** La longueur du ruban : exactement celle de la coche, pour qu'il s'y pose. */
function checkLength(): number {
  return (
    Math.hypot(CHECK[1][0] - CHECK[0][0], CHECK[1][1] - CHECK[0][1]) +
    Math.hypot(CHECK[2][0] - CHECK[1][0], CHECK[2][1] - CHECK[1][1])
  );
}

/** Adoucissement en cloche, pour que l'élan parte et s'arrête sans à-coup. */
function easeInOut(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;
}

/**
 * Où en est le cycle : combien le rail ondule encore, où en est l'éclat, et
 * l'avancée de la tête entre le repos et la fin du chemin.
 *
 * `advance` est une fraction, pas une longueur : l'ondulation du rail allonge
 * le chemin, donc la longueur totale n'est pas la même d'une image à l'autre.
 * La convertir en longueur trop tôt décalait le ruban, et il s'arrêtait à
 * côté de la coche au lieu de se poser dessus.
 */
function ribbonState(p: number): { advance: number; damp: number; burst: number } {
  if (p < PHASE.land) return { advance: 0, damp: 1, burst: 0 };
  if (p < PHASE.hold) {
    const k = easeInOut((p - PHASE.land) / (PHASE.hold - PHASE.land));
    return { advance: k, damp: 1 - k, burst: 0 };
  }
  if (p < PHASE.back) {
    return { advance: 1, damp: 0, burst: (p - PHASE.hold) / (PHASE.back - PHASE.hold) };
  }
  const k = (p - PHASE.back) / (1 - PHASE.back);
  return { advance: 1 - k, damp: k, burst: 0 };
}

/** La position de la tête sur le chemin de l'image courante. */
function headAt(advance: number, ribbonLen: number, total: number): number {
  const rest = ribbonLen + 28;
  return lerp(rest, total, advance);
}

export interface LoMorphProps {
  /** Largeur rendue, en pixels. La hauteur suit le rapport de la viewBox. */
  width?: number;
  /** Ne jouer qu'une fois — le cas de l'application, à l'arrivée de la réponse. */
  once?: boolean;
  /** Étiquette lue par les lecteurs d'écran. */
  label?: string;
}

/**
 * Le ruban ondule sur son rail, s'élance, revient par une courbe, prend
 * l'angle de la coche et s'arrête dessus. Six points éclatent à l'arrivée.
 *
 * Le tracé est recalculé image par image en JS plutôt qu'en SMIL : SMIL ne
 * démarre pas de façon fiable dans une WebView.
 */
export function LoMorph({ width = 112, once = false, label }: LoMorphProps) {
  const line = useRef<SVGPolylineElement>(null);
  const dots = useRef<SVGGElement>(null);

  useEffect(() => {
    const ribbonLen = checkLength();

    // Mouvement réduit demandé : on pose la coche, une bonne fois. Le CSS ne
    // peut rien ici — le tracé est écrit en JS, c'est donc à lui de se taire.
    const calme =
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
    if (calme) {
      const pts = guide(0, 0);
      const lens = arcLengths(pts);
      paintRibbon(line.current, pts, lens, lens[lens.length - 1], ribbonLen);
      return;
    }

    let frame = 0;
    let id = 0;

    const tick = () => {
      const p = once ? Math.min(1, (frame * 16) / CYCLE) : ((frame * 16) / CYCLE) % 1;
      const { advance, damp, burst } = ribbonState(p);
      // Le chemin est remesuré à chaque image : c'est lui qui rétrécit quand
      // l'ondulation s'éteint, et la tête doit suivre ce rétrécissement.
      const pts = guide(frame * 0.12, damp);
      const lens = arcLengths(pts);
      const head = headAt(advance, ribbonLen, lens[lens.length - 1]);
      paintRibbon(line.current, pts, lens, head, ribbonLen);
      paintBurst(dots.current, burst);
      frame++;
      if (once && p >= 1) window.clearInterval(id);
    };

    id = window.setInterval(tick, 16);
    return () => window.clearInterval(id);
  }, [once]);

  return (
    <svg
      className="lo-morph"
      width={width}
      height={(width * BOX.h) / BOX.w}
      viewBox={`0 0 ${BOX.w} ${BOX.h}`}
      role={label ? "img" : "presentation"}
      aria-label={label}
    >
      <polyline ref={line} points="" />
      <g ref={dots} />
    </svg>
  );
}

/** Réécrire les 33 points du ruban pour l'image courante. */
function paintRibbon(
  node: SVGPolylineElement | null,
  pts: Pt[],
  lens: number[],
  head: number,
  ribbonLen: number,
): void {
  if (!node) return;
  const out: string[] = [];
  for (let i = 0; i < RIBBON; i++) {
    const s = head - ribbonLen * (1 - i / (RIBBON - 1));
    const [x, y] = at(pts, lens, s);
    out.push(`${x.toFixed(2)},${y.toFixed(2)}`);
  }
  node.setAttribute("points", out.join(" "));
}

/** Les six points qui éclatent à l'arrivée, dans la couleur du ruban. */
function paintBurst(node: SVGGElement | null, burst: number): void {
  if (!node) return;
  if (burst <= 0) {
    if (node.childNodes.length) node.replaceChildren();
    return;
  }
  const ns = "http://www.w3.org/2000/svg";
  if (node.childNodes.length !== 6) {
    node.replaceChildren(
      ...Array.from({ length: 6 }, () => document.createElementNS(ns, "circle")),
    );
  }
  const [cx, cy] = CHECK[2];
  node.childNodes.forEach((child, i) => {
    const angle = (i / 6) * Math.PI * 2 - Math.PI / 2;
    const reach = 4 + burst * 9;
    const dot = child as SVGCircleElement;
    dot.setAttribute("cx", (cx + Math.cos(angle) * reach).toFixed(2));
    dot.setAttribute("cy", (cy + Math.sin(angle) * reach).toFixed(2));
    dot.setAttribute("r", (1.6 * (1 - burst)).toFixed(2));
    dot.setAttribute("opacity", (1 - burst).toFixed(2));
  });
}

/* ═══════════════════════════════════════════════════════════
   3. Squelette
   ═══════════════════════════════════════════════════════════ */

export interface LoSkeletonProps {
  width?: number | string;
  height?: number | string;
  radius?: number | string;
}

/** Une ligne en attente. Pour les listes, jamais pour une valeur qui change. */
export function LoSkeleton({ width = "100%", height = 14, radius }: LoSkeletonProps) {
  return (
    <span
      className="lo-skeleton"
      aria-hidden="true"
      style={{ display: "block", width, height, borderRadius: radius }}
    />
  );
}

/* ═══════════════════════════════════════════════════════════
   4. Rotateur
   ═══════════════════════════════════════════════════════════ */

export interface LoSpinnerProps {
  /** `sm` pour les barres denses (barre d'état, ligne de liste). */
  size?: "md" | "sm";
  label?: string;
}

/** Action courte sans progression connue. */
export function LoSpinner({ size = "md", label }: LoSpinnerProps) {
  return (
    <span
      className={size === "sm" ? "lo-spinner lo-spinner-sm" : "lo-spinner"}
      role={label ? "status" : "presentation"}
      aria-label={label}
    />
  );
}
