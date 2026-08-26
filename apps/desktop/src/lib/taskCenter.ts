// Lightweight global task store powering the notification center + footer.
// Any module can register/update a background task (download, image generation,
// model edit, workflow); the <TaskCenter> UI subscribes and renders them,
// colour-coded by type. No React context threading required.

import type { IconName } from "@locaryn/ui-core";
import { useSyncExternalStore } from "react";
import { notifierSysteme } from "./osNotify";

/** Douze entrées maximum : au-delà, la pile n'est plus lisible, elle archive. */
const MAX_TASKS = 12;

export type TaskType = "download" | "generation" | "edit" | "workflow" | "audio";
export type TaskStatus = "running" | "done" | "error";

export interface AppTask {
  id: string;
  type: TaskType;
  label: string;
  status: TaskStatus;
  /** 0-100 when known; undefined for indeterminate work. */
  progress?: number;
  /** Short status line (e.g. "51s", "downloading…"). */
  detail?: string;
  /** Populated on completion — e.g. a generated image data URL. */
  resultImageUrl?: string;
  /** Original disk path for the result (non-URL, usable by backend). */
  resultPath?: string;
  /** Populated on completion — e.g. a generated audio URL. */
  resultAudioUrl?: string;
  error?: string;
  /** Vrai dès que le centre de notifications a été ouvert sur cette entrée. */
  read?: boolean;
  createdAt: number;
  updatedAt: number;

  // ── Workflow tasks only ──────────────────────────────────────────────────
  /** LLM-generated plan; length is dynamic (varies per run). */
  steps?: string[];
  /** Number of steps completed so far (0..steps.length). Drives the bar. */
  stepIndex?: number;
  /** Retry counter — shown as "essai N" when the final check fails and the
   *  workflow is relaunched. 1 on the first run. */
  attempt?: number;
}

/**
 * Ce que chaque type met devant son libellé.
 *
 * `icon` est un nom du jeu partagé, pas un caractère : rendu tel quel, il
 * écrivait le mot « download » dans la barre d'état.
 */
export const TASK_META: Record<TaskType, { label: string; icon: IconName; color: string }> = {
  download: { label: "Téléchargement", icon: "download", color: "var(--info)" },
  generation: { label: "Génération", icon: "image", color: "var(--info)" },
  edit: { label: "Édition modèle", icon: "extensions", color: "var(--accent-300)" },
  workflow: { label: "Workflow", icon: "extensions", color: "var(--warn)" },
  audio: { label: "Synthèse vocale", icon: "mic", color: "var(--info)" },
};

let tasks: AppTask[] = [];
const listeners = new Set<() => void>();
let seq = 0;

function emit() {
  // New array identity so useSyncExternalStore detects the change.
  tasks = [...tasks];
  for (const l of listeners) l();
}

function now(): number {
  // Date.now avoided in workflow scripts, fine in the app runtime.
  return Date.now();
}

/** localStorage key used to persist completed/failed tasks across restarts. */
const STORAGE_KEY = "locaryn_gallery_tasks";

/** Serialize only terminal-state tasks (done / error) to localStorage. */
function persistTasks() {
  const terminal = tasks.filter((t) => t.status === "done" || t.status === "error");
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(terminal));
  } catch {
    // Storage full or unavailable — silently ignore.
  }
}

/** Restore previously persisted tasks from localStorage into the task list. */
function restoreTasks() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    const saved: AppTask[] = JSON.parse(raw);
    if (!Array.isArray(saved)) return;
    // Replay them in order (oldest first, so the list is chronological)
    for (const t of saved) {
      tasks.push(t);
    }
    // Bump the sequence counter past the restored ids
    for (const t of saved) {
      const num = Number.parseInt(t.id.replace(/^t/, ""), 10);
      if (num >= seq) seq = num + 1;
    }
  } catch {
    // Corrupted data — start fresh.
    localStorage.removeItem(STORAGE_KEY);
  }
}

// Restore persisted results on module load.
restoreTasks();

