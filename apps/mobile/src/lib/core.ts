import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import { signalerErreur } from "./reachability";

export interface MobileStatus {
  server_name: string | null;
  travelling: boolean;
  signed_in: boolean;
  servers: number;
}

export interface DiscoveredServer {
  name: string;
  url: string;
  ip: string;
  port: number;
  version?: string | null;
}

/** Une extension installée sur le serveur, vue du téléphone. */
/** Une capacité reconnue par le serveur : id, label français, description. */
export interface Capability {
  id: string;
  label: string;
  description: string;
}

export type RiskLevel = "low" | "medium" | "high" | "critical";
export type RiskScope = "once" | "session" | "always";

export interface ToolApprovalRequest {
  call_id: string;
  tool: string;
  args?: Record<string, unknown>;
  risk: RiskLevel;
  reason?: string;
  diff?: string;
  is_remote?: boolean;
}

export interface ToolApprovalDecision {
  call_id: string;
  allow: boolean;
  scope: RiskScope;
  audit_note?: string;
}

export interface UserProfile {
  id?: string | null;
  username: string;
  role: "admin" | "member" | string;
  server_url: string;
}

export interface PhoneUserSummary {
  id: string;
  username: string;
  role: string;
  disabled: boolean;
}

export interface PhoneExtension {
  name: string;
  display_name: string;
  version: string;
  description: string | null;
  enabled: boolean;
  capabilities: string[];
  kind?: "extension" | "plugin" | "core" | string;
  ecosystem?: "locaryn" | "gemini_cli" | "opencode" | string;
  components?: {
    commands?: number;
    skills?: number;
    agents?: number;
    mcp_servers?: number;
    rules?: number;
    hooks?: number;
    lsp_adapters?: number;
  };
  permissions?: string[];
  /** Ce que l'extension ajoute à l'interface. Vide quand elle est éteinte. */
  ui?: ExtensionUi;
}

/** Une entrée de menu ou un onglet que l'extension pose dans l'interface. */
export interface ExtensionUiEntry {
  id: string;
  label: string;
  icon: string | null;
}

export interface ExtensionUiSlotContribution {
  id: string;
  slot: string;
  order?: number;
  type?: "button" | "widget" | "action" | "custom-element" | "script";
  label?: string;
  icon?: string | null;
  hint?: string | null;
  action?: "insert" | "tool" | "navigate" | "event" | string;
  value?: string;
  tag?: string;
  entry?: string;
  /** Surfaces où cette contribution existe : "desktop", "mobile", "web".
   *  Absente ou vide : partout. Une extension peut donc réserver son grand
   *  panneau à l'ordinateur et donner au téléphone une forme à lui. */
  platforms?: string[];
}

/** Ce qu'une extension ajoute à l'interface du téléphone. */
export interface ExtensionUi {
  slots?: ExtensionUiSlotContribution[];
  nav_items?: ExtensionUiEntry[];
  studio_tabs?: ExtensionUiEntry[];
  composer_actions?: ComposerAction[];
  settings_sections?: SettingsSection[];
}

/**
 * Un bouton posé à côté du champ de saisie.
 *
 * `insert` écrit `value` dans le champ — un modèle de demande, une consigne
 * qui revient. `tool` appelle l'outil nommé par `value` avec ce que le champ
 * contient. Rien d'autre : faire tourner du code d'extension dans l'interface
 * reviendrait à lui donner l'écran entier.
 */
