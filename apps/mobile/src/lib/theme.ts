/**
 * Le thème du téléphone — mode sombre / clair et couleur d'accentuation.
 *
 * La mécanique est celle de `@locaryn/ui-core` : la même que sur l'ordinateur
 * et le web, pour que la dérivation de l'accent en mode clair ne diverge pas
 * d'un client à l'autre. Ici on ne fait que la garder sur l'appareil.
 */

import { ACCENT_PRESETS, type ThemeMode, applyTheme, resolveMode } from "@locaryn/ui-core";

export { ACCENT_PRESETS, type ThemeMode };

export interface ReglageTheme {
  /** Sombre, clair, ou le thème du système. */
  mode: ThemeMode;
  /** L'accent en mode sombre ; le mode clair l'assombrit tout seul. */
  hex: string;
}

const STORAGE_KEY = "locaryn:theme-mobile";

const DEFAUT: ReglageTheme = { mode: "dark", hex: ACCENT_PRESETS[0].hex };

/** Le réglage enregistré, ou le défaut. */
export function lireTheme(): ReglageTheme {
  try {
    const brut = localStorage.getItem(STORAGE_KEY);
    if (brut) {
      const p = JSON.parse(brut) as Partial<ReglageTheme>;
      return {
        mode: p.mode === "light" || p.mode === "system" ? p.mode : "dark",
        hex: typeof p.hex === "string" ? p.hex : DEFAUT.hex,
      };
    }
  } catch (err) {
    console.warn("Thème illisible sur l'appareil, retour au défaut.", err);
  }
  return DEFAUT;
}

/** Pose le thème sur les jetons CSS partagés, et le garde sur l'appareil. */
export function appliquerTheme(reglage: ReglageTheme, anime = false): void {
  applyTheme(reglage.mode, reglage.hex, anime);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(reglage));
  } catch (err) {
    console.warn("Thème non conservé : stockage indisponible.", err);
  }
}

/** Le mode réellement rendu, une fois « system » résolu. */
export function modeRendu(reglage: ReglageTheme) {
  return resolveMode(reglage.mode);
}
