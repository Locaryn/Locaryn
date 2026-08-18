/**
 * La couleur d'accentuation — la même palette que le bureau.
 *
 * Les jetons de design vivent dans `packages-ui/tokens` (--accent, --accent-rgb)
 * et sont prévus pour être remplacés à l'exécution. C'est ce que fait
 * `useTheme` sur l'ordinateur ; ici, la même chose, en plus petit : six teintes
 * prédéfinies plus une couleur personnalisée, conservées sur l'appareil.
 */

export const ACCENT_PRESETS = [
  { name: "Pine", hex: "#6F9C7F", rgb: "111,156,127" },
  { name: "Sage", hex: "#88A98F", rgb: "136,169,143" },
  { name: "Moss", hex: "#7E9B63", rgb: "126,155,99" },
  { name: "Fern", hex: "#5F9C78", rgb: "95,156,120" },
  { name: "Stone", hex: "#8E9188", rgb: "142,145,136" },
  { name: "Clay", hex: "#B08D6A", rgb: "176,141,106" },
] as const;

export interface Accent {
  hex: string;
  rgb: string;
}

const STORAGE_KEY = "locaryn:theme-mobile";

/** Convertir un hex (#RRGGBB) en « r,g,b » pour les compositions rgba(). */
function hexToRgb(hex: string): string {
  const m = hex.replace("#", "");
  const r = Number.parseInt(m.substring(0, 2), 16);
  const g = Number.parseInt(m.substring(2, 4), 16);
  const b = Number.parseInt(m.substring(4, 6), 16);
  return `${r},${g},${b}`;
}

/** L'accent enregistré, ou le défaut (Pine). */
export function lireAccent(): Accent {
  try {
    const brut = localStorage.getItem(STORAGE_KEY);
    if (brut) {
      const p = JSON.parse(brut) as Partial<Accent>;
      if (p?.hex && p?.rgb) return { hex: p.hex, rgb: p.rgb };
    }
  } catch {
    // stockage illisible : on repart du défaut
  }
  return { hex: ACCENT_PRESETS[0].hex, rgb: ACCENT_PRESETS[0].rgb };
}

/** Pose l'accent sur les jetons CSS partagés, et le garde sur l'appareil. */
export function appliquerAccent(accent: Accent): void {
  const root = document.documentElement;
  root.style.setProperty("--accent", accent.hex);
  root.style.setProperty("--accent-rgb", accent.rgb);
  root.style.setProperty("--accent-hex", accent.hex);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(accent));
  } catch {
    // stockage indisponible : l'accent tient pour cette session
  }
}

/** Construire un Accent depuis n'importe quel hex valide. */
export function accentDepuisHex(hex: string): Accent {
  return { hex, rgb: hexToRgb(hex) };
}
