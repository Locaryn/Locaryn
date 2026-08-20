import { useCallback, useEffect, useMemo, useState } from "react";
import { type MediaModel, type ModelPullProgress, api } from "../lib/core";
import { useCoucheRetour } from "../lib/navigation";
import { notifyModelDownloaded } from "../lib/notifications";
import { Screen } from "./Screen";

type Props = {
  onBack: () => void;
  /** L'onglet d'ouverture : « installés » ou « catalogue ». */
  initialTab?: "installed" | "marketplace";
};

type Tab = "installed" | "marketplace";
type CategoryFilter =
  | "all"
  | "llm"
  | "code"
  | "reasoning"
  | "vision"
  | "voice"
  | "audio"
  | "video_3d";

interface MarketplaceModelItem {
  id: string;
  name: string;
  label: string;
  category: CategoryFilter;
  categoryLabel: string;
  note: string;
  provider: "ollama" | "huggingface" | "airllm";
  sizeGb: number;
  tags: string[];
  url: string;
}

const MARKETPLACE_CATALOGUE: MarketplaceModelItem[] = [
  // ── Modèles de Langage & Chat ──
  {
    id: "qwen2.5:7b",
    name: "qwen2.5:7b",
    label: "Qwen 2.5 7B",
    category: "llm",
    categoryLabel: "Chat & Généraliste",
    note: "Polyvalent, rapide, excellent en français et raisonnement général",
    provider: "ollama",
    sizeGb: 4.7,
    tags: ["Instruct", "Multilingue", "Rapide"],
    url: "qwen2.5:7b",
  },
  {
    id: "llama3.3:70b",
    name: "llama3.3:70b",
    label: "Llama 3.3 70B",
    category: "llm",
    categoryLabel: "Chat & Généraliste",
    note: "Le modèle phare de Meta : capacités de niveau GPT-4o en local",
    provider: "ollama",
    sizeGb: 39.0,
    tags: ["Performant", "70B", "Meta"],
    url: "llama3.3:70b",
  },
  {
    id: "gemma2:9b",
    name: "gemma2:9b",
    label: "Gemma 2 9B",
    category: "llm",
    categoryLabel: "Chat & Généraliste",
    note: "Architecture Google compacte et extrêmement performante",
    provider: "ollama",
    sizeGb: 5.5,
    tags: ["Google", "9B", "Haute qualité"],
    url: "gemma2:9b",
  },
  {
    id: "mistral-nemo:12b",
    name: "mistral-nemo:12b",
    label: "Mistral Nemo 12B",
    category: "llm",
    categoryLabel: "Chat & Généraliste",
    note: "Conçu par Mistral AI : 128k de contexte, fluidité naturelle",
    provider: "ollama",
    sizeGb: 7.1,
    tags: ["Mistral AI", "128k", "Français"],
    url: "mistral-nemo:12b",
  },

  // ── Code & Développement ──
  {
    id: "qwen2.5-coder:7b",
    name: "qwen2.5-coder:7b",
    label: "Qwen 2.5 Coder 7B",
    category: "code",
    categoryLabel: "Code & Développement",
    note: "Spécialisé en écriture de code, refactoring et débogage",
    provider: "ollama",
    sizeGb: 4.7,
    tags: ["Code", "Dev", "SOTA 7B"],
    url: "qwen2.5-coder:7b",
  },
  {
    id: "qwen2.5-coder:14b",
    name: "qwen2.5-coder:14b",
    label: "Qwen 2.5 Coder 14B",
    category: "code",
    categoryLabel: "Code & Développement",
    note: "Excellente compréhension des architectures et projets complexes",
    provider: "ollama",
    sizeGb: 9.0,
    tags: ["Code", "14B", "Avancé"],
    url: "qwen2.5-coder:14b",
  },
  {
    id: "codellama:7b",
    name: "codellama:7b",
    label: "Code Llama 7B",
    category: "code",
    categoryLabel: "Code & Développement",
    note: "Modèle de code officiel Meta optimisé pour Python, JS, C++",
    provider: "ollama",
    sizeGb: 3.8,
    tags: ["Code", "Meta"],
    url: "codellama:7b",
  },

  // ── Raisonnement & Mathématiques ──
  {
    id: "deepseek-r1:8b",
    name: "deepseek-r1:8b",
    label: "DeepSeek R1 8B (Distill)",
    category: "reasoning",
    categoryLabel: "Raisonnement & Math",
    note: "Raisonnement étape par étape avec chaîne de pensée complète",
    provider: "ollama",
    sizeGb: 4.9,
    tags: ["Raisonnement", "Chaîne de pensée", "Maths"],
    url: "deepseek-r1:8b",
  },
  {
    id: "deepseek-r1:14b",
    name: "deepseek-r1:14b",
    label: "DeepSeek R1 14B (Distill)",
    category: "reasoning",
    categoryLabel: "Raisonnement & Math",
    note: "Raisonnement profond et logique mathématique avancée",
    provider: "ollama",
    sizeGb: 9.0,
    tags: ["Raisonnement", "14B", "Logique"],
    url: "deepseek-r1:14b",
  },

  // ── Vision & OCR ──
  {
    id: "llava:7b",
    name: "llava:7b",
    label: "LLaVA 1.6 7B (Vision)",
    category: "vision",
    categoryLabel: "Vision & OCR",
    note: "Compréhension d'images, lecture de graphiques et diagrammes",
    provider: "ollama",
    sizeGb: 4.7,
    tags: ["Vision", "Multimodal", "OCR"],
    url: "llava:7b",
  },
  {
    id: "minicpm-v:8b",
    name: "minicpm-v:8b",
    label: "MiniCPM-V 2.6 (Vision)",
    category: "vision",
    categoryLabel: "Vision & OCR",
    note: "Haute résolution visuelle et reconnaissance de texte dense",
    provider: "ollama",
    sizeGb: 5.5,
    tags: ["Vision", "Haute résolution"],
    url: "minicpm-v:8b",
  },

  // ── Voix & TTS ──
  {
    id: "hexgrad__Kokoro-82M",
    name: "hexgrad__Kokoro-82M",
    label: "Kokoro-82M (TTS)",
    category: "voice",
    categoryLabel: "Synthèse Vocale & TTS",
    note: "Qualité vocale studio naturelle, intonation humaine, 82M",
    provider: "huggingface",
    sizeGb: 0.3,
    tags: ["TTS", "Naturel", "Studio"],
    url: "https://huggingface.co/hexgrad/Kokoro-82M",
  },
  {
    id: "Qwen__Qwen3-TTS-12Hz-1.7B-CustomVoice",
    name: "Qwen__Qwen3-TTS-12Hz-1.7B-CustomVoice",
    label: "Qwen3-TTS 1.7B (Clonage)",
    category: "voice",
    categoryLabel: "Synthèse Vocale & TTS",
    note: "Synthèse expressive et clonage de voix à partir d'échantillons",
    provider: "huggingface",
    sizeGb: 3.4,
    tags: ["Clonage", "TTS", "Expressif"],
    url: "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
  },

  // ── Musique, Vidéo & 3D ──
  {
    id: "facebook/musicgen-small",
    name: "musicgen-small",
    label: "MusicGen Small",
    category: "audio",
    categoryLabel: "Musique & Audio",
    note: "Génération de pistes musicales instrumentales à partir d'un prompt",
    provider: "huggingface",
    sizeGb: 1.8,
    tags: ["Musique", "Meta", "Audio"],
    url: "https://huggingface.co/facebook/musicgen-small",
  },
  {
    id: "stabilityai/stable-video-diffusion-img2vid-xt",
    name: "stable-video-diffusion-img2vid",
    label: "Stable Video Diffusion (SVD)",
    category: "video_3d",
    categoryLabel: "Vidéo & 3D",
    note: "Animation et génération de vidéo fluide à partir d'une image",
    provider: "huggingface",
    sizeGb: 4.8,
    tags: ["Vidéo", "Animation", "SVD"],
    url: "https://huggingface.co/stabilityai/stable-video-diffusion-img2vid-xt",
  },
];

