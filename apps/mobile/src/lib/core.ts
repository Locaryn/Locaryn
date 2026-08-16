import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export interface MobileStatus {
  server_name: string | null;
  travelling: boolean;
  signed_in: boolean;
  servers: number;
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
  send: (text: string) => invoke<string>("send_message", { text }),
  /** Models the machine can generate with: kind = "image" | "audio". */
  listMediaModels: (kind: "image" | "audio") => invoke<string[]>("list_media_models", { kind }),
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
  send: async (t) => `Réponse de démonstration à « ${t} ».`,
  listMediaModels: async () => ["flux1-schnell-Q4_0.gguf", "hexgrad__Kokoro-82M"],
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
