import { useMemo, useState } from "react";
import { AudioGenPanel } from "../components/AudioGenPanel";
import { ImageGenPanel } from "../components/ImageGenPanel";
import { Model3DPanel } from "../components/Model3DPanel";
import { MusicGenPanel } from "../components/MusicGenPanel";
import { RegionEditPanel } from "../components/RegionEditPanel";
import { VideoGenPanel } from "../components/VideoGenPanel";
import { core } from "../lib/core";
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

const TABS: { id: StudioTab; label: string; icon: string }[] = [
  { id: "image", label: "Image", icon: "🎨" },
  { id: "audio", label: "Synthèse vocale", icon: "🎙️" },
  { id: "music", label: "Musique", icon: "🎵" },
  { id: "video", label: "Vidéo", icon: "🎬" },
  { id: "3d", label: "3D", icon: "🧩" },
  { id: "image-editing", label: "Édition image", icon: "✏️" },
  { id: "object-detection", label: "Détection", icon: "🎯" },
  { id: "translation", label: "Traduction", icon: "🌐" },
  { id: "text-analysis", label: "Analyse texte", icon: "📊" },
  { id: "question-answering", label: "Q&R", icon: "❓" },
];

type Props = {
  installedModels: string[];
  installedImageModels: string[];
  onOpenImageGen?: () => void;
  onCloseAudioGen?: () => void;
  /** Switch to the Marketplace (used when no model is installed). */
  onOpenMarketplace?: () => void;
  /** Send an image from the gallery to the active chat session. */
  onSendImageToChat?: (url: string, label: string) => void;
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
  onOpenMarketplace,
  onSendImageToChat,
}: Props) {
  const [active, setActive] = useState<StudioTab>("image");

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
  function renderGallery(items: GalleryItem[], icon: string, title: string) {
    if (items.length === 0) return null;
    return (
      <div className="locaryn-card" style={{ marginTop: 24, padding: 16 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 12 }}>
          <h4 style={{ margin: 0, fontSize: 14, fontWeight: 600, flex: 1 }}>
            {icon} {title} ({items.length})
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
            🗑️ Tout effacer
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
                // biome-ignore lint/a11y/useMediaCaption: média produit par le modèle sur cette machine ; aucune piste de sous-titres n'existe, et en fabriquer une vide n'aiderait personne.
                <audio
                  src={item.url}
                  controls
                  preload="none"
                  style={{ width: "100%", display: "block" }}
                />
              )}
              {item.mediaKind === "video" && (
                // biome-ignore lint/a11y/useMediaCaption: média produit par le modèle sur cette machine ; aucune piste de sous-titres n'existe, et en fabriquer une vide n'aiderait personne.
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
                  🧊
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
                  ⬇
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
        <button
          type="button"
          className="img-gen-install-btn"
          style={{ marginTop: 16 }}
          onClick={() => onOpenMarketplace?.()}
          disabled={!onOpenMarketplace}
        >
          🛒 Aller au Marketplace
        </button>
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
              onOpenMarketplace={onOpenMarketplace}
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
                    🖼️ Galerie ({galleryItems.length})
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
                    🗑️ Tout effacer
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
                          💬 Chat
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
              onOpenMarketplace={onOpenMarketplace}
            />
            {renderGallery(audioItems, "🎙️", "Synthèses vocales")}
          </>
        );
      case "video":
        return (
          <>
            <VideoGenPanel
              installedModels={installedModels}
              onClose={onCloseAudioGen ?? (() => {})}
              inline
              onOpenMarketplace={onOpenMarketplace}
            />
            {renderGallery(videoItems, "🎬", "Vidéos générées")}
          </>
        );
      case "music":
        return (
          <>
            <MusicGenPanel
              installedModels={installedModels}
              onClose={onCloseAudioGen ?? (() => {})}
              inline
              onOpenMarketplace={onOpenMarketplace}
            />
            {renderGallery(musicItems, "🎵", "Musiques générées")}
          </>
        );
      case "3d":
        return (
          <>
            <Model3DPanel
              installedModels={installedModels}
              onClose={onCloseAudioGen ?? (() => {})}
              inline
              onOpenMarketplace={onOpenMarketplace}
            />
            {renderGallery(model3dItems, "🧊", "Modèles 3D")}
          </>
        );
      case "image-editing":
        return (
          <RegionEditPanel
            installedModels={installedModels}
            onOpenMarketplace={onOpenMarketplace}
          />
        );
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
      default:
        return null;
    }
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
        {TABS.map((tab) => {
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
              <span style={{ marginRight: 4 }}>{tab.icon}</span>
              {tab.label}
            </button>
          );
        })}
      </div>

      {renderContent()}
    </div>
  );
}
