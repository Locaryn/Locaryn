import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { api } from "../../lib/core";
import type { ResolvedSlotContribution } from "./SlotRegistry";

interface Props {
  contribution: ResolvedSlotContribution;
  context?: {
    input?: string;
    setInput?: (text: string | ((prev: string) => string)) => void;
    send?: () => void;
    canCompose?: boolean;
    onNavigate?: (destination: string) => void;
    [key: string]: unknown;
  };
  className?: string;
  style?: React.CSSProperties;
}

type PluginApi = {
  version: string;
  chat: {
    getText: () => string;
    setText: (text: string) => void;
    insertText: (text: string) => void;
    submit: () => void;
    getSessionId: () => string | null;
  };
  files: { assetUrl: (path: string) => string };
  tools: { invoke: (tool: string, input: string | Record<string, unknown>) => Promise<unknown> };
  ui: {
    showToast: (message: string, type?: string) => void;
    dispatchAction: (name: string, payload?: unknown) => void;
  };
  events: {
    on: (name: string, handler: (data: unknown) => void) => () => void;
    emit: (name: string, data?: unknown) => void;
  };
};

const loadedScripts = new Set<string>();
const listeners = new Map<string, Set<(data: unknown) => void>>();

function installBridge(): PluginApi {
  const target = window as unknown as { locaryn?: PluginApi; LocarynPluginAPI?: PluginApi };
  if (target.locaryn) return target.locaryn;
  const plugin: PluginApi = {
    version: "1.0.0",
    chat: {
      getText: () => "",
      setText: () => {},
      insertText: () => {},
      submit: () => {},
      getSessionId: () => null,
    },
    files: {
      // The phone receives generated artifacts through the chat stream. This
      // fallback is only for an extension gallery and is intentionally not a
      // raw filesystem access API.
      assetUrl: (path) => path,
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
      dispatchAction: (name, payload) => {
        window.dispatchEvent(new CustomEvent(`locaryn:action:${name}`, { detail: payload }));
      },
    },
    events: {
      on: (name, handler) => {
        let group = listeners.get(name);
        if (!group) {
          group = new Set();
          listeners.set(name, group);
        }
        group.add(handler);
        return () => group?.delete(handler);
      },
      emit: (name, data) => {
        for (const handler of listeners.get(name) ?? []) handler(data);
      },
    },
  };
  target.locaryn = plugin;
  target.LocarynPluginAPI = plugin;
  return plugin;
}

export function DynamicPluginWidget({ contribution, context, className, style }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const container = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (contribution.type !== "custom-element" && contribution.type !== "script") return;
    const tag = contribution.tag?.toLowerCase();
    if (!tag || !contribution.entry) {
      setError("Cette extension ne déclare pas de composant d'interface.");
      return;
    }

    const mount = () => {
      if (cancelled || !container.current || !customElements.get(tag)) return;
      const element = document.createElement(tag);
      (element as unknown as { context?: unknown }).context = context;
      container.current.replaceChildren(element);
    };

    void (async () => {
      try {
        installBridge();
        const key = `${contribution.extensionId}:${contribution.entry}`;
        if (!loadedScripts.has(key)) {
          const source = await api.readExtensionAsset(
            contribution.extensionId,
            contribution.entry!,
          );
          if (cancelled) return;
          const execute = new Function("locaryn", "core", source);
          execute((window as unknown as { locaryn: PluginApi }).locaryn, api);
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
  }, [contribution.extensionId, contribution.entry, contribution.tag, contribution.type, context]);

  if (contribution.type === "custom-element" || contribution.type === "script") {
    return (
      <div
        ref={container}
        className={className}
        style={{ display: "block", width: "100%", ...style }}
      >
        {error && <p className="lo-error">Interface de l'extension indisponible : {error}</p>}
      </div>
    );
  }

  async function handleClick() {
    if (contribution.action === "insert") {
      const value = contribution.value ?? "";
      context?.setInput?.((previous: string) => (previous ? `${previous} ${value}` : value));
      return;
    }
    if (contribution.action === "navigate") {
      context?.onNavigate?.(contribution.value ?? contribution.id);
      return;
    }
    if (contribution.action === "tool") {
      if (!contribution.value) return;
      setBusy(true);
      setError(null);
      try {
        const result = await api.runComposerTool(contribution.value, context?.input ?? "");
        if (result) context?.setInput?.(result);
      } catch (cause) {
        setError(String(cause));
      } finally {
        setBusy(false);
      }
    }
  }

  const iconName = (contribution.icon || "extensions") as IconName;
  return (
    <button
      type="button"
      className={`lo-btn-ghost ${className || ""}`.trim()}
      disabled={busy || context?.canCompose === false}
      onClick={() => void handleClick()}
      title={error || contribution.hint || contribution.label || contribution.id}
      style={{ display: "inline-flex", alignItems: "center", gap: 4, ...style }}
    >
      <Icon name={iconName} size={16} />
      {contribution.label && <span>{contribution.label}</span>}
    </button>
  );
}