/** Une figure du serveur, vue du téléphone. */
export interface PhoneFigure {
  id: string;
  name: string;
  description: string;
  /** Ce que le modèle reçoit avant toute conversation. C'est le cœur. */
  instructions: string;
  /** Le modèle qui la fait tourner. Absent : celui de l'application. */
  model: string | null;
  /** Une première phrase, proposée à l'ouverture. */
  opening: string | null;
  /** Vrai quand la figure lit la mémoire de l'utilisateur. */
  uses_memory: boolean;
  /** Les outils qu'elle a le droit d'appeler. Absents : tout ce que l'application propose. */
  tools: string[] | null;
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

/** Une conversation d'une figure, telle que l'écran la liste. */
export interface PhoneFigureSession {
  id: string;
  title: string | null;
  last_message_at: string | null;
}

export interface ComposerAction {
  id: string;
  label: string;
  icon?: string | null;
  action: "insert" | "tool";
  value: string;
  hint?: string | null;
}

/** Une section de réglages apportée par une extension. */
export interface SettingsSection {
  id: string;
  title: string;
  description?: string | null;
  fields: SettingsField[];
}

export interface SettingsField {
  key: string;
  label: string;
  /** `boolean`, `select`, `model`, `string`, `number` ou `prompt`.
   *  Les anciens mots `toggle`, `choice`, `text` sont acceptés. */
  kind: string;
  hint?: string | null;
  options?: string[];
  default?: string | null;
}

/**
 * Le catalogue officiel.
 *
 * Écrit ici plutôt que demandé au serveur : ce sont les dépôts publiés par le
 * projet, ils changent au rythme des versions de l'application, et une liste
 * en dur ne peut pas tomber en panne de réseau au pire moment.
 */
export const CATALOGUE: { repo: string; label: string; note: string }[] = [
  { repo: "Locaryn/morph-image", label: "Images", note: "Générer et retoucher" },
  { repo: "Locaryn/morph-voice-tts", label: "Voix de synthèse", note: "Studio, voix" },
  { repo: "Locaryn/morph-vision-ocr", label: "Vision et OCR", note: "Lire une image" },
  { repo: "Locaryn/morph-translation", label: "Traduction", note: "Traduire un texte" },
  { repo: "Locaryn/morph-text-analysis", label: "Analyse de texte", note: "Résumés, extraction" },
  { repo: "Locaryn/morph-rag-qa", label: "Questions sur documents", note: "Vos fichiers" },
  { repo: "Locaryn/morph-music-gen", label: "Musique", note: "Studio, audio" },
  { repo: "Locaryn/morph-video-gen", label: "Vidéo", note: "Studio, vidéo" },
  { repo: "Locaryn/morph-3d-gen", label: "Objets 3D", note: "Studio, 3D" },
  {
    repo: "Locaryn/morph-model-training",
    label: "Entraînement (LoRA)",
    note: "Affiner un modèle",
  },
  { repo: "Locaryn/morph-ssh", label: "Machine distante (SSH)", note: "Exécuter ailleurs" },
  { repo: "Locaryn/morph-travel-tunnel", label: "Mode voyage", note: "Joindre depuis dehors" },
];

/** Les quatre groupes fixes de l'écran de mémoire. */
export type MemoryGroup = "vous" | "sujets" | "zones" | "personnes";

/** Une fiche que le serveur retient de son utilisateur — un sujet, pas une
 *  phrase. */
export interface MemoryEntry {
  id: string;
  group: MemoryGroup;
  title: string;
  /** Une ligne, montrée sans ouvrir la fiche. */
  summary: string;
  /** S'accumule au fil des conversations. */
  details: string[];
  source?: string;
  /** Quand le souvenir a été appris. Le service l'envoie toujours ; laissé
   *  optionnel pour les enregistrements d'avant son ajout. */
  created_at?: string;
}

/** Un modèle de génération, et s'il lui manque des fichiers pour tourner. */
export interface MediaModel {
  name: string;
  ready: boolean;
  missing: string[];
}

export interface PairingResult {
  server_name: string;
  travelling: boolean;
  message: string;
}

export interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
  /** Ce qu'un outil a produit pendant ce tour : images générées, s'il y en a. */
  images?: MediaResult[];
}

/** Un tour de conversation : les mots, et ce que les outils ont fabriqué. */
export interface ChatReply {
  text: string;
  images: MediaResult[];
  /** La conversation où ce tour a eu lieu, à garder pour la suivante. */
  conversation_id: string;
  approval?: ToolApprovalRequest | null;
}

/** Une conversation du serveur, partagée avec l'ordinateur. */
export interface Conversation {
  id: string;
  title: string;
  last_message_at: string | null;
}

/** Un projet ouvert sur le serveur. */
export interface PhoneProject {
  id: string;
  name: string;
}

/** Une conversation rangée aux archives, avec le projet d'où elle vient. */
export interface ArchivedConversation {
  id: string;
  title: string;
  archived_at: string | null;
  project: string;
}

