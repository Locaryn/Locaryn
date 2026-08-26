/**
 * Le thème — mode sombre / clair et couleur d'accentuation.
 *
 * Le bureau, le téléphone et le web partagent exactement ce fichier : la
 * bascule de mode et le calcul de l'accent ne peuvent pas diverger d'un client
 * à l'autre sans que le mode clair casse quelque part.
 *
 * Les valeurs de surface, de bordure et de texte ne sont PAS ici : elles vivent
 * dans `packages-ui/tokens/tokens.css`, sous `:root` et `[data-theme="light"]`.
 * Ce module ne pose que deux choses sur le document : l'attribut `data-theme`,
 * et l'accent (`--accent`, `--accent-rgb`, `--accent-fill`, …).
 */

/** Les trois réglages possibles. « system » suit le thème du système. */
export type ThemeMode = "dark" | "light" | "system";

/** Le mode réellement rendu, une fois « system » résolu. */
export type ResolvedMode = "dark" | "light";

export interface AccentPreset {
  /** Nom affiché, jamais traduit automatiquement. */
  name: string;
  /** La valeur en mode sombre. Le mode clair est dérivé. */
  hex: string;
}

/**
 * Six teintes muted, une seule vive à la fois à l'écran. La première est le
 * défaut du système visuel v2.
 */
export const ACCENT_PRESETS: readonly AccentPreset[] = [
  { name: "Pin", hex: "#5FA37E" },
  { name: "Sauge", hex: "#86A98F" },
  { name: "Mousse", hex: "#7E9E5F" },
  { name: "Océan", hex: "#5F9EA3" },
  { name: "Pierre", hex: "#8E9188" },
  { name: "Argile", hex: "#B08D6A" },
] as const;

/** Convertir un hex (#RGB ou #RRGGBB) en canaux 0–255. */
export function hexToChannels(hex: string): [number, number, number] {
  let m = hex.trim().replace("#", "");
  if (m.length === 3) m = m[0] + m[0] + m[1] + m[1] + m[2] + m[2];
  const n = Number.parseInt(m, 16);
  if (!Number.isFinite(n) || m.length !== 6) return [95, 163, 126];
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** Convertir un hex en « r,g,b », la forme attendue par `--accent-rgb`. */
export function hexToRgb(hex: string): string {
  return hexToChannels(hex).join(",");
}

function channelsToHex(r: number, g: number, b: number): string {
  const to = (v: number) =>
    Math.round(Math.max(0, Math.min(255, v)))
      .toString(16)
      .padStart(2, "0");
  return `#${to(r)}${to(g)}${to(b)}`;
}

/** La luminance relative WCAG d'une couleur, pour vérifier un contraste. */
function luminance([r, g, b]: [number, number, number]): number {
  const lin = (v: number) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** Le rapport de contraste entre une couleur et le blanc. */
function contrastWithWhite(rgb: [number, number, number]): number {
  return 1.05 / (luminance(rgb) + 0.05);
}

/**
 * L'accent du mode clair, dérivé de celui du mode sombre.
 *
 * Sur fond clair, le texte posé sur l'accent plein passe en blanc
 * (`--on-accent`) : l'accent doit donc s'assombrir jusqu'à tenir 4.5:1 contre
 * le blanc, sinon le libellé du bouton principal devient illisible. On
 * assombrit par pas mesurés plutôt qu'au jugé, pour que n'importe quelle
 * couleur personnalisée reste lisible.
 */
export function accentForLight(hex: string): string {
  let [r, g, b] = hexToChannels(hex);
  for (let i = 0; i < 24 && contrastWithWhite([r, g, b]) < 4.5; i++) {
    r *= 0.94;
    g *= 0.94;
    b *= 0.94;
  }
  return channelsToHex(r, g, b);
}

/** L'accent à poser, pour le mode demandé. */
export function accentForMode(hex: string, mode: ResolvedMode): string {
  return mode === "light" ? accentForLight(hex) : hex;
}

/** Ce que le système préfère, quand le mode est « system ». */
export function systemMode(): ResolvedMode {
  if (typeof window === "undefined" || !window.matchMedia) return "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** Résoudre « system » en un mode concret. */
export function resolveMode(mode: ThemeMode): ResolvedMode {
  return mode === "system" ? systemMode() : mode;
}

/**
 * Le fondu de bascule : un voile de l'ancien fond s'efface en 340ms, et
 * pendant ce temps `.lo-theming` pose une transition sur les couleurs. La
 * classe est retirée ensuite — laissée en place, elle ferait traîner chaque
 * survol.
 */
function fadeThemeSwitch(previousBackground: string): void {
  const root = document.documentElement;
  const veil = document.createElement("div");
  veil.setAttribute("aria-hidden", "true");
  veil.style.cssText = `position:fixed;inset:0;z-index:9999;pointer-events:none;background:${previousBackground};transition:opacity 340ms var(--ease, ease);`;
  document.body.appendChild(veil);
  root.classList.add("lo-theming");
  requestAnimationFrame(() => {
    veil.style.opacity = "0";
  });
  window.setTimeout(() => {
    veil.remove();
    root.classList.remove("lo-theming");
  }, 360);
}

/**
 * Poser le thème sur le document : le mode, puis l'accent adapté à ce mode.
 *
 * `animate` déclenche le fondu — à n'utiliser que sur une bascule voulue par
 * la personne, jamais au premier rendu où il ferait clignoter l'application.
 */
export function applyTheme(mode: ThemeMode, accentHex: string, animate = false): ResolvedMode {
  const root = document.documentElement;
  const resolved = resolveMode(mode);
  const previous = getComputedStyle(root).getPropertyValue("--bg").trim();
  const changed = root.getAttribute("data-theme") !== resolved;

  if (animate && changed && previous) fadeThemeSwitch(previous);

  root.setAttribute("data-theme", resolved);
  const accent = accentForMode(accentHex, resolved);
  root.style.setProperty("--accent", accent);
  root.style.setProperty("--accent-rgb", hexToRgb(accent));
  return resolved;
}

/**
 * Suivre le thème du système tant que le mode est « system ». Rend la fonction
 * de désabonnement.
 */
export function watchSystemMode(onChange: (mode: ResolvedMode) => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => undefined;
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = (e: MediaQueryListEvent) => onChange(e.matches ? "dark" : "light");
  query.addEventListener("change", handler);
  return () => query.removeEventListener("change", handler);
}
