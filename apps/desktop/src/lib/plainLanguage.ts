// Les chiffres techniques dits avec des mots, pour l'interface simplifiée.
// « 14 jetons par seconde » ne parle à personne ; « un paragraphe en 7
// secondes » si.

import type { Lang } from "./i18n";

export type SpeedClass = "instant" | "fluid" | "slow" | "crawl";

/** Un paragraphe, c'est environ cent jetons. */
const TOKENS_PER_PARAGRAPH = 100;

export function classifySpeed(tokensPerSecond: number): SpeedClass {
  if (tokensPerSecond >= 25) return "instant";
  if (tokensPerSecond >= 10) return "fluid";
  if (tokensPerSecond >= 4) return "slow";
  return "crawl";
}

const WORDS: Record<Lang, Record<SpeedClass, string>> = {
  fr: {
    instant: "Réponses instantanées",
    fluid: "Réponses fluides",
    slow: "Un peu lent",
    crawl: "Très lent sur cet ordinateur",
  },
  en: {
    instant: "Instant answers",
    fluid: "Smooth answers",
    slow: "A little slow",
    crawl: "Very slow on this computer",
  },
};

/** « Réponses fluides · un paragraphe en 7 s ». */
export function plainSpeed(tokensPerSecond: number, lang: Lang): string {
  const word = WORDS[lang][classifySpeed(tokensPerSecond)];
  if (!(tokensPerSecond > 0)) return word;
  const seconds = Math.max(1, Math.round(TOKENS_PER_PARAGRAPH / tokensPerSecond));
  const unit = seconds >= 60 ? `${Math.round(seconds / 60)} min` : `${seconds} s`;
  return lang === "fr" ? `${word} · un paragraphe en ${unit}` : `${word} · a paragraph in ${unit}`;
}

// ── La mémoire de la conversation ───────────────────────────────────────────
// « 11,2k / 16k jetons » ne dit rien à un débutant. On parle de mémoire et de
// pages : ce qui reste, et le moment où le modèle commence à se perdre.

export type MemoryLevel = "libre" | "moitie" | "bientot" | "pleine";

/** Une page de texte, c'est environ 500 jetons (350 mots). */
const TOKENS_PER_PAGE = 500;

/** Au-delà de 75 %, un petit modèle oublie le début et répète ; à 90 %, la
 *  réponse risque d'être coupée ou de tourner en boucle. */
export function memoryLevel(used: number, window: number): MemoryLevel {
  const part = window > 0 ? used / window : 0;
  if (part >= 0.9) return "pleine";
  if (part >= 0.75) return "bientot";
  if (part >= 0.5) return "moitie";
  return "libre";
}

/** Combien de pages de texte tiennent encore, arrondi pour rester une idée. */
export function pagesLeft(used: number, window: number): number {
  const reste = Math.max(0, window - used) / TOKENS_PER_PAGE;
  if (reste >= 20) return Math.round(reste / 5) * 5;
  return Math.max(0, Math.floor(reste));
}

export interface PlainMemory {
  level: MemoryLevel;
  /** Pourcentage rempli, 0 à 100. */
  percent: number;
  /** Quelques mots pour la pastille. */
  short: string;
  /** Ce que ça veut dire, et quoi faire. */
  detail: string;
}

/** La mémoire de la conversation dite avec des mots. */
export function plainMemory(used: number, window: number): PlainMemory {
  const level = memoryLevel(used, window);
  const percent = Math.min(100, Math.round((used / Math.max(window, 1)) * 100));
  const pages = pagesLeft(used, window);
  const reste =
    pages === 0
      ? "Il ne reste presque plus de place."
      : `Il reste la place d'environ ${pages} page${pages > 1 ? "s" : ""} de texte.`;
  switch (level) {
    case "libre":
      return {
        level,
        percent,
        short: "Mémoire libre",
        detail: `${reste} Le modèle se souvient de toute la conversation.`,
      };
    case "moitie":
      return {
        level,
        percent,
        short: "Mémoire à moitié pleine",
        detail: `${reste} Tout va bien ; pour un long document, mieux vaut commencer une nouvelle conversation.`,
      };
    case "bientot":
      return {
        level,
        percent,
        short: "Bientôt pleine",
        detail: `${reste} Le modèle commence à oublier le début et peut se répéter. Résumez la conversation ou commencez-en une nouvelle.`,
      };
    case "pleine":
      return {
        level,
        percent,
        short: "Mémoire pleine",
        detail:
          "Le modèle va se perdre : réponses coupées, répétitions en boucle, consignes oubliées. Résumez la conversation ou commencez-en une nouvelle.",
      };
  }
}

/** Le seul mot, sans durée (pastilles étroites). */
export function plainSpeedWord(tokensPerSecond: number, lang: Lang): string {
  return WORDS[lang][classifySpeed(tokensPerSecond)];
}