/** Un tour déjà écrit, relu depuis le serveur. */
export interface ChatTurn {
  id: string;
  role: string;
  content: string;
  /** Generic image artifacts returned by an enabled MCP extension. */
  images?: MediaResult[];
}

/** Ce que le téléphone sait de sa propre mise à jour. */
export interface UpdateStatus {
  current: string;
  latest: string | null;
  available: boolean;
  download_url: string | null;
  /** Ce que la version apporte, dit ici plutôt que sur une page web. */
  notes: string | null;
  size: number | null;
  /** Le paquet est déjà là : il n'y a plus qu'à relancer l'installation. */
  downloaded: boolean;
  error: string | null;
}

/** Un point d'avancement du téléchargement de la mise à jour. */
export interface ProgressionTelechargement {
  downloaded: number;
  /** Absent quand le manifeste n'a pas donné de taille : un octet compté ne
   *  dit rien sans un total à côté. */
  total: number | null;
  /** Absent dans le même cas — une barre indéterminée vaut mieux qu'un
   *  pourcentage inventé. */
  percentage: number | null;
}

/** Un point d'avancement du téléchargement d'un modèle, comme la mise à jour. */
export interface ModelPullProgress {
  downloaded: number;
  /** Absent quand le serveur n'a pas annoncé de taille : barre indéterminée. */
  total: number | null;
  percentage: number | null;
  /** Une phase en cours : « Installation des compagnons… ». */
  message: string | null;
}

/** A file generated on the machine at the other end, ready to show. */
export interface MediaResult {
  name: string;
  mime: string;
  /** Base64 payload; the webview has no access to the server's disk. */
  data_base64: string;
}

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Pour rythmer les simulations du mode démo — jamais utilisé hors de lui. */
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * Un seul endroit par lequel passe chaque commande, donc un seul endroit où
 * reconnaître un serveur devenu injoignable — plutôt que de le refaire dans
 * chaque écran qui appelle le serveur.
 */
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (e) {
    signalerErreur(String(e));
    throw e;
  }
}

/**
 * The phone's view of Locaryn.
 *
 * Everything heavy happens on the machine at the other end; what is here is a
 * conversation and the pairing that keeps it reachable.
 */
