import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export interface MobileStatus {
  server_name: string | null;
  travelling: boolean;
  signed_in: boolean;
  servers: number;
}

/** Une extension installée sur le serveur, vue du téléphone. */
export interface PhoneExtension {
  name: string;
  display_name: string;
  version: string;
  description: string | null;
  enabled: boolean;
  capabilities: string[];
}

/**
 * Le catalogue officiel.
 *
 * Écrit ici plutôt que demandé au serveur : ce sont les dépôts publiés par le
 * projet, ils changent au rythme des versions de l'application, et une liste
 * en dur ne peut pas tomber en panne de réseau au pire moment.
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
}

/** Ce que le téléphone sait de sa propre mise à jour. */
export interface UpdateStatus {
  current: string;
  latest: string | null;
  available: boolean;
  download_url: string | null;
  error: string | null;
}

/** A file generated on the machine at the other end, ready to show. */
export interface MediaResult {
  name: string;
  mime: string;
  /** Base64 payload; the webview has no access to the server's disk. */
  data_base64: string;
}

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args);
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
  /** Passe la main au système pour installer la nouvelle version. */
  openUpdate: (url: string) => invoke<void>("open_update", { url }),
  registerServer: (provisioningJson: string) =>
    invoke<MobileStatus>("register_server", { provisioningJson }),
  /** Enregistre un serveur depuis son adresse, sans code à scanner. */
  registerAddress: (address: string) => invoke<MobileStatus>("register_address", { address }),
  signIn: (username: string, password: string) =>
    invoke<MobileStatus>("sign_in", { username, password }),
  signOut: () => invoke<MobileStatus>("sign_out"),
  /** Verify a scanned code and apply it. Throws with a phrased message. */
  applyPairingLink: (uri: string) => invoke<PairingResult>("apply_pairing_link", { uri }),
  send: (text: string) => invoke<ChatReply>("send_message", { text }),
  /** Models the machine can generate with: kind = "image" | "audio". */
  listMediaModels: (kind: "image" | "audio") => invoke<MediaModel[]>("list_media_models", { kind }),
  /** Les extensions installées sur le serveur, et leur pilotage. */
  listExtensions: () => invoke<PhoneExtension[]>("list_extensions"),
  installExtension: (source: string) => invoke<PhoneExtension>("install_extension", { source }),
  setExtensionEnabled: (name: string, enabled: boolean) =>
    invoke<void>("set_extension_enabled", { name, enabled }),
  removeExtension: (name: string) => invoke<void>("remove_extension", { name }),
  /** Ce que le serveur retient de son utilisateur. */
  listMemory: () => invoke<MemoryEntry[]>("list_memory"),
  remember: (category: string, content: string) => invoke<void>("remember", { category, content }),
  forget: (id: string) => invoke<void>("forget", { id }),
  /** Écrit une image sur l'appareil ; renvoie le chemin du fichier. */
  saveImage: (img: MediaResult) =>
    invoke<string>("save_image", { name: img.name, dataBase64: img.data_base64 }),
  generateImage: (args: {
    model: string;
    prompt: string;
    negativePrompt?: string;
    width?: number;
    height?: number;
  }) => invoke<MediaResult>("generate_image", args),
  generateAudio: (args: {
    model: string;
    text: string;
    speed?: number;
    language?: string;
  }) => invoke<MediaResult>("generate_audio", args),
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
  // Dans un navigateur il n'y a pas d'APK à remplacer : la vérification dit
  // simplement qu'il n'y a rien à faire, et le bouton ne s'affiche pas.
  checkUpdate: async () => ({
    current: "0.0.0-dev",
    latest: null,
    available: false,
    download_url: null,
    error: null,
  }),
  openUpdate: async () => {},
  registerServer: async () => ({
    server_name: "Atelier Vasseur",
    travelling: false,
    signed_in: false,
    servers: 1,
  }),
  registerAddress: async (address: string) => ({
    server_name: address,
    travelling: false,
    signed_in: false,
    servers: 1,
  }),
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
  send: async (t) => ({ text: `Réponse de démonstration à « ${t} ».`, images: [] }),
  listMediaModels: async () => [
    { name: "sd_xl_turbo_1.0.q8_0.gguf", ready: true, missing: [] },
    {
      name: "flux1-schnell-Q4_0.gguf",
      ready: false,
      missing: ["un encodeur CLIP-L", "un encodeur T5-XXL"],
    },
  ],
  saveImage: async (img) => `/sdcard/Pictures/${img.name}`,
  listExtensions: async () => [
    {
      name: "plugin-image-gen",
      display_name: "Génération d'images",
      version: "0.1.0",
      description: "Studio, images",
      enabled: true,
      capabilities: ["image-gen"],
    },
  ],
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
  listMemory: async () => [
    { id: "1", category: "preference", content: "Je préfère les réponses courtes." },
  ],
  remember: async () => {},
  forget: async () => {},
  generateImage: async () => ({
    name: "demo.png",
    mime: "image/png",
    data_base64: "",
  }),
  generateAudio: async () => ({
    name: "demo.wav",
    mime: "audio/wav",
    data_base64: "",
  }),
};

export const api = isTauri ? core : demoCore;