export function Models({ onBack, initialTab }: Props) {
  const [tab, setTab] = useState<Tab>(initialTab ?? "installed");
  const [voices, setVoices] = useState<MediaModel[] | null>(null);
  const [llmModels, setLlmModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>("all");

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

  async function install(m: MarketplaceModelItem) {
    setBusy(m.url);
    setError(null);
    setProgress({
      downloaded: 0,
      total: null,
      percentage: 0,
      message: "Démarrage du téléchargement…",
    });
    try {
      await api.pullModel(m.url, setProgress);
      notifyModelDownloaded(m.name || m.label);
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
    return MARKETPLACE_CATALOGUE.filter((item) => {
      if (categoryFilter !== "all" && item.category !== categoryFilter) return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchLabel = item.label.toLowerCase().includes(q);
        const matchNote = item.note.toLowerCase().includes(q);
        const matchName = item.name.toLowerCase().includes(q);
        const matchTags = item.tags.some((t) => t.toLowerCase().includes(q));
        if (!matchLabel && !matchNote && !matchName && !matchTags) return false;
      }
      return true;
    });
  }, [categoryFilter, searchQuery]);

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
          <div className="lo-progress-bar">
            <div
              className="lo-progress-fill"
              style={{
                width: progress.percentage !== null ? `${progress.percentage}%` : "100%",
              }}
            />
          </div>
          {progress.message && <p className="lo-hint">{progress.message}</p>}
        </div>
      )}

      {error && <p className="lo-error">{error}</p>}

      {/* ── Onglet : Installés ── */}
      {tab === "installed" && (
        <>
          {/* Section : Modèles de conversation & code (LLM) */}
          <section className="lo-section">
            <h2 className="lo-section-title">Conversation & Code (LLM)</h2>
            {llmModels.length === 0 && (
              <p className="lo-sub">Aucun modèle LLM détecté sur le serveur.</p>
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

          {/* Section : Synthèse Vocale & Voix */}
          <section className="lo-section" style={{ marginTop: "var(--space-4)" }}>
            <h2 className="lo-section-title">Voix & Synthèse Vocale (TTS)</h2>
            {voices === null && !error && <p className="lo-sub">Chargement…</p>}
            {voices?.length === 0 && <p className="lo-sub">Aucune voix installée.</p>}
            <ul className="lo-cards">
              {voices?.map((m) => (
                <li key={m.name} className="lo-card">
                  <div className="lo-card-text">
                    <span className="lo-card-title">{m.name}</span>
                    <span className="lo-hint">Prêt pour la synthèse audio</span>
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
        </>
      )}

      {/* ── Onglet : Marketplace / Catalogue ── */}
      {tab === "marketplace" && (
        <>
          <div className="lo-search-box">
            <input
              type="text"
              className="lo-search-input"
              placeholder="Rechercher dans le marketplace (DeepSeek, FLUX, Qwen, Kokoro…)"
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
                ✕
              </button>
            )}
          </div>

          <div className="lo-chips" style={{ marginTop: 8 }}>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "all" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("all")}
            >
              Tous
            </button>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "llm" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("llm")}
            >
              Chat & LLM
            </button>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "code" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("code")}
            >
              Code & Dev
            </button>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "reasoning" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("reasoning")}
            >
              Raisonnement & Math
            </button>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "vision" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("vision")}
            >
              Vision & OCR
            </button>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "voice" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("voice")}
            >
              Voix & TTS
            </button>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "audio" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("audio")}
            >
              Musique
            </button>
            <button
              type="button"
              className={`lo-chip ${categoryFilter === "video_3d" ? "lo-chip-active" : ""}`}
              onClick={() => setCategoryFilter("video_3d")}
            >
              Vidéo & 3D
            </button>
          </div>

          <p className="lo-hint" style={{ margin: "6px 0 10px 0" }}>
            Les modèles de chat et les extensions sont téléchargés sur votre serveur Locaryn.
          </p>

          <ul className="lo-cards">
            {filteredMarketplace.map((c) => {
              const on = installedSet.has(c.id) || installedSet.has(c.name);
              const working = busy === c.url;

              return (
                <li
                  key={c.id}
                  className="lo-card"
                  style={{ flexDirection: "column", alignItems: "stretch" }}
                >
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "flex-start",
                      gap: 8,
                    }}
                  >
                    <div className="lo-card-text">
                      <div
                        style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}
                      >
                        <span className="lo-card-title">{c.label}</span>
                        <span
                          style={{
                            fontSize: 11,
                            padding: "1px 6px",
                            borderRadius: 4,
                            background: "rgba(255, 255, 255, 0.08)",
                            color: "var(--text-dim)",
                          }}
                        >
                          {c.categoryLabel}
                        </span>
                        <span style={{ fontSize: 11, color: "var(--text-faint)" }}>
                          ~{c.sizeGb} Go
                        </span>
                      </div>
                      <span className="lo-hint" style={{ marginTop: 2 }}>
                        {c.note}
                      </span>
                      <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 4 }}>
                        {c.tags.map((t) => (
                          <span
                            key={t}
                            style={{
                              fontSize: 10,
                              padding: "1px 5px",
                              borderRadius: 4,
                              background: "rgba(var(--accent-rgb), 0.1)",
                              color: "var(--accent)",
                            }}
                          >
                            {t}
                          </span>
                        ))}
                      </div>
                    </div>

                    <div style={{ flex: "none" }}>
                      {on ? (
                        <span
                          style={{
                            fontSize: 11,
                            padding: "3px 8px",
                            borderRadius: 10,
                            background: "rgba(var(--accent-rgb), 0.15)",
                            color: "var(--accent)",
                            fontWeight: 600,
                          }}
                        >
                          Installé
                        </span>
                      ) : (
                        <button
                          type="button"
                          className="lo-btn-small lo-btn-small-on"
                          disabled={working || busy !== null}
                          onClick={() => void install(c)}
                        >
                          {working ? "Téléchargement…" : "Obtenir"}
                        </button>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
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
                ✕
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
