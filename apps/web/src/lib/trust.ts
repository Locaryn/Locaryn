import type { TrustLevel } from "./core";

/**
 * Les niveaux de permission, du plus fermé au plus ouvert — mêmes libellés et
 * mêmes explications que l'application de bureau (`apps/desktop/src/lib/trust.ts`),
 * qu'il faut modifier ensemble. Le web les présentait à tort : « trusted »
 * s'y appelait « Tout autoriser », alors qu'il ne laisse passer que les lectures.
 */
export const TRUST_LEVELS: { value: TrustLevel; label: string; hint: string }[] = [
  {
    value: "sandbox",
    label: "Aperçu seul",
    hint: "Lecture seule : toute écriture et toute commande sont refusées, sans exception.",
  },
  {
    value: "untrusted",
    label: "Prudent",
    hint: "Sécurité maximale : chaque accès aux fichiers, chaque commande et chaque outil demande votre accord.",
  },
  {
    value: "trusted",
    label: "Confiance",
    hint: "Les lectures passent sans question. Écrire, lancer une commande ou appeler un autre outil demande toujours.",
  },
  {
    value: "autonomous",
    label: "Autonome",
    hint: "Tout ce qui se passe sur l'ordinateur serveur passe sans question. Installer quelque chose (morph, skill, connecteur, logiciel venu du web) ou agir sur une autre machine demande encore.",
  },
  {
    value: "unrestricted",
    label: "Tout autoriser",
    hint: "Aucune question, jamais, installations et machines distantes comprises. Dangereux : à réserver à une tâche où une erreur ne coûte rien.",
  },
];
