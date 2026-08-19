import { Icon } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { SpeedBadge, findMetric } from "../components/SpeedBadge";
import { type ModelMetric, core } from "../lib/core";
import { classifyModel, nsfwReason } from "../lib/modelSafety";

type Props = {
  installedModels: string[];
  onSelectModelForChat: (modelTag: string) => void;
  onDeleteModel?: (modelTag: string) => Promise<void> | void;
  onOpenMarketplace?: () => void;
};

type InstalledModelIdentity = {
  model: string;
  quantization: string | null;
  format: string;
};

/** Keep the identity visible when a repository contains several choices. */
function identifyInstalledModel(rawTag: string): InstalledModelIdentity {
  const normalized = rawTag.replace(/\\/g, "/");
  const fileName = normalized.split("/").pop() ?? normalized;
  const formatMatch = fileName.match(/\\.([a-z0-9]+)$/i);
  const format = formatMatch?.[1]?.toUpperCase() ?? "MODEL";
  const stem = formatMatch ? fileName.slice(0, -formatMatch[0].length) : fileName;
  const withoutShard = stem.replace(/[-_.]\\d{5}-of-\\d{5}$/i, "");
  const quantMatch = withoutShard.match(
    /(?:^|[-_.])((?:Q[2-8](?:_[K0-9]+)*|F(?:P)?16|BF16|INT8))(?:$|[-_.])/i,
  );
  const quantization = quantMatch?.[1]?.toUpperCase() ?? null;
  const model = (
    quantMatch ? withoutShard.slice(0, quantMatch.index ?? withoutShard.length) : withoutShard
  ).replace(/[-_.]+$/, "");
  return { model: model || withoutShard, quantization, format };
}

