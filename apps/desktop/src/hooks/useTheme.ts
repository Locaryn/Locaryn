import { useCallback, useEffect, useState } from "react";

/**
 * Locaryn theme — the UI is near-monochrome; the ONE thing worth customizing
 * is the accent hue. Everything else (surfaces, borders, text) is a fixed
 * warm-neutral scale in global.css. The accent is injected as CSS custom
 * properties on the document root so changes are instant and global.
 *
 * Persists to localStorage under `locaryn:theme`.
 */

/** Muted, natural accent presets (calm, not neon). First = default. */
export const ACCENT_PRESETS = [
  { name: "Pine", hex: "#6F9C7F", rgb: "111,156,127" },
  { name: "Sage", hex: "#88A98F", rgb: "136,169,143" },
  { name: "Moss", hex: "#7E9B63", rgb: "126,155,99" },
  { name: "Fern", hex: "#5F9C78", rgb: "95,156,120" },
  { name: "Stone", hex: "#8E9188", rgb: "142,145,136" },
  { name: "Clay", hex: "#B08D6A", rgb: "176,141,106" },
] as const;

export interface ThemeSettings {
  /** Accent color as hex string. */
  accentHex: string;
  /** Accent color as "r,g,b" for rgba() composition. */
  accentRgb: string;
}

const DEFAULT_THEME: ThemeSettings = {
  accentHex: ACCENT_PRESETS[0].hex,
  accentRgb: ACCENT_PRESETS[0].rgb,
};

const STORAGE_KEY = "locaryn:theme";

/** Convert a hex color (#RRGGBB) to "r,g,b". */
function hexToRgb(hex: string): string {
  const m = hex.replace("#", "");
  const r = Number.parseInt(m.substring(0, 2), 16);
  const g = Number.parseInt(m.substring(2, 4), 16);
  const b = Number.parseInt(m.substring(4, 6), 16);
  return `${r},${g},${b}`;
}

function loadTheme(): ThemeSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_THEME;
    const parsed = JSON.parse(raw) as Partial<ThemeSettings>;
    return { ...DEFAULT_THEME, ...parsed };
  } catch {
    return DEFAULT_THEME;
  }
}

function applyTheme(s: ThemeSettings): void {
  const root = document.documentElement;
  root.style.setProperty("--accent", s.accentHex);
  root.style.setProperty("--accent-hex", s.accentHex);
  root.style.setProperty("--accent-rgb", s.accentRgb);
}

export function useTheme() {
  const [settings, setSettings] = useState<ThemeSettings>(loadTheme);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    applyTheme(settings);
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch {
      // Ignore quota errors.
    }
  }, [settings]);

  const updateAccent = useCallback((hex: string) => {
    setSettings({ accentHex: hex, accentRgb: hexToRgb(hex) });
  }, []);

  const resetTheme = useCallback(() => {
    setSettings(DEFAULT_THEME);
  }, []);

  return { settings, settingsOpen, setSettingsOpen, updateAccent, resetTheme };
}

export type UseThemeReturn = ReturnType<typeof useTheme>;
