import { useEffect, useRef, useState } from "react";
import type { ExtensionUiSlotContribution, PhoneExtension } from "../lib/core";
import { api } from "../lib/core";

export type WebPluginContribution = ExtensionUiSlotContribution & {
  extensionId: string;
  extensionName: string;
};

type Props = {
  contribution: WebPluginContribution;
};

type WebPluginApi = {
  version: string;
  chat: {
    getText: () => string;
    setText: (text: string) => void;
    insertText: (text: string) => void;
    submit: () => void;
    getSessionId: () => string | null;
    appendAssistantMessage?: (content: string) => Promise<void>;
  };
  /** Où ce panneau tourne : `desktop`, `mobile` ou `web`. */
  surface: string;
  files: { assetUrl: (path: string) => string };
  tools: { invoke: (tool: string, input: string | Record<string, unknown>) => Promise<unknown> };
  ui: {
    showToast: (message: string, type?: string) => void;
    dispatchAction: (action: string, payload?: unknown) => void;
  };
  events: {
    on: (name: string, handler: (data: unknown) => void) => () => void;
    emit: (name: string, data?: unknown) => void;
  };
};

const loadedScripts = new Set<string>();
const eventListeners = new Map<string, Set<(data: unknown) => void>>();

function bridge(): WebPluginApi {
  const global = window as unknown as { locaryn?: WebPluginApi; LocarynPluginAPI?: WebPluginApi };
  if (global.locaryn) return global.locaryn;
  const pluginApi: WebPluginApi = {
    version: "1.0.0",
    surface: SURFACE,
    chat: {
      getText: () => "",
      setText: () => {},
      insertText: () => {},
      submit: () => {},
      getSessionId: () => null,
    },
    files: {
      assetUrl: (path) => {
        if (/^(?:data:|blob:|https?:|asset:)/i.test(path)) return path;
        return `/v1/extension-assets?path=${encodeURIComponent(path)}`;
      },
    },
    tools: {
      invoke: (tool, input) => {
        let args: Record<string, unknown>;
        if (typeof input === "string") {
          try {
            args = JSON.parse(input) as Record<string, unknown>;
          } catch {
            args = { text: input };
          }
        } else {
          args = input;
        }
        return api.invokeExtensionTool(tool, args);
      },
    },
    ui: {
      showToast: (message, type = "info") => {
        window.dispatchEvent(new CustomEvent("locaryn:toast", { detail: { message, type } }));
      },
      dispatchAction: (action, payload) => {
        window.dispatchEvent(new CustomEvent(`locaryn:action:${action}`, { detail: payload }));
      },
    },
    events: {
      on: (name, handler) => {
        let listeners = eventListeners.get(name);
        if (!listeners) {
          listeners = new Set();
          eventListeners.set(name, listeners);
        }
        listeners.add(handler);
        return () => listeners?.delete(handler);
      },
      emit: (name, data) => {
        for (const handler of eventListeners.get(name) ?? []) handler(data);
      },
    },
  };
  global.locaryn = pluginApi;
  global.LocarynPluginAPI = pluginApi;
  return pluginApi;
}

/** La surface sur laquelle cette interface tourne. */
export const SURFACE = "web";

/**
 * Cette contribution vise-t-elle la surface courante ?
 *
 * Sans `platforms`, oui. Le bureau et le téléphone respectaient déjà ce champ ;
 * le web l'ignorait, et un panneau explicitement réservé à l'ordinateur
 * s'affichait quand même dans le navigateur.
 */
export function targetsSurface(
  contribution: { platforms?: string[] },
  surface: string = SURFACE,
): boolean {
  const platforms = contribution.platforms;
  if (!platforms || platforms.length === 0) return true;
  return platforms.some((platform) => platform.trim().toLowerCase() === surface);
}

function resolvedContribution(extensions: PhoneExtension[], slot: string): WebPluginContribution[] {
  return extensions
    .filter((extension) => extension.enabled)
    .flatMap((extension) =>
      (extension.ui?.slots ?? [])
        .filter((contribution) => contribution.slot === slot && targetsSurface(contribution))
        .map((contribution) => ({
          ...contribution,
          extensionId: extension.name,
          extensionName: extension.display_name || extension.name,
        })),
    )
    .sort((a, b) => (a.order ?? 100) - (b.order ?? 100));
}

export function getWebStudioContributions(extensions: PhoneExtension[]): WebPluginContribution[] {
  return resolvedContribution(extensions, "studio.tabs");
}

export function PluginWidget({ contribution }: Props) {
  const container = useRef<HTMLDivElement | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const tag = contribution.tag?.toLowerCase();
    if (!tag || !contribution.entry) {
      setError("Cette extension ne déclare pas de composant d'interface.");
      return () => {
        cancelled = true;
      };
    }

    const mount = () => {
      if (cancelled || !container.current || !customElements.get(tag)) return;
      const element = document.createElement(tag);
      // Lisible depuis CSS sans une ligne de script :
      // `mon-panneau[data-locaryn-surface="web"] { … }`.
      element.setAttribute("data-locaryn-surface", SURFACE);
      container.current.replaceChildren(element);
    };

    const key = `${contribution.extensionId}:${contribution.entry}`;
    void (async () => {
      try {
        const plugin = bridge();
        if (!loadedScripts.has(key)) {
          const code = await api.readExtensionAsset(contribution.extensionId, contribution.entry!);
          if (cancelled) return;
          const execute = new Function("locaryn", "core", code);
          execute(plugin, api);
          loadedScripts.add(key);
        }
        mount();
      } catch (cause) {
        if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause));
      }
    })();

    return () => {
      cancelled = true;
      container.current?.replaceChildren();
    };
  }, [contribution.extensionId, contribution.entry, contribution.tag]);

  if (error) return <p className="lo-error">Interface de l'extension indisponible : {error}</p>;
  return (
    <div
      ref={container}
      className="lo-plugin-widget"
      aria-label={contribution.label ?? contribution.id}
    />
  );
}
