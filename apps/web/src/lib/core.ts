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

/** Une conversation du serveur — la même que sur le téléphone et le bureau. */
export interface Conversation {
  id: string;
  title: string;
  last_message_at: string | null;
}

/** Un projet ouvert sur le serveur — le même que sur le téléphone. */
export interface PhoneProject {
  id: string;
  name: string;
}

/** Une extension installée sur le serveur, vue du navigateur. */
export interface PhoneExtension {
  name: string;
  display_name: string;
  version: string;
  description: string | null;
  enabled: boolean;
  capabilities: string[];
}

/** Une chose que le serveur retient de son utilisateur. */
export interface MemoryEntry {
  id: string;
  /** `preference`, `habitude`, `projet` ou `fait`. */
  category: string;
  content: string;
  source?: string;
}

/** Un modèle de génération, et s'il lui manque des fichiers pour tourner. */
export interface MediaModel {
  name: string;
  ready: boolean;
  missing: string[];
}

/** Une figure du serveur, vue du navigateur. */
export interface PhoneFigure {
  id: string;
  name: string;
  description: string;
  instructions: string;
  model: string | null;
  opening: string | null;
  uses_memory: boolean;
  tools: string[] | null;
}

/** Une conversation d'une figure, telle que l'écran la liste. */
export interface PhoneFigureSession {
  id: string;
  title: string | null;
  last_message_at: string | null;
}

/** Ce qu'on envoie pour créer ou corriger une figure. */
export interface FigureDraft {
  name: string;
  description: string;
  instructions: string;
  model: string | null;
  opening: string | null;
  usesMemory: boolean;
  /** Les outils autorisés, séparés par des virgules. Vide : tout. */
  tools: string;
}

/**
 * Le catalogue officiel — écrit ici plutôt que demandé au serveur : ce sont
 * les dépôts publiés par le projet, comme sur le téléphone.
 */
export const CATALOGUE: { repo: string; label: string; note: string }[] = [
  { repo: "Locaryn/plugin-image-gen", label: "Génération d'images", note: "Studio, images" },
  { repo: "Locaryn/plugin-voice-tts", label: "Voix de synthèse", note: "Studio, voix" },
  { repo: "Locaryn/plugin-image-editor", label: "Retouche d'image", note: "Modifier une zone" },
  { repo: "Locaryn/plugin-vision-ocr", label: "Vision et OCR", note: "Lire une image" },
  { repo: "Locaryn/plugin-translation", label: "Traduction", note: "Traduire un texte" },
  { repo: "Locaryn/plugin-text-analysis", label: "Analyse de texte", note: "Résumés, extraction" },
  { repo: "Locaryn/plugin-rag-qa", label: "Questions sur documents", note: "Vos fichiers" },
  { repo: "Locaryn/plugin-music-gen", label: "Musique", note: "Studio, audio" },
  { repo: "Locaryn/plugin-video-gen", label: "Vidéo", note: "Studio, vidéo" },
  { repo: "Locaryn/plugin-3d-gen", label: "Objets 3D", note: "Studio, 3D" },
  {
    repo: "Locaryn/plugin-model-training",
    label: "Entraînement (LoRA)",
    note: "Affiner un modèle",
  },
  { repo: "Locaryn/plugin-ssh", label: "Machine distante (SSH)", note: "Exécuter ailleurs" },
  { repo: "Locaryn/plugin-travel-tunnel", label: "Mode voyage", note: "Joindre depuis dehors" },
];

/** Les lignes telles que le daemon les renvoie. */
interface SessionRow {
  id: string;
  title: string | null;
  last_message_at: string | null;
  archived_at: string | null;
}
interface MessageRow {
  id: string;
  role: string;
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

/**
 * La plus récente en tête — le même ordre que sur le téléphone, où le Rust
 * trie exactement ainsi. Celles sans date de dernier message vont en fin de
 * liste, quelle que soit leur ancienneté.
 */
function sortByRecent(a: Conversation, b: Conversation): number {
  // Vide d'abord (sans date), puis décroissant — les estampilles ISO 8601 du
  // serveur se comparent lexicographiquement.
  const ta = a.last_message_at ?? "";
  const tb = b.last_message_at ?? "";
  if (ta === tb) return 0;
  if (ta === "") return 1;
  if (tb === "") return -1;
  return ta < tb ? 1 : -1;
}

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
  /**
   * Ce que les extensions actives du serveur apportent.
   *
   * Décide de la présence du Studio : installer la génération d'images sur le
   * serveur la fait apparaître ici, la retirer la fait disparaître — sur le
   * téléphone comme sur le bureau, puisque tous lisent la même liste.
   */
  async serverCapabilities(): Promise<string[]> {
    try {
      const exts = await http<{ enabled: boolean; capabilities?: string[] }[]>("/v1/extensions");
      const caps = new Set<string>();
      for (const e of exts) {
        if (!e.enabled) continue;
        for (const c of e.capabilities ?? []) caps.add(c);
      }
      return [...caps];
    } catch {
      return [];
    }
  },

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

  /** Qui je suis, selon le serveur. */
  async me(): Promise<{ username: string; role: string; local?: boolean }> {
    return http("/v1/auth/me");
  },

  /** Changer mon mot de passe. L'actuel doit être fourni. */
  async changePassword(current: string, nouveau: string): Promise<void> {
    await http("/v1/auth/password", { method: "POST", body: { current, nouveau } });
  },

  /** Les extensions installées sur le serveur, et leur pilotage. */
  async listExtensions(): Promise<PhoneExtension[]> {
    return http("/v1/extensions");
  },

  async installExtension(source: string): Promise<PhoneExtension> {
    return http("/v1/extensions/install", { method: "POST", body: { source } });
  },

