import { Icon, type IconName } from "@locaryn/ui-core";
import { useEffect, useMemo, useState } from "react";
import { type CloudModel, core as coreApi } from "../lib/core";
import { core } from "../lib/core";
import {
  type ModelFamily,
  SEED_CATALOG,
  classifyModel,
  fetchFullRegistry,
  isChatModel,
} from "../lib/modelRegistry";
import { formatContext, formatPrice, useCloudProviders } from "./cloud/CloudProviderFolder";

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
  category: "text" | "code" | "reasoning" | "vision";
  categoryLabel: string;
  icon: IconName;
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
  const [registry, setRegistry] = useState<ModelFamily[]>(SEED_CATALOG);
  /* Les catalogues distants apportés par les morphs. Ils apparaissent comme un
     dossier au milieu des modèles installés : on clique, et on est dans la
     liste du fournisseur — sans quitter le sélecteur. */
  const { providers: cloudProviders } = useCloudProviders();
  const [openFolder, setOpenFolder] = useState<string | null>(null);
  const [cloudModels, setCloudModels] = useState<CloudModel[]>([]);
  const [cloudBusy, setCloudBusy] = useState(false);
  const [cloudError, setCloudError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    fetchFullRegistry((q, cat) => core.searchOllamaLibrary(q, cat))
      .then((res) => setRegistry(res.families))
      .catch(() => {});
  }, [isOpen]);

  const dedupedModels = useMemo(
    () =>
      Array.from(
        new Set(
          installedModels.length > 0
            ? installedModels
            : ([activeModel].filter(Boolean) as string[]),
        ),
      ).sort((a, b) => a.localeCompare(b)),
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
          let categoryLabel = "Texte";
          let icon: IconName = "chat";

          if (kind === "code") {
            category = "code";
            categoryLabel = "Code";
            icon = "cpu";
          } else if (kind === "reasoning") {
            category = "reasoning";
            categoryLabel = "Raisonnement";
            icon = "memory";
          } else if (kind === "vision") {
            category = "vision";
            categoryLabel = "Vision";
            icon = "image";
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
        // Only offer models suitable for chat (LLM / vision / code / reasoning) and strictly 100% local.
        // Specialized media models are owned by their extensions.
        .filter((o) => isChatModel(o.tag) && o.isLocal)
    );
  }, [dedupedModels, isProviderLocal, registry]);

  // Le dossier ouvert lit sa liste chez le fournisseur — une fois, et gardée
  // ensuite par l'hôte : rouvrir le dossier ne relance pas d'appel réseau.
  useEffect(() => {
    if (!openFolder) return;
    let active = true;
    setCloudBusy(true);
    setCloudError(null);
    coreApi
      .cloudProviderModels(openFolder)
      .then((list) => {
        if (active) setCloudModels(list);
      })
      .catch((e: unknown) => {
        if (active) setCloudError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (active) setCloudBusy(false);
      });
    return () => {
      active = false;
    };
  }, [openFolder]);

  // Refermer le sélecteur doit le rendre à son état d'entrée : rouvrir sur la
  // liste d'un fournisseur qu'on avait quitté serait déroutant.
  useEffect(() => {
    if (!isOpen) {
      setOpenFolder(null);
      setQuery("");
    }
  }, [isOpen]);

  const openedProvider = cloudProviders.find((p) => p.id === openFolder) ?? null;

  const filteredCloud = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return cloudModels;
    return cloudModels.filter((m) =>
      `${m.id} ${m.name} ${m.description}`.toLowerCase().includes(q),
    );
  }, [cloudModels, query]);

  async function selectCloudModel(providerId: string, model: string) {
    setCloudError(null);
    try {
      // Surtout pas `onSelectModel` : ce chemin-là reconfigure le fournisseur
      // *local* avec le nom du modèle. Il écraserait le fournisseur distant
      // qu'on vient d'activer, et la conversation repartirait vers
      // llama-server avec un identifiant qu'il ne sait pas charger.
      await coreApi.cloudProviderSelect(providerId, model);
      window.dispatchEvent(
        new CustomEvent("locaryn:cloud-model-selected", {
          detail: { provider: providerId, model },
        }),
      );
      onClose();
    } catch (e) {
      setCloudError(e instanceof Error ? e.message : String(e));
    }
  }

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
            {openedProvider ? (
              <>
                <button
                  type="button"
                  className="locaryn-icon-btn"
                  onClick={() => setOpenFolder(null)}
                  title="Revenir aux modèles installés"
                >
                  <Icon name="back" size={15} />
                </button>
                <Icon name="cloud" size={15} /> {openedProvider.label}
              </>
            ) : (
              <>
                <Icon name="speed" size={15} /> Changer de Modèle Installé
              </>
            )}
          </h3>
          <button type="button" className="locaryn-icon-btn" onClick={onClose}>
            <Icon name="close" size={16} />
          </button>
        </div>

        {/* Category Tabs (Text to Text, Code, Reasoning, Vision...) */}
        <p style={{ margin: "0 0 10px", fontSize: "11px", color: "var(--text-faint)" }}>
          {openedProvider
            ? `Modèles servis par ${openedProvider.label}, facturés sur votre clé.`
            : "Les fonctionnalités spécialisées sont fournies par leurs extensions."}
        </p>
        <div
          style={{
            display: openedProvider ? "none" : "flex",
            gap: "4px",
            marginBottom: "10px",
            flexWrap: "wrap",
          }}
        >
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
          placeholder={
            openedProvider
              ? `Rechercher parmi ${cloudModels.length} modèles ${openedProvider.label}…`
              : "Rechercher parmi vos modèles installés…"
          }
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
          {/* Dans un dossier : la liste du fournisseur, et rien d'autre. */}
          {openedProvider && (
            <>
              {cloudError && <div className="locaryn-cloud-error">{cloudError}</div>}
              {cloudBusy && cloudModels.length === 0 && (
                <div className="locaryn-cloud-empty">Lecture du catalogue…</div>
              )}
              {!openedProvider.has_key && (
                <div className="locaryn-cloud-notice">
                  Aucune clé enregistrée pour {openedProvider.label}. Ouvrez son dossier dans « Mes
                  modèles » pour la coller — sans elle, le choix sera refusé.
                </div>
              )}
              {filteredCloud.map((model) => {
                const isActive = activeModel === model.id;
                return (
                  <button
                    key={model.id}
                    type="button"
                    className={`locaryn-cloud-row${isActive ? " locaryn-active" : ""}`}
                    onClick={() => void selectCloudModel(openedProvider.id, model.id)}
                  >
                    <span style={{ minWidth: 0, textAlign: "left" }}>
                      <span className="locaryn-cloud-model-name">{model.name}</span>
                      <span className="locaryn-cloud-model-id">{model.id}</span>
                    </span>
                    <span className="locaryn-cloud-model-facts">
                      <span>{formatContext(model.context_length)}</span>
                      <span>
                        {model.prompt_price_per_m === 0
                          ? "gratuit"
                          : `${formatPrice(model.prompt_price_per_m)}/M`}
                      </span>
                      {isActive && <span className="locaryn-tag locaryn-tag-installed">Actif</span>}
                    </span>
                  </button>
                );
              })}
              {!cloudBusy && filteredCloud.length === 0 && !cloudError && (
                <div className="locaryn-cloud-empty">Aucun modèle ne correspond.</div>
              )}
            </>
          )}

          {/* Hors dossier : les dossiers d'abord, puis les modèles installés. */}
          {!openedProvider &&
            cloudProviders.map((provider) => (
              <button
                key={`cloud:${provider.id}`}
                type="button"
                className="locaryn-cloud-row locaryn-cloud-row-folder"
                onClick={() => setOpenFolder(provider.id)}
                title={`Voir les modèles ${provider.label}`}
              >
                <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                  <Icon name="cloud" size={16} />
                  <span style={{ minWidth: 0, textAlign: "left" }}>
                    <span className="locaryn-cloud-model-name">{provider.label}</span>
                    <span className="locaryn-cloud-model-id">
                      {provider.active_model ?? `${provider.model_count} modèles distants`}
                    </span>
                  </span>
                </span>
                <Icon name="forward" size={15} />
              </button>
            ))}

          {!openedProvider && filtered.length === 0 ? (
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
          ) : openedProvider ? null : (
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
                  <div
                    className="locaryn-quick-model-info"
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "10px",
                      // Un nom de modèle peut être un chemin de dépôt entier.
                      // Sans plancher à zéro, la ligne refusait de rétrécir et
                      // le nom passait sous les étiquettes de droite.
                      minWidth: 0,
                      flex: "1 1 auto",
                    }}
                  >
                    <div
                      style={{
                        width: "30px",
                        height: "30px",
                        borderRadius: "var(--radius-xs)",
                        background: "rgba(255,255,255,0.05)",
                        border: "1px solid var(--border)",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: "var(--text-dim)",
                        flexShrink: 0,
                      }}
                    >
                      <Icon name={item.icon} size={16} />
                    </div>
                    <div style={{ minWidth: 0 }}>
                      <div
                        className="locaryn-quick-model-name"
                        style={{
                          fontWeight: 600,
                          fontSize: "13px",
                          color: "var(--text)",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                        title={item.name}
                      >
                        {item.name}
                      </div>
                      <div
                        className="locaryn-quick-model-tag"
                        style={{
                          fontSize: "11px",
                          color: "var(--text-faint)",
                          marginTop: "2px",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                        title={`${item.brand} — ${item.tag}`}
                      >
                        {item.brand} — <code style={{ fontSize: "10px" }}>{item.tag}</code>
                      </div>
                    </div>
                  </div>

                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "6px",
                      // Les étiquettes gardent leur place : c'est le nom qui
                      // cède, avec des points de suspension et son texte
                      // complet en infobulle.
                      flexShrink: 0,
                    }}
                  >
                    <span
                      style={{
                        fontSize: "10px",
                        padding: "2px 6px",
                        borderRadius: "4px",
                        background: "rgba(255,255,255,0.06)",
                        color: "var(--text-faint)",
                        border: "1px solid var(--border)",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {item.categoryLabel}
                    </span>
                    <span className="locaryn-tag" style={{ fontSize: "10px" }}>
                      LOCAL
                    </span>
                    {isActive && (
                      <span
                        className="locaryn-tag locaryn-tag-installed"
                        style={{ fontSize: "10px" }}
                      >
                        ACTIF
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
