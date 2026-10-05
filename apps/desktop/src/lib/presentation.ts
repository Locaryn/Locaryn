import type { PresentationStep } from "@locaryn/ui-core";

/** Clé de la présentation vue. Changer le numéro la fait revoir à tout le monde. */
export const PRESENTATION_KEY = "locaryn.presentation.bureau.v1";

/** Rejouer la présentation depuis n'importe quel écran (« À propos »). */
export const PRESENTATION_REPLAY = "locaryn:presentation-replay";

/**
 * Ce que la présentation explique : ce qui surprend quand on arrive d'un
 * assistant en ligne, ou qui ne se devine pas à l'écran.
 */
export const PRESENTATION_BUREAU: PresentationStep[] = [
  {
    scene: "local",
    title: "Votre IA tourne sur cet ordinateur",
    body: "Les modèles s'exécutent ici, vos conversations restent ici. Le premier message charge le modèle en mémoire : c'est plus long la première fois, puis instantané.",
  },
  {
    scene: "chat",
    title: "Le modèle réfléchit avant de répondre",
    body: "Sa réflexion s'affiche en direct puis se replie au-dessus de la réponse. Ouvrez-la pour voir comment il est arrivé à sa conclusion.",
  },
  {
    scene: "folder",
    title: "Un dossier de travail par conversation",
    body: "Choisissez le dossier avant d'envoyer le premier message : le modèle y lit et y écrit. Sans dossier, la conversation est libre et range ses fichiers à part.",
  },
  {
    scene: "permissions",
    title: "Vous décidez de ce qu'il peut faire",
    body: "De « Aperçu seul » à « Tout autoriser », cinq niveaux. Quand il doit demander, un bandeau apparaît au-dessus du champ : autorisez une fois, pour la session ou pour toujours.",
  },
  {
    scene: "extensions",
    title: "Morphs, skills et connecteurs",
    body: "Ajoutez des capacités : image, voix, contrôle de l'ordinateur, ou le serveur MCP d'un logiciel. Le modèle peut les installer lui-même — avec votre accord, sauf au niveau « Tout autoriser ».",
  },
  {
    scene: "drag",
    title: "Rangez d'un geste",
    body: "Prenez une conversation dans la liste et déposez-la sur la corbeille pour l'archiver, sur un projet pour l'y ranger, ou sur une autre pour les réunir. Lâchée ailleurs, elle reprend sa place.",
  },
  {
    scene: "phone",
    title: "Votre téléphone, en un scan",
    body: "Activez le mode serveur dans les réglages et scannez le code avec l'application mobile : vous retrouvez vos modèles et vos conversations sur le même Wi-Fi.",
  },
];
