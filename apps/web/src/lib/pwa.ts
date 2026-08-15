/**
 * Installation PWA.
 *
 * Android offers `beforeinstallprompt`; iOS offers nothing, so the page shows
 * its own instructions (Share → Add to Home Screen). Either way the visitor's
 * answer — installed, or "not now" — is remembered in localStorage, so the
 * question is never asked twice.
 */

const STATE_KEY = "locaryn.pwa";

interface PwaState {
  dismissed: boolean;
  installed: boolean;
  at: number;
}

function loadState(): PwaState {
  try {
    const raw = localStorage.getItem(STATE_KEY);
    if (!raw) return { dismissed: false, installed: false, at: 0 };
    return { dismissed: false, installed: false, at: 0, ...(JSON.parse(raw) as Partial<PwaState>) };
  } catch {
    return { dismissed: false, installed: false, at: 0 };
  }
}

function saveState(state: PwaState): void {
  localStorage.setItem(STATE_KEY, JSON.stringify(state));
}

/** True when the page runs inside an installed app (standalone window). */
export function isStandalone(): boolean {
  return (
    window.matchMedia("(display-mode: standalone)").matches ||
    // Legacy iOS.
    (navigator as Navigator & { standalone?: boolean }).standalone === true
  );
}

/** A phone or tablet running iOS (including iPadOS, which fakes macOS). */
export function isIOS(): boolean {
  const ua = navigator.userAgent;
  return (
    /iphone|ipad|ipod/i.test(ua) ||
    (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1)
  );
}

/**
 * Whether to surface the install prompt. Never twice, never once installed:
 * a standalone window is the strongest possible "installed" signal.
 */
export function shouldPrompt(): boolean {
  if (isStandalone()) return false;
  const state = loadState();
  return !state.dismissed && !state.installed;
}

export function rememberDismissed(): void {
  saveState({ ...loadState(), dismissed: true, at: Date.now() });
}

export function rememberInstalled(): void {
  saveState({ ...loadState(), installed: true, at: Date.now() });
}
