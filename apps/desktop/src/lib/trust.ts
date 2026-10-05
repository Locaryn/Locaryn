import type { TrustLevel } from "./core";

/**
 * Les niveaux de permission, décrits une seule fois, du plus fermé au plus
 * ouvert.
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
    value: "sandbox",
    label: "Aperçu seul",
    hint: "Lecture seule : toute écriture et toute commande sont refusées, sans exception.",
    color: "var(--text-faint)",
  },
  {
    value: "untrusted",
    label: "Prudent",
    hint: "Sécurité maximale : chaque accès aux fichiers, chaque commande et chaque outil demande votre accord. C'est le réglage par défaut.",
    color: "var(--warn)",
  },
  {
    value: "trusted",
    label: "Confiance",
    hint: "Les lectures et les outils annoncés en lecture seule passent sans question. Écrire un fichier, lancer une commande ou appeler un autre outil demande toujours.",
    color: "var(--accent-300)",
  },
  {
    value: "autonomous",
    label: "Autonome",
    hint: "Tout ce qui se passe sur cet ordinateur passe sans question : lectures, écritures, commandes, outils. Deux choses demandent encore : installer quelque chose (un morph, un skill, un connecteur, un logiciel ou un script venu du web, dont rien ne garantit la provenance) et agir sur une autre machine.",
    color: "var(--accent)",
  },
  {
    value: "unrestricted",
    label: "Tout autoriser",
    hint: "Aucune question, jamais : installations et machines distantes comprises. Dangereux — le modèle peut tout faire, y compris installer un logiciel inconnu ou effacer des fichiers. À réserver à une machine ou une tâche où une erreur ne coûte rien.",
    color: "var(--danger)",
  },
];

export function trustInfo(level: TrustLevel) {
  return TRUST_LEVELS.find((n) => n.value === level) ?? TRUST_LEVELS[1];
}
