import type { IconName } from "@locaryn/ui-core";

/**
 * L'espace de travail : le panneau de droite, fait d'onglets.
 *
 * Il remplace deux panneaux qui se disputaient la même place (l'aperçu des
 * « artefacts » et les paramètres du modèle) et en ajoute quatre : un
 * navigateur que le modèle sait piloter, les fichiers et les modifications du
 * dossier de la conversation, des terminaux. Un petit magasin plutôt qu'un
 * contexte : une exécution, le modèle (événement Rust) ou la barre du haut
 * ouvrent un onglet sans que le panneau ait à être monté.
 */

export type WorkspaceKind = "browser" | "files" | "terminal" | "changes" | "preview" | "model";

export interface WorkspaceTab {
  id: string;
  kind: WorkspaceKind;
}

export interface WorkspaceState {
  open: boolean;
  tabs: WorkspaceTab[];
  /** `null` : le lanceur (nouvel onglet). */
  active: string | null;
}

export interface KindInfo {
  label: string;
  icon: IconName;
  description: string;
  /** Plusieurs onglets de ce genre à la fois. */
  multiple: boolean;
}

export const KINDS: Record<WorkspaceKind, KindInfo> = {
  browser: {
    label: "Navigateur",
    icon: "globe",
    description: "Une vraie page web, que le modèle peut aussi lire et utiliser.",
    multiple: false,
  },
  files: {
    label: "Fichiers",
    icon: "folder-open",
    description: "Le dossier de la conversation, en lecture.",
    multiple: false,
  },
  terminal: {
    label: "Terminal",
    icon: "terminal-window",
    description: "Une ligne de commande dans le dossier de la conversation.",
    multiple: true,
  },
  changes: {
    label: "Modifications",
    icon: "git-branch",
    description: "Ce qui a changé dans le dépôt, fichier par fichier.",
    multiple: false,
  },
  preview: {
    label: "Aperçu",
    icon: "eye",
    description: "Le résultat d'un bloc de code exécuté : sortie ou page rendue.",
    multiple: false,
  },
  model: {
    label: "Modèle",
    icon: "sliders",
    description: "Échantillonnage, fenêtre de contexte, permissions de la conversation.",
    multiple: false,
  },
};

/** L'ordre du lanceur. */
export const LAUNCHER_ORDER: WorkspaceKind[] = [
  "browser",
  "files",
  "terminal",
  "changes",
  "preview",
  "model",
];

const STORAGE_KEY = "locaryn.workspace";

function isKind(value: unknown): value is WorkspaceKind {
  return typeof value === "string" && value in KINDS;
}

/** Les onglets de la dernière fois : une commodité, jamais une condition. */
function restore(): WorkspaceState {
  const empty: WorkspaceState = { open: false, tabs: [], active: null };
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return empty;
    const parsed = JSON.parse(raw) as Partial<WorkspaceState>;
    const tabs = Array.isArray(parsed.tabs)
      ? parsed.tabs.filter((t): t is WorkspaceTab => typeof t?.id === "string" && isKind(t?.kind))
      : [];
    const active =
      typeof parsed.active === "string" && tabs.some((t) => t.id === parsed.active)
        ? parsed.active
        : (tabs[0]?.id ?? null);
    return { open: parsed.open === true, tabs, active };
  } catch (e) {
    console.warn("espace de travail non restauré :", e);
    return empty;
  }
}

let state: WorkspaceState = restore();
const listeners = new Set<() => void>();
let counter = 0;

function commit(next: WorkspaceState) {
  state = next;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch (e) {
    console.warn("espace de travail non enregistré :", e);
  }
  for (const l of listeners) l();
}

export function getWorkspace(): WorkspaceState {
  return state;
}

export function subscribeWorkspace(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

function newId(kind: WorkspaceKind): string {
  counter += 1;
  return `${kind}-${Date.now().toString(36)}-${counter}`;
}

/**
 * Ouvrir (ou retrouver) un onglet et montrer le panneau. Un genre unique est
 * retrouvé s'il existe ; `fresh` force un nouvel onglet pour un genre multiple.
 */
export function openTab(kind: WorkspaceKind, fresh = false): void {
  const existing = state.tabs.find((t) => t.kind === kind);
  if (existing && !(fresh && KINDS[kind].multiple)) {
    commit({ ...state, open: true, active: existing.id });
    return;
  }
  const tab = { id: newId(kind), kind };
  commit({ open: true, tabs: [...state.tabs, tab], active: tab.id });
}

export function activateTab(id: string | null): void {
  commit({ ...state, open: true, active: id });
}

export function closeTab(id: string): void {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index < 0) return;
  const tabs = state.tabs.filter((t) => t.id !== id);
  // L'onglet voisin prend la place, comme dans un navigateur.
  const active =
    state.active === id ? (tabs[Math.min(index, tabs.length - 1)]?.id ?? null) : state.active;
  commit({ ...state, tabs, active });
}

export function setWorkspaceOpen(open: boolean): void {
  if (open === state.open) return;
  commit({ ...state, open });
}

/** Le bouton d'un onglet dans la barre du haut : l'ouvre, ou referme le
 *  panneau s'il le montre déjà. */
export function toggleTab(kind: WorkspaceKind): void {
  const current = state.tabs.find((t) => t.id === state.active);
  if (state.open && current?.kind === kind) {
    setWorkspaceOpen(false);
    return;
  }
  openTab(kind);
}
