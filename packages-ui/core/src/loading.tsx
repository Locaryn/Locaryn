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

/**
 * Le dessin, dans les unités de la viewBox.
 *
 * Le ruban suit un chemin guide : un rail droit, une boucle ronde vers le haut
 * et l'arrière, puis la coche. Les nœuds sont lissés en Catmull-Rom, sauf la
 * coche elle-même — deux segments rectilignes, pour que l'angle reste vif.
 */
const KNOTS: Array<[number, number]> = [
  [0, 15],
  [28, 15],
  [56, 15],
  [61, 10],
  [56, 3],
  [42, 2],
  [28, 7],
  [18, 15],
];
const CORNER: [number, number] = [25, 22];
const END: [number, number] = [40, 8];

/** Points du ruban (le tracé en compte N + 1). */
const N = 32;
/** Durée d'un cycle complet, en millisecondes. */
const CYCLE = 4000;
/** Bornes des quatre temps : ondulation, atterrissage, coche tenue, retour. */
const WAVE_END = 0.45;
const LAND = 0.78;
const HOLD = 0.95;
/** Amplitude de l'ondulation, en unités de la viewBox. */
const AMP = 4;
/** Échantillons par segment du chemin guide. */
const STEP = 40;

type Pt = [number, number];

function lerpPt(a: Pt, b: Pt, t: number): Pt {
  return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
}

/** Un segment droit, échantillonné — la coche n'est pas lissée. */
function straight(a: Pt, b: Pt, n: number): Pt[] {
  return Array.from({ length: n }, (_, s) => lerpPt(a, b, (s + 1) / n));
}

/** Le chemin guide complet, et la longueur cumulée le long de celui-ci. */
function buildGuide() {
  const c = [KNOTS[0], ...KNOTS, KNOTS[KNOTS.length - 1]];
  const guide: Pt[] = [];
  const knotArc = [0];
  for (let i = 1; i < c.length - 2; i++) {
    const [p0, p1, p2, p3] = [c[i - 1], c[i], c[i + 1], c[i + 2]];
    for (let s = 0; s < STEP; s++) {
      const t = s / STEP;
      const t2 = t * t;
      const t3 = t2 * t;
      const axis = (k: 0 | 1) =>
        0.5 *
        (2 * p1[k] +
          (-p0[k] + p2[k]) * t +
          (2 * p0[k] - 5 * p1[k] + 4 * p2[k] - p3[k]) * t2 +
          (-p0[k] + 3 * p1[k] - 3 * p2[k] + p3[k]) * t3);
      guide.push([axis(0), axis(1)]);
    }
  }
  const start = KNOTS[KNOTS.length - 1];
  guide.push(start, ...straight(start, CORNER, STEP), ...straight(CORNER, END, STEP));

  const cum = [0];
  for (let i = 1; i < guide.length; i++) {
    cum.push(cum[i - 1] + Math.hypot(guide[i][0] - guide[i - 1][0], guide[i][1] - guide[i - 1][1]));
    if (i % STEP === 0) knotArc.push(cum[i]);
  }
  return { guide, cum, knotArc, start };
}

/** La coche au repos, dont l'angle tombe exactement sur un point du ruban. */
function buildCheck(start: Pt) {
  const legA = Math.hypot(CORNER[0] - start[0], CORNER[1] - start[1]);
  const legB = Math.hypot(END[0] - CORNER[0], END[1] - CORNER[1]);
  const length = legA + legB;
  const cut = Math.round((N * legA) / length);
  const points: Pt[] = Array.from({ length: N + 1 }, (_, i) =>
    i <= cut ? lerpPt(start, CORNER, i / cut) : lerpPt(CORNER, END, (i - cut) / (N - cut)),
  );
  return { points, length };
}

/** Le point situé à `arc` unités du début du chemin. */
function pointAt(guide: Pt[], cum: number[], arc: number): Pt {
  const total = cum[cum.length - 1];
  const a = Math.max(0, Math.min(total, arc));
  let lo = 0;
  let hi = cum.length - 1;
  while (lo < hi - 1) {
    const mid = (lo + hi) >> 1;
    if (cum[mid] <= a) lo = mid;
    else hi = mid;
  }
  const span = cum[hi] - cum[lo];
  return lerpPt(guide[lo], guide[hi], span ? (a - cum[lo]) / span : 0);
}