  async setExtensionEnabled(name: string, enabled: boolean): Promise<void> {
    await http(`/v1/extensions/${encodeURIComponent(name)}/${enabled ? "enable" : "disable"}`, {
      method: "POST",
    });
  },

  async removeExtension(name: string): Promise<void> {
    await http(`/v1/extensions/${encodeURIComponent(name)}`, { method: "DELETE" });
  },

  /** Ce que le serveur retient de son utilisateur. */
  async listMemory(): Promise<MemoryEntry[]> {
    return http("/v1/memory");
  },

  async remember(category: string, content: string): Promise<void> {
    await http("/v1/memory", { method: "POST", body: { category, content } });
  },

  async forget(id: string): Promise<void> {
    await http(`/v1/memory/${encodeURIComponent(id)}`, { method: "DELETE" });
  },

  /** Les figures du serveur, et leur pilotage. */
  async listFigures(): Promise<PhoneFigure[]> {
    return http("/v1/figures");
  },

  async saveFigure(f: FigureDraft): Promise<PhoneFigure> {
    return http("/v1/figures", {
      method: "POST",
      body: {
        name: f.name,
        description: f.description,
        instructions: f.instructions,
        model: f.model,
        opening: f.opening,
        uses_memory: f.usesMemory,
        tools: f.tools
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      },
    });
  },

  async deleteFigure(id: string): Promise<void> {
    await http(`/v1/figures/${encodeURIComponent(id)}`, { method: "DELETE" });
  },

  /** Une conversation d'une figure, pour la reprendre. */
  async figureSessions(figureId: string): Promise<PhoneFigureSession[]> {
    return http(`/v1/figures/${encodeURIComponent(figureId)}/sessions`);
  },

  /** Ouvre une conversation neuve tenue par la figure — la même séquence que
   *  sur le téléphone : une session, puis la figure attachée. */
  async startFigureChat(figureId: string): Promise<string> {
    const projectId = await ensureFreeChatProject();
    const session = await http<{ id: string }>(`/v1/projects/${projectId}/sessions`, {
      method: "POST",
      body: { ephemeral: false },
    });
    await http(`/v1/sessions/${session.id}/figure`, {
      method: "POST",
      body: { figure_id: figureId },
    });
    return session.id;
  },

  /**
   * Un modèle de génération par type — `image` ou `audio`.
   *
   * Le serveur détaille les modèles d'image (`details` : prêt ou non, ce qui
   * manque) mais un serveur plus ancien ne renvoie que des noms (`models`) —
   * on les considère alors utilisables, comme le fait le téléphone.
   */
  async listMediaModels(kind: "image" | "audio"): Promise<MediaModel[]> {
    const r = await http<{ details?: MediaModel[]; models?: string[] }>(
      `/v1/media/models?kind=${encodeURIComponent(kind)}`,
    );
    if (Array.isArray(r.details)) return r.details;
    return (r.models ?? []).map((name) => ({ name, ready: true, missing: [] }));
  },

  /**
   * Les conversations libres du serveur — les mêmes que sur le téléphone :
   * reprendre une conversation ici la retrouve sur l'ordinateur et inversement.
   */
  async listConversations(): Promise<Conversation[]> {
    const projects = await http<Array<{ id: string; path?: string }>>("/v1/projects");
    const free = projects.find((p) => p.path === FREE_CHAT_PROJECT_PATH);
    if (!free) return [];
    const sessions = await http<SessionRow[]>(`/v1/projects/${free.id}/sessions`);
    return sessions
      .filter((s) => !s.archived_at)
      .map((s) => ({
        id: s.id,
        title: s.title ?? "Conversation",
        last_message_at: s.last_message_at ?? null,
      }))
      .sort(sortByRecent);
  },

  /** Relire une conversation depuis le serveur, messages dans l'ordre. */
  async loadConversation(id: string): Promise<Message[]> {
    const rows = await http<MessageRow[]>(`/v1/sessions/${id}/messages`);
    return rows
      .filter((m) => m.role === "user" || m.role === "assistant")
      .map((m) => ({ id: m.id, role: m.role as "user" | "assistant", content: m.content }));
  },

  /**
   * Les projets du serveur, hors « Conversations libres » (qui vit déjà dans
   * la liste du dessus) — les mêmes que sur le téléphone.
   */
  async listProjects(): Promise<PhoneProject[]> {
    const projects =
      await http<Array<{ id: string; path?: string; name?: string }>>("/v1/projects");
    return projects
      .filter((p) => p.path !== FREE_CHAT_PROJECT_PATH)
      .map((p) => ({ id: p.id, name: p.name ?? "Projet" }));
  },

  /** Les conversations d'un projet, pour son dépliage dans l'historique. */
  async listProjectConversations(projectId: string): Promise<Conversation[]> {
    const sessions = await http<SessionRow[]>(`/v1/projects/${projectId}/sessions`);
    return sessions
      .filter((s) => !s.archived_at)
      .map((s) => ({
        id: s.id,
        title: s.title ?? "Conversation",
        last_message_at: s.last_message_at ?? null,
      }))
      .sort(sortByRecent);
  },

  /** Une conversation neuve, prête à recevoir son premier message. */
  async newConversation(): Promise<{ id: string }> {
    const projectId = await ensureFreeChatProject();
    return http<{ id: string }>(`/v1/projects/${projectId}/sessions`, {
      method: "POST",
    });
  },

  /** One reply, gathered from the token events of the SSE stream. */
  async send(text: string, sessionId: string): Promise<string> {
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    const token = getToken();
    if (token) headers.Authorization = `Bearer ${token}`;

    const resp = await fetch(`/v1/sessions/${sessionId}/messages`, {
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
