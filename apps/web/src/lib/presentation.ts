import type { PresentationStep } from "@locaryn/ui-core";

/** Clé de la présentation vue. Changer le numéro la fait revoir à tout le monde. */
export const PRESENTATION_KEY = "locaryn.presentation.web.v1";

/** Ce qui ne se devine pas en ouvrant Locaryn dans un navigateur. */
export const PRESENTATION_WEB: PresentationStep[] = [
  {
    scene: "local",
    title: "Une IA hébergée chez vous",
    body: "Cette page parle à Locaryn sur un ordinateur de votre réseau : les modèles s'y exécutent, vos conversations y restent. Le premier message charge le modèle, c'est plus long la première fois.",
  },
  {
    scene: "chat",
    title: "Le modèle réfléchit avant de répondre",
    body: "Sa réflexion se replie au-dessus de la réponse. Ouvrez-la pour voir comment il est arrivé à sa conclusion.",
  },
  {
    scene: "permissions",
    title: "Vous décidez de ce qu'il peut faire",
    body: "Dans les réglages, cinq niveaux, de « Aperçu seul » à « Tout autoriser ». Survolez-les : chacun dit exactement ce qu'il laisse passer.",
  },
  {
    scene: "extensions",
    title: "Morphs, skills et connecteurs",
    body: "Les capacités installées sur l'ordinateur — image, voix, outils — sont disponibles ici. Le modèle peut aussi en installer, avec votre accord.",
  },
];
