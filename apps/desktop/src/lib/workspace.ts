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

export type WorkspaceKind =
  | "browser"
  | "files"
  | "terminal"
  | "changes"
  | "preview"
  | "model"
  | "tasks";

export interface WorkspaceTab {
  id: string;
  kind: WorkspaceKind;
}

export interface WorkspaceState {
  open: boolean;
  tabs: WorkspaceTab[];
  /** `null` : le lanceur (nouvel onglet). */
  active: string | null;
  /** Largeur du panneau en pixels ; absente, la largeur par défaut. */
  width?: number;
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
  tasks: {
    label: "Tâches",
    icon: "list-bullets",
    description:
      "Ce qui tourne en arrière-plan : sous-agents, plans, téléchargements, générations.",
    multiple: false,
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
  "tasks",
];

/**
 * Chaque conversation a son propre espace de travail : panneau ouvert ou non,
 * onglets, onglet actif, largeur. Changer de conversation reprend le sien ;
 * une nouvelle conversation part de l'espace vide.
 */
const STORAGE_KEY = "locaryn.workspace.conversations";

/** Au-delà, les espaces les plus anciens sont oubliés. */
const MAX_CONVERSATIONS = 200;

const EMPTY: WorkspaceState = { open: false, tabs: [], active: null };

function isKind(value: unknown): value is WorkspaceKind {
  return typeof value === "string" && value in KINDS;
}

function lire(valeur: unknown): WorkspaceState | null {
  if (typeof valeur !== "object" || valeur === null) return null;
  const parsed = valeur as Partial<WorkspaceState>;
  const tabs = Array.isArray(parsed.tabs)
    ? parsed.tabs.filter((t): t is WorkspaceTab => typeof t?.id === "string" && isKind(t?.kind))
    : [];
  const active =
    typeof parsed.active === "string" && tabs.some((t) => t.id === parsed.active)
      ? parsed.active
      : (tabs[0]?.id ?? null);
  const width = typeof parsed.width === "number" && parsed.width > 0 ? parsed.width : undefined;
  return { open: parsed.open === true, tabs, active, width };
}

/** Les espaces de la dernière fois : une commodité, jamais une condition. */
function restore(): Map<string, WorkspaceState> {
  const espaces = new Map<string, WorkspaceState>();
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return espaces;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    for (const [conversation, valeur] of Object.entries(parsed)) {
      const espace = lire(valeur);
      if (espace) espaces.set(conversation, espace);
    }
  } catch (e) {
    console.warn("espace de travail non restauré :", e);
  }
  return espaces;
}

/** Clé de la conversation affichée ; `""` hors de toute conversation. */
let conversation = "";
const espaces = restore();
/** Les conversations ouvertes depuis le lancement : leurs onglets restent
 *  montés (un terminal garde sa sortie, le navigateur sa page). */
const visitees = new Set<string>([conversation]);
let state: WorkspaceState = espaces.get(conversation) ?? EMPTY;
const listeners = new Set<() => void>();
let counter = 0;

function enregistrer() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(Object.fromEntries(espaces)));
  } catch (e) {
    console.warn("espace de travail non enregistré :", e);
  }
}

/** Poser l'espace d'une conversation, en le rangeant parmi les plus récents. */
function ranger(cle: string, espace: WorkspaceState) {
  espaces.delete(cle);
  espaces.set(cle, espace);
  for (const ancienne of espaces.keys()) {
    if (espaces.size <= MAX_CONVERSATIONS) break;
    if (ancienne !== conversation) espaces.delete(ancienne);
  }
  enregistrer();
}

function notifier() {
  for (const l of listeners) l();
}

function commit(next: WorkspaceState) {
  state = next;
  ranger(conversation, next);
  notifier();
}

export function getWorkspace(): WorkspaceState {
  return state;
}

export function subscribeWorkspace(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Afficher l'espace de `id` (la conversation qu'on vient d'ouvrir). */
export function setWorkspaceConversation(id: string | null): void {
  const cle = id ?? "";
  if (cle === conversation) return;
  conversation = cle;
  visitees.add(cle);
  state = espaces.get(cle) ?? EMPTY;
  notifier();
}

/** La conversation dont l'espace est affiché. */
export function getWorkspaceConversation(): string {
  return conversation;
}

/** Les espaces des conversations ouvertes depuis le lancement. */
export function getVisitedWorkspaces(): Array<[string, WorkspaceState]> {
  return [...visitees].map((cle) => [
    cle,
    cle === conversation ? state : (espaces.get(cle) ?? EMPTY),
  ]);
}

/** Oublier l'espace d'une conversation supprimée. */
export function forgetWorkspace(id: string): void {
  visitees.delete(id);
  if (!espaces.delete(id)) return;
  enregistrer();
  notifier();
}

export function setWorkspaceWidth(width: number): void {
  if (width === state.width) return;
  commit({ ...state, width });
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
  commit(avecOnglet(state, kind, fresh));
}

function avecOnglet(espace: WorkspaceState, kind: WorkspaceKind, fresh: boolean): WorkspaceState {
  const existing = espace.tabs.find((t) => t.kind === kind);
  if (existing && !(fresh && KINDS[kind].multiple)) {
    return { ...espace, open: true, active: existing.id };
  }
  const tab = { id: newId(kind), kind };
  return { ...espace, open: true, tabs: [...espace.tabs, tab], active: tab.id };
}

/**
 * Ouvrir un onglet dans l'espace d'une conversation précise — le modèle d'une
 * conversation en arrière-plan ouvre son navigateur sans toucher à celle
 * qu'on regarde.
 */
export function openTabFor(id: string | null, kind: WorkspaceKind): void {
  const cle = id ?? "";
  if (cle === conversation) {
    openTab(kind);
    return;
  }
  visitees.add(cle);
  ranger(cle, avecOnglet(espaces.get(cle) ?? EMPTY, kind, false));
  notifier();
}

/** Le fichier que l'onglet Fichiers doit ouvrir en arrivant. */
let fichierEnAttente: string | null = null;

/** L'événement que l'onglet Fichiers écoute pour ouvrir un fichier. */
export const OUVRIR_FICHIER = "locaryn:workspace-open-file";

/**
 * Ouvrir un fichier du dossier de la conversation dans le panneau Fichiers —
 * depuis un fichier que le modèle vient de créer, cité sous sa réponse.
 * L'onglet peut ne pas exister encore : le chemin attend qu'il le reprenne.
 */
export function ouvrirFichier(chemin: string): void {
  fichierEnAttente = chemin;
  openTab("files");
  window.dispatchEvent(new CustomEvent(OUVRIR_FICHIER, { detail: chemin }));
}

/** Le fichier en attente d'ouverture, une seule fois. */
export function prendreFichierEnAttente(): string | null {
  const c = fichierEnAttente;
  fichierEnAttente = null;
  return c;
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
