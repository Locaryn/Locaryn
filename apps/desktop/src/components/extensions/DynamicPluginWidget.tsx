import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { core } from "../../lib/core";
import { pluginBridge } from "../../lib/pluginBridge";
import type { ResolvedSlotContribution } from "./SlotRegistry";

interface Props {
  contribution: ResolvedSlotContribution;
  context?: {
    input?: string;
    setInput?: (text: string | ((prev: string) => string)) => void;
    send?: () => void;
    canCompose?: boolean;
    [key: string]: unknown;
  };
  className?: string;
  style?: React.CSSProperties;
}

const loadedScripts = new Set<string>();

export function DynamicPluginWidget({ contribution, context, className, style }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const customElementContainerRef = useRef<HTMLDivElement | null>(null);

  // ── Chargement dynamique du script / custom element si requis ──────────
  useEffect(() => {
    if (contribution.type !== "custom-element" && contribution.type !== "script") return;
    if (!contribution.entry) return;

    const scriptKey = `${contribution.extensionId}:${contribution.entry}`;
    if (loadedScripts.has(scriptKey)) {
      mountCustomElement();
      return;
    }

    let cancelled = false;
    core
      .readExtensionAsset(contribution.extensionId, contribution.entry)
      .then((code) => {
        if (cancelled || !code) return;
        try {
          // Évaluation isolée du script du plugin avec injection du SDK Locaryn
          const execute = new Function("locaryn", "core", code);
          execute((window as unknown as { locaryn?: unknown }).locaryn || pluginBridge, core);
          loadedScripts.add(scriptKey);
          mountCustomElement();
        } catch (err) {
          console.error(`[Plugin UI] Erreur d'exécution pour ${contribution.id}:`, err);
          setError(String(err));
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.warn(`[Plugin UI] Impossible de lire l'asset pour ${contribution.id}:`, err);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [contribution.id, contribution.extensionId, contribution.entry, contribution.type]);

  function mountCustomElement() {
    if (!customElementContainerRef.current || !contribution.tag) return;
    const tag = contribution.tag.toLowerCase();
    if (!customElements.get(tag)) {
      // Le Custom Element n'est pas encore défini par le script
      return;
    }
    // Nettoyer et monter l'élément
    customElementContainerRef.current.innerHTML = "";
    const el = document.createElement(tag);
    (el as unknown as { context?: unknown }).context = context;
    customElementContainerRef.current.appendChild(el);
  }

  useEffect(() => {
    if (contribution.type === "custom-element" && contribution.tag) {
      mountCustomElement();
    }
  });

  // ── Rendu Custom Element ─────────────────────────────────────────────
  if (contribution.type === "custom-element") {
    return (
      <div
        ref={customElementContainerRef}
        className={className}
        style={{ display: "block", width: "100%", ...style }}
      />
    );
  }

  // ── Rendu Bouton d'action standard ───────────────────────────────────
  async function handleClick() {
    if (contribution.action === "insert") {
      const val = contribution.value ?? "";
      if (context?.setInput) {
        context.setInput((prev: string) => (prev ? `${prev} ${val}` : val));
      } else {
        pluginBridge.registerChatContext(
          () => "",
          () => {},
          () => {},
        );
      }
      return;
    }

    if (contribution.action === "tool") {
      const toolName = contribution.value;
      if (!toolName) return;
      setBusy(true);
      setError(null);
      try {
        const text = await core.runComposerTool(toolName, context?.input ?? "");
        if (text && context?.setInput) {
          context.setInput(text);
        }
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
      return;
    }

    if (contribution.action === "event") {
      const eventName = contribution.value || `locaryn:action:${contribution.id}`;
      window.dispatchEvent(new CustomEvent(eventName, { detail: { contribution, context } }));
    }
  }

  const iconName = (contribution.icon || "extensions") as IconName;

  return (
    <button
      type="button"
      className={className || "locaryn-chip-btn"}
      title={error || contribution.hint || contribution.label}
      disabled={context?.canCompose === false || busy}
      onClick={handleClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "5px",
        ...style,
      }}
    >
      <span style={{ display: "inline-flex" }}>
        <Icon name={iconName} size={15} />
      </span>
      {contribution.label && <span>{contribution.label}</span>}
    </button>
  );
}
