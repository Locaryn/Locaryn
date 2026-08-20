import { Icon, type IconName, isIconName } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { AudioGenPanel } from "../components/AudioGenPanel";
import { Model3DPanel } from "../components/Model3DPanel";
import { MusicGenPanel } from "../components/MusicGenPanel";
import { VideoGenPanel } from "../components/VideoGenPanel";
import { DynamicPluginWidget } from "../components/extensions/DynamicPluginWidget";
import {
  type ResolvedSlotContribution,
  getSlotContributions,
} from "../components/extensions/SlotRegistry";
import type { InstalledExtension } from "../lib/core";
import { taskCenter, useTasks } from "../lib/taskCenter";

type StudioTab =
  | "video"
  | "audio"
  | "music"
  | "3d"
  | "object-detection"
  | "translation"
  | "text-analysis"
  | "question-answering";

const TABS: { id: StudioTab; label: string; icon: IconName }[] = [
  { id: "audio", label: "Synthèse vocale", icon: "mic" },
  { id: "music", label: "Musique", icon: "music" },
  { id: "video", label: "Vidéo", icon: "video" },
  { id: "3d", label: "3D", icon: "cube" },
  { id: "object-detection", label: "Détection", icon: "target" },
  { id: "translation", label: "Traduction", icon: "translate" },
  { id: "text-analysis", label: "Analyse texte", icon: "chart" },
  { id: "question-answering", label: "Q&R", icon: "question" },
];

const TAB_CAPABILITIES: Record<string, string> = {
  audio: "voice-tts",
  music: "music-gen",
  video: "video-gen",
  "3d": "3d-gen",
  "object-detection": "vision-ocr",
  translation: "translation",
  "text-analysis": "text-analysis",
  "question-answering": "rag-qa",
};

type Props = {
  installedModels: string[];
  onCloseAudioGen?: () => void;
  extensions?: InstalledExtension[];
};

interface GalleryItem {
  id: string;
  url: string;
  path?: string;
  label: string;
  detail?: string;
  mediaKind: "audio" | "video" | "model3d";
}

/**
 * The host supplies only generic Studio slots. Image generation is not a
 * native Locaryn feature; plugin-image-gen contributes its own tab and UI.
 */
