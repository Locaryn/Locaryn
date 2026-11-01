// Interface language. The app was French-first with English leftovers scattered
// around (Install / Server / Search…), which read as broken. Everything the user
// sees now goes through `t()`.
//
// Product nouns are NEVER translated: model names, brands, file formats and tool
// names (Qwen3, GGUF, LoRA, llama.cpp, MCP…) must stay verbatim — translating
// them would be meaningless and would break search.

import { useSyncExternalStore } from "react";
import { type Lang as Langue, appliquerLangue, localiser, traduire } from "./autoTranslate";

/** La langue des textes écrits dans le code : français, ou anglais pour tout le reste.
 *  L'espagnol et l'allemand se posent ensuite sur l'anglais. */
export type Lang = "fr" | "en";
export type { Langue };

const STORAGE_KEY = "locaryn.lang";

/** Terms that must survive translation untouched, whatever the language. */
export const DO_NOT_TRANSLATE = [
  "Locaryn",
  "llama.cpp",
  "llama-server",
  "stable-diffusion.cpp",
  "GGUF",
  "LoRA",
  "MCP",
  "RAG",
  "JSON",
  "VRAM",
  "RAM",
  "GPU",
  "CPU",
  "KV",
  "MoE",
  "RPC",
  "SSH",
  "Qwen",
  "Llama",
  "Gemma",
  "Mistral",
  "DeepSeek",
  "Phi",
  "GLM",
  "Z-Image",
  "HuggingFace",
  "Ollama",
  "Tauri",
  "Vulkan",
  "CUDA",
];

/** fr → en. Keys are the French source strings (the app's base language). */
const EN: Record<string, string> = {
  // Navigation & views
  "Chat & Assistant Agent": "Chat & Agent Assistant",
  "Mes Modèles Installés": "My Installed Models",
  "Marketplace Modèles": "Model Marketplace",
  "Entraînement & Oblitération": "Model Studio",
  "Connecteurs & MCP": "Connectors & MCP",
  "Paramètres Système": "System Settings",
  "Paramètres de l'application": "Application Settings",
  "Paramètres du chat": "Chat Settings",
  "Nouveau Chat Libre": "New Free Chat",
  "Conversations Libres": "Free Conversations",
  "Aucune conversation libre": "No free conversation",
  "Projets Code": "Code Projects",
  "Ajouter un projet": "Add a project",

  // Common actions
  Installer: "Install",
  Télécharger: "Download",
  Supprimer: "Delete",
  Annuler: "Cancel",
  Enregistrer: "Save",
  Rechercher: "Search",
  Fermer: "Close",
  Ouvrir: "Open",
  Utiliser: "Use",
  Tester: "Test",
  Actualiser: "Refresh",
  Effacer: "Clear",
  Ajouter: "Add",
  Retirer: "Remove",
  Archiver: "Archive",
  Envoyer: "Send",
  Générer: "Generate",
  "Chargement…": "Loading…",

  // Settings sections
  "Moteur IA": "AI Engine",
  Performance: "Performance",
  Projets: "Projects",
  Extensions: "Extensions",
  Apparence: "Appearance",
  Stockage: "Storage",
  "À propos": "About",
  Modèle: "Model",
  Langue: "Language",
  Serveur: "Server",
  Version: "Version",
  "Parcourir les modèles": "Browse models",
  "Tous les paramètres →": "All settings →",

  // Chat
  "Posez votre question à Locaryn…": "Ask Locaryn…",
  Image: "Image",
  Créer: "Create",
  Documents: "Documents",
  Réflexion: "Reasoning",
  "Et ensuite ?": "What's next?",
  "Suggestions…": "Suggestions…",
  "Nouvelle conversation": "New conversation",

  // Project settings
  "Autorisations des outils": "Tool permissions",
  "Base de connaissances (RAG)": "Knowledge base (RAG)",
  Actions: "Actions",
  Confiance: "Trusted",
  Prudent: "Cautious",
  "Aperçu seul": "Preview only",
};

const CONNUES: readonly Langue[] = ["fr", "en", "es", "de"];

function estLang(v: string | null): v is Langue {
  return v !== null && (CONNUES as readonly string[]).includes(v);
}

/** La langue du système (« de-CH » → « de »), l'anglais si elle n'est pas proposée. */
export function langueSysteme(): Langue {
  const langues =
    typeof navigator === "undefined" ? [] : (navigator.languages ?? [navigator.language]);
  for (const l of langues) {
    const court = (l ?? "").slice(0, 2).toLowerCase();
    if (estLang(court)) return court;
  }
  return "en";
}

/** Langue de départ : celle déjà choisie ; sinon celle du système. */
function initialLang(): Langue {
  if (typeof localStorage === "undefined") return "fr";
  try {
    const kept = localStorage.getItem(STORAGE_KEY);
    if (estLang(kept)) return kept;
    return langueSysteme();
  } catch (e) {
    console.warn("[Langue] stockage illisible, langue du système :", e);
    return langueSysteme();
  }
}

let current: Langue = initialLang();

const listeners = new Set<() => void>();

function getLangue(): Langue {
  return current;
}

function setLang(l: Langue) {
  current = l;
  try {
    localStorage.setItem(STORAGE_KEY, l);
  } catch (e) {
    console.warn("[Langue] choix non mémorisé :", e);
  }
  if (typeof document !== "undefined") document.documentElement.lang = l;
  // Toute l'interface suit, pas seulement les textes passés à `t()`.
  appliquerLangue(l);
  for (const fn of listeners) fn();
}

// La langue gardée d'une session précédente s'applique dès que l'interface
// existe.
if (typeof document !== "undefined" && current !== "fr") {
  const lancer = () => {
    document.documentElement.lang = current;
    appliquerLangue(current);
  };
  if (document.body) lancer();
  else document.addEventListener("DOMContentLoaded", lancer, { once: true });
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Translate a French source string. Unknown strings pass through unchanged,
 *  so an untranslated label degrades to French rather than to a missing key. */
export function t(fr: string): string {
  if (current === "fr") return fr;
  const en = EN[fr];
  if (en !== undefined) return localiser(en, current);
  return traduire(fr) ?? fr;
}

/** React hook: `const { t, lang, setLang } = useI18n()`. Re-renders on change. */
export function useI18n() {
  const langue = useSyncExternalStore(subscribe, getLangue, getLangue);
  const lang: Lang = langue === "fr" ? "fr" : "en";
  return { lang, langue, setLang, t } as const;
}

export const LANGUAGES: { id: Langue; label: string; flag: string }[] = [
  { id: "fr", label: "Français", flag: "🇫🇷" },
  { id: "en", label: "English", flag: "🇬🇧" },
  { id: "es", label: "Español", flag: "🇪🇸" },
  { id: "de", label: "Deutsch", flag: "🇩🇪" },
];
