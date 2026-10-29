// Ce que l'accueil d'une conversation vide dit et propose, selon la personne.
//
// Deux niveaux : l'interface simplifiée parle de la vie de tous les jours,
// l'interface complète de code et d'outils. Les phrases viennent d'abord de ce
// que le modèle a préparé d'avance d'après la mémoire (voir le jardinier) ; à
// défaut, des phrases génériques du bon niveau.

import { useEffect, useState } from "react";
import { core } from "./core";
import { type UiMode, useProfile } from "./profile";

export interface Welcome {
  sub: string;
  suggestions: string[];
}

const DEFAUTS: Record<UiMode, Welcome> = {
  expert: {
    sub: "Locaryn peut lire, chercher, éditer et exécuter du code dans ce projet ou en chat libre.",
    suggestions: [
      "Explique la structure de ce projet",
      "Écris des tests unitaires pour la logique principale",
      "Cherche les commentaires FIXME et TODO",
    ],
  },
  simple: {
    sub: "Posez une question ou demandez un coup de main : tout reste sur votre ordinateur.",
    suggestions: [
      "Aide-moi à écrire un message poli",
      "Explique-moi un sujet simplement",
      "Donne-moi des idées pour ce week-end",
    ],
  },
};

/** Les phrases d'accueil à afficher pour ce niveau, personnalisées si elles
 *  existent. Jamais vide : les défauts servent tant que rien n'est préparé. */
export function useWelcome(): Welcome {
  const { mode } = useProfile();
  const [prepared, setPrepared] = useState<Record<UiMode, string[]>>({ simple: [], expert: [] });

  useEffect(() => {
    let off = false;
    core
      .welcomeSuggestions()
      .then((w) => !off && setPrepared({ simple: w.simple, expert: w.expert }))
      .catch((e) => console.warn("[Accueil] phrases préparées illisibles :", e));
    return () => {
      off = true;
    };
  }, []);

  const own = prepared[mode];
  return {
    sub: DEFAUTS[mode].sub,
    suggestions: own.length >= 3 ? own.slice(0, 3) : DEFAUTS[mode].suggestions,
  };
}