export function StudioView({ installedModels, onCloseAudioGen, extensions = [] }: Props) {
  const [active, setActive] = useState<string>("");
  const tasks = useTasks();

  const tabs = useMemo(() => {
    const slotContributions = getSlotContributions(extensions, "studio.tabs");
    const extensionTabIds = new Set(
      extensions.flatMap((ext) => (ext.ui?.studio_tabs ?? []).map((tab) => tab.id)),
    );
    const capabilities = new Set(extensions.flatMap((ext) => ext.capabilities ?? []));
    const nativeTabs = TABS.filter((tab) => {
      const required = TAB_CAPABILITIES[tab.id];
      return !required || capabilities.has(required) || extensionTabIds.has(tab.id);
    });
    const used = new Set<string>(nativeTabs.map((tab) => tab.id));
    const pluginTabs = slotContributions.flatMap((contribution) => {
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
    });
    return [...nativeTabs, ...pluginTabs] as Array<{
      id: string;
      label: string;
      icon: IconName;
      source?: string;
      contribution?: ResolvedSlotContribution;
    }>;
  }, [extensions]);

  useEffect(() => {
    setActive((current) =>
      tabs.some((tab) => tab.id === current) ? current : (tabs[0]?.id ?? ""),
    );
  }, [tabs]);

  const audioItems = useMemo<GalleryItem[]>(
    () =>
      tasks
        .filter(
          (task) =>
            task.type === "audio" &&
            task.status === "done" &&
            !!task.resultAudioUrl &&
            task.label?.startsWith("TTS"),
        )
        .map((task) => ({
          id: task.id,
          url: task.resultAudioUrl!,
          path: task.resultPath,
          label: task.label,
          detail: task.detail,
          mediaKind: "audio",
        })),
    [tasks],
  );

  const musicItems = useMemo<GalleryItem[]>(
    () =>
      tasks
        .filter(
          (task) =>
            task.type === "audio" &&
            task.status === "done" &&
            !!task.resultAudioUrl &&
            task.label?.startsWith("Musique"),
        )
        .map((task) => ({
          id: task.id,
          url: task.resultAudioUrl!,
          path: task.resultPath,
          label: task.label,
          detail: task.detail,
          mediaKind: "audio",
        })),
    [tasks],
  );

  const videoItems = useMemo<GalleryItem[]>(
    () =>
      tasks
        .filter(
          (task) =>
            task.type === "generation" &&
            task.status === "done" &&
            !!task.resultAudioUrl &&
            task.label?.startsWith("Vidéo"),
        )
        .map((task) => ({
          id: task.id,
          url: task.resultAudioUrl!,
          path: task.resultPath,
          label: task.label,
          detail: task.detail,
          mediaKind: "video",
        })),
    [tasks],
  );

  const model3dItems = useMemo<GalleryItem[]>(
    () =>
      tasks
        .filter(
          (task) =>
            task.type === "generation" &&
            task.status === "done" &&
            !!task.resultAudioUrl &&
            task.label?.startsWith("3D"),
        )
        .map((task) => ({
          id: task.id,
          url: task.resultAudioUrl!,
          path: task.resultPath,
          label: task.label,
          detail: task.detail,
          mediaKind: "model3d",
        })),
    [tasks],
  );

  function renderGallery(items: GalleryItem[], icon: IconName, title: string) {
    if (items.length === 0) return null;
    return (
      <div className="locaryn-card" style={{ marginTop: 24, padding: 16 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
          <h4 style={{ margin: 0, fontSize: 14, fontWeight: 600, flex: 1 }}>
            <Icon name={icon} size={16} /> {title} ({items.length})
          </h4>
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{
              fontSize: 11,
              padding: "3px 10px",
              color: "var(--danger)",
              borderColor: "var(--danger)",
            }}
            onClick={() => taskCenter.clearGallery()}
          >
            <Icon name="trash" size={15} /> Tout effacer
          </button>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
            gap: 12,
          }}
        >
          {items.map((item) => (
            <div
              key={item.id}
              style={{
                borderRadius: 8,
                overflow: "hidden",
                border: "1px solid var(--border)",
                background: "var(--bg-alt)",
              }}
            >
              {item.mediaKind === "audio" && (
                <audio
                  src={item.url}
                  controls
                  preload="none"
                  style={{ width: "100%", display: "block" }}
                />
              )}
              {item.mediaKind === "video" && (
                <video
                  src={item.url}
                  controls
                  preload="metadata"
                  style={{ width: "100%", display: "block", maxHeight: 160 }}
                />
              )}
              {item.mediaKind === "model3d" && (
                <div
                  style={{
                    height: 80,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    background: "var(--surface)",
                    color: "var(--text-faint)",
                  }}
                >
                  <Icon name="cube" size={28} />
                </div>
              )}
              <div
                style={{
                  padding: "8px 10px",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 8,
                }}
              >
                <span
                  style={{
                    fontSize: 11,
                    color: "var(--text-faint)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {item.detail ?? "Terminé"}
                </span>
                <a
                  href={item.url}
                  download={item.label}
                  target="_blank"
                  rel="noreferrer"
                  style={{ fontSize: 11, color: "var(--accent)", textDecoration: "none" }}
                >
                  Télécharger
                </a>
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

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
    switch (active) {
      case "audio":
        return (
          <>
            <AudioGenPanel
              installedModels={installedModels}
              onClose={onCloseAudioGen ?? (() => {})}
              inline
            />
            {renderGallery(audioItems, "mic", "Synthèses vocales")}
          </>
        );
      case "video":
        return (
          <>
            <VideoGenPanel
              installedModels={installedModels}
              onClose={onCloseAudioGen ?? (() => {})}
              inline
            />
            {renderGallery(videoItems, "video", "Vidéos générées")}
          </>
        );
      case "music":
        return (
          <>
            <MusicGenPanel
              installedModels={installedModels}
              onClose={onCloseAudioGen ?? (() => {})}
              inline
            />
            {renderGallery(musicItems, "music", "Musiques générées")}
          </>
        );
      case "3d":
        return (
          <>
            <Model3DPanel
              installedModels={installedModels}
              onClose={onCloseAudioGen ?? (() => {})}
              inline
            />
            {renderGallery(model3dItems, "cube", "Modèles 3D")}
          </>
        );
      case "object-detection":
        return placeholder(
          "Détection d'objets",
          "Détection, segmentation et annotation d'objets dans des images et vidéos.",
        );
      case "translation":
        return placeholder(
          "Traduction automatique",
          "Traduction de texte et de documents entre de nombreuses langues.",
        );
      case "text-analysis":
        return placeholder(
          "Analyse de texte",
          "Classification, reconnaissance d'entités et analyse sémantique.",
        );
      case "question-answering":
        return placeholder(
          "Question-réponse",
          "Réponses à partir d'un corpus de documents ou d'un contexte donné.",
        );
      default: {
        const tab = tabs.find((candidate) => candidate.id === active);
        // Seule une contribution qui apporte sa propre interface a un panneau.
        // Un onglet déclaré à l'ancienne (`studio_tabs`) n'en a pas : sans ce
        // filtre, le corps de l'onglet affichait le bouton d'action générique,
        // seul au milieu de la page.
        const fournitUnPanneau =
          tab?.contribution?.type === "custom-element" && !!tab.contribution.entry;
        return fournitUnPanneau && tab?.contribution ? (
          <div style={{ width: "100%" }}>
            <DynamicPluginWidget contribution={tab.contribution} />
          </div>
        ) : (
          placeholder(
            tab?.label ?? active,
            tab?.source
              ? `${tab.source} annonce cet onglet mais ne fournit pas d'interface pour le remplir.`
              : "Onglet inconnu.",
          )
        );
      }
    }
  }

  if (tabs.length === 0) {
    return (
      <div className="locaryn-view-container">
        <div className="locaryn-view-header">
          <h2>Studio</h2>
          <p className="locaryn-view-desc">
            Installez une extension multimodale pour ajouter un module au Studio.
          </p>
        </div>
        <div
          className="locaryn-card"
          style={{ padding: 48, textAlign: "center", maxWidth: 540, margin: "40px auto" }}
        >
          <Icon name="studio" size={40} />
          <h3 style={{ margin: "16px 0 8px" }}>Aucun module de Studio installé</h3>
          <p className="locaryn-field-hint">
            Les extensions ajoutent leurs propres onglets et leur interface.
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
          Les extensions actives ajoutent leurs propres outils multimodaux.
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
