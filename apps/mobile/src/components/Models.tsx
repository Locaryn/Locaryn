import { Icon, LoProgress } from "@locaryn/ui-core";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  type CatalogueMarketplace,
  type MarketplaceFamily,
  type MarketplaceVariant,
  type MediaModel,
  type ModelPullProgress,
  api,
} from "../lib/core";
import { useCoucheRetour } from "../lib/navigation";
import { notifyModelDownloaded } from "../lib/notifications";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  /** L'onglet d'ouverture : « installés » ou « catalogue ». */
  initialTab?: "installed" | "marketplace";
};

type Tab = "installed" | "marketplace";

/** Une variante prête à télécharger, avec la famille qui la porte. */
type Offre = { famille: MarketplaceFamily; variante: MarketplaceVariant };

/** La plus légère d'abord : c'est elle qui a le plus de chances de tenir. */
function variantesTriees(f: MarketplaceFamily): MarketplaceVariant[] {
  return [...f.variants].sort((x, y) => (x.storageGb ?? 0) - (y.storageGb ?? 0));
}

export function Models({ onBack, initialTab }: Props) {
  const [tab, setTab] = useState<Tab>(initialTab ?? "installed");
  const [voices, setVoices] = useState<MediaModel[] | null>(null);
  const [llmModels, setLlmModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");
  /** Le Marketplace du serveur : modèles du socle et des morphs actifs. */
  const [catalogue, setCatalogue] = useState<CatalogueMarketplace | null>(null);
  const [erreurCatalogue, setErreurCatalogue] = useState<string | null>(null);

  // Téléchargement personnalisé
  const [showCustomPull, setShowCustomPull] = useState(false);
  const [customModelUrl, setCustomModelUrl] = useState("");

  // Le retour d'Android referme la fenêtre de téléchargement personnalisé.
  useCoucheRetour(showCustomPull, () => setShowCustomPull(false));

  /** L'avancement du téléchargement en cours */
  const [progress, setProgress] = useState<ModelPullProgress | null>(null);

  const reload = useCallback(async () => {
    try {
      const [v, l] = await Promise.all([
        api.listMediaModels("audio"),
        api.listModels().catch(() => []),
      ]);
      setVoices(v);
      setLlmModels(l);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  // Relu à chaque ouverture de l'onglet : un morph activé entre-temps y ajoute
  // ses catégories et ses modèles.
  useEffect(() => {
    if (tab !== "marketplace") return;
    let annule = false;
    api
      .marketplace()
      .then((c) => {
        if (annule) return;
        setCatalogue(c);
        setErreurCatalogue(null);
      })
      .catch((e) => {
        if (!annule) setErreurCatalogue(String(e));
      });
    return () => {
      annule = true;
    };
  }, [tab]);

  async function install({ famille, variante }: Offre) {
    setBusy(variante.tag);
    setError(null);
    setProgress({
      downloaded: 0,
      total: null,
      percentage: 0,
      message: "Démarrage du téléchargement…",
    });
    try {
      await api.pullModel(variante.tag, setProgress, variante.downloads);
      notifyModelDownloaded(`${famille.name} ${variante.size}`);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  }

  async function handleCustomInstall(e: React.FormEvent) {
    e.preventDefault();
    const url = customModelUrl.trim();
    if (!url) return;
    setShowCustomPull(false);
    setCustomModelUrl("");

    setBusy(url);
    setError(null);
    setProgress({
      downloaded: 0,
      total: null,
      percentage: 0,
      message: "Démarrage du téléchargement…",
    });
    try {
      await api.pullModel(url, setProgress);
      notifyModelDownloaded(url.split("/").pop() ?? url);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  }

  async function remove(name: string) {
    if (!window.confirm(`Supprimer le modèle « ${name} » du serveur ?`)) return;
    setBusy(name);
    setError(null);
    try {
      await api.removeModel(name);
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }

  const installedSet = useMemo(() => {
    return new Set([...(voices ?? []).map((m) => m.name), ...(llmModels ?? [])]);
  }, [voices, llmModels]);

  const filteredMarketplace = useMemo(() => {
    if (!catalogue) return [];
    const categorie = catalogue.categories.find((c) => c.id === categoryFilter);
    const q = searchQuery.trim().toLowerCase();
    return catalogue.families.filter((f) => {
      if (categorie) {
        const caps = f.marketplaceCapabilities ?? [];
        if (!categorie.matches.some((m) => caps.includes(m))) return false;
      }
      if (q) {
        const foin = `${f.name} ${f.brand ?? ""} ${f.description ?? ""}`.toLowerCase();
        if (!foin.includes(q)) return false;
      }
      return true;
    });
  }, [catalogue, categoryFilter, searchQuery]);

  const totalInstalled = (voices?.length ?? 0) + (llmModels.length ?? 0);

  return (
    <Screen
      title="Modèles"
      onBack={onBack}
      action={
        <button type="button" className="lo-bar-action" onClick={() => setShowCustomPull(true)}>
          + Télécharger
        </button>
      }
    >
      <div className="lo-tabs">
        <button
          type="button"
          className={`lo-tab ${tab === "installed" ? "lo-tab-active" : ""}`}
          onClick={() => setTab("installed")}
        >
          Installés ({totalInstalled})
        </button>
        <button
          type="button"
          className={`lo-tab ${tab === "marketplace" ? "lo-tab-active" : ""}`}
          onClick={() => setTab("marketplace")}
        >
          Marketplace / Catalogue
        </button>
      </div>

      {progress && (
        <div
          style={{
            padding: 12,
            background: "var(--surface)",
            border: "1px solid var(--accent)",
            borderRadius: "var(--radius)",
            marginBottom: 12,
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              fontSize: 13,
              fontWeight: 600,
            }}
          >
            <span>Téléchargement en cours…</span>
            <span>{progress.percentage !== null ? `${progress.percentage}%` : ""}</span>
          </div>
          <LoProgress
            value={progress.percentage === null ? null : progress.percentage / 100}
            on="surface"
            label="Téléchargement"
          />
          {progress.message && <p className="lo-hint">{progress.message}</p>}
        </div>
      )}

      {error && <p className="lo-error">{error}</p>}

      {/* ── Onglet : Installés ── */}
      {tab === "installed" && (
        <>
          {/* Section : Modèles de conversation & code (LLM) */}
          <section className="lo-section">
            <h2 className="lo-section-title">Conversation</h2>
            {llmModels.length === 0 && (
              <p className="lo-sub">Aucun modèle de conversation sur le serveur.</p>
            )}
            <ul className="lo-cards">
              {llmModels.map((name) => (
                <li key={name} className="lo-card">
                  <div className="lo-card-text">
                    <span className="lo-card-title">{name}</span>
                    <span className="lo-hint">Prêt pour le Chat & les Agents</span>
                  </div>
                  <div className="lo-card-actions">
                    <button
                      type="button"
                      className="lo-btn-small"
                      disabled={busy === name}
                      onClick={() => void remove(name)}
                    >
                      {busy === name ? "Retrait…" : "Retirer"}
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          </section>

          {/* Les modèles audio n'apparaissent que si un morph en a installé. */}
          {voices && voices.length > 0 && (
            <section className="lo-section" style={{ marginTop: "var(--space-4)" }}>
              <h2 className="lo-section-title">Modèles audio</h2>
              <ul className="lo-cards">
                {voices?.map((m) => (
                  <li key={m.name} className="lo-card">
                    <div className="lo-card-text">
                      <span className="lo-card-title">{m.name}</span>
                      <span className="lo-hint">Modèle audio</span>
                    </div>
                    <div className="lo-card-actions">
                      <button
                        type="button"
                        className="lo-btn-small"
                        disabled={busy === m.name}
                        onClick={() => void remove(m.name)}
                      >
                        {busy === m.name ? "Retrait…" : "Retirer"}
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}

      {/* ── Onglet : Marketplace / Catalogue ── */}
      {tab === "marketplace" && (
        <>
          <div className="lo-search-box">
            <input
              type="text"
              className="lo-search-input"
              placeholder="Rechercher un modèle"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
            />
            {searchQuery && (
              <button
                type="button"
                className="lo-btn-ghost"
                style={{
                  position: "absolute",
                  right: 8,
                  width: "auto",
                  minHeight: "auto",
                  padding: 4,
                  border: "none",
                }}
                onClick={() => setSearchQuery("")}
              >
                <Icon name="close" size={18} />
              </button>
            )}
          </div>

          <div className="lo-chips" style={{ marginTop: 8 }}>
            {[{ id: "all", label: "Tous" }, ...(catalogue?.categories ?? [])].map((c) => (
              <button
                key={c.id}
                type="button"
                className={`lo-chip ${categoryFilter === c.id ? "lo-chip-active" : ""}`}
                onClick={() => setCategoryFilter(c.id)}
              >
                {c.label}
              </button>
            ))}
          </div>

          <p className="lo-hint" style={{ margin: "6px 0 10px 0" }}>
            Les modèles de chat et les extensions sont téléchargés sur votre serveur Locaryn.
          </p>

          {erreurCatalogue && <p className="lo-error">{erreurCatalogue}</p>}
          {!catalogue && !erreurCatalogue && (
            <p className="lo-hint">
              <span className="lo-spinner" aria-hidden /> Lecture du Marketplace du serveur…
            </p>
          )}
          {catalogue && filteredMarketplace.length === 0 && (
            <p className="lo-hint">Aucun modèle ne correspond.</p>
          )}
          <ul className="lo-cards">
            {filteredMarketplace.map((f) => (
              <li key={f.id} className="lo-card lo-offre">
                <div className="lo-card-text">
                  <span className="lo-card-title">{f.name}</span>
                  <span className="lo-hint">
                    {[f.brand, f.marketplaceOwner && `apporté par ${f.marketplaceOwner}`]
                      .filter(Boolean)
                      .join(" · ")}
                  </span>
                  {f.description && <span className="lo-hint lo-offre-desc">{f.description}</span>}
                </div>
                <div className="lo-offre-variantes">
                  {variantesTriees(f).map((v) => {
                    const installe = installedSet.has(v.tag) || installedSet.has(f.id);
                    const enCours = busy === v.tag;
                    return (
                      <button
                        key={v.tag}
                        type="button"
                        className="lo-offre-variante"
                        disabled={installe || busy !== null}
                        onClick={() => void install({ famille: f, variante: v })}
                      >
                        <strong>{v.size}</strong>
                        <span>
                          {installe
                            ? "Installé"
                            : enCours
                              ? "Téléchargement…"
                              : v.storageGb
                                ? `${v.storageGb} Go`
                                : "Obtenir"}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </li>
            ))}
          </ul>
        </>
      )}

      {/* ── Modal de téléchargement personnalisé ── */}
      {showCustomPull && (
        <div className="lo-modal-backdrop" onClick={() => setShowCustomPull(false)}>
          <div className="lo-modal" onClick={(e) => e.stopPropagation()}>
            <div className="lo-modal-header">
              <span className="lo-modal-title">Télécharger un modèle</span>
              <button
                type="button"
                className="lo-btn-ghost"
                style={{ width: "auto", minHeight: "auto", padding: "4px 8px", border: "none" }}
                onClick={() => setShowCustomPull(false)}
              >
                <Icon name="close" size={18} />
              </button>
            </div>
            <form onSubmit={handleCustomInstall}>
              <div className="lo-modal-body">
                <p className="lo-hint">
                  Entrez le tag d'un modèle Ollama (ex: <code>qwen2.5:7b</code>), un dépôt
                  HuggingFace (ex: <code>deepseek-ai/DeepSeek-R1-Distill-Qwen-7B</code>) ou un lien
                  direct GGUF.
                </p>
                <div>
                  <label className="lo-label">Tag ou URL du modèle</label>
                  <input
                    type="text"
                    className="lo-input"
                    placeholder="ex: mistral:7b, https://huggingface.co/..."
                    value={customModelUrl}
                    onChange={(e) => setCustomModelUrl(e.target.value)}
                    autoFocus
                    required
                  />
                </div>
              </div>
              <div className="lo-modal-footer">
                <button type="submit" className="lo-btn" disabled={!customModelUrl.trim()}>
                  Démarrer le téléchargement
                </button>
                <button
                  type="button"
                  className="lo-btn-ghost"
                  onClick={() => setShowCustomPull(false)}
                >
                  Annuler
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </Screen>
  );
}
