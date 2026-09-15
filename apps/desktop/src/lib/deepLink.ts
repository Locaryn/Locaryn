/**
 * Deep-link intents (`locaryn://install?src=…`, `locaryn://connect?…`).
 *
 * A link can open the app from a cold start (the URL arrives as a CLI
 * argument, read through the deep-link plugin's `get_current`) or while it is
 * already running (forwarded by Rust as an event). Either way the intent must
 * survive until its receiver is mounted — the panel or the modal may not exist
 * yet when the URL lands. A tiny store rather than context, so any receiver
 * can subscribe without the app shell having to know its state.
 */

export interface InstallIntent {
  /** Source à pré-remplir dans la fenêtre d'installation (owner/repo, URL, chemin…). */
  source: string;
}

/** Demande de connexion portée par `locaryn://connect?…` — le lien que le
 *  .exe du serveur et le QR du téléphone présentent à l'utilisateur. */
export interface ConnectIntent {
  /** URL du serveur (`https://192.168.1.10:7474`). */
  server: string;
  /** Identifiant pré-rempli, optionnel : la connexion reste à confirmer. */
  user?: string;
  /** Mot de passe pré-enregistré par l'utilisateur lui-même dans le .exe —
   *  jamais généré par un tiers. Absent : il sera demandé. */
  password?: string;
  /** URL HTTPS du paquet certificat+clé à installer avant/avec la connexion. */
  cert?: string;
  /** URL HTTPS du certificat d'autorité (serveur sans autorité publique). */
  ca?: string;
}

/** Union des intents reconnus, taguée pour le dispatch. */
export type DeepLinkIntent =
  | ({ action: "install" } & InstallIntent)
  | ({ action: "connect" } & ConnectIntent);

let pending: DeepLinkIntent | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export function subscribeDeepLink(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function getPendingInstall(): DeepLinkIntent | null {
  return pending;
}

/** Compat : l'ancien nom reste exact pour un intent `install`. */
export function setPendingInstall(intent: DeepLinkIntent | null) {
  pending = intent;
  emit();
}

/** Récupère l'intention en attente et l'efface (son récepteur l'a consommée). */
export function consumePendingInstall(): DeepLinkIntent | null {
  const i = pending;
  pending = null;
  emit();
  return i;
}

/** Transforme une URL `locaryn://…` en intention typée, sinon null.
 *  `install` garde sa forme historique ; `connect` porte les coordonnées du
 *  serveur et, s'il y en a, les URLs de certificats à installer. Le mot de
 *  passe n'arrive que d'un lien que l'utilisateur lui-même a exporté : la
 *  modale le dit, et il n'est jamais replacé dans un champ visible. */
export function parseDeepLink(url: string): DeepLinkIntent | null {
  try {
    const u = new URL(url);
    if (u.protocol !== "locaryn:") return null;
    const action = u.hostname || u.pathname.replace(/^\//, "");
    if (action === "install") {
      const src = u.searchParams.get("src");
      return src ? { action: "install", source: src } : null;
    }
    if (action === "connect") {
      const server = u.searchParams.get("server")?.trim();
      if (!server || !/^https?:\/\//i.test(server)) return null;
      return {
        action: "connect",
        server,
        user: u.searchParams.get("user")?.trim() || undefined,
        password: u.searchParams.get("password") || undefined,
        cert: u.searchParams.get("cert")?.trim() || undefined,
        ca: u.searchParams.get("ca")?.trim() || undefined,
      };
    }
    return null;
  } catch {
    return null;
  }
}

/** Forme historique : ne reconnaît que `install`. */
export function parseInstallLink(url: string): InstallIntent | null {
  const intent = parseDeepLink(url);
  return intent?.action === "install" ? { source: intent.source } : null;
}
