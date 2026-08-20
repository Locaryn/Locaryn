import { Icon } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { SpeedBadge, findMetric } from "../components/SpeedBadge";
import { type InstalledExtension, type ModelMetric, type StoredWeight, core } from "../lib/core";
import { claimantOf, loadExtensionMarketplaces } from "../lib/extensionMarketplace";
import { classifyModel, nsfwReason } from "../lib/modelSafety";

type Props = {
  installedModels: string[];
  onSelectModelForChat: (modelTag: string) => void;
  onDeleteModel?: (modelTag: string) => Promise<void> | void;
  onOpenMarketplace?: () => void;
  /** Extensions actives : chacune revendique ses propres poids par catalogue. */
  extensions?: InstalledExtension[];
};

/** Une taille lisible, pour un écran dont le sujet est la place occupée. */
function humanSize(bytes: number): string {
  if (bytes <= 0) return "";
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(gb >= 10 ? 0 : 1)} Go`;
  return `${Math.max(1, Math.round(bytes / 1024 ** 2))} Mo`;
}

type InstalledModelIdentity = {
  model: string;
  quantization: string | null;
  format: string;
};

/** Keep the identity visible when a repository contains several choices. */
function identifyInstalledModel(rawTag: string): InstalledModelIdentity {
  const normalized = rawTag.replace(/\\/g, "/");
  const fileName = normalized.split("/").pop() ?? normalized;
  const formatMatch = fileName.match(/\.([a-z0-9]+)$/i);
  const format = formatMatch?.[1]?.toUpperCase() ?? "MODEL";
  const stem = formatMatch ? fileName.slice(0, -formatMatch[0].length) : fileName;
  const withoutShard = stem.replace(/[-_.]\d{5}-of-\d{5}$/i, "");
  const quantMatch = withoutShard.match(
    /(?:^|[-_.])((?:Q[2-8](?:_[K0-9]+)*|F(?:P)?16|BF16|INT8))(?:$|[-_.])/i,
  );
  const quantization = quantMatch?.[1]?.toUpperCase() ?? null;
  const model = (
    quantMatch ? withoutShard.slice(0, quantMatch.index ?? withoutShard.length) : withoutShard
  ).replace(/[-_.]+$/, "");
  return { model: model || withoutShard, quantization, format };
}

/** D'où vient un modèle stocké, et ce qu'on peut en faire.
 *
 *  `chat` : le moteur de conversation le charge. `extension` : une extension
 *  s'en sert — l'hôte ne sait pas à quoi. `incompatible` : un dépôt que rien
 *  n'exécute ici, gardé visible pour pouvoir libérer la place. */
type EntryKind = "chat" | "extension" | "incompatible";

type StoredEntry = {
  /** L'identifiant que les commandes de suppression et d'ouverture attendent. */
  tag: string;
  kind: EntryKind;
  title: string;
  /** Ligne d'en-tête : qui charge ce modèle. */
  brand: string;
  tags: string[];
  path: string;
  hint?: string;
};

export function InstalledModelsView({
  installedModels,
  onSelectModelForChat,
  onDeleteModel,
  onOpenMarketplace,
  extensions = [],
}: Props) {
  const [query, setQuery] = useState("");
  const [riskFilter, setRiskFilter] = useState<"all" | "safe" | "uncensored" | "nsfw">("all");
  const [activatingModel, setActivatingModel] = useState<string | null>(null);
  const [modelsDir, setModelsDir] = useState("");
  const [metrics, setMetrics] = useState<ModelMetric[]>([]);
  const [incompatibleModels, setIncompatibleModels] = useState<string[]>([]);
  // Les poids que la conversation ne charge pas. Ils étaient simplement
  // absents de cet écran : installés, occupant des gigaoctets, invisibles.
  const [otherWeights, setOtherWeights] = useState<StoredWeight[]>([]);
  const [weightOwners, setWeightOwners] = useState<Record<string, string>>({});
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
    void core
      .listIncompatibleModels()
      .then(setIncompatibleModels)
      .catch(() => setIncompatibleModels([]));
    void core
      .listNonChatModels()
      .then(setOtherWeights)
      .catch(() => setOtherWeights([]));
  }, []);

  // Qui revendique quoi. Le socle ignore à quoi sert un poids ; seule
  // l'extension qui le gère sait le reconnaître, par son catalogue.
  useEffect(() => {
    let cancelled = false;
    if (otherWeights.length === 0) {
      setWeightOwners({});
      return;
    }
    void loadExtensionMarketplaces(extensions, core.refreshExtensionAsset).then((catalogue) => {
      if (cancelled) return;
      const names = new Map(
        extensions.map((ext) => [ext.id, ext.display_name || ext.name] as const),
      );
      const owners: Record<string, string> = {};
      for (const weight of otherWeights) {
        const owner = claimantOf(weight.name, catalogue.claims);
        if (owner) owners[weight.name] = names.get(owner) ?? owner;
      }
      setWeightOwners(owners);
    });
    return () => {
      cancelled = true;
    };
  }, [extensions, otherWeights]);

  // Une seule liste. Séparer « modèles de conversation » et « poids
  // d'extension » en deux blocs demandait de chercher deux fois pour une
  // question qui n'en fait qu'une : qu'est-ce qui est installé, et où.
  const entries = useMemo<StoredEntry[]>(() => {
    const chat: StoredEntry[] = dedupedModels.map((rawTag) => {
      const cleanName = rawTag.replace(/^http.*[/\\]/, "").replace(/:latest$/, "");
      const identity = identifyInstalledModel(rawTag);
      return {
        tag: rawTag,
        kind: "chat",
        title: identity.model,
        brand: "TEXTE · llama-server",
        tags: [identity.quantization, identity.format].filter((tag): tag is string => !!tag),
        path: `${modelsDir}\\${cleanName}`,
      };
    });

    const owned: StoredEntry[] = otherWeights.map((weight) => {
      const identity = identifyInstalledModel(weight.name);
      const owner = weightOwners[weight.name];
      return {
        tag: weight.name,
        kind: "extension",
        title: identity.model,
        brand: owner ? owner.toUpperCase() : "AUCUNE EXTENSION",
        tags: [identity.quantization, identity.format, humanSize(weight.size_bytes)].filter(
          (tag): tag is string => !!tag,
        ),
        path: `${modelsDir}\\${weight.name}`,
        hint: owner
          ? undefined
          : "Aucune extension installée ne revendique ces poids. Ils occupent de la place sans servir.",
      };
    });

    const broken: StoredEntry[] = incompatibleModels.map((name) => ({
      tag: name,
      kind: "incompatible",
      title: name,
      brand: "TRANSFORMERS",
      tags: ["SAFETENSORS"],
      path: `${modelsDir}\\${name}`,
      hint: "Le moteur de conversation local ne charge pas ce dépôt. Gardé ici pour pouvoir libérer l'espace disque.",
    }));

    return [...chat, ...owned, ...broken].sort((a, b) =>
      a.title.localeCompare(b.title, "fr", { sensitivity: "base" }),
    );
  }, [dedupedModels, otherWeights, weightOwners, incompatibleModels, modelsDir]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries.filter((entry) => {
      if (riskFilter !== "all" && classifyModel(entry.tag).risk !== riskFilter) return false;
      if (!q) return true;
      return (
        entry.title.toLowerCase().includes(q) ||
        entry.tag.toLowerCase().includes(q) ||
        entry.brand.toLowerCase().includes(q)
      );
    });
  }, [entries, query, riskFilter]);

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

  async function handleDelete(entry: StoredEntry) {
    if (!onDeleteModel) return;
    const detail =
      entry.kind === "incompatible"
        ? "Ce dépôt Transformers complet sera supprimé définitivement, y compris tous ses shards Safetensors."
        : entry.kind === "extension"
          ? "Ces poids seront supprimés définitivement. L'extension qui les utilise ne les retrouvera pas."
          : "Tous les shards de cette variante seront supprimés définitivement.";
    if (!window.confirm(`Supprimer « ${entry.tag} » ?\n\n${detail}`)) return;
    await onDeleteModel(entry.tag);
    if (entry.kind === "incompatible") {
      setIncompatibleModels((current) => current.filter((item) => item !== entry.tag));
    }
    if (entry.kind === "extension") {
      setOtherWeights((current) => current.filter((item) => item.name !== entry.tag));
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
              <Icon name="models" size={18} /> Mes modèles installés ({entries.length})
            </h2>
            <p className="locaryn-view-desc">
              Tout ce qui est stocké localement : les modèles de conversation et ceux qu'une
              extension utilise.
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
            {entries.length === 0
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
        {filtered.map((entry) => {
          const classification = classifyModel(entry.tag);
          return (
            <div
              key={`${entry.kind}:${entry.tag}`}
              className="locaryn-box-card"
              style={{ padding: 16, display: "flex", flexDirection: "column", gap: 10 }}
            >
              <div style={{ display: "flex", justifyContent: "space-between", gap: 8 }}>
                <div style={{ minWidth: 0 }}>
                  <span className="locaryn-box-brand" style={{ fontSize: 10 }}>
                    {entry.brand}
                  </span>
                  {entry.kind === "chat" && (
                    <SpeedBadge metric={findMetric(metrics, entry.tag, "chat")} />
                  )}
                  {classification.risk !== "safe" && (
                    <span
                      className="locaryn-tag"
                      style={{ color: "var(--danger)", marginLeft: 6 }}
                      title={nsfwReason(entry.tag) ?? classification.label}
                    >
                      {classification.icon} {classification.label}
                    </span>
                  )}
                  <h3
                    className="locaryn-box-name"
                    style={{ fontSize: 14, margin: "4px 0 0" }}
                    title={entry.tag}
                  >
                    {entry.title}
                  </h3>
                  <div style={{ display: "flex", gap: 5, flexWrap: "wrap", marginTop: 5 }}>
                    {entry.tags.map((tag) => (
                      <span key={tag} className="locaryn-tag locaryn-tag-soft">
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
                <span className="locaryn-tag locaryn-tag-installed" style={{ fontSize: 10 }}>
                  Stocké
                </span>
              </div>

              {entry.hint && (
                <p className="locaryn-field-hint" style={{ margin: 0 }}>
                  {entry.hint}
                </p>
              )}

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
                <Icon name="project" size={14} /> {entry.path}
              </div>

              <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginTop: "auto" }}>
                {entry.kind === "chat" && (
                  <button
                    type="button"
                    className="locaryn-btn-primary"
                    style={{ flex: 1, fontSize: 12 }}
                    disabled={activatingModel === entry.tag}
                    onClick={() => void handleUseForChat(entry.tag)}
                  >
                    {activatingModel === entry.tag ? "Sélection…" : "Utiliser dans le chat"}
                  </button>
                )}
                {entry.kind === "incompatible" && onOpenMarketplace && (
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ fontSize: 12 }}
                    onClick={onOpenMarketplace}
                  >
                    <Icon name="marketplace" size={15} /> Chercher un GGUF
                  </button>
                )}
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  style={{ fontSize: 12 }}
                  onClick={() => void handleOpenFolder(entry.path)}
                >
                  <Icon name="project" size={15} /> Emplacement
                </button>
                {onDeleteModel && (
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{ color: "var(--danger)", fontSize: 12 }}
                    onClick={() => void handleDelete(entry)}
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
