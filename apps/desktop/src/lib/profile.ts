// Le profil de la personne : ce qu'elle a répondu à l'accueil, et ce que
// l'interface en fait. Source unique — le rail, les réglages, la barre du haut
// et l'accueil lisent tous ici, aucun ne garde sa propre copie.
//
// Deux interfaces : « expert » (l'application complète) et « simple »
// (l'essentiel, le reste rangé dans les paramètres avancés). Le choix se
// change à tout moment dans Compte → Apparence & Langue.

import { useSyncExternalStore } from "react";

export type UiMode = "simple" | "expert";

/** Ce que la personne dit vouloir faire. Sert à proposer les bons outils. */
export type Need = "browser" | "computer" | "images" | "voice" | "phone";

/**
 * Version des questions de l'accueil. L'augmenter quand on en ajoute : toute
 * personne qui a répondu à une version plus ancienne revoit l'accueil une fois.
 */
export const ONBOARDING_VERSION = 2;

export interface Profile {
  /** L'accueil de la version courante a été traversé. */
  onboarded: boolean;
  /** Version de l'accueil à laquelle la personne a répondu (0 : aucune). */
  onboardingVersion: number;
  mode: UiMode;
  needs: Need[];
  /** Réglages avancés dépliés dans les réglages (mode simple). */
  advancedOpen: boolean;
}

const KEY = "locaryn.profile.v1";

const DEFAUT: Profile = {
  onboarded: false,
  onboardingVersion: 0,
  mode: "expert",
  needs: [],
  advancedOpen: false,
};

const NEEDS: Need[] = ["browser", "computer", "images", "voice", "phone"];

function lire(): Profile {
  try {
    const brut = window.localStorage.getItem(KEY);
    if (brut) {
      const p = JSON.parse(brut) as Partial<Profile>;
      const version = typeof p.onboardingVersion === "number" ? p.onboardingVersion : 0;
      return {
        // Un accueil plus ancien que l'actuel ne compte pas : les nouvelles
        // questions doivent être posées après la mise à jour.
        onboarded: p.onboarded === true && version >= ONBOARDING_VERSION,
        onboardingVersion: version,
        mode: p.mode === "simple" ? "simple" : "expert",
        needs: Array.isArray(p.needs) ? p.needs.filter((n): n is Need => NEEDS.includes(n)) : [],
        advancedOpen: p.advancedOpen === true,
      };
    }
  } catch (e) {
    console.warn("[Profil] lecture impossible, valeurs par défaut :", e);
  }
  return DEFAUT;
}

let courant: Profile = typeof window === "undefined" ? DEFAUT : lire();
const ecouteurs = new Set<() => void>();

function sauver(): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(courant));
  } catch (e) {
    console.warn("[Profil] écriture impossible, le choix ne survivra pas au redémarrage :", e);
  }
}

/** Fusionne un changement, le garde et prévient l'interface. */
export function updateProfile(changes: Partial<Profile>): void {
  courant = { ...courant, ...changes };
  sauver();
  for (const fn of ecouteurs) fn();
}

export function getProfile(): Profile {
  return courant;
}

function subscribe(fn: () => void): () => void {
  ecouteurs.add(fn);
  return () => ecouteurs.delete(fn);
}

/** `const profile = useProfile()` — se met à jour au moindre changement. */
export function useProfile(): Profile {
  return useSyncExternalStore(subscribe, getProfile, getProfile);
}

/** Vrai quand l'interface simplifiée est active. */
export function useSimpleUi(): boolean {
  return useProfile().mode === "simple";
}
