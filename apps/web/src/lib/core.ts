/**
 * The web client talks to the daemon directly — same origin, since the daemon
 * serves this very page. Everything here mirrors what the phone does from
 * Rust, but with fetch: the pairing decision still lives server-side, the
 * token is the only credential the browser keeps.
 */

const SESSION_KEY = "locaryn.session";

interface Session {
  token: string;
  username: string;
  role: string;
}

export interface WebStatus {
  server_name: string | null;
  username: string | null;
  role: string | null;
  travelling: boolean;
  signed_in: boolean;
  servers: number;
}

export interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
}

/** A file generated on the machine at the other end, ready to show. */
export interface MediaResult {
  name: string;
  mime: string;
  /** Base64 payload; the page has no access to the server's disk. */
  data_base64: string;
}

const FREE_CHAT_PROJECT_PATH = "__locaryn_free_chats__";

function loadSession(): Session | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<Session>;
    if (!parsed.token) return null;
    return { token: parsed.token, username: parsed.username ?? "", role: parsed.role ?? "" };
  } catch {
    return null;
  }
}

function saveSession(session: Session): void {
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
}

export function getToken(): string | null {
  return loadSession()?.token ?? null;
}

export function clearSession(): void {
  localStorage.removeItem(SESSION_KEY);
}

async function http<T>(
  path: string,
  init?: Omit<RequestInit, "body"> & { body?: unknown },
): Promise<T> {
  const headers: Record<string, string> = { Accept: "application/json" };
  const token = getToken();
  if (token) headers.Authorization = `Bearer ${token}`;

  let body: BodyInit | undefined;
  if (init?.body !== undefined) {
    headers["Content-Type"] = "application/json";
    body = JSON.stringify(init.body);
  }

  const resp = await fetch(path, { ...init, headers, body });
  if (resp.status === 401) {
    // The token is gone or expired: drop it so the next status() shows the
    // sign-in screen instead of looping on a dead session.
    clearSession();
    throw new Error("Votre session a expiré. Reconnectez-vous.");
  }
  if (!resp.ok) {
    const text = await resp.text();
    let message = `Le serveur a répondu ${resp.status}.`;
    try {
      const parsed = JSON.parse(text) as { error?: string };
      if (parsed.error) message = parsed.error;
    } catch {
      if (text) message = text;
    }
    throw new Error(message);
  }
  if (resp.status === 204) return undefined as T;
  return (await resp.json()) as T;
}

/** The machine's name as shown in the top bar. */
const SERVER_NAME = "Locaryn";

export const api = {
  /** Le service exige-t-il un compte ? Faux quand il n'écoute que la machine. */
  async authRequired(): Promise<boolean> {
    try {
      const info = await http<{ auth_required?: boolean }>("/v1/info");
      return info.auth_required !== false;
    } catch {
      // Injoignable : on garde l'écran de connexion plutôt que d'ouvrir une
      // interface qui ne pourra rien afficher.
      return true;
    }
  },

  /** Local view of the session: the token is validated lazily, the moment an
   * API call answers 401 (which clears it). No round-trip on every load. */
  status(): WebStatus {
    const session = loadSession();
    return session
      ? {
          server_name: SERVER_NAME,
          username: session.username || null,
          role: session.role || null,
          travelling: false,
          signed_in: true,
          servers: 1,
        }
      : {
          server_name: SERVER_NAME,
          username: null,
          role: null,
          travelling: false,
          signed_in: false,
          servers: 0,
        };
  },

  async signIn(username: string, password: string): Promise<WebStatus> {
    const resp = await http<{
      token: string;
      user?: { username?: string; role?: string };
    }>("/v1/auth/login", {
      method: "POST",
      body: { username, password, label: "navigateur" },
    });
    saveSession({
      token: resp.token,
      username: resp.user?.username ?? username,
      role: resp.user?.role ?? "member",
    });
    return api.status();
  },

  async signOut(): Promise<WebStatus> {
    clearSession();
    return api.status();
  },

  /** One reply, gathered from the token events of the SSE stream. */
  async send(text: string): Promise<string> {
    const projectId = await ensureFreeChatProject();
    const session = await http<{ id: string }>(`/v1/projects/${projectId}/sessions`, {
      method: "POST",
    });

    const headers: Record<string, string> = { "Content-Type": "application/json" };
    const token = getToken();
    if (token) headers.Authorization = `Bearer ${token}`;

    const resp = await fetch(`/v1/sessions/${session.id}/messages`, {
      method: "POST",
      headers,
      body: JSON.stringify({ content: text }),
    });
    if (resp.status === 401) {
      clearSession();
      throw new Error("Votre session a expiré. Reconnectez-vous.");
    }
    if (!resp.ok) throw new Error(`Le serveur a refusé la demande (${resp.status}).`);
    if (!resp.body) throw new Error("Le serveur n'a renvoyé aucun flux.");

    // The daemon answers with server-sent events; keep the `token` events,
    // which carry the assistant's words as they are produced.
    const reader = resp.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let reply = "";
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      for (;;) {
        const sep = buffer.indexOf("\n\n");
        if (sep === -1) break;
        const block = buffer.slice(0, sep);
        buffer = buffer.slice(sep + 2);
        for (const line of block.split("\n")) {
          if (!line.startsWith("data:")) continue;
          const data = line.slice(5).trim();
          if (!data) continue;
          try {
            const ev = JSON.parse(data) as { type?: string; text?: string };
            if (ev.type === "token" && ev.text) reply += ev.text;
          } catch {
            // A malformed event is not worth failing the whole reply for.
          }
        }
      }
    }
    return reply;
  },

  async listMediaModels(kind: "image" | "audio"): Promise<string[]> {
    const r = await http<{ kind: string; models: string[] }>(
      `/v1/media/models?kind=${encodeURIComponent(kind)}`,
    );
    return r.models;
  },

  generateImage: (args: {
    model: string;
    prompt: string;
    negativePrompt?: string;
    width?: number;
    height?: number;
  }) =>
    http<MediaResult>("/v1/media/image", {
      method: "POST",
      body: {
        model: args.model,
        prompt: args.prompt,
        negative_prompt: args.negativePrompt,
        width: args.width,
        height: args.height,
      },
    }),

  generateAudio: (args: { model: string; text: string; speed?: number; language?: string }) =>
    http<MediaResult>("/v1/media/audio", {
      method: "POST",
      body: {
        model: args.model,
        text: args.text,
        speed: args.speed,
        language: args.language,
      },
    }),
};

async function ensureFreeChatProject(): Promise<string> {
  const projects = await http<Array<{ id: string; path?: string }>>("/v1/projects");
  const existing = projects.find((p) => p.path === FREE_CHAT_PROJECT_PATH);
  if (existing) return existing.id;
  const created = await http<{ id: string }>("/v1/projects", {
    method: "POST",
    body: {
      path: FREE_CHAT_PROJECT_PATH,
      name: "Conversations libres",
      trust_level: "sandbox",
    },
  });
  return created.id;
}
