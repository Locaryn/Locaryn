import { convertFileSrc } from "@tauri-apps/api/core";
import { type CloudModel, type CloudProvider, type CloudProviderStatus, core } from "./core";

/**
 * Interface d'interaction exposée à tous les scripts et Web Components de plugins.
 * Disponible sur `window.locaryn` ou injectée dans les contextes de slots.
 */
export interface LocarynPluginAPI {
  version: string;
  /** Où ce panneau tourne : `desktop`, `mobile` ou `web`.
   *
   *  Une extension peut déclarer une contribution par plateforme, avec son
   *  propre fichier et sa propre balise. Mais un panneau unique qui s'adapte
   *  reste souvent plus simple à tenir que trois panneaux qui divergent — et
   *  pour ça il lui faut savoir où il est. */
  surface: "desktop" | "mobile" | "web";
  chat: {
    getText: () => string;
    setText: (text: string) => void;
    insertText: (text: string) => void;
    submit: () => void;
    getSessionId: () => string | null;
    appendMessage: (role: "user" | "assistant", content: string) => Promise<void>;
    appendAssistantMessage: (content: string) => Promise<void>;
    /** Let an extension consume a composer message before the native agent. */
    onSubmit: (handler: (text: string) => boolean | Promise<boolean>) => () => void;
  };
  files: {
    /** Convert a host-owned path to a URL usable by an extension web component. */
    assetUrl: (path: string) => string;
  };
  audio: {
    isRecordingSupported: () => boolean;
  };
  tools: {
    invoke: (toolName: string, input: string | Record<string, unknown>) => Promise<unknown>;
  };
  /** Le catalogue distant qu'apporte cette extension.
   *
   *  Un panneau d'extension ne touche jamais à la clé : il demande à l'hôte
   *  de l'écrire dans le trousseau du système, et apprend seulement qu'elle
   *  existe. Sans cette asymétrie, installer une extension reviendrait à lui
   *  confier de quoi dépenser l'argent de son utilisateur. */
  providers: {
    /** Tous les catalogues distants actifs, celui de cette extension compris. */
    list: () => Promise<CloudProvider[]>;
    /** Écrire la clé. Elle ne ressort jamais du trousseau. */
    setKey: (provider: string, key: string) => Promise<void>;
    clearKey: (provider: string) => Promise<void>;
    /** La liste des modèles, relue chez le fournisseur quand elle a vieilli. */
    models: (provider: string, refresh?: boolean) => Promise<CloudModel[]>;
    /** Activer un modèle pour la conversation. */
    select: (provider: string, model: string) => Promise<void>;
    /** La passerelle locale répond-elle ? */
    status: (provider: string) => Promise<CloudProviderStatus>;
    /** La démarrer avec la commande déclarée par le manifeste — et aucune
     *  autre : le panneau ne choisit pas ce qui s'exécute. */
    start: (provider: string) => Promise<CloudProviderStatus>;
    /** L'installer avec la commande déclarée par le manifeste. */
    install: (provider: string) => Promise<string>;
    /** Ouvrir son tableau de bord dans le navigateur du système. */
    openDashboard: (provider: string) => Promise<string>;
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
  private currentSessionId: string | null = null;
  private submitHandlers = new Set<(text: string) => boolean | Promise<boolean>>();
  private eventListeners: Map<string, Set<(data: unknown) => void>> = new Map();

  constructor() {
    this.setupGlobalAPI();
  }

  public registerChatContext(
    getText: () => string,
    setText: (text: string) => void,
    submit: () => void,
    sessionId: string | null = null,
  ) {
    this.currentInputGetter = getText;
    this.currentInputSetter = setText;
    this.currentSubmitter = submit;
    this.currentSessionId = sessionId;
  }

  public unregisterChatContext() {
    this.currentInputGetter = null;
    this.currentInputSetter = null;
    this.currentSubmitter = null;
    this.currentSessionId = null;
  }

  public async dispatchSubmit(text: string): Promise<boolean> {
    for (const handler of [...this.submitHandlers]) {
      try {
        if (await handler(text)) return true;
      } catch (error) {
        console.error("[Locaryn Plugin Bridge] submit handler failed:", error);
      }
    }
    return false;
  }

  private setupGlobalAPI() {
    const api: LocarynPluginAPI = {
      version: "1.0.0",
      surface: "desktop",
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
        getSessionId: () => this.currentSessionId,
        appendMessage: async (role: "user" | "assistant", content: string) => {
          const sessionId = this.currentSessionId;
          if (!sessionId) throw new Error("Aucune conversation active.");
          await core.appendChatMessage(sessionId, role, content);
          window.dispatchEvent(
            new CustomEvent("locaryn:chat-message", {
              detail: { sessionId, role, content },
            }),
          );
        },
        appendAssistantMessage: async (content: string) => {
          const sessionId = this.currentSessionId;
          if (!sessionId) throw new Error("Aucune conversation active.");
          await core.appendAssistantMessage(sessionId, content);
          window.dispatchEvent(
            new CustomEvent("locaryn:chat-message", {
              detail: { sessionId, role: "assistant", content },
            }),
          );
        },
        onSubmit: (handler: (text: string) => boolean | Promise<boolean>) => {
          this.submitHandlers.add(handler);
          return () => this.submitHandlers.delete(handler);
        },
      },
      files: {
        assetUrl: (path: string) => {
          if (path.startsWith("data:") || /^[a-z][a-z0-9+.-]*:\/\//i.test(path)) return path;
          return convertFileSrc(path.replace(/\\/g, "/"));
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
          const args =
            typeof input === "string"
              ? (() => {
                  try {
                    return JSON.parse(input) as Record<string, unknown>;
                  } catch {
                    return { text: input };
                  }
                })()
              : input;
          return core.invokeExtensionTool(toolName, args);
        },
      },
      providers: {
        list: () => core.cloudProviders(),
        setKey: (provider: string, key: string) => core.cloudProviderSetKey(provider, key),
        clearKey: (provider: string) => core.cloudProviderClearKey(provider),
        models: (provider: string, refresh?: boolean) =>
          core.cloudProviderModels(provider, refresh),
        status: (provider: string) => core.cloudProviderStatus(provider),
        start: (provider: string) => core.cloudProviderStart(provider),
        install: (provider: string) => core.cloudProviderInstall(provider),
        openDashboard: (provider: string) => core.cloudProviderOpenDashboard(provider),
        select: async (provider: string, model: string) => {
          await core.cloudProviderSelect(provider, model);
          // Le sélecteur de modèle du chat écoute : sans ce signal, le nom
          // affiché sous le champ de saisie resterait celui d'avant.
          window.dispatchEvent(
            new CustomEvent("locaryn:cloud-model-selected", { detail: { provider, model } }),
          );
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