export const core = {
  status: () => invoke<MobileStatus>("status"),
  /** Ce que les extensions actives du serveur apportent. */
  serverCapabilities: () => invoke<string[]>("server_capabilities"),
  /** Y a-t-il une version plus récente publiée ? */
  checkUpdate: () => invoke<UpdateStatus>("check_update"),
  /**
   * Télécharge la nouvelle version et ouvre l'installateur d'Android.
   *
   * `onProgress` reçoit l'avancement au fil de l'eau — un canal, pas une
   * promesse : l'écran dessine la barre sans attendre la fin du téléchargement.
   */
  installUpdate: (
    url: string,
    size: number | null,
    onProgress: (p: ProgressionTelechargement) => void,
  ) => {
    const canal = new Channel<ProgressionTelechargement>();
    canal.onmessage = onProgress;
    return invoke<string>("install_update", { url, size, onProgress: canal });
  },
  /** Relance l'installation d'un paquet déjà téléchargé. */
  resumeInstall: (url: string) => invoke<void>("resume_install", { url }),
  registerServer: (provisioningJson: string) =>
    invoke<MobileStatus>("register_server", { provisioningJson }),
  /** Enregistre un serveur depuis son adresse, sans code à scanner. */
  registerAddress: (address: string) => invoke<MobileStatus>("register_address", { address }),
  /** Recherche les serveurs Locaryn sur le réseau local. */
  discoverServers: () => invoke<DiscoveredServer[]>("discover_servers"),
  /** Reprendre le serveur actif à une nouvelle adresse : même autorité, même
   *  session, même historique — seule l'adresse change. */
  reconnectActiveServer: (address: string) =>
    invoke<MobileStatus>("reconnect_active_server", { address }),
  signIn: (username: string, password: string) =>
    invoke<MobileStatus>("sign_in", { username, password }),
  signOut: () => invoke<MobileStatus>("sign_out"),
  /** Verify a scanned code and apply it. Throws with a phrased message. */
  applyPairingLink: (uri: string) => invoke<PairingResult>("apply_pairing_link", { uri }),
  send: (text: string, conversationId: string | null, ephemeral = false) =>
    invoke<ChatReply>("send_message", { text, conversationId, ephemeral }),
  /** Les conversations du serveur — les mêmes que sur l'ordinateur. */
  listConversations: () => invoke<Conversation[]>("list_conversations"),
  /** Les projets du serveur, et les conversations de l'un d'eux. */
  listProjects: () => invoke<PhoneProject[]>("list_projects"),
  listProjectConversations: (projectId: string) =>
    invoke<Conversation[]>("list_project_conversations", { projectId }),
  /** Ranger une conversation aux archives, ou l'en ressortir. */
  archiveConversation: (id: string, archived: boolean) =>
    invoke<void>("archive_session", { id, archived }),
  /** Déplacer une conversation dans un projet. */
  moveConversation: (id: string, projectId: string) =>
    invoke<void>("move_session", { id, projectId }),
  /** Créer un projet sur le serveur, depuis le téléphone. */
  createProject: (name: string) => invoke<PhoneProject>("create_project", { name }),
  /** Toutes les conversations rangées aux archives, quel que soit leur projet. */
  archivedConversations: () => invoke<ArchivedConversation[]>("list_archived"),
  loadConversation: (id: string) => invoke<ChatTurn[]>("load_conversation", { id }),
  /** Models the machine can generate with: kind = "image" | "audio". */
  listMediaModels: (kind: "image" | "audio") => invoke<MediaModel[]>("list_media_models", { kind }),
  /** Les extensions installées sur le serveur, et leur pilotage. */
  listExtensions: () => invoke<PhoneExtension[]>("list_extensions"),
  /** La liste canonique des capacités, telle que le serveur la connaît. */
  listCapabilities: () => invoke<Capability[]>("list_capabilities"),
  /** Les figures du serveur, et leur pilotage. */
  listFigures: () => invoke<PhoneFigure[]>("list_figures"),
  saveFigure: (f: FigureDraft) =>
    invoke<PhoneFigure>("save_figure", {
      name: f.name,
      description: f.description,
      instructions: f.instructions,
      model: f.model,
      opening: f.opening,
      usesMemory: f.usesMemory,
      tools: f.tools
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean),
    }),
  deleteFigure: (id: string) => invoke<void>("delete_figure", { id }),
  startFigureChat: (figureId: string) => invoke<string>("start_figure_chat", { figureId }),
  figureSessions: (figureId: string) =>
    invoke<PhoneFigureSession[]>("figure_sessions", { figureId }),

  installExtension: (source: string) => invoke<PhoneExtension>("install_extension", { source }),
  setExtensionEnabled: (name: string, enabled: boolean) =>
    invoke<void>("set_extension_enabled", { name, enabled }),
  /**
   * Installer un modèle du catalogue sur le serveur.
   *
   * `onProgress` reçoit l'avancement au fil de l'eau — un canal, pas une
   * promesse : la barre se dessine sans attendre la fin du téléchargement.
   */
  pullModel: (url: string, onProgress: (p: ModelPullProgress) => void) => {
    const canal = new Channel<ModelPullProgress>();
    canal.onmessage = onProgress;
    return invoke<{ name: string; size: number }>("pull_model", { url, onProgress: canal });
  },
  /** Retirer un modèle installé du serveur : ses fichiers sont effacés. */
  removeModel: (name: string) => invoke<void>("remove_model", { name }),
  removeExtension: (name: string) => invoke<void>("remove_extension", { name }),
  /** Appeler l'outil qu'un bouton d'extension désigne, avec le texte du champ. */
  runComposerTool: (tool: string, text: string) =>
    invoke<string>("run_composer_tool", { tool, text }),
  /** Lire le script d'interface d'une extension active. */
  readExtensionAsset: (extensionId: string, assetPath: string) =>
    invoke<string>("read_extension_asset", { extensionId, assetPath }),
  /** Appeler un outil MCP d'extension avec ses paramètres structurés. */
  invokeExtensionTool: (tool: string, args: Record<string, unknown>) =>
    invoke<string>("invoke_extension_tool", { tool, args }),
  /** Les réglages déclarés par les extensions, clés `extension.champ`. */
  extensionConfig: () => invoke<Record<string, string>>("extension_config"),
  setExtensionConfig: (extension: string, key: string, value: string) =>
    invoke<void>("set_extension_config", { extension, key, value }),
  /** Les modèles installés sur le serveur — pour un réglage de type `model`. */
  listModels: () => invoke<string[]>("list_models"),
  /** Ce que le serveur retient de son utilisateur. */
  listMemory: () => invoke<MemoryEntry[]>("list_memory"),
  remember: (group: MemoryGroup, title: string, detail: string) =>
    invoke<void>("remember", { group, title, detail }),
  removeMemoryDetail: (id: string, detail: string) =>
    invoke<void>("remove_memory_detail", { id, detail }),
  forget: (id: string) => invoke<void>("forget", { id }),
  /** Écrit une image sur l'appareil ; renvoie le chemin du fichier. */
  saveImage: (img: MediaResult) =>
    invoke<string>("save_image", { name: img.name, dataBase64: img.data_base64 }),
  generateAudio: (args: {
    model: string;
    text: string;
    speed?: number;
    language?: string;
  }) => invoke<MediaResult>("generate_audio", args),
  currentUser: () => invoke<UserProfile>("current_user"),
  changePassword: (current: string, nouveau: string) =>
    invoke<void>("change_password", { current, nouveau }),
  listServerUsers: () => invoke<PhoneUserSummary[]>("list_server_users"),
  createServerUser: (username: string, password: string, isAdmin = false) =>
    invoke<void>("create_server_user", { username, password, isAdmin }),
  deleteServerUser: (userId: string) => invoke<void>("delete_server_user", { userId }),
  approveToolCall: (decision: ToolApprovalDecision) =>
    invoke<void>("approve_tool_call", { decision }),
};

