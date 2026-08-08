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
  registerServer: (provisioningJson: string) =>
    invoke<MobileStatus>("register_server", { provisioningJson }),
  signIn: (username: string, password: string) =>
    invoke<MobileStatus>("sign_in", { username, password }),
  signOut: () => invoke<MobileStatus>("sign_out"),
  /** Verify a scanned code and apply it. Throws with a phrased message. */
  applyPairingLink: (uri: string) => invoke<PairingResult>("apply_pairing_link", { uri }),
  send: (text: string) => invoke<string>("send_message", { text }),
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
  registerServer: async () => ({
    server_name: "Atelier Vasseur",
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
};

export const api = isTauri ? core : demoCore;
