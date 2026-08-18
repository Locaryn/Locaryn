import { Icon, type IconName, isIconName } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { AudioGenPanel } from "../components/AudioGenPanel";
import { ImageGenPanel } from "../components/ImageGenPanel";
import { Model3DPanel } from "../components/Model3DPanel";
import { MusicGenPanel } from "../components/MusicGenPanel";
import { RegionEditPanel } from "../components/RegionEditPanel";
import { VideoGenPanel } from "../components/VideoGenPanel";
import { DynamicPluginWidget } from "../components/extensions/DynamicPluginWidget";
import {
  type ResolvedSlotContribution,
  getSlotContributions,
} from "../components/extensions/SlotRegistry";
import { type InstalledExtension, core } from "../lib/core";
import { taskCenter, useTasks } from "../lib/taskCenter";

type StudioTab =
  | "image"
  | "video"
  | "audio"
  | "music"
  | "3d"
  | "image-editing"
  | "object-detection"
  | "translation"
  | "text-analysis"
  | "question-answering";

const TABS: { id: StudioTab; label: string; icon: IconName }[] = [
  { id: "image", label: "Image", icon: "studio" },
  { id: "audio", label: "Synthèse vocale", icon: "mic" },
  { id: "music", label: "Musique", icon: "music" },
  { id: "video", label: "Vidéo", icon: "video" },
  { id: "3d", label: "3D", icon: "cube" },
  { id: "image-editing", label: "Édition image", icon: "edit" },
  { id: "object-detection", label: "Détection", icon: "target" },
  { id: "translation", label: "Traduction", icon: "translate" },
  { id: "text-analysis", label: "Analyse texte", icon: "chart" },
  { id: "question-answering", label: "Q&R", icon: "question" },
];

/** Capacité requise par chaque panneau natif du Studio. Un onglet n'est pas
 * une promesse générale de l'application : il apparaît seulement si une
 * extension active fournit réellement cette capacité. */
const TAB_CAPABILITIES: Record<string, string> = {
  image: "image-gen",
  audio: "voice-tts",
  music: "music-gen",
  video: "video-gen",
  "3d": "3d-gen",
  "image-editing": "image-editor",
  "object-detection": "vision-ocr",
  translation: "translation",
  "text-analysis": "text-analysis",
  "question-answering": "rag-qa",
};

type Props = {
  installedModels: string[];
  installedImageModels: string[];
  onOpenImageGen?: () => void;
  onCloseAudioGen?: () => void;
  /** Send an image from the gallery to the active chat session. */
  onSendImageToChat?: (url: string, label: string) => void;
  /** Extensions actives : leurs `studio_tabs` s'ajoutent aux onglets, sans
   *  jamais recouvrir un onglet natif. */
  extensions?: InstalledExtension[];
};

/**
 * Studio view — single entry point for all generative categories inspired by
 * the HuggingFace Spaces directory. Most tabs start as placeholders exposing
 * the model registry; the actively wired tabs are Image and Audio (TTS).
 */
