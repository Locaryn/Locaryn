import type { TrustLevel } from "./core";

/**
 * Les trois niveaux de permission, décrits une seule fois.
 *
 * Ce qu'ils font réellement est fixé par la table d'approbation de la boucle
 * d'outils (`approval_decision`) : seuls les outils en lecture, sans risque,
 * s'exécutent sans confirmation — et seulement en « Confiance ». Une écriture
 * ou une commande demande toujours, quel que soit le niveau. Les libellés
 * disent cela : deux écrans affirmaient qu'un niveau « agit sans demander »,
 * ce que rien dans le code ne permet.
 */
export const TRUST_LEVELS: {
  value: TrustLevel;
  label: string;
  hint: string;
  color: string;
}[] = [
  {
    value: "untrusted",
    label: "Prudent",
    hint: "Chaque accès aux fichiers et chaque commande demande une confirmation. C'est le réglage par défaut.",
    color: "var(--warn)",
  },
  {
    value: "trusted",
    label: "Confiance",
    hint: "Les lectures s'exécutent sans confirmation. Écrire un fichier ou lancer une commande demande toujours.",
    color: "var(--accent-300)",
  },
  {
    value: "sandbox",
    label: "Aperçu seul",
    hint: "Lecture seule : toute écriture et toute commande sont refusées.",
    color: "var(--danger)",
  },
];

export function trustInfo(level: TrustLevel) {
  return TRUST_LEVELS.find((n) => n.value === level) ?? TRUST_LEVELS[0];
}