export const taskCenter = {
  add(input: { type: TaskType; label: string; progress?: number; detail?: string }): string {
    const id = `t${++seq}`;
    const t: AppTask = {
      id,
      type: input.type,
      label: input.label,
      status: "running",
      progress: input.progress,
      detail: input.detail,
      read: false,
      createdAt: now(),
      updatedAt: now(),
    };
    tasks = [t, ...tasks].slice(0, MAX_TASKS);
    emit();
    return id;
  },
  update(id: string, patch: Partial<Omit<AppTask, "id">>) {
    tasks = tasks.map((t) => (t.id === id ? { ...t, ...patch, updatedAt: now() } : t));
    emit();
  },
  done(id: string, patch?: Partial<Omit<AppTask, "id" | "status">>) {
    const label = tasks.find((t) => t.id === id)?.label;
    this.update(id, { status: "done", progress: 100, ...patch });
    persistTasks();
    void notifierSysteme("Terminé", label);
  },
  fail(id: string, error: string) {
    const label = tasks.find((t) => t.id === id)?.label;
    this.update(id, { status: "error", error });
    persistTasks();
    // L'échec porte sa cause : c'est la seule chose qu'on puisse faire de
    // cette bannière sans rouvrir l'application.
    void notifierSysteme("Échec", label ? `${label} — ${error}` : error);
  },

  // ── Workflow driving API (plan generated by the LLM, dynamic length) ──────
  /** Start a workflow task before the plan is known (attempt 1). */
  addWorkflow(label: string): string {
    const id = `t${++seq}`;
    const t: AppTask = {
      id,
      type: "workflow",
      label,
      status: "running",
      attempt: 1,
      stepIndex: 0,
      steps: [],
      read: false,
      createdAt: now(),
      updatedAt: now(),
    };
    tasks = [t, ...tasks].slice(0, MAX_TASKS);
    emit();
    return id;
  },
  /** Attach the LLM-generated plan (variable number of steps). */
  setPlan(id: string, steps: string[]) {
    this.update(id, { steps, stepIndex: 0 });
  },
  /** Mark the current step done and move to the next. */
  advanceStep(id: string) {
    const t = tasks.find((x) => x.id === id);
    if (t) this.update(id, { stepIndex: Math.min((t.stepIndex ?? 0) + 1, t.steps?.length ?? 0) });
  },
  /** Final check failed → relaunch: bump the attempt counter, reset progress. */
  retryWorkflow(id: string) {
    const t = tasks.find((x) => x.id === id);
    if (t)
      this.update(id, {
        attempt: (t.attempt ?? 1) + 1,
        stepIndex: 0,
        status: "running",
        error: undefined,
      });
  },

  /** Le centre a été ouvert : la pastille de non-lu retombe à zéro. */
  markAllRead() {
    if (!tasks.some((t) => !t.read)) return;
    tasks = tasks.map((t) => (t.read ? t : { ...t, read: true }));
    emit();
  },
  remove(id: string) {
    tasks = tasks.filter((t) => t.id !== id);
    emit();
  },
  /** Vide la pile de tout ce qui est terminé — le « Tout effacer » du centre
   *  de notifications comme celui de la galerie. */
  clearGallery() {
    tasks = tasks.filter((t) => t.status === "running");
    emit();
    // Persist empty list so restored tasks don't come back on reload.
    persistTasks();
  },
  snapshot(): AppTask[] {
    return tasks;
  },
  subscribe(l: () => void): () => void {
    listeners.add(l);
    return () => listeners.delete(l);
  },
};

/** React hook: re-renders on any task change. */
export function useTasks(): AppTask[] {
  return useSyncExternalStore(taskCenter.subscribe, taskCenter.snapshot, taskCenter.snapshot);
}

// Dev hook: drive the notification center from the browser console for testing
// (e.g. window.__taskCenter.addWorkflow(...)). Stripped from production builds.
if ((import.meta as unknown as { env?: { DEV?: boolean } }).env?.DEV) {
  (globalThis as unknown as { __taskCenter?: typeof taskCenter }).__taskCenter = taskCenter;
}