export function StudioView({
  installedModels,
  installedImageModels,
  onOpenImageGen,
  onCloseAudioGen,
  onSendImageToChat,
  extensions = [],
}: Props) {
  const [active, setActive] = useState<string>("");

  // Le socle d'abord : les onglets natifs sont filtrés par les capacités des
  // extensions actives. Une extension peut aussi déclarer explicitement un
  // onglet natif ; dans ce cas sa déclaration suffit à l'afficher. Les onglets
  // personnalisés sont ensuite ajoutés sans jamais recouvrir un id natif.
  const onglets = useMemo(() => {
    const slotContributions = getSlotContributions(extensions, "studio.tabs");
    const extensionTabIds = new Set(
      extensions.flatMap((ext) => (ext.ui?.studio_tabs ?? []).map((t) => t.id)),
    );
    const capabilities = new Set(extensions.flatMap((ext) => ext.capabilities ?? []));
    const natifs = TABS.filter((tab) => {
      const required = TAB_CAPABILITIES[tab.id];
      return !required || capabilities.has(required) || extensionTabIds.has(tab.id);
    });
    const pris = new Set<string>(natifs.map((t) => t.id));
    const depuisSlots = slotContributions.flatMap((t) => {
      if (pris.has(t.id)) return [];
      pris.add(t.id);
      return [
        {
          id: t.id,
          label: t.label || t.id,
          icon: (isIconName(t.icon) ? t.icon : "extensions") as IconName,
          source: t.extensionName,
          contribution: t,
        },
      ];
    });

    return [...natifs, ...depuisSlots] as {
      id: string;
      label: string;
      icon: IconName;
      source?: string;
      contribution?: ResolvedSlotContribution;
    }[];
  }, [extensions]);

  // Si l'extension active change, ne pas rester sur un panneau retiré : on
  // sélectionne le premier onglet encore disponible.
  useEffect(() => {
    setActive((current) =>
      onglets.some((t) => t.id === current) ? current : (onglets[0]?.id ?? ""),
    );
  }, [onglets]);

  // ── Galleries par type ───────────────────────────────────────────────
  const tasks = useTasks();

  const galleryItems = useMemo(() => {
    return tasks
      .filter((t) => {
        const isImageTask = t.type === "generation" || t.type === "edit";
        return (
          isImageTask &&
          t.status === "done" &&
          !!t.resultImageUrl &&
          !t.resultImageUrl.startsWith("data:")
        );
      })
      .map((t) => ({
        id: t.id,
        url: t.resultImageUrl!,
        path: t.resultPath,
        label: t.label,
        detail: t.detail,
        mediaKind: "image" as const,
      }));
  }, [tasks]);

  const audioItems = useMemo(() => {
    return tasks
      .filter(
        (t) =>
          t.type === "audio" &&
          t.status === "done" &&
          !!t.resultAudioUrl &&
          t.label?.startsWith("TTS"),
      )
      .map((t) => ({
        id: t.id,
        url: t.resultAudioUrl!,
        path: t.resultPath,
        label: t.label,
        detail: t.detail,
        mediaKind: "audio" as const,
      }));
  }, [tasks]);

  const musicItems = useMemo(() => {
    return tasks
      .filter(
        (t) =>
          t.type === "audio" &&
          t.status === "done" &&
          !!t.resultAudioUrl &&
          t.label?.startsWith("Musique"),
      )
      .map((t) => ({
        id: t.id,
        url: t.resultAudioUrl!,
        path: t.resultPath,
        label: t.label,
        detail: t.detail,
        mediaKind: "audio" as const,
      }));
  }, [tasks]);

  const videoItems = useMemo(() => {
    return tasks
      .filter(
        (t) =>
          t.type === "generation" &&
          t.status === "done" &&
          !!t.resultAudioUrl &&
          t.label?.startsWith("Vidéo"),
      )
      .map((t) => ({
        id: t.id,
        url: t.resultAudioUrl!,
        path: t.resultPath,
        label: t.label,
        detail: t.detail,
        mediaKind: "video" as const,
      }));
  }, [tasks]);

  const model3dItems = useMemo(() => {
    return tasks
      .filter(
        (t) =>
          t.type === "generation" &&
          t.status === "done" &&
          !!t.resultAudioUrl &&
          t.label?.startsWith("3D"),
      )
      .map((t) => ({
        id: t.id,
        url: t.resultAudioUrl!,
        path: t.resultPath,
        label: t.label,
        detail: t.detail,
        mediaKind: "model3d" as const,
      }));
  }, [tasks]);

  interface GalleryItem {
    id: string;
    url: string;
    path?: string;
    label: string;
    detail?: string;
    mediaKind: "image" | "audio" | "video" | "model3d";
  }

  /** Shared gallery card rendering for all media types. */
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
            title="Supprimer toutes les entrées de la galerie"
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
                transition: "transform 0.15s, box-shadow 0.15s",
              }}
              onMouseEnter={(e) => {
                const el = e.currentTarget as HTMLElement;
                el.style.transform = "scale(1.02)";
                el.style.boxShadow = "0 4px 12px rgba(0,0,0,0.3)";
              }}
              onMouseLeave={(e) => {
                const el = e.currentTarget as HTMLElement;
                el.style.transform = "";
                el.style.boxShadow = "";
              }}
            >
              {/* Media preview */}
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
                    fontSize: 28,
                    color: "var(--text-faint)",
                  }}
                >
                  <Icon name="cube" size={16} />
                </div>
              )}
              {/* Label + download */}
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
                    flex: 1,
                  }}
                >
                  {item.detail ?? "Terminé"}
                </span>
                <a
                  href={item.url}
                  download={item.label}
                  title={item.path ? "Télécharger le fichier" : "Ouvrir"}
                  style={{
                    fontSize: 11,
                    color: "var(--accent)",
                    textDecoration: "none",
                    flex: "none",
                  }}
                  target="_blank"
                  rel="noreferrer"
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

  function renderPlaceholder(title: string, description: string) {
    return (
      <div className="locaryn-card" style={{ padding: 40, textAlign: "center" }}>
        <div style={{ fontSize: 48, marginBottom: 16 }} />
        <h3 style={{ marginBottom: 12 }}>{title}</h3>
        <p className="locaryn-field-hint" style={{ maxWidth: 520, margin: "0 auto" }}>
          {description}
        </p>
        <p className="locaryn-field-hint" style={{ marginTop: 20 }}>
          Les modèles correspondants sont listés dans le Marketplace avec le filtre approprié.
        </p>
      </div>
    );
  }

  function renderContent() {
    switch (active) {
      case "image":
        return (
          <>
            <ImageGenPanel
              installedModels={installedImageModels}
              inline
              onClose={() => {}}
              onInstallRequested={async (tag, consent) => {
                const providers = await core.listProviders();
                const active = providers.find((p) => p.is_active) ?? providers[0];
                if (!active) return;
                await core.pullModel(active.endpoint, tag, undefined, undefined, consent);
              }}
            />

            {/* ── Galerie des images générées ── */}
            {galleryItems.length > 0 && (
              <div className="locaryn-card" style={{ marginTop: 24, padding: 16 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
                  <h4 style={{ margin: 0, fontSize: 14, fontWeight: 600, flex: 1 }}>
                    <Icon name="image" size={15} /> Galerie ({galleryItems.length})
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
                    title="Supprimer toutes les entrées de la galerie"
                  >
                    <Icon name="trash" size={15} /> Tout effacer
                  </button>
                </div>
                <div
                  style={{
                    display: "grid",
                    gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
                    gap: 10,
                  }}
                >
                  {galleryItems.map((item) => (
                    <div
                      key={item.id}
                      style={{
                        position: "relative",
                        borderRadius: 8,
                        overflow: "hidden",
                        border: "1px solid var(--border)",
                        background: "var(--bg-alt)",
                        transition: "transform 0.15s, box-shadow 0.15s",
                        cursor: "grab",
                      }}
                      draggable
                      onDragStart={(e) => {
                        e.dataTransfer.setData("text/plain", item.url);
                        if (item.path)
                          e.dataTransfer.setData("text/x-locaryn-image-path", item.path);
                        e.dataTransfer.effectAllowed = "copy";
                      }}
                      onMouseEnter={(e) => {
                        const el = e.currentTarget as HTMLElement;
                        el.style.transform = "scale(1.03)";
                        el.style.boxShadow = "0 4px 12px rgba(0,0,0,0.3)";
                      }}
                      onMouseLeave={(e) => {
                        const el = e.currentTarget as HTMLElement;
                        el.style.transform = "";
                        el.style.boxShadow = "";
                      }}
                    >
                      <a
                        href={item.url}
                        target="_blank"
                        rel="noreferrer"
                        title={item.label}
                        style={{ display: "block", textDecoration: "none", color: "inherit" }}
                        onClick={(e) => e.stopPropagation()}
                      >
                        <img
                          src={item.url}
                          alt={item.label}
                          style={{
                            width: "100%",
                            height: 120,
                            objectFit: "cover",
                            display: "block",
                          }}
                        />
                        <div
                          style={{
                            padding: "6px 8px",
                            fontSize: 11,
                            color: "var(--text-faint)",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {item.detail ?? "Terminé"}
                        </div>
                      </a>
                      {/* Bouton Envoyer au chat — visible au survol */}
                      {onSendImageToChat && (
                        <button
                          type="button"
                          title="Envoyer dans le chat actif"
                          onClick={(e) => {
                            e.stopPropagation();
                            onSendImageToChat(item.url, item.label);
                          }}
                          className="img-gallery-send-btn"
                        >
                          <Icon name="chat" size={15} /> Chat
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        );
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
      case "image-editing":
        return <RegionEditPanel installedModels={installedModels} />;
      case "object-detection":
        return renderPlaceholder(
          "Détection d'objets",
          "Détection, segmentation et annotation d'objets dans des images et vidéos. Nécessite YOLO, DETR ou SAM.",
        );
      case "translation":
        return renderPlaceholder(
          "Traduction automatique",
          "Traduction de texte et de documents entre de nombreuses langues. Nécessite NLLB, M2M-100 ou Opus-MT.",
        );
      case "text-analysis":
        return renderPlaceholder(
          "Analyse de texte",
          "Classification de sentiment, reconnaissance d'entités (NER), embeddings et analyse sémantique.",
        );
      case "question-answering":
        return renderPlaceholder(
          "Question-réponse",
          "Réponses précises à partir d'un corpus de documents ou d'un contexte donné.",
        );
      default: {
        const onglet = onglets.find((t) => t.id === active);
        if (
          onglet?.contribution &&
          (onglet.contribution.entry ||
            onglet.contribution.tag ||
            onglet.contribution.type === "custom-element")
        ) {
          return (
            <div className="locaryn-card" style={{ padding: "24px" }}>
              <DynamicPluginWidget contribution={onglet.contribution} />
            </div>
          );
        }
        return renderPlaceholder(
          onglet?.label ?? active,
          onglet?.source
            ? `Onglet apporté par ${onglet.source} — le contenu vit dans l'extension.`
            : "Onglet inconnu.",
        );
      }
    }
  }

  if (onglets.length === 0) {
    return (
      <div className="locaryn-view-container">
        <div className="locaryn-view-header" style={{ flexShrink: 0 }}>
          <h2>Studio de génération</h2>
          <p className="locaryn-view-desc">
            Le mode Studio regroupe les outils de génération multimodale (images, voix, musique,
            vidéo, 3D).
          </p>
        </div>
        <div
          className="locaryn-card"
          style={{
            padding: "48px 24px",
            textAlign: "center",
            maxWidth: "540px",
            margin: "40px auto",
          }}
        >
          <div style={{ display: "flex", justifyContent: "center", marginBottom: "16px" }}>
            <Icon name="studio" size={40} />
          </div>
          <h3 style={{ marginBottom: "8px", fontSize: "16px", fontWeight: 600 }}>
            Aucun module de Studio installé
          </h3>
          <p className="locaryn-field-hint" style={{ margin: "0 auto", lineHeight: 1.5 }}>
            Le Studio s'active automatiquement dès qu'un plugin multimodal est installé (Génération
            d'images, Synthèse vocale, Musique, Vidéo ou 3D). Rendez-vous dans la section{" "}
            <strong>Extensions</strong> pour en ajouter un.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="locaryn-view-container">
      <div className="locaryn-view-header" style={{ flexShrink: 0 }}>
        <h2>Studio de génération</h2>
        <p className="locaryn-view-desc">
          Tous les outils de génération multimodaux locaux réunis dans un espace unique.
        </p>
      </div>

      <div
        className="locaryn-studio-tabs"
        style={{
          display: "flex",
          gap: "6px",
          overflowX: "auto",
          paddingBottom: "8px",
          marginBottom: "16px",
          flexShrink: 0,
        }}
      >
        {onglets.map((tab) => {
          const isActive = active === tab.id;
          return (
            <button
              key={tab.id}
              type="button"
              className={`locaryn-chip${isActive ? " locaryn-chip-on" : ""}`}
              onClick={() => setActive(tab.id)}
              style={{
                whiteSpace: "nowrap",
                fontSize: "12px",
                ...(isActive
                  ? { background: "var(--accent)", color: "#fff", borderColor: "var(--accent)" }
                  : {}),
              }}
            >
              <span style={{ marginRight: 4, display: "inline-flex" }}>
                <Icon name={tab.icon} size={15} />
              </span>
              {tab.label}
            </button>
          );
        })}
      </div>

      {renderContent()}
    </div>
  );
}
