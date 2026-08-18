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

export function DynamicPluginWidget({ contribution, context, className, style }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const customElementContainerRef = useRef<HTMLDivElement | null>(null);

  function mountCustomElement() {
    if (!customElementContainerRef.current || !contribution.tag) return;
    const tag = contribution.tag.toLowerCase();
    if (!customElements.get(tag)) return;
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

  if (contribution.type === "custom-element") {
    return (
      <div
        ref={customElementContainerRef}
        className={className}
        style={{ display: "inline-flex", alignItems: "center", ...style }}
      />
    );
  }

  async function handleClick() {
    if (contribution.action === "insert") {
      const val = contribution.value ?? "";
      if (context?.setInput) {
        context.setInput((prev: string) => (prev ? `${prev} ${val}` : val));
      }
      return;
    }

    if (contribution.action === "navigate") {
      const dest = contribution.value ?? contribution.id;
      if (context?.onNavigate) {
        context.onNavigate(dest);
      } else {
        window.dispatchEvent(
          new CustomEvent("locaryn:navigate", { detail: { destination: dest } }),
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
        const text = await api.runComposerTool(toolName, context?.input ?? "");
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
      className={`lo-btn-ghost ${className || ""}`.trim()}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 4,
        padding: "4px 8px",
        minHeight: "36px",
        fontSize: 13,
        ...style,
      }}
      disabled={busy}
      onClick={handleClick}
      title={contribution.hint || contribution.label || contribution.id}
      aria-label={contribution.label || contribution.id}
    >
      <Icon name={iconName} size={16} />
      {contribution.label && <span>{contribution.label}</span>}
      {busy && <span style={{ fontSize: 10 }}>…</span>}
      {error && <span style={{ color: "var(--danger)", fontSize: 10 }}>!</span>}
    </button>
  );
}
