// Les textes de l'accueil, dans les deux langues. L'accueil s'affiche avant que
// la langue ne soit choisie : il porte ses propres textes plutôt que de compter
// sur le dictionnaire général.

import type { Lang } from "../lib/i18n";
import type { ModelTier } from "../lib/onboardingModels";

export interface Texts {
  aria: string;
  next: string;
  back: string;
  skip: string;
  later: string;
  start: string;
  install: string;
  gb: string;
  recommended: string;
  tooHeavy: string;
  languageTitle: string;
  languageBody: string;
  welcomeTitle: string;
  welcomeBody: string;
  welcomePoints: { title: string; body: string }[];
  levelTitle: string;
  levelBody: string;
  levelExpert: string;
  levelExpertHint: string;
  levelSimple: string;
  levelSimpleHint: string;
  levelNote: string;
  modelsTitle: string;
  modelsBody: string;
  modelsNote: string;
  modelsLoading: string;
  npuReason: string;
  tiers: Record<ModelTier, { name: string; hint: string }>;
  needsTitle: string;
  needsBody: string;
  suggestTitle: string;
  suggestBody: string;
  readyTitle: string;
  readyBody: string;
  readyPhone: string;
  readyAdvanced: string;
}

const fr: Texts = {
  aria: "Bienvenue dans Locaryn",
  next: "Continuer",
  back: "Retour",
  skip: "Passer",
  later: "Plus tard",
  start: "C'est parti",
  install: "Installer",
  gb: "Go",
  recommended: "Conseillé",
  tooHeavy: "Trop lourd pour cet ordinateur",
  languageTitle: "Choose your language · Choisissez votre langue",
  languageBody: "Vous pourrez la changer à tout moment dans les réglages.",
  welcomeTitle: "Bienvenue dans Locaryn",
  welcomeBody: "Une intelligence artificielle qui vous aide, et qui reste chez vous.",
  welcomePoints: [
    {
      title: "Elle tourne sur cet ordinateur",
      body: "Pas de compte, pas d'abonnement, rien à envoyer sur Internet.",
    },
    {
      title: "Vos conversations restent privées",
      body: "Elles sont gardées ici, et nulle part ailleurs.",
    },
    {
      title: "Elle peut apprendre de nouveaux gestes",
      body: "Images, voix, navigateur… vous ajoutez ce dont vous avez besoin.",
    },
  ],
  levelTitle: "Êtes-vous à l'aise avec l'IA et l'informatique ?",
  levelBody: "Cela nous dit comment vous présenter l'application.",
  levelExpert: "Oui, je m'y connais",
  levelExpertHint: "L'application complète, tous les réglages à portée de main.",
  levelSimple: "Non, pas vraiment",
  levelSimpleHint: "Une interface épurée : on s'occupe de l'essentiel pour vous.",
  levelNote: "Vous pourrez changer d'avis à tout moment dans Réglages → Compte → Apparence.",
  modelsTitle: "Choisissez votre assistant",
  modelsBody:
    "Proposés selon votre ordinateur. Un clic : il se télécharge pendant que vous continuez.",
  modelsNote: "Pas de panique, vous pourrez en changer ou en ajouter plus tard.",
  modelsLoading: "On regarde la puissance de votre ordinateur…",
  npuReason: "Votre ordinateur a une puce dédiée à l'IA : cet outil sait s'en servir.",
  tiers: {
    fast: { name: "Éclair", hint: "Très rapide, réponses simples" },
    balanced: { name: "Équilibré", hint: "Rapide et assez malin" },
    smart: { name: "Cerveau", hint: "Très intelligent, plus lent" },
  },
  needsTitle: "Qu'aimeriez-vous en faire ?",
  needsBody: "Cochez ce qui vous intéresse, on prépare le reste. Plusieurs choix possibles.",
  suggestTitle: "Voici ce qui vous irait",
  suggestBody: "Ces outils correspondent à vos réponses. Installez-les maintenant ou plus tard.",
  readyTitle: "Tout est prêt",
  readyBody: "Écrivez votre premier message. Le modèle finit de se télécharger en arrière-plan.",
  readyPhone: "Pour l'utiliser sur votre téléphone : Réglages → Serveur & fonctions.",
  readyAdvanced:
    "Un besoin particulier ? Les réglages avancés se déplient tout en bas des Réglages.",
};

const en: Texts = {
  aria: "Welcome to Locaryn",
  next: "Continue",
  back: "Back",
  skip: "Skip",
  later: "Later",
  start: "Let's go",
  install: "Install",
  gb: "GB",
  recommended: "Recommended",
  tooHeavy: "Too heavy for this computer",
  languageTitle: "Choose your language · Choisissez votre langue",
  languageBody: "You can change it at any time in the settings.",
  welcomeTitle: "Welcome to Locaryn",
  welcomeBody: "An AI that helps you, and stays at home.",
  welcomePoints: [
    {
      title: "It runs on this computer",
      body: "No account, no subscription, nothing to send over the Internet.",
    },
    { title: "Your conversations stay private", body: "They are kept here and nowhere else." },
    { title: "It can learn new tricks", body: "Images, voice, browser… add what you need." },
  ],
  levelTitle: "Are you comfortable with AI and computers?",
  levelBody: "This tells us how to present the app to you.",
  levelExpert: "Yes, I know my way around",
  levelExpertHint: "The full app, every setting within reach.",
  levelSimple: "Not really",
  levelSimpleHint: "A clean interface: we take care of the essentials for you.",
  levelNote: "You can change your mind at any time in Settings → Account → Appearance.",
  modelsTitle: "Pick your assistant",
  modelsBody: "Suggested for your computer. One click: it downloads while you carry on.",
  modelsNote: "No worries, you can switch or add more later.",
  modelsLoading: "Checking how powerful your computer is…",
  npuReason: "Your computer has a chip dedicated to AI: this tool knows how to use it.",
  tiers: {
    fast: { name: "Lightning", hint: "Very fast, simple answers" },
    balanced: { name: "Balanced", hint: "Quick and fairly smart" },
    smart: { name: "Brainy", hint: "Very smart, slower" },
  },
  needsTitle: "What would you like to do with it?",
  needsBody: "Tick what interests you and we'll set up the rest. You can pick several.",
  suggestTitle: "Here is what would suit you",
  suggestBody: "These tools match your answers. Install them now or later.",
  readyTitle: "All set",
  readyBody: "Write your first message. The model finishes downloading in the background.",
  readyPhone: "To use it on your phone: Settings → Server & features.",
  readyAdvanced:
    "Need something specific? Advanced settings unfold at the very bottom of Settings.",
};

export const TXT: Record<Lang, Texts> = { fr, en };
