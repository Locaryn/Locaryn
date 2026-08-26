import {
  ACCENT_PRESETS,
  type ResolvedMode,
  type ThemeMode,
  applyTheme,
  resolveMode,
  watchSystemMode,
} from "@locaryn/ui-core";
import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Le thème de Locaryn — l'interface est quasi monochrome ; les deux seules
 * choses réglables sont le mode (sombre, clair, ou celui du système) et la
 * teinte de l'accent. Tout le reste — surfaces, bordures, texte — vient des
 * jetons de `packages-ui/tokens/tokens.css`.
 *
 * La mécanique est partagée avec le téléphone et le web (`@locaryn/ui-core`) :
 * l'accent du mode clair est dérivé de celui du mode sombre, pour qu'un
 * libellé posé sur l'accent plein reste lisible dans les deux modes.
 *
 * Conservé dans localStorage sous `locaryn:theme`.
 */

export { ACCENT_PRESETS };

export interface ThemeSettings {
  /** Sombre, clair, ou le thème du système. */
  mode: ThemeMode;
  /** L'accent en mode sombre ; le mode clair l'assombrit tout seul. */
  accentHex: string;
}

const DEFAULT_THEME: ThemeSettings = {
  mode: "dark",
  accentHex: ACCENT_PRESETS[0].hex,
};

const STORAGE_KEY = "locaryn:theme";

function loadTheme(): ThemeSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_THEME;
    const parsed = JSON.parse(raw) as Partial<ThemeSettings>;
    return {
      mode: parsed.mode === "light" || parsed.mode === "system" ? parsed.mode : "dark",
      accentHex: typeof parsed.accentHex === "string" ? parsed.accentHex : DEFAULT_THEME.accentHex,
    };
  } catch (err) {
    console.warn("Thème illisible dans le stockage local, retour au défaut.", err);
    return DEFAULT_THEME;
  }
}

export function useTheme() {
  const [settings, setSettings] = useState<ThemeSettings>(loadTheme);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [resolved, setResolved] = useState<ResolvedMode>(() => resolveMode(loadTheme().mode));
  // Le premier rendu pose le thème sans fondu : un voile au démarrage se
  // verrait comme un clignotement. Une référence, pas un état : le passage à
  // « monté » ne doit relancer ni le rendu ni l'effet.
  const mounted = useRef(false);

  useEffect(() => {
    setResolved(applyTheme(settings.mode, settings.accentHex, mounted.current));
    mounted.current = true;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch (err) {
      console.warn("Thème non conservé : stockage local indisponible.", err);
    }
  }, [settings]);

  // Tant que le mode est « system », suivre le thème de la machine.
  useEffect(() => {
    if (settings.mode !== "system") return;
    return watchSystemMode(() => {
      setResolved(applyTheme("system", settings.accentHex, true));
    });
  }, [settings.mode, settings.accentHex]);

  const updateAccent = useCallback((hex: string) => {
    setSettings((s) => ({ ...s, accentHex: hex }));
  }, []);

  const updateMode = useCallback((mode: ThemeMode) => {
    setSettings((s) => ({ ...s, mode }));
  }, []);

  const resetTheme = useCallback(() => {
    setSettings(DEFAULT_THEME);
  }, []);

  return {
    settings,
    /** Le mode réellement rendu, une fois « system » résolu. */
    resolved,
    settingsOpen,
    setSettingsOpen,
    updateAccent,
    updateMode,
    resetTheme,
  };
}

export type UseThemeReturn = ReturnType<typeof useTheme>;
