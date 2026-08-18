import { core } from "./core";

/**
 * Interface d'interaction exposée à tous les scripts et Web Components de plugins.
 * Disponible sur `window.locaryn` ou injectée dans les contextes de slots.
 */
export interface LocarynPluginAPI {
  version: string;
  chat: {
    getText: () => string;
    setText: (text: string) => void;
    insertText: (text: string) => void;
    submit: () => void;
  };
  audio: {
    isRecordingSupported: () => boolean;
  };
  tools: {
    invoke: (toolName: string, input: string | Record<string, unknown>) => Promise<unknown>;
  };
  ui: {
    showToast: (message: string, type?: "info" | "success" | "warning" | "error") => void;
    dispatchAction: (actionId: string, payload?: unknown) => void;
  };
  events: {
    on: (eventName: string, handler: (data: unknown) => void) => () => void;
    emit: (eventName: string, data?: unknown) => void;
  };
}

class PluginBridgeManager {
  private currentInputGetter: (() => string) | null = null;
  private currentInputSetter: ((text: string) => void) | null = null;
  private currentSubmitter: (() => void) | null = null;
  private eventListeners: Map<string, Set<(data: unknown) => void>> = new Map();

  constructor() {
    this.setupGlobalAPI();
  }

  public registerChatContext(
    getText: () => string,
    setText: (text: string) => void,
    submit: () => void,
  ) {
    this.currentInputGetter = getText;
    this.currentInputSetter = setText;
    this.currentSubmitter = submit;
  }

  public unregisterChatContext() {
    this.currentInputGetter = null;
    this.currentInputSetter = null;
    this.currentSubmitter = null;
  }

  private setupGlobalAPI() {
    const api: LocarynPluginAPI = {
      version: "1.0.0",
      chat: {
        getText: () => this.currentInputGetter?.() ?? "",
        setText: (text: string) => {
          this.currentInputSetter?.(text);
        },
        insertText: (text: string) => {
          if (!this.currentInputSetter) return;
          const current = this.currentInputGetter?.() ?? "";
          if (!current) {
            this.currentInputSetter(text);
          } else {
            this.currentInputSetter(`${current} ${text}`);
          }
        },
        submit: () => {
          this.currentSubmitter?.();
        },
      },
      audio: {
        isRecordingSupported: () => {
          return (
            typeof window !== "undefined" &&
            !!(
              navigator.mediaDevices?.getUserMedia ||
              (window as unknown as { webkitSpeechRecognition?: unknown }).webkitSpeechRecognition
            )
          );
        },
      },
      tools: {
        invoke: async (toolName: string, input: string | Record<string, unknown>) => {
          const raw = typeof input === "string" ? input : JSON.stringify(input);
          return core.runComposerTool(toolName, raw);
        },
      },
      ui: {
        showToast: (message: string, type: "info" | "success" | "warning" | "error" = "info") => {
          window.dispatchEvent(
            new CustomEvent("locaryn:toast", {
              detail: { message, type },
            }),
          );
        },
        dispatchAction: (actionId: string, payload?: unknown) => {
          window.dispatchEvent(
            new CustomEvent(`locaryn:action:${actionId}`, {
              detail: payload,
            }),
          );
        },
      },
      events: {
        on: (eventName: string, handler: (data: unknown) => void) => {
          if (!this.eventListeners.has(eventName)) {
            this.eventListeners.set(eventName, new Set());
          }
          this.eventListeners.get(eventName)?.add(handler);
          return () => {
            this.eventListeners.get(eventName)?.delete(handler);
          };
        },
        emit: (eventName: string, data?: unknown) => {
          const listeners = this.eventListeners.get(eventName);
          if (listeners) {
            for (const fn of listeners) {
              try {
                fn(data);
              } catch (err) {
                console.error(`[Locaryn Plugin Bridge] Error in listener for ${eventName}:`, err);
              }
            }
          }
        },
      },
    };

    if (typeof window !== "undefined") {
      (window as unknown as { locaryn?: LocarynPluginAPI }).locaryn = api;
      (window as unknown as { LocarynPluginAPI?: LocarynPluginAPI }).LocarynPluginAPI = api;
    }
  }
}

export const pluginBridge = new PluginBridgeManager();
