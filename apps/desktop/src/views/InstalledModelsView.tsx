import { useMemo, useState, useEffect} from "react";
import { core } from "../lib/core";
import { IMAGE_GEN_MODELS } from "../lib/modelRegistry";
import { classifyModel, nsfwReason } from "../lib/modelSafety";
import { dedupeModelsByDirectory } from "../lib/modelList";

type Props = {
  installedModels: string[];
  onSelectModelForChat: (modelTag: string) => void;
  onOpenImageGen?: () => void;
  onDeleteModel?: (modelTag: string) => Promise<void> | void;
  onOpenMarketplace?: () => void;
};

export function InstalledModelsView({
  installedModels,
  onSelectModelForChat,
  onOpenImageGen,
  onDeleteModel,
  onOpenMarketplace,
}: Props) {
  const [query, setQuery] = useState("");
  const [filterType, setFilterType] = useState<"all" | "text" | "image">("all");
  const [riskFilter, setRiskFilter] = useState<"all" | "safe" | "uncensored" | "nsfw">("all");
  const [activatingModel, setActivatingModel] = useState<string | null>(null);

  // Real weights directory, read from the backend. Hardcoding it broke
  // "Ouvrir le dossier" on any machine that is not the dev box.
  const [modelsDir, setModelsDir] = useState("");
  useEffect(() => {
    core.appInfo()
      .then((i) => setModelsDir(i.models_dir || `${i.data_dir}/models`))
      .catch(() => {});
  }, []);

  const dedupedModels = useMemo(() => dedupeModelsByDirectory(installedModels), [installedModels]);

  const parsedModels = useMemo(() => {
    return dedupedModels.map((m) => {
      const mLower = m.toLowerCase();
      const isImage =
        mLower.includes("z-image") ||
        mLower.includes("z_image") ||
        mLower.includes("image") ||
        mLower.includes("flux") ||
        mLower.includes("stable-diffusion") ||
        mLower.includes("stable_diffusion") ||
        mLower.includes("sdxl") ||
        mLower.includes("sd_xl") ||
        mLower.includes("sd15") ||
        mLower.includes("sd_1") ||
        mLower.includes("pony") ||
        mLower.includes("realistic") ||
        mLower.includes("vision") ||
        mLower.includes("mmproj") ||
        mLower.includes("diffusion") ||
        mLower.includes("ae.safetensors") ||
        mLower.endsWith(".png") ||
        IMAGE_GEN_MODELS.some((f) =>
          f.variants.some((v) => {
            const fileName = v.tag.split("/").pop()!.toLowerCase();
            return mLower.includes(fileName) || fileName.includes(mLower) || mLower.includes(f.id);
          })
        );

      const isGguf = m.toLowerCase().endsWith(".gguf");
      const cleanName = m.replace(/^http.*[/\\]/, "").replace(/:latest$/, "");
      const classification = classifyModel(m, { uncensored: undefined });

      return {
        rawTag: m,
        cleanName,
        isImage,
        isGguf,
        isNsfw: classification.risk !== "safe",
        risk: classification.risk,
        fullPath: `${modelsDir}\\${cleanName}`,
        engine: isImage ? "sd.exe" : "llama-server",
      };
    });
  }, [dedupedModels]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return parsedModels.filter((m) => {
      if (filterType === "text" && m.isImage) return false;
      if (filterType === "image" && !m.isImage) return false;
      if (riskFilter === "safe" && m.risk !== "safe") return false;
      if (riskFilter === "uncensored" && m.risk !== "uncensored") return false;
      if (riskFilter === "nsfw" && m.risk !== "nsfw") return false;
      if (q && !m.cleanName.toLowerCase().includes(q) && !m.rawTag.toLowerCase().includes(q)) {
        return false;
      }
      return true;
    });
  }, [parsedModels, query, filterType]);

  async function handleUseForChat(model: string) {
    setActivatingModel(model);
    try {
      const providers = await core.listProviders();
      const active = providers.find((p) => p.is_active) ?? providers[0];
      if (active) {
        await core.configureProvider(active.endpoint, model);
      }
      onSelectModelForChat(model);
    } catch (e) {
      console.error("Failed to set active model:", e);
      onSelectModelForChat(model);
    } finally {
      setActivatingModel(null);
    }
  }

  async function handleOpenFolder(filePath?: string) {
    try {
      await core.openModelsFolder(filePath);
    } catch (e) {
      console.error("Failed to open folder:", e);
    }
  }

  return (
    <div className="locaryn-view-container">
      {/* ── View Header ── */}
      <div className="locaryn-view-header">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
          <div>
            <h2>💾 Mes Modèles Installés ({dedupedModels.length})</h2>
            <p className="locaryn-view-desc">
              Gérez vos modèles d'IA stockés localement sur votre disque. Ouvrez leur emplacement ou sélectionnez-les directement pour vos chats et générations.
            </p>
          </div>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{ fontSize: "12px", border: "1px solid var(--border-strong)" }}
              onClick={() => handleOpenFolder(modelsDir)}
            >
              📁 Ouvrir le dossier des modèles
            </button>
            {onOpenMarketplace && (
              <button
                type="button"
                className="locaryn-btn-primary"
                style={{ fontSize: "12px" }}
                onClick={onOpenMarketplace}
              >
                🛒 Explorer le Marketplace
              </button>
            )}
          </div>
        </div>
      </div>

      {/* ── Controls Bar ── */}
      <div style={{ display: "flex", gap: "10px", alignItems: "center", marginBottom: "16px" }}>
        <input
          type="text"
          className="locaryn-input"
          style={{ flex: 1, fontSize: "13px" }}
          placeholder="Filtrer mes modèles installés (Gemma, Qwen, DeepSeek, SD...)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />

        <div style={{ display: "flex", gap: "4px" }}>
          <button
            type="button"
            className={`locaryn-chip${filterType === "all" ? " locaryn-chip-on" : ""}`}
            onClick={() => setFilterType("all")}
          >
            Tous ({dedupedModels.length})
          </button>
          <button
            type="button"
            className={`locaryn-chip${filterType === "text" ? " locaryn-chip-on" : ""}`}
            onClick={() => setFilterType("text")}
          >
            💬 LLM Texte
          </button>
          <button
            type="button"
            className={`locaryn-chip${filterType === "image" ? " locaryn-chip-on" : ""}`}
            onClick={() => setFilterType("image")}
          >
            🎨 Modèles Image
          </button>
        </div>
        <div style={{ display: "flex", gap: "4px" }}>
          <button
            type="button"
            className={`locaryn-chip${riskFilter === "safe" ? " locaryn-chip-on" : ""}`}
            onClick={() => setRiskFilter((prev) => (prev === "safe" ? "all" : "safe"))}
          >
            🛡️ Safe
          </button>
          <button
            type="button"
            className={`locaryn-chip${riskFilter === "uncensored" ? " locaryn-chip-on" : ""}`}
            onClick={() => setRiskFilter((prev) => (prev === "uncensored" ? "all" : "uncensored"))}
          >
            🔓 Sans limite
          </button>
          <button
            type="button"
            className={`locaryn-chip${riskFilter === "nsfw" ? " locaryn-chip-on" : ""}`}
            onClick={() => setRiskFilter((prev) => (prev === "nsfw" ? "all" : "nsfw"))}
          >
            🔞 NSFW
          </button>
        </div>
      </div>

      {/* ── Empty state ── */}
      {filtered.length === 0 && (
        <div
          style={{
            textAlign: "center",
            padding: "48px 24px",
            background: "var(--surface)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius)",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: "12px",
          }}
        >
          <span style={{ fontSize: "36px" }}>💾</span>
          <div style={{ fontSize: "15px", fontWeight: 700, color: "var(--text)" }}>
            {installedModels.length === 0 ? "Aucun modèle installé localement" : "Aucun modèle ne correspond à votre recherche"}
          </div>
          <p style={{ fontSize: "13px", color: "var(--text-faint)", maxWidth: "420px", margin: 0 }}>
            {installedModels.length === 0
              ? "Allez dans le Marketplace pour télécharger des modèles d'IA texte ou image exécutables hors-ligne."
              : "Essayez de modifier votre terme de recherche ou de réinitialiser le filtre."}
          </p>
          {onOpenMarketplace && (
            <button type="button" className="locaryn-btn-primary" onClick={onOpenMarketplace}>
              🛒 Télécharger un modèle depuis le Marketplace
            </button>
          )}
        </div>
      )}

      {/* ── Models List / Cards ── */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
          gap: "12px",
        }}
      >
        {filtered.map((m) => (
          <div
            key={m.rawTag}
            className="locaryn-box-card"
            style={{
              padding: "16px",
              display: "flex",
              flexDirection: "column",
              gap: "10px",
              background: "var(--surface)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius)",
            }}
          >
            {/* Title & Engine tag */}
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: "8px" }}>
              <div style={{ minWidth: 0, flex: 1 }}>                <span className="locaryn-box-brand" style={{ fontSize: "10px" }}>
                    {m.isImage ? "🎨 IMAGE MODEL" : "💬 TEXT LLM"} · {m.engine}
                  </span>
                  {(() => {
                    const c = classifyModel(m.rawTag);
                    if (c.risk === "safe") return null;
                    return (
                      <span className="locaryn-tag" style={{ background: "rgba(204,125,114,0.2)", color: "var(--danger)", border: "1px solid rgba(204,125,114,0.4)", fontSize: "10px", marginLeft: "6px" }} title={nsfwReason(m.rawTag) ?? c.label}>
                        {c.icon} {c.label}
                      </span>
                    );
                  })()}
                <h3
                  className="locaryn-box-name"
                  style={{
                    fontSize: "14px",
                    fontWeight: 700,
                    margin: "2px 0 0",
                    wordBreak: "break-all",
                  }}
                >
                  {m.cleanName}
                </h3>
              </div>
              <span
                className="locaryn-tag locaryn-tag-installed"
                style={{ fontSize: "10px", padding: "2px 6px" }}
              >
                Stocké localement ✓
              </span>
            </div>

            {/* Path info */}
            <div
              style={{
                fontSize: "11px",
                color: "var(--text-faint)",
                background: "var(--bg)",
                padding: "6px 8px",
                borderRadius: "var(--radius-xs)",
                border: "1px solid var(--border)",
                wordBreak: "break-all",
                fontFamily: "var(--font-mono)",
              }}
            >
              📂 {m.fullPath}
            </div>

            {/* Actions Bar */}
            <div style={{ display: "flex", gap: "6px", flexWrap: "wrap", marginTop: "auto", paddingTop: "6px" }}>
              {m.isImage ? (
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  style={{ flex: 1, fontSize: "12px", whiteSpace: "nowrap" }}
                  onClick={() => onOpenImageGen?.()}
                >
                  🎨 Générer des images
                </button>
              ) : (
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  style={{ flex: 1, fontSize: "12px", whiteSpace: "nowrap" }}
                  disabled={activatingModel === m.rawTag}
                  onClick={() => handleUseForChat(m.rawTag)}
                >
                  {activatingModel === m.rawTag ? "Sélection…" : "💬 Utiliser dans le Chat"}
                </button>
              )}

              <button
                type="button"
                className="locaryn-btn-ghost"
                style={{ fontSize: "12px", padding: "4px 8px" }}
                onClick={() => handleOpenFolder(m.fullPath)}
                title="Ouvrir l'emplacement de ce fichier sur le disque"
              >
                📁 Emplacement
              </button>

              {onDeleteModel && (
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  style={{ color: "var(--danger)", fontSize: "12px", padding: "4px 8px" }}
                  onClick={() => onDeleteModel(m.rawTag)}
                  title="Supprimer ce modèle pour libérer de l'espace disque"
                >
                  🗑 Supprimer
                </button>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