/** "tauri" on a phone, "demo" in a browser during development. */
export const coreMode: "tauri" | "demo" = isTauri ? "tauri" : "demo";

/**
 * Browser stand-in, so the layout can be worked on without a device.
 *
 * Deliberately not a second implementation of anything that matters: the
 * pairing decision lives in Rust and is tested there.
 */
export const demoCore: typeof core = {
  status: async () => ({
    server_name: "Atelier Vasseur",
    travelling: false,
    signed_in: true,
    servers: 1,
  }),
  // Sans serveur enregistré, le navigateur montre le Studio : c'est
  // l'interface qu'on met au point, pas une machine réelle.
  serverCapabilities: async () => ["image-gen"],
  // Il n'y a pas de vrai APK à remplacer dans un navigateur, mais une mise à
  // jour disponible et simulée : sans elle, tout l'écran — bouton, barre de
  // progression, tailles — resterait du code mort, invérifiable autrement
  // qu'avec un téléphone et une release plus récente déjà publiée.
  checkUpdate: async () => ({
    current: "0.0.0-dev",
    latest: "0.0.0-dev+1",
    available: true,
    download_url: "https://example.invalid/demo.apk",
    notes: "Aperçu de démonstration : rien n'est réellement téléchargé ni installé.",
    size: 42 * 1024 * 1024,
    downloaded: false,
    error: null,
  }),
  installUpdate: async (_url, _size, onProgress) => {
    const total = 42 * 1024 * 1024;
    for (const pct of [8, 27, 51, 74, 92, 100]) {
      await sleep(180);
      onProgress({ downloaded: Math.round((pct / 100) * total), total, percentage: pct });
    }
    return "/tmp/demo.apk";
  },
  resumeInstall: async () => {},
  registerServer: async () => ({
    server_name: "Atelier Vasseur",
    travelling: false,
    signed_in: false,
    servers: 1,
  }),
  reconnectActiveServer: async (_address: string): Promise<MobileStatus> => ({
    server_name: "démo",
    travelling: false,
    signed_in: true,
    servers: 1,
  }),
  registerAddress: async (address: string) => ({
    server_name: address,
    travelling: false,
    signed_in: false,
    servers: 1,
  }),
  discoverServers: async () => [
    {
      name: "Atelier Vasseur (Démo)",
      url: "http://192.168.1.50:7474",
      ip: "192.168.1.50",
      port: 7474,
      version: "0.3.11",
    },
  ],
  signIn: async () => ({
    server_name: "Atelier Vasseur",
    travelling: false,
    signed_in: true,
    servers: 1,
  }),
  signOut: async () => ({
    server_name: "Atelier Vasseur",
    travelling: false,
    signed_in: false,
    servers: 1,
  }),
  applyPairingLink: async () => ({
    server_name: "Atelier Vasseur",
    travelling: true,
    message: "Connecté à Atelier Vasseur depuis l'extérieur.",
  }),
  send: async (t) => ({
    text: `Réponse de démonstration à « ${t} ».`,
    images: [],
    conversation_id: "demo",
  }),
  listConversations: async () => [
    { id: "demo", title: "Conversation de démonstration", last_message_at: null },
    { id: "demo-2", title: "Préparer la terrasse", last_message_at: null },
  ],
  listProjects: async () => [{ id: "p1", name: "Atelier" }],
  listProjectConversations: async () => [],
  archiveConversation: async () => {},
  moveConversation: async () => {},
  createProject: async (name) => ({
    id: `p-${Date.now()}`,
    name,
  }),
  archivedConversations: async () => [
    { id: "demo-old", title: "Ancienne discussion", archived_at: null, project: "Atelier" },
    { id: "demo-old-2", title: "Recherche de matériaux", archived_at: null, project: "Jardin" },
  ],
  loadConversation: async () => [],
  listMediaModels: async () => [
    { name: "sd_xl_turbo_1.0.q8_0.gguf", ready: true, missing: [] },
    {
      name: "flux1-schnell-Q4_0.gguf",
      ready: false,
      missing: ["un encodeur CLIP-L", "un encodeur T5-XXL"],
    },
  ],
  saveImage: async (img) => `/sdcard/Pictures/${img.name}`,
  listFigures: async () => [],
  saveFigure: async (f: FigureDraft) => ({
    id: "demo",
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
  }),
  deleteFigure: async () => {},
  startFigureChat: async (figureId: string) => `demo-${figureId}`,
  figureSessions: async () => [],
  listExtensions: async () => [
    {
      name: "morph-image",
      display_name: "Images",
      version: "2.0.0",
      description: "Générer et retoucher",
      enabled: true,
      capabilities: ["image-gen", "image-editor"],
    },
  ],
  listCapabilities: async () => [],
  installExtension: async (source) => ({
    name: source.split("/").pop() ?? source,
    display_name: source,
    version: "0.1.0",
    description: null,
    enabled: true,
    capabilities: [],
  }),
  setExtensionEnabled: async () => {},
  removeExtension: async () => {},
  pullModel: async (_url, onProgress) => {
    const total = 250 * 1024 * 1024;
    for (const pct of [3, 12, 27, 46, 68, 87, 100]) {
      await sleep(160);
      onProgress({
        downloaded: Math.round((pct / 100) * total),
        total,
        percentage: pct,
        message: null,
      });
    }
    return { name: _url.split("/").pop() ?? _url, size: total };
  },
  removeModel: async () => {},
  runComposerTool: async (_tool: string, text: string) => text,
  readExtensionAsset: async () => "",
  invokeExtensionTool: async () => "{}",
  extensionConfig: async () => ({}) as Record<string, string>,
  setExtensionConfig: async () => {},
  listModels: async () => ["qwen2.5:3b"],
  listMemory: async () => [
    {
      id: "1",
      group: "vous",
      title: "Préférences",
      summary: "Préfère les réponses courtes.",
      details: ["Préfère les réponses courtes."],
    },
  ],
  remember: async () => {},
  removeMemoryDetail: async () => {},
  forget: async () => {},
  generateAudio: async () => ({
    name: "demo.wav",
    mime: "audio/wav",
    data_base64: "",
  }),
  currentUser: async () => ({
    id: "usr-demo",
    username: "Marie",
    role: "admin",
    server_url: "https://192.168.1.188:7474",
  }),
  changePassword: async () => {},
  listServerUsers: async () => [
    { id: "usr-1", username: "Marie", role: "admin", disabled: false },
    { id: "usr-2", username: "Lucas", role: "member", disabled: false },
  ],
  createServerUser: async () => {},
  deleteServerUser: async () => {},
  approveToolCall: async () => {},
};

export const api = isTauri ? core : demoCore;
