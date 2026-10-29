// Le profil de la personne : ce qu'elle a répondu à l'accueil, et ce que
// l'interface en fait. Source unique — le rail, les réglages, la barre du haut
// et l'accueil lisent tous ici, aucun ne garde sa propre copie.
//
// Deux interfaces : « expert » (l'application complète) et « simple »
// (l'essentiel, le reste rangé dans les paramètres avancés). Le choix se
// change à tout moment dans Compte → Apparence & Langue.

import { useSyncExternalStore } from "react";
import { PRESENTATION_KEY } from "./presentation";

export type UiMode = "simple" | "expert";

/** Ce que la personne dit vouloir faire. Sert à proposer les bons outils. */
export type Need = "browser" | "computer" | "images" | "voice" | "phone";

export interface Profile {
  /** L'accueil a été traversé (ou ignoré : poste déjà utilisé). */
  onboarded: boolean;
  mode: UiMode;
  needs: Need[];
  /** Réglages avancés dépliés dans les réglages (mode simple). */
  advancedOpen: boolean;
}

const KEY = "locaryn.profile.v1";

const DEFAUT: Profile = { onboarded: false, mode: "expert", needs: [], advancedOpen: false };

const NEEDS: Need[] = ["browser", "computer", "images", "voice", "phone"];

function lire(): Profile {
  try {
    const brut = window.localStorage.getItem(KEY);
    if (brut) {
      const p = JSON.parse(brut) as Partial<Profile>;
      return {
        onboarded: p.onboarded === true,
        mode: p.mode === "simple" ? "simple" : "expert",
        needs: Array.isArray(p.needs) ? p.needs.filter((n): n is Need => NEEDS.includes(n)) : [],
        advancedOpen: p.advancedOpen === true,
      };
    }
    // Poste qui a déjà vu l'ancienne présentation : une mise à jour ne doit pas
    // rejouer l'accueil ni changer l'interface qu'on connaît.
    if (window.localStorage.getItem(PRESENTATION_KEY) === "1") {
      return { ...DEFAUT, onboarded: true };
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
