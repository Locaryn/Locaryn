import { Icon, type IconName, isIconName } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { DynamicPluginWidget } from "../components/extensions/DynamicPluginWidget";
import {
  type ResolvedSlotContribution,
  getSlotContributions,
} from "../components/extensions/SlotRegistry";
import type { InstalledExtension } from "../lib/core";

type Props = {
  installedModels?: string[];
  onCloseAudioGen?: () => void;
  extensions?: InstalledExtension[];
};

/**
 * Pure dynamic Studio View.
 * The host supplies only the generic Studio container and tab switcher.
 * All functional modules (Image, 3D, Music, Video, Voice, Translation, etc.)
 * are contributed dynamically by extensions via the `studio.tabs` slot.
 */
export function StudioView({ extensions = [] }: Props) {
  const [active, setActive] = useState<string>("");

  const tabs = useMemo(() => {
    const slotContributions = getSlotContributions(extensions, "studio.tabs");
    const used = new Set<string>();

    return slotContributions.flatMap((contribution) => {
      if (used.has(contribution.id)) return [];
      used.add(contribution.id);
      return [
        {
          id: contribution.id,
          label: contribution.label || contribution.id,
          icon: (isIconName(contribution.icon) ? contribution.icon : "extensions") as IconName,
          source: contribution.extensionName,
          contribution,
        },
      ];
    }) as Array<{
      id: string;
      label: string;
      icon: IconName;
      source?: string;
      contribution: ResolvedSlotContribution;
    }>;
  }, [extensions]);

  useEffect(() => {
    setActive((current) =>
      tabs.some((tab) => tab.id === current) ? current : (tabs[0]?.id ?? ""),
    );
  }, [tabs]);

  function placeholder(title: string, description: string) {
    return (
      <div className="locaryn-card" style={{ padding: 40, textAlign: "center" }}>
        <h3 style={{ marginBottom: 12 }}>{title}</h3>
        <p className="locaryn-field-hint" style={{ maxWidth: 520, margin: "0 auto" }}>
          {description}
        </p>
      </div>
    );
  }

  function renderContent() {
    const tab = tabs.find((candidate) => candidate.id === active);
    if (!tab) {
      return placeholder(
        "Module introuvable",
        "Veuillez sélectionner un module dans la liste ci-dessus.",
      );
    }

    const providesCustomElement =
      tab.contribution.type === "custom-element" && !!tab.contribution.entry;

    if (providesCustomElement) {
      return (
        <div style={{ width: "100%" }}>
          <DynamicPluginWidget contribution={tab.contribution} />
        </div>
      );
    }

    return placeholder(
      tab.label,
      tab.source
        ? `${tab.source} déclare cet onglet mais ne fournit pas d'interface custom-element.`
        : "Module sans interface personnalisée.",
    );
  }

  if (tabs.length === 0) {
    return (
      <div className="locaryn-view-container">
        <div className="locaryn-view-header">
          <h2>Studio</h2>
          <p className="locaryn-view-desc">
            Installez une extension officielle ou communautaire pour ajouter un module au Studio.
          </p>
        </div>
        <div
          className="locaryn-card"
          style={{ padding: 48, textAlign: "center", maxWidth: 540, margin: "40px auto" }}
        >
          <Icon name="studio" size={40} />
          <h3 style={{ margin: "16px 0 8px" }}>Aucun module de Studio actif</h3>
          <p className="locaryn-field-hint">
            Les extensions (Image, Musique, Vidéo, 3D, Voix, etc.) contribuent dynamiquement leurs
            propres onglets et interfaces.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="locaryn-view-container">
      <div className="locaryn-view-header" style={{ flexShrink: 0 }}>
        <h2>Studio</h2>
        <p className="locaryn-view-desc">
          Les extensions actives ajoutent leurs propres outils et interfaces multimodales.
        </p>
      </div>
      <div
        className="locaryn-studio-tabs"
        style={{
          display: "flex",
          gap: 6,
          overflowX: "auto",
          paddingBottom: 8,
          marginBottom: 16,
          flexShrink: 0,
        }}
      >
        {tabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className={`locaryn-chip${active === tab.id ? " locaryn-chip-on" : ""}`}
            onClick={() => setActive(tab.id)}
            style={{ whiteSpace: "nowrap", fontSize: 12 }}
          >
            <span style={{ marginRight: 4, display: "inline-flex" }}>
              <Icon name={tab.icon} size={15} />
            </span>
            {tab.label}
          </button>
        ))}
      </div>
      {renderContent()}
    </div>
  );
}