export function InstalledModelsView({
  installedModels,
  onSelectModelForChat,
  onDeleteModel,
  onOpenMarketplace,
}: Props) {
  const [query, setQuery] = useState("");
  const [riskFilter, setRiskFilter] = useState<"all" | "safe" | "uncensored" | "nsfw">("all");
  const [activatingModel, setActivatingModel] = useState<string | null>(null);
  const [modelsDir, setModelsDir] = useState("");
  const [metrics, setMetrics] = useState<ModelMetric[]>([]);
  // The backend already groups shards, but deliberately keeps separate
  // quantisations/variants. Do not collapse by directory here or Q4 and Q8
  // from the same HuggingFace repository would appear as one model again.
  const dedupedModels = useMemo(
    () => Array.from(new Set(installedModels)).sort((a, b) => a.localeCompare(b)),
    [installedModels],
  );

  useEffect(() => {
    core
      .appInfo()
      .then((info) => setModelsDir(info.models_dir || `${info.data_dir}/models`))
      .catch(() => {});
    void core
      .listModelMetrics()
      .then(setMetrics)
      .catch(() => setMetrics([]));
  }, []);

  const parsedModels = useMemo(
    () =>
      dedupedModels.map((rawTag) => {
        const cleanName = rawTag.replace(/^http.*[/\\]/, "").replace(/:latest$/, "");
        const classification = classifyModel(rawTag, { uncensored: undefined });
        const identity = identifyInstalledModel(rawTag);
        return {
          rawTag,
          cleanName,
          ...identity,
          risk: classification.risk,
          fullPath: `${modelsDir}\\${cleanName}`,
        };
      }),
    [dedupedModels, modelsDir],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return parsedModels.filter((model) => {
      if (riskFilter !== "all" && model.risk !== riskFilter) return false;
      return (
        !q || model.cleanName.toLowerCase().includes(q) || model.rawTag.toLowerCase().includes(q)
      );
    });
  }, [parsedModels, query, riskFilter]);

  async function handleUseForChat(model: string) {
    setActivatingModel(model);
    try {
      const providers = await core.listProviders();
      const active = providers.find((provider) => provider.is_active) ?? providers[0];
      if (active) await core.configureProvider(active.endpoint, model);
      onSelectModelForChat(model);
    } catch (error) {
      console.error("Failed to set active model:", error);
      onSelectModelForChat(model);
    } finally {
      setActivatingModel(null);
    }
  }

  async function handleOpenFolder(filePath?: string) {
    try {
      await core.openModelsFolder(filePath);
    } catch (error) {
      console.error("Failed to open folder:", error);
    }
  }

  return (
    <div className="locaryn-view-container">
      <div className="locaryn-view-header">
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "flex-start",
            gap: 16,
          }}
        >
          <div>
            <h2>
              <Icon name="models" size={18} /> Mes modèles installés ({dedupedModels.length})
            </h2>
            <p className="locaryn-view-desc">
              Gérez les modèles de conversation stockés localement.
            </p>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              onClick={() => handleOpenFolder(modelsDir)}
            >
              <Icon name="project" size={15} /> Ouvrir le dossier
            </button>
            {onOpenMarketplace && (
              <button type="button" className="locaryn-btn-primary" onClick={onOpenMarketplace}>
                <Icon name="marketplace" size={15} /> Marketplace
              </button>
            )}
          </div>
        </div>
      </div>

      <div style={{ display: "flex", gap: 10, alignItems: "center", marginBottom: 16 }}>
        <input
          className="locaryn-input"
          style={{ flex: 1, fontSize: 13 }}
          placeholder="Filtrer mes modèles installés…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        {(["safe", "uncensored", "nsfw"] as const).map((risk) => (
          <button
            key={risk}
            type="button"
            className={`locaryn-chip${riskFilter === risk ? " locaryn-chip-on" : ""}`}
            onClick={() => setRiskFilter((current) => (current === risk ? "all" : risk))}
          >
            {risk === "safe" ? (
              <Icon name="shield" size={14} />
            ) : risk === "uncensored" ? (
              <Icon name="lock" size={14} />
            ) : (
              <Icon name="warning" size={14} />
            )}{" "}
            {risk === "safe" ? "Safe" : risk === "uncensored" ? "Sans limite" : "NSFW"}
          </button>
        ))}
      </div>

      {filtered.length === 0 && (
        <div className="locaryn-card" style={{ textAlign: "center", padding: 48 }}>
          <Icon name="models" size={36} />
          <div style={{ fontSize: 15, fontWeight: 700, marginTop: 12 }}>
            {installedModels.length === 0
              ? "Aucun modèle installé localement"
              : "Aucun modèle ne correspond à votre recherche"}
          </div>
          {onOpenMarketplace && (
            <button
              type="button"
              className="locaryn-btn-primary"
              style={{ marginTop: 16 }}
              onClick={onOpenMarketplace}
            >
              <Icon name="marketplace" size={15} /> Télécharger un modèle
            </button>
          )}
        </div>
      )}

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
          gap: 12,
        }}
      >
        {filtered.map((model) => {
          const classification = classifyModel(model.rawTag);
          return (
            <div
              key={model.rawTag}
              className="locaryn-box-card"
              style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}
            >
              <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                <div style={{ minWidth: 0 }}>
                  <span className="locaryn-box-brand" style={{ fontSize: 10 }}>
                    TEXTE · llama-server
                  </span>
                  <SpeedBadge metric={findMetric(metrics, model.rawTag, "chat")} />
                  {classification.risk !== "safe" && (
                    <span
                      className="locaryn-tag"
                      style={{ color: "var(--danger)", marginLeft: 6 }}
                      title={nsfwReason(model.rawTag) ?? classification.label}
                    >
                      {classification.icon} {classification.label}
                    </span>
                  )}
                  <h3
                    className="locaryn-box-name"
                    style={{ fontSize: 14, margin: "4px 0 0" }}
                    title={model.model}
                  >
                    {model.model}
                  </h3>
                  <div style={{ display: "flex", gap: 5, flexWrap: "wrap", marginTop: 5 }}>
                    <span className="locaryn-tag locaryn-tag-soft">Variante</span>
                    {model.quantization && (
                      <span className="locaryn-tag">{model.quantization}</span>
                    )}
                    <span className="locaryn-tag locaryn-tag-soft">{model.format}</span>
                  </div>
                </div>
                <span className="locaryn-tag locaryn-tag-installed" style={{ fontSize: 10 }}>
                  Stocké
                </span>
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: "var(--text-faint)",
                  background: "var(--bg)",
                  padding: "6px 8px",
                  borderRadius: "var(--radius-xs)",
                  wordBreak: "break-all",
                  fontFamily: "var(--font-mono)",
                }}
              >
                <Icon name="project" size={14} /> {model.fullPath}
              </div>
              <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: "auto" }}>
                <button
                  type="button"
                  className="locaryn-btn-primary"
                  style={{ flex: 1, fontSize: 12 }}
                  disabled={activatingModel === model.rawTag}
                  onClick={() => void handleUseForChat(model.rawTag)}
                >
                  {activatingModel === model.rawTag ? "Sélection…" : "Utiliser dans le chat"}
                </button>
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  style={{ fontSize: 12 }}
                  onClick={() => void handleOpenFolder(model.fullPath)}
                >
                  <Icon name="project" size={15} /> Emplacement
                </button>
                {onDeleteModel && (
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ color: "var(--danger)", fontSize: 12 }}
                    onClick={() => void onDeleteModel(model.rawTag)}
                  >
                    <Icon name="trash" size={15} /> Supprimer
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
