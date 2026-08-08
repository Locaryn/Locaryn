// Interface language. The app was French-first with English leftovers scattered
// around (Install / Server / Search…), which read as broken. Everything the user
// sees now goes through `t()`.
//
// Product nouns are NEVER translated: model names, brands, file formats and tool
// names (Qwen3, GGUF, LoRA, llama.cpp, MCP…) must stay verbatim — translating
// them would be meaningless and would break search.

import { useSyncExternalStore } from "react";

export type Lang = "fr" | "en";

const STORAGE_KEY = "locaryn.lang";

/** Terms that must survive translation untouched, whatever the language. */
export const DO_NOT_TRANSLATE = [
  "Locaryn", "llama.cpp", "llama-server", "stable-diffusion.cpp", "GGUF", "LoRA",
  "MCP", "RAG", "JSON", "VRAM", "RAM", "GPU", "CPU", "KV", "MoE", "RPC", "SSH",
  "Qwen", "Llama", "Gemma", "Mistral", "DeepSeek", "Phi", "GLM", "Z-Image",
  "HuggingFace", "Ollama", "Tauri", "Vulkan", "CUDA",
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
  "Installer": "Install",
  "Télécharger": "Download",
  "Supprimer": "Delete",
  "Annuler": "Cancel",
  "Enregistrer": "Save",
  "Rechercher": "Search",
  "Fermer": "Close",
  "Ouvrir": "Open",
  "Utiliser": "Use",
  "Tester": "Test",
  "Actualiser": "Refresh",
  "Effacer": "Clear",
  "Ajouter": "Add",
  "Retirer": "Remove",
  "Archiver": "Archive",
  "Envoyer": "Send",
  "Générer": "Generate",
  "Chargement…": "Loading…",

  // Settings sections
  "Moteur IA": "AI Engine",
  "Performance": "Performance",
  "Projets": "Projects",
  "Extensions": "Extensions",
  "Apparence": "Appearance",
  "Stockage": "Storage",
  "À propos": "About",
  "Modèle": "Model",
  "Langue": "Language",
  "Serveur": "Server",
  "Version": "Version",
  "Parcourir les modèles": "Browse models",
  "Tous les paramètres →": "All settings →",

  // Chat
  "Posez votre question à Locaryn…": "Ask Locaryn…",
  "Image": "Image",
  "Créer": "Create",
  "Documents": "Documents",
  "Réflexion": "Reasoning",
  "Et ensuite ?": "What's next?",
  "Suggestions…": "Suggestions…",
  "Nouvelle conversation": "New conversation",

  // Project settings
  "Autorisations des outils": "Tool permissions",
  "Base de connaissances (RAG)": "Knowledge base (RAG)",
  "Actions": "Actions",
  "Confiance": "Trusted",
  "Prudent": "Cautious",
  "Bac à sable": "Sandbox",
};

let current: Lang =
  (typeof localStorage !== "undefined" && (localStorage.getItem(STORAGE_KEY) as Lang)) || "fr";

const listeners = new Set<() => void>();

export function getLang(): Lang {
  return current;
}

export function setLang(l: Lang) {
  current = l;
  try {
    localStorage.setItem(STORAGE_KEY, l);
  } catch {
    // Private mode / storage disabled — the choice just won't persist.
  }
  if (typeof document !== "undefined") document.documentElement.lang = l;
  listeners.forEach((fn) => fn());
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Translate a French source string. Unknown strings pass through unchanged,
 *  so an untranslated label degrades to French rather than to a missing key. */
export function t(fr: string): string {
  if (current === "fr") return fr;
  return EN[fr] ?? fr;
}

/** React hook: `const { t, lang, setLang } = useI18n()`. Re-renders on change. */
export function useI18n() {
  const lang = useSyncExternalStore(subscribe, getLang, getLang);
  return { lang, setLang, t } as const;
}

export const LANGUAGES: { id: Lang; label: string; flag: string }[] = [
  { id: "fr", label: "Français", flag: "🇫🇷" },
  { id: "en", label: "English", flag: "🇬🇧" },
];
