// Ce que la personne dit vouloir faire → les outils du catalogue à lui
// proposer. Le noyau ne connaît aucun outil par son nom : il cherche, dans ce
// que chaque entrée du catalogue annonce, des mots qui correspondent au besoin.

import type { IconName } from "@locaryn/ui-core";
import type { CatalogEntry } from "./core";
import type { Need } from "./profile";

export interface NeedDef {
  id: Need;
  icon: IconName;
  fr: { label: string; hint: string };
  en: { label: string; hint: string };
  /** Mots (minuscules) cherchés dans l'annonce d'une entrée. Vide : rien à installer. */
  words: string[];
}

export const NEEDS: NeedDef[] = [
  {
    id: "browser",
    icon: "globe",
    fr: {
      label: "Utiliser mon navigateur",
      hint: "Chercher, remplir des formulaires, naviguer à ma place",
    },
    en: { label: "Use my web browser", hint: "Search, fill in forms, browse for me" },
    words: ["browser", "navigateur"],
  },
  {
    id: "computer",
    icon: "monitor",
    fr: {
      label: "Piloter mon ordinateur",
      hint: "Ouvrir des applications, cliquer, saisir du texte",
    },
    en: { label: "Control my computer", hint: "Open apps, click, type text" },
    words: ["computer-use", "desktop", "bureau", "ordinateur"],
  },
  {
    id: "images",
    icon: "image",
    fr: { label: "Créer des images", hint: "Illustrations, photos, retouches" },
    en: { label: "Create images", hint: "Illustrations, photos, edits" },
    words: ["image-gen", "image"],
  },
  {
    id: "voice",
    icon: "microphone",
    fr: { label: "Parler et écouter", hint: "Lecture à voix haute, dictée" },
    en: { label: "Talk and listen", hint: "Read aloud, dictation" },
    words: ["voice", "tts", "voix", "speech"],
  },
  {
    id: "phone",
    icon: "device-mobile",
    fr: { label: "Utiliser sur mon téléphone", hint: "Retrouver mes conversations sur mobile" },
    en: { label: "Use it on my phone", hint: "Get my conversations on mobile" },
    words: [],
  },
];

export interface Suggestion {
  need: Need;
  entry: CatalogEntry;
}

function haystack(e: CatalogEntry): string {
  return [e.name, e.display_name, ...(e.advertised ?? []), ...(e.keywords ?? [])]
    .join(" ")
    .toLowerCase();
}

/** Au plus `perNeed` entrées installables par besoin, jamais deux fois la même. */
export function suggest(entries: CatalogEntry[], needs: Need[], perNeed = 1): Suggestion[] {
  const used = new Set<string>();
  const out: Suggestion[] = [];
  for (const need of needs) {
    const def = NEEDS.find((n) => n.id === need);
    if (!def || def.words.length === 0) continue;
    let n = 0;
    for (const e of entries) {
      if (n >= perNeed) break;
      if (e.installed || !e.install_source || e.compat === "unsupported" || used.has(e.id))
        continue;
      const h = haystack(e);
      if (!def.words.some((w) => h.includes(w))) continue;
      used.add(e.id);
      out.push({ need, entry: e });
      n++;
    }
  }
  return out;
}
