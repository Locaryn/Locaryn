/**
 * What the right-hand panel is currently showing.
 *
 * Running a snippet from an answer used to dump its output back into the
 * conversation, which buries the discussion under logs and makes a long build
 * unreadable. Output belongs beside the chat, not inside it: a terminal view
 * for anything executed, a live page for anything rendered.
 *
 * A tiny store rather than context, so the chat can push a run without the
 * panel having to be mounted first.
 */

export interface TerminalRun {
  kind: "terminal";
  lang: string;
  /** What was actually executed, shown as the prompt line. */
  command: string;
  cwd: string;
  lines: string[];
  running: boolean;
  exitCode?: number;
}

export interface WebRun {
  kind: "web";
  title: string;
  /** Rendered in a sandboxed iframe — never same-origin. */
  html: string;
}

export type RunView = TerminalRun | WebRun;

let current: RunView | null = null;
const listeners = new Set<() => void>();
/** Set by App so a run can reveal the panel that displays it. */
let reveal: (() => void) | null = null;

function emit() {
  listeners.forEach((l) => l());
}

export function subscribeRun(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function getRun(): RunView | null {
  return current;
}

/** Let the panel owner expose "open me" without importing App state. */
export function setRunReveal(fn: (() => void) | null) {
  reveal = fn;
}

export function clearRun() {
  current = null;
  emit();
}

/** Start a terminal run and reveal the panel. */
export function startTerminalRun(lang: string, command: string, cwd: string) {
  current = { kind: "terminal", lang, command, cwd, lines: [], running: true };
  reveal?.();
  emit();
}

export function appendRunLine(text: string) {
  if (current?.kind !== "terminal") return;
  // Bound the buffer: a runaway loop must not grow the page until it stalls.
  const lines = [...current.lines, text];
  current = { ...current, lines: lines.length > 2000 ? lines.slice(-2000) : lines };
  emit();
}

export function finishTerminalRun(exitCode?: number) {
  if (current?.kind !== "terminal") return;
  current = { ...current, running: false, exitCode };
  emit();
}

/** Show a page live. Used for HTML and SVG blocks from an answer. */
export function showWebRun(html: string, title = "Aperçu") {
  current = { kind: "web", title, html };
  reveal?.();
  emit();
}
