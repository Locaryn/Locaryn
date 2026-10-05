import type { TrustLevel } from "./core";

/**
 * Les niveaux de permission, décrits une seule fois.
 *
 * Ce qu'ils font réellement est fixé par la table d'approbation de la boucle
 * d'outils (`approval_decision`). Chaque libellé doit dire exactement cela : un
 * menu qui promet « Auto-approbation des modifications de code » alors que le
 * code redemande à chaque écriture est pire qu'un menu absent. Le risque d'un
 * outil MCP vient de ce que son serveur en annonce (lecture seule → faible,
 * destructif → élevé, sinon moyen).
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
    hint: "Les lectures et les outils annoncés en lecture seule s'exécutent sans confirmation. Écrire un fichier, appeler un autre outil ou lancer une commande demande toujours.",
    color: "var(--accent-300)",
  },
  {
    value: "autonomous",
    label: "Autonome",
    hint: "Lit, écrit dans le projet et appelle les outils qui ne détruisent rien sans demander. Les commandes, les outils annoncés comme destructifs et les machines distantes demandent toujours.",
    color: "var(--accent)",
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
