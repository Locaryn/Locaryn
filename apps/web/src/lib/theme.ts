/**
 * Le thème du client web — mode sombre / clair et couleur d'accentuation.
 *
 * La mécanique est celle de `@locaryn/ui-core`, comme sur l'ordinateur et le
 * téléphone. Ce qui change ici : le défaut est « system ». Une page web
 * s'ouvre dans le thème du navigateur, pas dans celui qu'on lui impose ; c'est
 * aussi ce qu'attend une PWA installée.
 */

import { ACCENT_PRESETS, type ThemeMode, applyTheme, resolveMode } from "@locaryn/ui-core";

export { ACCENT_PRESETS, type ThemeMode };

export interface ReglageTheme {
  /** Sombre, clair, ou le thème du système. */
  mode: ThemeMode;
  /** L'accent en mode sombre ; le mode clair l'assombrit tout seul. */
  hex: string;
}

const STORAGE_KEY = "locaryn:theme-web";

const DEFAUT: ReglageTheme = { mode: "system", hex: ACCENT_PRESETS[0].hex };

/** Le réglage enregistré, ou le défaut. */
export function lireTheme(): ReglageTheme {
  try {
    const brut = localStorage.getItem(STORAGE_KEY);
    if (brut) {
      const p = JSON.parse(brut) as Partial<ReglageTheme>;
      return {
        mode: p.mode === "light" || p.mode === "dark" ? p.mode : "system",
        hex: typeof p.hex === "string" ? p.hex : DEFAUT.hex,
      };
    }
  } catch (err) {
    console.warn("Thème illisible dans le stockage du navigateur, retour au défaut.", err);
  }
  return DEFAUT;
}

/** Pose le thème sur les jetons CSS partagés, et le garde dans le navigateur. */
export function appliquerTheme(reglage: ReglageTheme, anime = false): void {
  applyTheme(reglage.mode, reglage.hex, anime);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(reglage));
  } catch (err) {
    console.warn("Thème non conservé : stockage du navigateur indisponible.", err);
  }
}

/** Le mode réellement rendu, une fois « system » résolu. */
export function modeRendu(reglage: ReglageTheme) {
  return resolveMode(reglage.mode);
}
