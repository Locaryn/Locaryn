import type { PresentationStep } from "@locaryn/ui-core";

/** Clé de la présentation vue. Changer le numéro la fait revoir à tout le monde. */
export const PRESENTATION_KEY = "locaryn.presentation.mobile.v1";

/** Ce qui ne se devine pas en ouvrant l'application sur un téléphone. */
export const PRESENTATION_MOBILE: PresentationStep[] = [
  {
    scene: "phone",
    title: "Votre ordinateur, dans votre poche",
    body: "Cette application se connecte à Locaryn sur votre ordinateur. Sur l'ordinateur, activez le mode serveur dans Réglages → Serveur & fonctions, puis scannez le code affiché, sur le même Wi-Fi.",
  },
  {
    scene: "local",
    title: "L'IA tourne chez vous",
    body: "Les modèles s'exécutent sur l'ordinateur : le téléphone envoie vos messages et affiche les réponses. Le premier message charge le modèle, c'est plus long la première fois.",
  },
  {
    scene: "chat",
    title: "Le modèle réfléchit avant de répondre",
    body: "Sa réflexion se replie au-dessus de la réponse. Touchez-la pour voir comment il est arrivé à sa conclusion.",
  },
  {
    scene: "permissions",
    title: "Il agit selon vos règles",
    body: "Le niveau de permission choisi sur l'ordinateur s'applique ici aussi : selon lui, le modèle agit seul ou vous demande votre accord.",
  },
  {
    scene: "extensions",
    title: "Les capacités de l'ordinateur",
    body: "Image, voix, outils : ce que les morphs et connecteurs apportent à l'ordinateur est disponible depuis le téléphone.",
  },
];
