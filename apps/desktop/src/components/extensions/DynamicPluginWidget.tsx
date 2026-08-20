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
  /** Vrai une fois l'élément personnalisé réellement posé dans le conteneur. */
  const [mounted, setMounted] = useState(false);
  const customElementContainerRef = useRef<HTMLDivElement | null>(null);

  // ── Chargement dynamique du script / custom element si requis ──────────
  useEffect(() => {
    if (contribution.type !== "custom-element" && contribution.type !== "script") return;
    if (!contribution.entry) return;

    const scriptKey = `${contribution.extensionId}@${contribution.extensionVersion}:${contribution.entry}`;
    if (loadedScripts.has(scriptKey)) {
      mountCustomElement();
      return;
    }

    let cancelled = false;
    core
      .readExtensionAsset(contribution.extensionId, contribution.entry)
      .then((code) => {
        if (cancelled) return;
        // Un asset vide laissait le cadre sur « Chargement… » indéfiniment :
        // l'extension annonce une interface qu'elle ne livre pas.
        if (!code) {
          setError(`l'extension ne fournit pas ${contribution.entry}`);
          return;
        }
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
          setError(String(err));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    contribution.id,
    contribution.extensionId,
    contribution.extensionVersion,
    contribution.entry,
    contribution.type,
  ]);

  function mountCustomElement() {
    const host = customElementContainerRef.current;
    if (!host || !contribution.tag) return;
    const tag = contribution.tag.toLowerCase();
    if (!customElements.get(tag)) {
      // Le script est passé sans définir l'élément qu'il annonce : le dire,
      // plutôt que de laisser un cadre vide qu'on ne peut pas diagnostiquer.
      if (
        loadedScripts.has(
          `${contribution.extensionId}@${contribution.extensionVersion}:${contribution.entry}`,
        )
      ) {
        setError(`l'extension n'a pas défini l'élément « ${tag} »`);
      }
      return;
    }
    // Déjà monté : ne pas recréer l'élément. Ce montage tournait à chaque
    // rendu du parent, et chaque passage repartait d'un panneau vierge —
    // saisie perdue, requêtes relancées, résultat effacé.
    if (host.firstElementChild?.tagName.toLowerCase() === tag) {
      const existing = host.firstElementChild as unknown as {
        context?: unknown;
        pluginUpdated?: () => void;
      };
      existing.context = context;
      const renderedVersion = host.firstElementChild.getAttribute("data-locaryn-extension-version");
      if (renderedVersion !== contribution.extensionVersion) {
        host.firstElementChild.setAttribute(
          "data-locaryn-extension-version",
          contribution.extensionVersion,
        );
        existing.pluginUpdated?.();
      }
      return;
    }
    host.innerHTML = "";
    const el = document.createElement(tag);
    el.setAttribute("data-locaryn-extension-version", contribution.extensionVersion);
    (el as unknown as { context?: unknown }).context = context;
    host.appendChild(el);
    setMounted(true);
    setError(null);
  }

  useEffect(() => {
    if (contribution.type === "custom-element" && contribution.tag) {
      mountCustomElement();
    }
  });

  // ── Rendu Custom Element ─────────────────────────────────────────────
  if (contribution.type === "custom-element") {
    return (
      <div className={className} style={{ display: "block", width: "100%", ...style }}>
        <div ref={customElementContainerRef} style={{ display: "block", width: "100%" }} />
        {!mounted && (
          <div className="locaryn-card" style={{ padding: 32, textAlign: "center" }}>
            <p className="locaryn-field-hint" style={{ margin: 0 }}>
              {error
                ? `Interface indisponible — ${error}`
                : `Chargement de l'interface fournie par ${contribution.extensionName}…`}
            </p>
          </div>
        )}
      </div>
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
