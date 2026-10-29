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

/** Le seul mot, sans durée (pastilles étroites). */
export function plainSpeedWord(tokensPerSecond: number, lang: Lang): string {
  return WORDS[lang][classifySpeed(tokensPerSecond)];
}