/** Adoucissement en cloche : l'élan part et s'arrête sans à-coup. */
function easeInOut(u: number): number {
  return u < 0.5 ? 4 * u * u * u : 1 - (-2 * u + 2) ** 3 / 2;
}

export interface LoMorphProps {
  /** Largeur rendue, en pixels. La hauteur suit le rapport de la viewBox. */
  width?: number;
  /** Étiquette lue par les lecteurs d'écran. */
  label?: string;
}

/**
 * Le ruban ondule sur son rail, s'élance, revient par une boucle, prend
 * l'angle de la coche et s'arrête dessus. Six étincelles marquent l'arrivée.
 *
 * Le tracé est recalculé image par image en JS plutôt qu'en SMIL : SMIL ne
 * démarre pas de façon fiable dans une WebView.
 */
export function LoMorph({ width = 118, label }: LoMorphProps) {
  const line = useRef<SVGPolylineElement>(null);

  useEffect(() => {
    const { guide, cum, knotArc, start } = buildGuide();
    const check = buildCheck(start);
    const total = cum[cum.length - 1];
    /** Fin du rail droit : au-delà, le ruban quitte l'horizontale. */
    const rail = knotArc[2];

    // Mouvement réduit demandé : on pose la coche, une bonne fois. Le CSS ne
    // peut rien ici — le tracé est écrit en JS, c'est donc à lui de se taire.
    const calme =
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
    if (calme) {
      line.current?.setAttribute(
        "points",
        check.points.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join(" "),
      );
      return;
    }

    let frame = 0;
    const tick = () => {
      const ms = (frame * 16) % CYCLE;
      const t = ms / CYCLE;
      const phase = (ms / 900) * 8;

      let head = rail;
      let len = rail;
      let amp = AMP;
      if (t >= WAVE_END && t < LAND) {
        const u = (t - WAVE_END) / (LAND - WAVE_END);
        const e = easeInOut(u);
        head = rail + (total - rail) * e;
        len = rail + (check.length - rail) * e;
        amp = AMP * Math.max(0, 1 - u / 0.38);
      } else if (t >= LAND && t < HOLD) {
        head = total;
        len = check.length;
        amp = 0;
      }

      // Sur la fin du parcours, on rejoint la répartition exacte de la coche :
      // c'est ce qui rend l'angle net au lieu d'un coude mou.
      let snap = 0;
      if (t >= WAVE_END && t < LAND) {
        snap = Math.max(0, ((t - WAVE_END) / (LAND - WAVE_END) - 0.62) / 0.38);
      } else if (t >= LAND && t < HOLD) {
        snap = 1;
      }
      if (snap > 0) amp *= 1 - snap;

      const pts: string[] = [];
      for (let i = 0; i <= N; i++) {
        const p = pointAt(guide, cum, head - len + (i / N) * len);
        const x = p[0] + (check.points[i][0] - p[0]) * snap;
        const y =
          p[1] + (check.points[i][1] - p[1]) * snap - amp * Math.sin(((i + phase) * Math.PI) / 4);
        pts.push(`${x.toFixed(2)},${y.toFixed(2)}`);
      }
      line.current?.setAttribute("points", pts.join(" "));
      frame++;
    };

    const id = window.setInterval(tick, 16);
    return () => window.clearInterval(id);
  }, []);

  /** Six étincelles, régulièrement réparties, qui éclatent à l'arrivée. */
  const sparks = [0, 60, 120, 180, 240, 300];

  return (
    <span
      className="lo-morph"
      style={{ width, height: (width * 45) / 118 }}
      role={label ? "img" : undefined}
      aria-label={label}
    >
      <svg viewBox="0 0 78 30" fill="none" shapeRendering="geometricPrecision" aria-hidden="true">
        <polyline ref={line} points="" />
      </svg>
      <span className="lo-morph-sparks" aria-hidden="true">
        {sparks.map((deg) => (
          <span key={deg} style={{ transform: `rotate(${deg}deg)` }}>
            <i />
          </span>
        ))}
      </span>
    </span>
  );
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
