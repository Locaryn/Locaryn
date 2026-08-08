/**
 * Deep-link intents (`lochor://install?src=…`).
 *
 * A link can open the app from a cold start (the URL arrives as a CLI
 * argument, read through the deep-link plugin's `get_current`) or while it is
 * already running (forwarded by Rust as an event). Either way the intent must
 * survive until the extensions panel is mounted — the panel may not exist yet
 * when the URL lands. A tiny store rather than context, so the receiver can
 * subscribe without the app shell having to know the panel's state.
 */

export interface InstallIntent {
  /** Source à pré-remplir dans la fenêtre d'installation (owner/repo, URL, chemin…). */
  source: string;
}

let pending: InstallIntent | null = null;
const listeners = new Set<() => void>();

function emit() {
  listeners.forEach((l) => l());
}

export function subscribeDeepLink(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function getPendingInstall(): InstallIntent | null {
  return pending;
}

export function setPendingInstall(intent: InstallIntent | null) {
  pending = intent;
  emit();
}

/** Récupère l'intention en attente et l'efface (le panneau l'a consommée). */
export function consumePendingInstall(): InstallIntent | null {
  const i = pending;
  pending = null;
  emit();
  return i;
}

/** Transforme une URL `lochor://install?src=…` en intention, sinon null. */
export function parseInstallLink(url: string): InstallIntent | null {
  try {
    const u = new URL(url);
    if (u.protocol !== "lochor:") return null;
    const action = u.hostname || u.pathname.replace(/^\//, "");
    if (action !== "install") return null;
    const src = u.searchParams.get("src");
    if (!src) return null;
    return { source: src };
  } catch {
    return null;
  }
}
