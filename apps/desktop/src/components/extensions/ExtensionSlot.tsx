import { useEffect, useState } from "react";
import { type InstalledExtension, core } from "../../lib/core";
import { DynamicPluginWidget } from "./DynamicPluginWidget";
import { getSlotContributions } from "./SlotRegistry";

interface Props {
  name: string;
  context?: {
    input?: string;
    setInput?: (text: string | ((prev: string) => string)) => void;
    send?: () => void;
    canCompose?: boolean;
    [key: string]: unknown;
  };
  extensions?: InstalledExtension[];
  className?: string;
  itemClassName?: string;
  style?: React.CSSProperties;
  children?: React.ReactNode;
}

export function ExtensionSlot({
  name,
  context,
  extensions: propExtensions,
  className,
  itemClassName,
  style,
  children,
}: Props) {
  const [extensions, setExtensions] = useState<InstalledExtension[]>(propExtensions || []);

  useEffect(() => {
    if (propExtensions) {
      setExtensions(propExtensions);
      return;
    }

    let cancelled = false;
    const fetchExtensions = () => {
      core
        .listExtensions()
        .then((list) => {
          if (!cancelled) setExtensions(list);
        })
        .catch(() => {
          if (!cancelled) setExtensions([]);
        });
    };

    fetchExtensions();
    window.addEventListener("locaryn:extensions-changed", fetchExtensions);
    return () => {
      cancelled = true;
      window.removeEventListener("locaryn:extensions-changed", fetchExtensions);
    };
  }, [propExtensions]);

  const contributions = getSlotContributions(extensions, name);

  if (contributions.length === 0) {
    return children ? <>{children}</> : null;
  }

  return (
    <div
      className={`locaryn-extension-slot ${className || ""}`.trim()}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "6px",
        ...style,
      }}
    >
      {contributions.map((c) => (
        <DynamicPluginWidget
          key={`${c.extensionId}:${c.id}`}
          contribution={c}
          context={context}
          className={itemClassName}
        />
      ))}
      {children}
    </div>
  );
}
