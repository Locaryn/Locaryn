import { Icon } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { core } from "../lib/core";
import { dedupeModelsByDirectory } from "../lib/modelList";
import {
  IMAGE_GEN_MODELS,
  type ModelFamily,
  SEED_CATALOG,
  classifyModel,
  fetchFullRegistry,
  isChatModel,
  looksLikeImageModel,
} from "../lib/modelRegistry";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  activeModel: string;
  installedModels?: string[];
  isProviderLocal?: boolean;
  onSelectModel: (tag: string) => Promise<void> | void;
  onOpenMarketplace?: () => void;
};

export interface ModelOptionItem {
  tag: string;
  name: string;
  brand: string;
  isLocal: boolean;
  size: string;
  category: "text" | "image" | "code" | "reasoning" | "vision";
  categoryLabel: string;
  icon: string;
}

export function QuickModelSelector({
  isOpen,
  onClose,
  activeModel,
  installedModels = [],
  isProviderLocal = true,
  onSelectModel,
  onOpenMarketplace,
}: Props) {
  const [query, setQuery] = useState("");
  const [activeTab, setActiveTab] = useState<"all" | "text" | "code" | "reasoning" | "vision">(
    "all",
  );
  const [registry, setRegistry] = useState<ModelFamily[]>([...SEED_CATALOG, ...IMAGE_GEN_MODELS]);

  useEffect(() => {
    if (!isOpen) return;
    fetchFullRegistry((q, cat) => core.searchOllamaLibrary(q, cat))
      .then((res) => setRegistry(res.families))
      .catch(() => {});
  }, [isOpen]);

  const dedupedModels = useMemo(
    () =>
      dedupeModelsByDirectory(
        installedModels.length > 0 ? installedModels : ([activeModel].filter(Boolean) as string[]),
      ),
    [installedModels, activeModel],
  );

  // Les dépendances suivent ce que le mémo lit réellement : `dedupedModels`.
  // Déclarer ses deux sources à la place le faisait recalculer sans raison, et
  // manquait le cas où la déduplication change sans qu'elles bougent.
  const options = useMemo<ModelOptionItem[]>(() => {
    const tagsToList = dedupedModels;

    return (
      tagsToList
        .map((tag) => {
          let match: { name: string; brand: string; size: string; family: ModelFamily } | null =
            null;

          for (const family of registry) {
            for (const variant of family.variants) {
              if (
                variant.tag === tag ||
                `${variant.tag}:latest` === tag ||
                variant.tag === `${tag}:latest`
              ) {
                match = {
                  name: `${family.name} (${variant.size})`,
                  brand: family.brand,
                  size: variant.size,
                  family,
                };
                break;
              }
            }
            if (match) break;
          }

          // Use the canonical classifier (centralised in modelRegistry)
          const { kind } = classifyModel(tag);

          let category: ModelOptionItem["category"] = "text";
          let categoryLabel = "💬 Text to Text";
          let icon = "💬";

          if (kind === "image-gen") {
            category = "image";
            categoryLabel = "🎨 Text to Image";
            icon = "🎨";
          } else if (kind === "code") {
            category = "code";
            categoryLabel = "💻 Code & Dev";
            icon = "💻";
          } else if (kind === "reasoning") {
            category = "reasoning";
            categoryLabel = "🧠 Raisonnement";
            icon = "🧠";
          } else if (kind === "vision") {
            category = "vision";
            categoryLabel = "🖼️ Vision";
            icon = "🖼️";
          }

          const isRemoteTag =
            tag.includes("openrouter") || tag.includes("openai") || tag.includes("cloud");
          const isLocal = isProviderLocal && !isRemoteTag;

          return {
            tag,
            name: match ? match.name : tag,
            brand: match ? match.brand : "Modèle Local",
            isLocal,
            size: match ? match.size : "",
            category,
            categoryLabel,
            icon,
          };
        })
        // Only offer models suitable for chat (LLM / vision / code / reasoning).
        // Exclude image-gen, TTS, music, video, 3D, etc.
        .filter((o) => isChatModel(o.tag))
    );
  }, [dedupedModels, isProviderLocal, registry]);

  const filtered = useMemo(() => {
    let result = options;

    if (activeTab !== "all") {
      result = result.filter((o) => o.category === activeTab);
    }

    const q = query.trim().toLowerCase();
    if (q) {
      result = result.filter(
        (o) =>
          o.tag.toLowerCase().includes(q) ||
          o.name.toLowerCase().includes(q) ||
          o.brand.toLowerCase().includes(q) ||
          o.categoryLabel.toLowerCase().includes(q),
      );
    }

    return result;
  }, [options, activeTab, query]);

  if (!isOpen) return null;

  return (
    <div
      className="locaryn-settings-backdrop"
      onClick={(e) => {
        // Seul un clic sur le fond ferme : un clic parti de la carte remonte jusqu'ici.
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      <div
        className="locaryn-card"
        style={{
          width: "520px",
          maxHeight: "80vh",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          margin: "60px auto",
          border: "1px solid var(--border-strong)",
          boxShadow: "0 16px 40px rgba(0,0,0,0.7)",
        }}
      >
        {/* Header */}
        <div className="locaryn-field-head" style={{ marginBottom: "10px" }}>
          <h3
            style={{
              margin: 0,
              fontSize: "var(--text-md)",
              display: "flex",
              alignItems: "center",
              gap: "6px",
            }}
          >
            <Icon name="speed" size={15} /> Changer de Modèle Installé
          </h3>
          <button type="button" className="locaryn-icon-btn" onClick={onClose}>
            <Icon name="close" size={16} />
          </button>
        </div>

        {/* Category Tabs (Text to Text, Code, Reasoning, Vision...) */}
        <p style={{ margin: "0 0 10px", fontSize: "11px", color: "var(--text-faint)" }}>
          Les modèles de génération d’image se gèrent dans le panneau “Génération d’Images IA”.
        </p>
        <div style={{ display: "flex", gap: "4px", marginBottom: "10px", flexWrap: "wrap" }}>
          <button
            type="button"
            className={`locaryn-chip${activeTab === "all" ? " locaryn-chip-on" : ""}`}
            onClick={() => setActiveTab("all")}
          >
            Tous ({options.length})
          </button>
          <button
            type="button"
            className={`locaryn-chip${activeTab === "text" ? " locaryn-chip-on" : ""}`}
            onClick={() => setActiveTab("text")}
          >
            <Icon name="chat" size={15} /> Text to Text (
            {options.filter((o) => o.category === "text").length})
          </button>
          <button
            type="button"
            className={`locaryn-chip${activeTab === "code" ? " locaryn-chip-on" : ""}`}
            onClick={() => setActiveTab("code")}
          >
            <Icon name="cpu" size={15} /> Code (
            {options.filter((o) => o.category === "code").length})
          </button>
          <button
            type="button"
            className={`locaryn-chip${activeTab === "reasoning" ? " locaryn-chip-on" : ""}`}
            onClick={() => setActiveTab("reasoning")}
          >
            <Icon name="memory" size={15} /> Raisonnement (
            {options.filter((o) => o.category === "reasoning").length})
          </button>
          <button
            type="button"
            className={`locaryn-chip${activeTab === "vision" ? " locaryn-chip-on" : ""}`}
            onClick={() => setActiveTab("vision")}
          >
            <Icon name="image" size={15} /> Vision (
            {options.filter((o) => o.category === "vision").length})
          </button>
        </div>

        {/* Search Bar */}
        <input
          className="locaryn-input"
          placeholder="🔍 Rechercher parmi vos modèles installés..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          style={{ marginBottom: "10px" }}
        />

        {/* Models list */}
        <div
          style={{
            flex: 1,
            overflowY: "auto",
            display: "flex",
            flexDirection: "column",
            gap: "6px",
          }}
        >
          {filtered.length === 0 ? (
            <div style={{ textAlign: "center", color: "var(--text-faint)", padding: "24px" }}>
              <p style={{ margin: "0 0 12px 0", fontSize: "13px" }}>
                Aucun modèle correspondant dans cette catégorie.
              </p>
              {onOpenMarketplace && (
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  style={{ fontSize: "12px" }}
                  onClick={() => {
                    onClose();
                    onOpenMarketplace();
                  }}
                >
                  <Icon name="forward" size={15} /> Installer des modèles dans le Marketplace
                </button>
              )}
            </div>
          ) : (
            filtered.map((item) => {
              const isActive = activeModel === item.tag;
              return (
                <button
                  key={item.tag}
                  type="button"
                  className="locaryn-box-variant-row"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "10px 12px",
                    background: isActive ? "rgba(100, 150, 255, 0.15)" : "var(--bg)",
                    border: isActive ? "1px solid var(--accent)" : "1px solid var(--border)",
                    borderRadius: "var(--radius-xs)",
                    cursor: "pointer",
                    textAlign: "left",
                  }}
                  onClick={async () => {
                    await onSelectModel(item.tag);
                    onClose();
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                    <span style={{ fontSize: "18px" }}>{item.icon}</span>
                    <div>
                      <div style={{ fontWeight: 600, fontSize: "13px", color: "var(--text)" }}>
                        {item.name}
                      </div>
                      <div
                        style={{ fontSize: "11px", color: "var(--text-faint)", marginTop: "2px" }}
                      >
                        {item.brand} — <code style={{ fontSize: "10px" }}>{item.tag}</code>
                      </div>
                    </div>
                  </div>

                  <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                    <span
                      style={{
                        fontSize: "10px",
                        padding: "2px 6px",
                        borderRadius: "4px",
                        background: "rgba(255,255,255,0.06)",
                        color: "var(--text-faint)",
                        border: "1px solid var(--border)",
                      }}
                    >
                      {item.categoryLabel}
                    </span>
                    <span className="locaryn-tag" style={{ fontSize: "10px" }}>
                      {item.isLocal ? "LOCAL" : "CLOUD"}
                    </span>
                    {isActive && (
                      <span
                        className="locaryn-tag locaryn-tag-installed"
                        style={{ fontSize: "10px" }}
                      >
                        ACTIF ✓
                      </span>
                    )}
                  </div>
                </button>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
