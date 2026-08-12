import { useEffect, useMemo, useState } from "react";
import { core } from "../lib/core";
import {
  MODEL_CATEGORIES,
  type ModelCategory,
  type ModelFamily,
  SIZE_BUCKETS,
  clearRegistryCache,
  fetchFullRegistry,
  fetchHuggingFaceModels,
  fetchHuggingFaceTTSModels,
  isCloudOnlyFamily,
  looksLikeImageModel,
} from "../lib/modelRegistry";
import { classifyModel, nsfwReason } from "../lib/modelSafety";
import { HardwareBenchmarkModal } from "./HardwareBenchmarkModal";
import { ModelObliterator } from "./ModelObliterator";
import { ResponsibilityGate } from "./ResponsibilityGate";

type Props = {
  /** Download / Install model to local storage. */
  onInstall: (
    tag: string,
    onProgress?: (pct: number) => void,
    heretic?: boolean,
    consent?: boolean,
  ) => Promise<void> | void;
  /** Cancel an active model download in progress. */
  onCancelInstall?: () => Promise<void> | void;
  /** Delete an installed model locally. */
  onDelete?: (tag: string) => Promise<void> | void;
  /** Model tags actually available on the server (for "installed" flags). */
  installed?: string[];
  /** Open training / obliteration view. */
  onOpenTraining?: () => void;
  /** Select an installed model for Chat view. */
  onSelectModelForChat?: (tag: string) => void;
  /** Open ImageGen modal. */
  onOpenImageGen?: () => void;
  /** Launch an AirLLM model: activates the AirLlm provider and opens Chat. */
  onLaunchAirllm?: (repo: string) => void;
};

/**
 * Curated AirLLM-compatible models (HF repo + full-precision size). AirLLM
 * loads one transformer layer at a time, so a 4 Go de VRAM suffit même pour
 * des modèles de 70B+ ; les MoE (Kimi K3, Qwen3-235B) streament un expert à
 * la fois (~3 Go de VRAM). Tous ces repos sont ouverts (pas de token gated) :
 * si un repo devient gated, l'install échouera avec un message clair.
 *
 * `sizeGb` = poids fp16 bruts du dépôt. AirLLM convertit ensuite les couches
 * en shards (≈2× l'espace pendant la première conversion, puis l'original
 * peut être supprimé via `delete_original`).
 */
const AIRLLM_MODELS: Record<string, { repo: string; sizeGb: number }> = {
  "kimi-k3": { repo: "moonshotai/Kimi-K3", sizeGb: 1450 }, // 2,8T MoE MXFP4, ~1,4 To — ~3 Go VRAM (expert streaming)
  qwen3: { repo: "Qwen/Qwen3-235B-A22B-Instruct-2507", sizeGb: 470 }, // 235B MoE, ~3 Go VRAM
  llama4: { repo: "meta-llama/Llama-4-Scout-17B-16E-Instruct", sizeGb: 34 }, // 109B MoE
  "llama3.3": { repo: "meta-llama/Llama-3.3-70B-Instruct", sizeGb: 140 }, // gated Meta — token HF requis
  "llama3.1": { repo: "meta-llama/Llama-3.1-70B-Instruct", sizeGb: 140 }, // gated Meta — token HF requis
  "qwen2.5": { repo: "Qwen/Qwen2.5-72B-Instruct", sizeGb: 145 },
  qwen2_5: { repo: "Qwen/Qwen2.5-72B-Instruct", sizeGb: 145 },
  "deepseek-r1": { repo: "deepseek-ai/DeepSeek-R1-Distill-Llama-70B", sizeGb: 140 },
  deepseek_r1: { repo: "deepseek-ai/DeepSeek-R1-Distill-Llama-70B", sizeGb: 140 },
  "mistral-nemo": { repo: "mistralai/Mistral-Nemo-Instruct-2407", sizeGb: 27 },
  mistral: { repo: "mistralai/Mistral-7B-Instruct-v0.3", sizeGb: 30 },
};

function capBadges(f: ModelFamily) {
  const caps: string[] = [];
  const isCloud = f.variants.length > 0 && f.variants.every((v) => v.quants.includes("cloud"));
  if (isCloud) caps.push("☁️ Cloud");
  if (f.imageGen) caps.push("🎨 Image Gen");
  if (f.tts) caps.push("🎙️ TTS");
  if (f.voiceCloning) caps.push("🎭 Clonage");
  if (f.videoGen) caps.push("🎬 Vidéo");
  if (f.musicGen) caps.push("🎵 Musique");
  if (f.model3d) caps.push("🧩 3D");
  if (f.translation) caps.push("🌐 Traduction");
  if (f.objectDetection) caps.push("🎯 Détection");
  if (f.textAnalysis) caps.push("📊 Texte");
  if (f.imageEditing) caps.push("✏️ Édition");
  if (f.questionAnswering) caps.push("❓ Q&R");
  if (f.vision) caps.push("🖼️ Vision");
  if (f.audio) caps.push("🔊 Audio");
  if (f.code) caps.push("💻 Code");
  if (f.reasoning) caps.push("🧠 Raisonnement");
  if (f.instruct) caps.push(" Instruct");
  return caps;
}

function getQuantTag(baseTag: string, quant: string): string {
  if (!quant || quant === "default") return baseTag;
  if (quant === "cloud") return baseTag;

  // Expand hf.co/ shorthand to a real URL so the backend can download it.
  const tag = baseTag.startsWith("hf.co/")
    ? baseTag.replace("hf.co/", "https://huggingface.co/")
    : baseTag;

  if (tag.startsWith("http://") || tag.startsWith("https://")) {
    if (tag.match(/-(q[4568]_[a-z0-9_]+|f16|fp16)\.gguf$/i)) {
      return tag.replace(/-(q[4568]_[a-z0-9_]+|f16|fp16)\.gguf$/i, `-${quant}.gguf`);
    }
    return tag;
  }

  const qLower = quant.toLowerCase();
  if (/-(q[4568]_[a-z0-9_]+|fp16|f16)$/i.test(baseTag)) {
    return baseTag.replace(/-(q[4568]_[a-z0-9_]+|fp16|f16)$/i, `-${qLower}`);
  }

  if (qLower === "q4_k_m" || qLower === "q4_0") {
    return baseTag;
  }
  return `${baseTag}-${qLower}`;
}

function getQuantStorageGb(baseStorageGb: number, quant: string): number {
  if (!quant || quant === "default") return baseStorageGb;
  const q = quant.toLowerCase();
  if (q.includes("q4")) return Math.round(baseStorageGb * 10) / 10;
  if (q.includes("q5")) return Math.round(baseStorageGb * 1.15 * 10) / 10;
  if (q.includes("q6")) return Math.round(baseStorageGb * 1.3 * 10) / 10;
  if (q.includes("q8")) return Math.round(baseStorageGb * 1.6 * 10) / 10;
  if (q.includes("fp16") || q.includes("f16")) return Math.round(baseStorageGb * 2.8 * 10) / 10;
  return baseStorageGb;
}

// ── Hardware compatibility ────────────────────────────────────────────────
// Turns a model's memory footprint + the detected PC into an intuitive verdict
// so the user never has to fiddle with size filters to know what will run.
type HwSpec = { total_ram_gb: number; total_vram_gb: number };
type CompatLevel = "cloud" | "gpu" | "offload" | "airllm" | "heavy" | "unknown";
type Compat = { level: CompatLevel; label: string; short: string; color: string; icon: string };

const COMPAT_RANK: Record<CompatLevel, number> = {
  cloud: 0,
  gpu: 1,
  offload: 2,
  airllm: 3,
  heavy: 4,
  unknown: 5,
};

/** localStorage key for the user's favorite models (stable across refreshes). */
const FAVORITES_KEY = "locaryn_model_favorites_v1";
/** localStorage key for the AirLLM (low-VRAM inference engine) toggle. */
const AIRLLM_KEY = "locaryn_model_airllm_v1";

/**
 * Hardware verdict for one variant.
 * With `airllm` on, models that would not run on this PC (too heavy for its
 * VRAM/RAM) are converted to AirLLM execution — the open-source engine that
 * loads transformer layers one at a time so a 4 GB VRAM GPU can run 70B+.
 */
function variantCompat(storageGb: number, hw: HwSpec | null, airllm = false): Compat {
  if (storageGb === 0) {
    return {
      level: "cloud",
      label: "Modèle cloud — exécution distante, aucun stockage local requis",
      short: "Cloud",
      color: "#60a5fa",
      icon: "☁️",
    };
  }
  if (!hw) {
    return {
      level: "unknown",
      label: "Analyse PC requise pour estimer",
      short: "?",
      color: "var(--text-faint)",
      icon: "•",
    };
  }
  const vram = hw.total_vram_gb || 0;
  const ram = hw.total_ram_gb || 0;
  const need = storageGb * 1.15; // weights + ~15% for KV cache / overhead
  if (vram > 0 && need <= vram) {
    return {
      level: "gpu",
      label: "Tient dans votre VRAM — fluide sur GPU",
      short: "Fluide GPU",
      color: "#5aa86a",
      icon: "🟢",
    };
  }
  if (need <= ram * 0.85) {
    return {
      level: "offload",
      label: "Trop gros pour la VRAM, mais tourne via la RAM (offload CPU, plus lent)",
      short: "OK via RAM",
      color: "#d4a03a",
      icon: "🟡",
    };
  }
  if (airllm) {
    return {
      level: "airllm",
      label:
        "Trop lourd pour la VRAM/RAM de ce PC, mais exécutable localement via AirLLM " +
        "(chargement des couches une par une — un GPU 4 Go de VRAM suffit)",
      short: "AirLLM",
      color: "#a78bfa",
      icon: "🟣",
    };
  }
  return {
    level: "heavy",
    label: "Dépasse la mémoire de ce PC — non recommandé",
    short: "Trop lourd",
    color: "#cc7d72",
    icon: "🔴",
  };
}

function familyBestCompat(
  variants: { storageGb: number }[],
  hw: HwSpec | null,
  airllm = false,
): Compat {
  if (variants.length === 0) return variantCompat(0, hw, airllm);
  const best = variants.reduce((a, b) => (a.storageGb <= b.storageGb ? a : b));
  return variantCompat(best.storageGb, hw, airllm);
}

/**
 * Rough AirLLM throughput estimate (tokens/s) for a converted (too heavy)
 * variant on this PC. AirLLM loads transformer layers one at a time, so the
 * dominant costs are layer reloads (VRAM residency, RAM cache, disk stream)
 * plus GPU compute. MoE models only move their active experts per token.
 */
function estimateAirllmTokPerSec(
  storageGb: number,
  hw: HwSpec | null,
  sizeLabel: string,
  fallbackSizeGb?: number,
): number | null {
  if (!hw) return null;
  const isMoe = /moe/i.test(sizeLabel);
  const vram = Math.max(hw.total_vram_gb || 0, 1); // AirLLM still needs some GPU
  const ram = Math.max(hw.total_ram_gb || 0, 1);
  // Cloud-only variants (storageGb = 0) fall back to the curated repo size.
  // Réaffecter le paramètre masquait la valeur reçue : la taille effective
  // porte donc son propre nom.
  const effectiveGb = storageGb <= 0 && fallbackSizeGb ? fallbackSizeGb : storageGb;
  const modelGb = Math.max(effectiveGb, 0.5);
  // Base curve (dense, mid GPU): ~21 tok/s for a 5 GB model, ~2.5 tok/s for 40 GB.
  let tok = 90 / modelGb ** 0.9;
  // More VRAM keeps more layers resident → fewer reloads per token.
  tok *= Math.min(1.15, 0.65 + 0.09 * vram);
  // RAM caches layers between passes; beyond RAM, layers stream from disk.
  const cached = Math.min(1, (ram * 0.75) / modelGb);
  tok *= 0.55 + 0.45 * cached;
  // MoE only moves the active experts per token → much faster.
  if (isMoe) tok *= 8;
  // CPU-only: no GPU compute, noticeably slower.
  if (!(hw.total_vram_gb > 0)) tok *= 0.5;
  return Math.max(tok, 0.05);
}

function fmtTokPerSec(t: number): string {
  return t >= 10 ? `${Math.round(t)} tok/s` : `${t.toFixed(1)} tok/s`;
}

function isVariantInstalled(tag: string, installedSet: Set<string>): boolean {
  if (installedSet.has(tag)) return true;
  if (installedSet.has(`${tag}:latest`)) return true;
  const fileName = tag.startsWith("http") ? tag.split("/").pop()! : tag;
  if (installedSet.has(fileName)) return true;

  // HuggingFace repo URLs (e.g. https://huggingface.co/coqui/XTTS-v2) are
  // downloaded as ZIP archives and extracted into a subdirectory named
  // "author__repo". list_models reports files inside as "author__repo/file".
  // Detect that format so repo-based TTS models show as installed.
  if (tag.startsWith("https://huggingface.co/")) {
    const repoPart = tag.replace("https://huggingface.co/", "").replace(/\/+$/, "");
    const dirName = repoPart.replace("/", "__");
    for (const inst of installedSet) {
      if (inst.startsWith(`${dirName}/`)) return true;
    }
  }

  for (const inst of installedSet) {
    if (inst === fileName || inst === tag || inst.startsWith(`${tag}-`)) {
      return true;
    }
  }
  return false;
}

export function ModelBrowser({
  onInstall,
  onCancelInstall,
  onDelete,
  onOpenTraining,
  onSelectModelForChat,
  onOpenImageGen,
  onLaunchAirllm,
  installed = [],
}: Props) {
  const [query, setQuery] = useState("");
  const [customTagInput, setCustomTagInput] = useState("");
  const [category, setCategory] = useState<ModelCategory>("all");
  const [brand, setBrand] = useState("all");
  const [size, setSize] = useState("all");
  const [yearFilter, setYearFilter] = useState("all");
  const [sortBy, setSortBy] = useState<"compat" | "newest" | "name" | "pulls">("compat");
  const [onlyFinetunable, setOnlyFinetunable] = useState(false);
  const [riskFilter, setRiskFilter] = useState<"all" | "safe" | "uncensored" | "nsfw">("all");
  const [onlyRecommended, setOnlyRecommended] = useState(false);
  // AirLLM : basculer l'affichage pour que les modèles qui ne tourneraient pas
  // sur ce PC (trop lourds) soient convertis en exécutables via le moteur
  // AirLLM (inférence basse VRAM, chargement des couches une par une).
  const [airllmEnabled, setAirllmEnabled] = useState<boolean>(() => {
    try {
      return localStorage.getItem(AIRLLM_KEY) === "1";
    } catch {
      return false;
    }
  });
  const [airllmModalOpen, setAirllmModalOpen] = useState(false);
  const [airllmInstalled, setAirllmInstalled] = useState<Set<string>>(new Set());
  const [airllmBusy, setAirllmBusy] = useState<Record<string, boolean>>({});
  const [airllmLog, setAirllmLog] = useState<string[]>([]);
  const [airllmError, setAirllmError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"list" | "grid">("grid");
  const [selectedQuants, setSelectedQuants] = useState<Record<string, string>>({});

  // Favorites — persisted in localStorage so they survive refreshes/restarts.
  const [favorites, setFavorites] = useState<Set<string>>(() => {
    try {
      const raw = localStorage.getItem(FAVORITES_KEY);
      return raw ? new Set(JSON.parse(raw) as string[]) : new Set();
    } catch {
      return new Set();
    }
  });
  const [onlyFavorites, setOnlyFavorites] = useState(false);

  useEffect(() => {
    try {
      localStorage.setItem(FAVORITES_KEY, JSON.stringify([...favorites]));
    } catch {
      // localStorage unavailable — favorites just won't persist.
    }
  }, [favorites]);

  useEffect(() => {
    try {
      localStorage.setItem(AIRLLM_KEY, airllmEnabled ? "1" : "0");
    } catch {
      // localStorage unavailable — the toggle just won't persist.
    }
  }, [airllmEnabled]);

  function toggleFavorite(id: string) {
    setFavorites((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  // Dynamic API Models & Registry
  const [registryModels, setRegistryModels] = useState<ModelFamily[]>([]);
  const [isLoadingRegistry, setIsLoadingRegistry] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);
  const [liveApiModels, setLiveApiModels] = useState<ModelFamily[]>([]);
  const [isFetchingLive, setIsFetchingLive] = useState(false);

  const [expandedCards, setExpandedCards] = useState<Record<string, boolean>>({});
  const [openId, setOpenId] = useState<string | null>(null);

  const [installProgress, setInstallProgress] = useState<Record<string, number>>({});
  const [deletingTag, setDeletingTag] = useState<string | null>(null);

  const [obliteratorOpen, setObliteratorOpen] = useState(false);
  const [hardwareModalOpen, setHardwareModalOpen] = useState(false);
  const [hardwareSpec, setHardwareSpec] = useState<{
    total_ram_gb: number;
    total_vram_gb: number;
    gpu_vendor?: string;
  } | null>(null);
  const [nsfwGateOpen, setNsfwGateOpen] = useState(false);
  const [pendingNsfwInstall, setPendingNsfwInstall] = useState<{
    tag: string;
    heretic: boolean;
  } | null>(null);

  const installedSet = useMemo(() => new Set(installed), [installed]);

  // Refresh the list of AirLLM-downloaded models on mount.
  useEffect(() => {
    let active = true;
    core
      .airllmInstalled()
      .then((models) => {
        if (active) setAirllmInstalled(new Set(models.map((m) => m.repo)));
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  // Auto-detect the PC on mount so compatibility is shown without the user
  // having to open the hardware modal or toggle any filter.
  useEffect(() => {
    let active = true;
    core
      .checkHardware()
      .then((hw) => {
        if (active && hw)
          setHardwareSpec({
            total_ram_gb: hw.total_ram_gb,
            total_vram_gb: hw.total_vram_gb,
            gpu_vendor: hw.gpu_vendor,
          });
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  // Load full registry dynamically on mount
  useEffect(() => {
    let active = true;
    setIsLoadingRegistry(true);
    fetchFullRegistry((q, cat) => core.searchOllamaLibrary(q, cat))
      .then((res) => {
        if (active) {
          setRegistryModels(res.families);
          setLastUpdated(res.lastFetched ?? Date.now());
          setIsLoadingRegistry(false);
        }
      })
      .catch(() => {
        if (active) setIsLoadingRegistry(false);
      });
    return () => {
      active = false;
    };
  }, []);

  // Background refresh: re-sync with Ollama and HuggingFace every hour even
  // while this browser stays open (no remount needed). fetchFullRegistry only
  // refetches when the 1h cache is stale, so a manual refresh mid-hour is
  // never doubled — the tick just returns the fresh cache.
  useEffect(() => {
    const interval = setInterval(
      () => {
        fetchFullRegistry((q, cat) => core.searchOllamaLibrary(q, cat))
          .then((res) => {
            setRegistryModels(res.families);
            setLastUpdated(res.lastFetched ?? Date.now());
          })
          .catch(() => {
            // Silent: keep showing the last good list on network hiccups.
          });
      },
      60 * 60 * 1000,
    );
    return () => clearInterval(interval);
  }, []);

  const allBrands = useMemo(() => {
    const combined = [...registryModels, ...liveApiModels];
    return Array.from(new Set(combined.map((f) => f.brand))).sort();
  }, [registryModels, liveApiModels]);

  const years = useMemo(() => {
    const combined = [...registryModels, ...liveApiModels];
    const ySet = new Set(combined.map((f) => f.releaseYear));
    return Array.from(ySet).sort((a, b) => b - a);
  }, [registryModels, liveApiModels]);

  const families = useMemo(() => {
    const q = query.trim().toLowerCase();
    const bucket = SIZE_BUCKETS.find((b) => b.id === size);
    const catalogSource = [...registryModels, ...liveApiModels];

    return catalogSource
      .map((f) => {
        const matchingVariants = f.variants.filter((v) => {
          if (bucket && !bucket.test(v.params)) return false;
          if (onlyRecommended) {
            const ram = hardwareSpec?.total_ram_gb || 16;
            // A model is recommended if its storage requirement doesn't exceed 85% of total system RAM.
            // With AirLLM enabled, heavy models still run locally (layer-by-layer
            // offloading), so they pass too.
            if (!airllmEnabled && v.storageGb > ram * 0.85) return false;
          }
          return true;
        });

        if (matchingVariants.length === 0) return null;

        if (category === "code" && !f.code) return null;
        if (category === "vision" && !f.vision) return null;
        if (category === "reasoning" && !f.reasoning) return null;
        if (category === "image-gen" && !f.imageGen) return null;
        if (category === "speech-synthesis" && !f.tts) return null;
        if (category === "video-generation" && !f.videoGen) return null;
        if (category === "language-translation" && !f.translation) return null;
        if (category === "3d-modeling" && !f.model3d) return null;
        if (category === "music-generation" && !f.musicGen) return null;
        if (category === "object-detection" && !f.objectDetection) return null;
        if (category === "text-analysis" && !f.textAnalysis) return null;
        if (category === "image-editing" && !f.imageEditing) return null;
        if (category === "question-answering" && !f.questionAnswering) return null;
        if (category === "audio" && !f.audio && !f.tts && !f.musicGen) return null;
        if (category === "chat" && !f.instruct) return null;

        if (brand !== "all" && f.brand !== brand) return null;
        if (yearFilter !== "all" && f.releaseYear.toString() !== yearFilter) return null;
        if (onlyFinetunable && !f.finetunable) return null;
        if (riskFilter !== "all") {
          const risk = classifyModel(`${f.name} ${f.id}`, { uncensored: f.uncensored }).risk;
          if (riskFilter === "safe" && risk !== "safe") return null;
          if (riskFilter === "uncensored" && risk !== "uncensored") return null;
          if (riskFilter === "nsfw" && risk !== "nsfw") return null;
        }
        if (
          q &&
          !`${f.name} ${f.brand} ${f.description} ${f.releaseDate}`.toLowerCase().includes(q)
        ) {
          return null;
        }

        // Cloud-only families are never shown : cette app est 100 % locale,
        // aucun LLM distant n'est proposé.
        if (isCloudOnlyFamily(f)) {
          return null;
        }
        if (onlyFavorites && !favorites.has(f.id)) {
          return null;
        }

        return {
          ...f,
          variants: matchingVariants,
        };
      })
      .filter((f): f is ModelFamily => f !== null)
      .sort((a, b) => {
        if (sortBy === "compat") {
          // Compatible-first: models that run on this PC float to the top,
          // then newest within the same compatibility tier. With AirLLM on,
          // heavy models convert to AirLLM execution and move up.
          const ra = COMPAT_RANK[familyBestCompat(a.variants, hardwareSpec, airllmEnabled).level];
          const rb = COMPAT_RANK[familyBestCompat(b.variants, hardwareSpec, airllmEnabled).level];
          if (ra !== rb) return ra - rb;
          return b.releaseDate.localeCompare(a.releaseDate);
        }
        if (sortBy === "pulls") {
          return (b.pulls || 0) - (a.pulls || 0);
        }
        if (sortBy === "newest") {
          return b.releaseDate.localeCompare(a.releaseDate);
        }
        return a.name.localeCompare(b.name);
      });
  }, [
    query,
    category,
    brand,
    size,
    yearFilter,
    sortBy,
    onlyFinetunable,
    riskFilter,
    onlyRecommended,
    onlyFavorites,
    favorites,
    registryModels,
    liveApiModels,
    hardwareSpec,
    airllmEnabled,
  ]);

  function toggleCardExpand(id: string) {
    setExpandedCards((prev) => ({ ...prev, [id]: !prev[id] }));
  }

  async function handleFetchLiveApiModels() {
    setIsFetchingLive(true);
    try {
      const q = query.trim() || "gguf";
      // Fetch both GGUF chat models and safetensors TTS/voice repos so the
      // live search also surfaces voice models (Pocket TTS, etc.).
      const [gguf, tts] = await Promise.all([
        fetchHuggingFaceModels(q),
        fetchHuggingFaceTTSModels(q === "gguf" ? "tts" : q),
      ]);
      setLiveApiModels([...gguf, ...tts]);
    } finally {
      setIsFetchingLive(false);
    }
  }

  /** `heretic` = uncensored family: the backend then auto-installs the
   *  abliterated companions so the model works without any manual setup. */
  function requestInstall(tag: string, familyName: string, heretic: boolean) {
    if (classifyModel(`${familyName} ${tag}`).risk !== "safe") {
      setPendingNsfwInstall({ tag, heretic });
      setNsfwGateOpen(true);
    } else {
      handleInstallModel(tag, heretic);
    }
  }

  async function handleInstallModel(tag: string, heretic?: boolean, consent = false) {
    setInstallProgress((prev) => ({ ...prev, [tag]: 0 }));
    try {
      await onInstall(
        tag,
        (pct) => {
          setInstallProgress((prev) => ({ ...prev, [tag]: pct }));
        },
        heretic,
        consent,
      );
    } finally {
      setInstallProgress((prev) => {
        const copy = { ...prev };
        delete copy[tag];
        return copy;
      });
    }
  }

  function confirmNsfwInstall() {
    setNsfwGateOpen(false);
    if (pendingNsfwInstall) {
      const { tag, heretic } = pendingNsfwInstall;
      setPendingNsfwInstall(null);
      handleInstallModel(tag, heretic, true);
    }
  }

  async function handleCancelInstall(tag: string) {
    try {
      await onCancelInstall?.();
    } finally {
      setInstallProgress((prev) => {
        const copy = { ...prev };
        delete copy[tag];
        return copy;
      });
    }
  }

  async function handleDeleteModel(tag: string) {
    if (
      !window.confirm(
        `Voulez-vous vraiment supprimer le modèle local "${tag}" pour libérer de l'espace disque ?`,
      )
    ) {
      return;
    }
    setDeletingTag(tag);
    try {
      await onDelete?.(tag);
    } finally {
      setDeletingTag(null);
    }
  }

  /** Make sure the AirLLM Python runtime is installed (venv + package). */
  async function ensureAirllmRuntime(onLine: (l: string) => void) {
    const st = await core.airllmStatus();
    if (!st.python) {
      throw new Error("Python introuvable — installez Python 3.10+ (Paramètres → Système).");
    }
    if (!st.airllmInstalled || !st.torch) {
      onLine("Installation du moteur AirLLM (pip install airllm)…");
      await core.airllmSetup(onLine);
    }
  }

  /** Install (if needed) then launch an AirLLM model. */
  async function handleAirllmInstall(f: ModelFamily) {
    const entry = AIRLLM_MODELS[f.id];
    if (!entry) {
      setAirllmModalOpen(true);
      return;
    }
    const repo = entry.repo;
    setAirllmError(null);
    setAirllmLog([]);
    setAirllmBusy((prev) => ({ ...prev, [repo]: true }));
    const onLine = (l: string) => setAirllmLog((prev) => [...prev.slice(-40), l]);
    try {
      await ensureAirllmRuntime(onLine);
      const installed = await core.airllmInstalled();
      if (!installed.some((m) => m.repo === repo)) {
        onLine(`Téléchargement des poids ${repo} (~${entry.sizeGb} Go fp16)…`);
        await core.airllmInstall(repo, onLine);
      }
      const after = await core.airllmInstalled();
      setAirllmInstalled(new Set(after.map((m) => m.repo)));
      setAirllmLog((prev) => [...prev, `✅ ${repo} prêt — démarrage du moteur AirLLM…`]);
      await onLaunchAirllm?.(repo);
    } catch (e) {
      setAirllmError(String(e));
    } finally {
      setAirllmBusy((prev) => ({ ...prev, [repo]: false }));
    }
  }

  const isFilterActive =
    size !== "all" ||
    query !== "" ||
    brand !== "all" ||
    onlyRecommended ||
    riskFilter !== "all" ||
    onlyFavorites;

  return (
    <div className="locaryn-models">
      {/* Top Controls & Filters */}
      <div className="locaryn-models-top-bar">
        <div className="locaryn-models-search-row">
          <input
            className="locaryn-input locaryn-input-text"
            placeholder="Rechercher parmi les modèles (Gemma 4, Kimi K3, MiMo, GLM 5.2, DeepSeek-R1, Qwen2.5...)"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <select
            className="locaryn-select"
            value={brand}
            onChange={(e) => setBrand(e.target.value)}
            aria-label="Filtrer par marque"
          >
            <option value="all">Toutes les marques</option>
            {allBrands.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>

          <select
            className="locaryn-select"
            value={yearFilter}
            onChange={(e) => setYearFilter(e.target.value)}
            aria-label="Filtrer par année de sortie"
          >
            <option value="all">Toutes les années</option>
            {years.map((y) => (
              <option key={y} value={y.toString()}>
                Sortie {y}
              </option>
            ))}
          </select>

          <select
            className="locaryn-select"
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value as "compat" | "newest" | "name" | "pulls")}
            aria-label="Trier les modèles"
          >
            <option value="compat">🟢 Compatibles d'abord</option>
            <option value="newest">📅 Plus récents</option>
            <option value="pulls">🔥 Plus populaires (Pulls)</option>
            <option value="name">🔤 Nom (A-Z)</option>
          </select>
        </div>

        {/* AirLLM toggle — converts too-heavy models into low-VRAM executable ones */}
        <div className="locaryn-airllm-bar">
          <button
            type="button"
            role="switch"
            aria-checked={airllmEnabled}
            className={`locaryn-airllm-switch${airllmEnabled ? " locaryn-airllm-on" : ""}`}
            onClick={() => setAirllmEnabled((prev) => !prev)}
            title="Basculer le moteur AirLLM : les modèles trop lourds pour ce PC deviennent exécutables localement (chargement des couches une par une, un GPU 4 Go de VRAM suffit)"
          >
            <span className="locaryn-airllm-track">
              <span className="locaryn-airllm-thumb" />
            </span>
            <span className="locaryn-airllm-label">🔮 AirLLM — Gros modèles sur petit GPU</span>
          </button>
          <span className="locaryn-airllm-hint">
            {airllmEnabled
              ? "Actif : tous les modèles deviennent exécutables — les modèles trop lourds pour ce PC tournent en local via AirLLM (chargement couche par couche, ex. Kimi K3 sur un GPU 4 Go de VRAM)."
              : "Inactif : seuls les modèles tenant dans ce PC sont proposés en téléchargement local."}
          </span>
        </div>

        {/* AirLLM install / runtime progress */}
        {(airllmLog.length > 0 || airllmError) && (
          <div
            style={{
              marginTop: "8px",
              padding: "8px 12px",
              borderRadius: "var(--radius-sm)",
              border: airllmError
                ? "1px solid var(--danger)"
                : "1px solid rgba(167, 139, 250, 0.35)",
              background: airllmError ? "rgba(204, 125, 114, 0.08)" : "rgba(167, 139, 250, 0.06)",
              fontFamily: "var(--font-mono)",
              fontSize: "11px",
              color: airllmError ? "var(--danger)" : "var(--text-dim)",
              maxHeight: "110px",
              overflowY: "auto",
              whiteSpace: "pre-wrap",
            }}
          >
            {airllmError ? `⚠️ ${airllmError}` : airllmLog.slice(-6).join("\n")}
          </div>
        )}

        {/* Category Filter Bar */}
        <div style={{ display: "flex", gap: "6px", flexWrap: "wrap", marginTop: "8px" }}>
          {MODEL_CATEGORIES.map((cat) => (
            <button
              key={cat.id}
              type="button"
              className={`locaryn-chip${category === cat.id ? " locaryn-chip-on" : ""}`}
              onClick={() => setCategory(cat.id)}
            >
              {cat.icon} {cat.label}
            </button>
          ))}
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{ fontSize: "11px", marginLeft: "auto", padding: "2px 8px" }}
            onClick={async () => {
              clearRegistryCache();
              setIsLoadingRegistry(true);
              const res = await fetchFullRegistry((q, cat) => core.searchOllamaLibrary(q, cat));
              setRegistryModels(res.families);
              setLastUpdated(res.lastFetched ?? Date.now());
              setIsLoadingRegistry(false);
            }}
            disabled={isLoadingRegistry}
            title="Vider le cache local et rafraîchir la liste"
          >
            {isLoadingRegistry ? "🔄 Chargement..." : "⚡ Rafraîchir catalogue"}
          </button>
          {lastUpdated && (
            <span
              style={{
                fontSize: "11px",
                color: "var(--text-faint)",
                alignSelf: "center",
                whiteSpace: "nowrap",
              }}
              title="Le catalogue se met à jour automatiquement toutes les heures"
            >
              🔄 MAJ {new Date(lastUpdated).toLocaleTimeString()}
            </span>
          )}
        </div>

        {/* Custom Model Tag & HuggingFace Pull Input + Live API Fetch Button */}
        <div style={{ display: "flex", gap: "8px", marginTop: "8px" }}>
          <input
            className="locaryn-input"
            style={{ flex: 1, fontSize: "12px" }}
            placeholder="➕ Télécharger un modèle spécifique ou dépôt HuggingFace (ex: gemma4:2b, kimi-k3:8b, mimo:7b, glm5.2:9b, hf.co/user/repo)..."
            value={customTagInput}
            onChange={(e) => setCustomTagInput(e.target.value)}
          />
          <button
            type="button"
            className="locaryn-btn-primary"
            style={{ fontSize: "12px", whiteSpace: "nowrap" }}
            disabled={!customTagInput.trim()}
            onClick={() => {
              requestInstall(customTagInput.trim(), customTagInput.trim(), false);
              setCustomTagInput("");
            }}
          >
            ⬇️ Télécharger ce modèle
          </button>
          <button
            type="button"
            className="locaryn-btn-ghost"
            style={{
              fontSize: "12px",
              border: "1px solid var(--accent)",
              color: "var(--accent)",
              whiteSpace: "nowrap",
            }}
            onClick={handleFetchLiveApiModels}
            disabled={isFetchingLive}
            title="Interroger directement les API HuggingFace Hub pour découvrir les derniers modèles en temps réel"
          >
            {isFetchingLive ? "Recherche en direct..." : "🌐 Recherche API HuggingFace"}
          </button>
        </div>

        <div className="locaryn-models-toolbar" style={{ marginTop: "8px" }}>
          <div className="locaryn-size-chips">
            <button
              type="button"
              className={`locaryn-chip${size === "all" ? " locaryn-chip-on" : ""}`}
              onClick={() => setSize("all")}
            >
              Toutes tailles
            </button>
            {SIZE_BUCKETS.map((b) => (
              <button
                key={b.id}
                type="button"
                className={`locaryn-chip${size === b.id ? " locaryn-chip-on" : ""}`}
                onClick={() => setSize(b.id)}
              >
                {b.label}
              </button>
            ))}

            <button
              type="button"
              className={`locaryn-chip locaryn-chip-ft${onlyRecommended ? " locaryn-chip-on" : ""}`}
              style={
                onlyRecommended
                  ? {
                      background: "rgba(100, 200, 120, 0.2)",
                      borderColor: "#64c878",
                      color: "#64c878",
                    }
                  : {}
              }
              onClick={() => setOnlyRecommended((prev) => !prev)}
              title="Filtrer uniquement les modèles adaptés aux composants de votre PC"
            >
              🟢 Recommandés pour mon PC
            </button>

            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{ fontSize: "11px", marginLeft: "auto", padding: "2px 8px" }}
              onClick={() => setHardwareModalOpen(true)}
              title="Analyser les composants de mon PC pour adapter les recommandations"
            >
              ⚙️ Analyser mon PC
            </button>

            <button
              type="button"
              className={`locaryn-chip locaryn-chip-ft${onlyFinetunable ? " locaryn-chip-on" : ""}`}
              onClick={() => setOnlyFinetunable((prev) => !prev)}
              title="Afficher uniquement les modèles réentraînables via Fine-Tuning / LoRA"
            >
              🎯 Réentraînable / LoRA
            </button>
            <button
              type="button"
              className={`locaryn-chip locaryn-chip-ft${riskFilter === "safe" ? " locaryn-chip-on" : ""}`}
              style={
                riskFilter === "safe"
                  ? {
                      background: "rgba(90, 168, 106, 0.2)",
                      borderColor: "#5aa86a",
                      color: "#5aa86a",
                    }
                  : {}
              }
              onClick={() => setRiskFilter((prev) => (prev === "safe" ? "all" : "safe"))}
              title="Afficher uniquement les modèles classiques avec garde-fous"
            >
              🛡️ Safe
            </button>
            <button
              type="button"
              className={`locaryn-chip locaryn-chip-ft${riskFilter === "uncensored" ? " locaryn-chip-on" : ""}`}
              style={
                riskFilter === "uncensored"
                  ? {
                      background: "rgba(204, 125, 114, 0.25)",
                      borderColor: "var(--danger)",
                      color: "var(--danger)",
                    }
                  : {}
              }
              onClick={() =>
                setRiskFilter((prev) => (prev === "uncensored" ? "all" : "uncensored"))
              }
              title="Afficher uniquement les modèles sans garde-fous / oblitérés"
            >
              🔓 Sans limite
            </button>
            <button
              type="button"
              className={`locaryn-chip locaryn-chip-ft${riskFilter === "nsfw" ? " locaryn-chip-on" : ""}`}
              style={
                riskFilter === "nsfw"
                  ? {
                      background: "rgba(204, 125, 114, 0.25)",
                      borderColor: "var(--danger)",
                      color: "var(--danger)",
                    }
                  : {}
              }
              onClick={() => setRiskFilter((prev) => (prev === "nsfw" ? "all" : "nsfw"))}
              title="Afficher uniquement les modèles NSFW / sans garde-fous connus"
            >
              🔞 NSFW
            </button>
            <button
              type="button"
              className={`locaryn-chip locaryn-chip-ft${onlyFavorites ? " locaryn-chip-on" : ""}`}
              style={
                onlyFavorites
                  ? {
                      background: "rgba(255, 200, 87, 0.18)",
                      borderColor: "#ffc857",
                      color: "#ffc857",
                    }
                  : {}
              }
              onClick={() => setOnlyFavorites((prev) => !prev)}
              title="Afficher uniquement les modèles marqués comme favoris"
            >
              ★ Favoris{favorites.size > 0 ? ` (${favorites.size})` : ""}
            </button>
          </div>

          <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{
                fontSize: "12px",
                border: "1px solid var(--accent)",
                color: "var(--accent)",
              }}
              onClick={() => setHardwareModalOpen(true)}
              title="Tester les composants de votre PC et analyser les performances d'inférence"
            >
              📊 Analyse Perf PC
            </button>

            <button
              type="button"
              className="locaryn-btn-ghost"
              style={{
                color: "var(--danger)",
                fontSize: "12px",
                border: "1px solid rgba(204, 125, 114, 0.3)",
              }}
              onClick={() => {
                if (onOpenTraining) {
                  onOpenTraining();
                } else {
                  setObliteratorOpen(true);
                }
              }}
              title="Ouvrir le studio d'oblitération de modèle dans le menu Entraînement"
            >
              🔓 Studio d'Oblitération RepE
            </button>

            {/* View mode switcher */}
            <div className="locaryn-view-toggle">
              <button
                type="button"
                className={`locaryn-view-toggle-btn ${viewMode === "grid" ? "locaryn-active" : ""}`}
                onClick={() => setViewMode("grid")}
                title="Affichage en Grille / Cartes"
              >
                <svg
                  aria-hidden="true"
                  width="15"
                  height="15"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <rect x="3" y="3" width="7" height="7" />
                  <rect x="14" y="3" width="7" height="7" />
                  <rect x="14" y="14" width="7" height="7" />
                  <rect x="3" y="14" width="7" height="7" />
                </svg>
                Grille
              </button>
              <button
                type="button"
                className={`locaryn-view-toggle-btn ${viewMode === "list" ? "locaryn-active" : ""}`}
                onClick={() => setViewMode("list")}
                title="Affichage en Liste Détaillée"
              >
                <svg
                  aria-hidden="true"
                  width="15"
                  height="15"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <line x1="8" y1="6" x2="21" y2="6" />
                  <line x1="8" y1="12" x2="21" y2="12" />
                  <line x1="8" y1="18" x2="21" y2="18" />
                  <line x1="3" y1="6" x2="3.01" y2="6" />
                  <line x1="3" y1="12" x2="3.01" y2="12" />
                  <line x1="3" y1="18" x2="3.01" y2="18" />
                </svg>
                Liste
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Intuitive hardware compatibility banner — no filter needed */}
      {hardwareSpec ? (
        (() => {
          const counts = families.reduce(
            (acc, f) => {
              acc[familyBestCompat(f.variants, hardwareSpec, airllmEnabled).level]++;
              return acc;
            },
            { gpu: 0, offload: 0, airllm: 0, heavy: 0, unknown: 0 } as Record<
              CompatLevel,
              number
            >,
          );
          return (
            <div className="locaryn-hw-banner">
              <span className="locaryn-hw-banner-pc">
                🖥️ Votre PC&nbsp;: <b>{hardwareSpec.total_ram_gb} Go RAM</b>
                {hardwareSpec.total_vram_gb > 0 && (
                  <>
                    {" "}
                    · <b>{hardwareSpec.total_vram_gb} Go VRAM</b>
                    {hardwareSpec.gpu_vendor && hardwareSpec.gpu_vendor !== "unknown" && (
                      <>
                        {" "}
                        (<span style={{ textTransform: "capitalize" }}>{hardwareSpec.gpu_vendor}</span>)
                      </>
                    )}
                  </>
                )}
              </span>
              <span className="locaryn-hw-banner-counts">
                {airllmEnabled ? (
                  <>
                    <span style={{ color: "#a78bfa" }}>🟣 {counts.airllm} via AirLLM</span>
                    <span style={{ color: "#5aa86a" }}>🟢 {counts.gpu} fluides GPU</span>
                    <span style={{ color: "#d4a03a" }}>🟡 {counts.offload} via RAM</span>
                    <span className="locaryn-hw-banner-note">
                      — AirLLM actif : chaque modèle est exécutable localement
                    </span>
                  </>
                ) : (
                  <>
                    <span style={{ color: "#5aa86a" }}>🟢 {counts.gpu} fluides GPU</span>
                    <span style={{ color: "#d4a03a" }}>🟡 {counts.offload} via RAM</span>
                    <span style={{ color: "#cc7d72" }}>🔴 {counts.heavy} trop lourds</span>
                    <span className="locaryn-hw-banner-note">
                      — triés du plus adapté au plus lourd
                    </span>
                  </>
                )}
              </span>
            </div>
          );
        })()
      ) : (
        <div className="locaryn-hw-banner locaryn-hw-banner-muted">
          <span>
            🖥️ Analyse du PC en cours… la compatibilité de chaque modèle s'affichera automatiquement.
          </span>
        </div>
      )}

      {families.length === 0 && (
        <div className="locaryn-field-hint" style={{ marginTop: "24px", textAlign: "center" }}>
          Aucun modèle ne correspond à vos filtres.
        </div>
      )}

      {/* GRID / BOXES VIEW */}
      {viewMode === "grid" && (
        <div className="locaryn-model-grid">
          {families.map((f) => {
            const isExpanded = Boolean(isFilterActive || expandedCards[f.id]);
            const paramNums = f.variants.map((v) => v.params);
            const minP = Math.min(...paramNums);
            const maxP = Math.max(...paramNums);
            const cleanSizeRange = minP === maxP ? `${minP}B` : `${minP}B – ${maxP}B`;
            const expandLabel = isExpanded
              ? "▲ Masquer les variantes"
              : `▼ ${f.variants.length} variantes (${cleanSizeRange})`;
            const compat = familyBestCompat(f.variants, hardwareSpec, airllmEnabled);
            // Family-level AirLLM speed estimate: best case = smallest variant.
            const bestVariant = f.variants.reduce((a, b) => (a.storageGb <= b.storageGb ? a : b));
            const familyAirSpeed =
              compat.level === "airllm"
                ? estimateAirllmTokPerSec(
                    bestVariant.storageGb,
                    hardwareSpec,
                    bestVariant.size,
                    // Cloud-only families (storageGb = 0) fall back to the
                    // curated AirLLM repo size (e.g. Kimi K3 ≈ 1,45 To).
                    AIRLLM_MODELS[f.id]?.sizeGb,
                  )
                : null;

            return (
              <div key={f.id} className={`locaryn-box-card locaryn-compat-${compat.level}`}>
                <div className="locaryn-box-head">
                  <div style={{ minWidth: 140, flex: "1 1 55%" }}>
                    <span className="locaryn-box-brand">{f.brand}</span>
                    <h3 className="locaryn-box-name">{f.name}</h3>
                  </div>
                  <div className="locaryn-box-badges">
                    <span
                      className="locaryn-tag"
                      style={{
                        background: `${compat.color}22`,
                        color: compat.color,
                        border: `1px solid ${compat.color}66`,
                      }}
                      title={compat.label}
                    >
                      {compat.icon} {compat.short}
                    </span>
                    {familyAirSpeed && (
                      <span
                        className="locaryn-tag"
                        style={{ background: "rgba(167, 139, 250, 0.18)", color: "#a78bfa" }}
                        title="Estimation AirLLM sur ce PC — débit réel selon VRAM / RAM / disque"
                      >
                        ⚡ ~{fmtTokPerSec(familyAirSpeed)}
                      </span>
                    )}
                    <span
                      className="locaryn-tag"
                      style={{ background: "rgba(100, 150, 255, 0.15)", color: "var(--accent)" }}
                    >
                      {cleanSizeRange}
                    </span>
                    <span
                      className="locaryn-tag locaryn-tag-soft"
                      title="Date de sortie officielle"
                    >
                      📅 {f.releaseDate}
                    </span>
                    {(() => {
                      const c = classifyModel(`${f.name} ${f.id}`, { uncensored: f.uncensored });
                      if (c.risk === "safe") return null;
                      return (
                        <span
                          className="locaryn-tag"
                          style={{
                            background: "rgba(204,125,114,0.2)",
                            color: "var(--danger)",
                            border: "1px solid rgba(204,125,114,0.4)",
                          }}
                          title={nsfwReason(`${f.name} ${f.id}`) ?? c.label}
                        >
                          {c.icon} {c.label}
                        </span>
                      );
                    })()}
                    {f.finetunable && (
                      <span
                        className="locaryn-tag locaryn-tag-ft"
                        title="Modèle prêt pour le fine-tuning LoRA"
                      >
                        🎯 LoRA
                      </span>
                    )}
                    {f.source === "huggingface" && (
                      <span
                        className="locaryn-tag"
                        title="Découvert automatiquement sur le Hub HuggingFace"
                        style={{
                          background: "rgba(255, 200, 87, 0.14)",
                          color: "#ffc857",
                          border: "1px solid rgba(255, 200, 87, 0.35)",
                        }}
                      >
                        🤗 HuggingFace
                      </span>
                    )}
                    {capBadges(f).map((c) => (
                      <span key={c} className="locaryn-tag">
                        {c}
                      </span>
                    ))}
                  </div>
                  <button
                    type="button"
                    onClick={() => toggleFavorite(f.id)}
                    aria-pressed={favorites.has(f.id)}
                    title={favorites.has(f.id) ? "Retirer des favoris" : "Ajouter aux favoris"}
                    style={{
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      fontSize: "15px",
                      lineHeight: 1,
                      padding: "2px 4px",
                      color: favorites.has(f.id) ? "#ffc857" : "var(--text-faint)",
                      flex: "none",
                    }}
                  >
                    {favorites.has(f.id) ? "★" : "☆"}
                  </button>
                </div>

                <p className="locaryn-box-desc">{f.description}</p>

                <div className="locaryn-box-stats">
                  {f.contextWindow && (
                    <div className="locaryn-stat-item">
                      <span className="locaryn-stat-label">Contexte</span>
                      <span className="locaryn-stat-value">{f.contextWindow}</span>
                    </div>
                  )}
                  <div className="locaryn-stat-item">
                    <span className="locaryn-stat-label">Licence</span>
                    <span className="locaryn-stat-value">{f.license}</span>
                  </div>
                  <div className="locaryn-stat-item">
                    <span className="locaryn-stat-label">Sortie</span>
                    <span className="locaryn-stat-value">{f.releaseDate}</span>
                  </div>
                </div>

                {!isFilterActive && (
                  <button
                    type="button"
                    className="locaryn-btn-ghost"
                    style={{
                      width: "100%",
                      marginTop: "8px",
                      fontSize: "12px",
                      border: "1px dashed var(--border-strong)",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                    onClick={() => toggleCardExpand(f.id)}
                  >
                    <span
                      style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                    >
                      {expandLabel}
                    </span>
                    <span
                      className="locaryn-tag locaryn-tag-soft"
                      style={{ flex: "none", marginLeft: "6px" }}
                    >
                      {f.variants.length} modèles
                    </span>
                  </button>
                )}

                {isExpanded && (
                  <div className="locaryn-box-variants" style={{ marginTop: "12px" }}>
                    <span className="locaryn-box-variants-title">Variantes & Quantisations :</span>
                    {f.variants.map((v) => {
                      const activeQuant = selectedQuants[v.tag] || v.quants[0] || "q4_K_M";
                      const targetTag = getQuantTag(v.tag, activeQuant);
                      const targetStorageGb = getQuantStorageGb(v.storageGb, activeQuant);
                      const isInstalled =
                        isVariantInstalled(targetTag, installedSet) ||
                        isVariantInstalled(v.tag, installedSet);
                      const progress = installProgress[targetTag] ?? installProgress[v.tag];
                      const isInstalling = progress !== undefined;
                      const isDeleting = deletingTag === targetTag || deletingTag === v.tag;
                      const compatV = variantCompat(targetStorageGb, hardwareSpec, airllmEnabled);

                      return (
                        <div
                          key={v.tag}
                          className="locaryn-box-variant-row"
                          style={{ flexDirection: "column", alignItems: "stretch", gap: "6px" }}
                        >
                          <div
                            style={{
                              display: "flex",
                              justifyContent: "space-between",
                              alignItems: "center",
                            }}
                          >
                            <div className="locaryn-box-variant-info">
                              <span className="locaryn-variant-size">{v.size}</span>
                              <span className="locaryn-stat-vram">💾 ~{targetStorageGb} Go</span>
                              <span
                                className="locaryn-tag"
                                style={{ background: `${compatV.color}22`, color: compatV.color }}
                                title={compatV.label}
                              >
                                {compatV.icon} {compatV.short}
                              </span>
                              {compatV.level === "airllm" && (
                                <span
                                  className="locaryn-tag"
                                  style={{
                                    background: "rgba(167, 139, 250, 0.18)",
                                    color: "#a78bfa",
                                  }}
                                  title="Estimation AirLLM sur ce PC — débit réel selon VRAM / RAM / disque"
                                >
                                  ⚡ ~
                                  {fmtTokPerSec(
                                    estimateAirllmTokPerSec(
                                      targetStorageGb,
                                      hardwareSpec,
                                      v.size,
                                    ) ?? 0,
                                  )}
                                </span>
                              )}
                              {isInstalled && (
                                <span className="locaryn-tag locaryn-tag-installed">
                                  Installé ✓
                                </span>
                              )}
                            </div>

                            <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                              {isInstalled ? (
                                <div style={{ display: "flex", gap: "4px", alignItems: "center" }}>
                                  {f.imageGen || looksLikeImageModel(targetTag) ? (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary"
                                      style={{ padding: "3px 8px", fontSize: "11px" }}
                                      onClick={() => onOpenImageGen?.()}
                                      title="Ouvrir la génération d'images avec ce modèle"
                                    >
                                      🎨 Générer
                                    </button>
                                  ) : (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary"
                                      style={{ padding: "3px 8px", fontSize: "11px" }}
                                      onClick={() => onSelectModelForChat?.(targetTag)}
                                      title="Utiliser ce modèle dans le Chat"
                                    >
                                      💬 Utiliser
                                    </button>
                                  )}
                                  <button
                                    type="button"
                                    className="locaryn-btn-ghost"
                                    style={{
                                      color: "var(--danger)",
                                      padding: "3px 8px",
                                      fontSize: "11px",
                                    }}
                                    onClick={() => handleDeleteModel(targetTag)}
                                    disabled={isDeleting}
                                    title="Supprimer ce modèle du disque dur local"
                                  >
                                    {isDeleting ? "..." : "Supprimer"}
                                  </button>
                                </div>
                              ) : isInstalling ? (
                                <button
                                  type="button"
                                  className="locaryn-btn-ghost"
                                  style={{
                                    color: "var(--danger)",
                                    border: "1px solid var(--danger)",
                                    padding: "3px 8px",
                                    fontSize: "11px",
                                  }}
                                  onClick={() => handleCancelInstall(targetTag)}
                                  title="Annuler le téléchargement en cours"
                                >
                                  ⛔ Annuler ({progress}%)
                                </button>
                              ) : compatV.level === "airllm" ? (
                                (() => {
                                  const entry = AIRLLM_MODELS[f.id];
                                  const installed = entry ? airllmInstalled.has(entry.repo) : false;
                                  const busy = entry ? airllmBusy[entry.repo] : false;
                                  if (!entry) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-ghost"
                                        style={{ border: "1px dashed #a78bfa", color: "#a78bfa" }}
                                        onClick={() => setAirllmModalOpen(true)}
                                        title="Cette architecture n'est pas encore supportée par AirLLM (Llama, Mistral, Qwen2…)"
                                      >
                                        🔮 Info AirLLM
                                      </button>
                                    );
                                  }
                                  if (installed) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-primary locaryn-variant-use"
                                        style={{ background: "#a78bfa", color: "#111" }}
                                        onClick={() => onLaunchAirllm?.(entry.repo)}
                                        title={`Lancer ${entry.repo} via le moteur AirLLM`}
                                      >
                                        🚀 Lancer avec AirLLM
                                      </button>
                                    );
                                  }
                                  return (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary locaryn-variant-use"
                                      style={{ background: "#a78bfa", color: "#111" }}
                                      disabled={busy}
                                      onClick={() => handleAirllmInstall(f)}
                                      title={`Télécharger ${entry.repo} (~${entry.sizeGb} Go fp16) puis lancer via AirLLM`}
                                    >
                                      {busy
                                        ? "⏳ AirLLM…"
                                        : `🚀 Installer via AirLLM (~${entry.sizeGb} Go)`}
                                    </button>
                                  );
                                })()
                              ) : (
                                <button
                                  type="button"
                                  className="locaryn-btn-primary locaryn-variant-use"
                                  onClick={() =>
                                    requestInstall(targetTag, f.name, Boolean(f.uncensored))
                                  }
                                  title={`Installer la quantisation ${activeQuant}`}
                                >
                                  Installer ({activeQuant})
                                </button>
                              )}
                            </div>
                          </div>

                          <div
                            style={{
                              display: "flex",
                              alignItems: "center",
                              gap: "4px",
                              flexWrap: "wrap",
                              marginTop: "2px",
                            }}
                          >
                            <span style={{ fontSize: "10px", color: "var(--text-faint)" }}>
                              quant:
                            </span>
                            {v.quants.map((q) => {
                              const isSelected = activeQuant === q;
                              return (
                                <button
                                  key={q}
                                  type="button"
                                  className={`locaryn-quant-chip${isSelected ? " locaryn-quant-chip-active" : ""}`}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setSelectedQuants((prev) => ({ ...prev, [v.tag]: q }));
                                  }}
                                  title={`Choisir la quantisation ${q}`}
                                >
                                  {q}
                                </button>
                              );
                            })}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* ACCORDION LIST VIEW */}
      {viewMode === "list" && (
        <div className="locaryn-model-list">
          {families.map((f) => {
            const open = openId === f.id;
            return (
              <div key={f.id} className="locaryn-model-card">
                <div style={{ display: "flex", alignItems: "center", minWidth: 0 }}>
                  <div style={{ flex: "1 1 auto", minWidth: 0 }}>
                    <button
                      type="button"
                      className="locaryn-model-head"
                      onClick={() => setOpenId(open ? null : f.id)}
                      aria-expanded={open}
                    >
                      <div className="locaryn-model-title">
                        <span className="locaryn-model-name">{f.name}</span>
                        <span className="locaryn-model-brand">{f.brand}</span>
                      </div>
                      <div className="locaryn-model-badges">
                        <span className="locaryn-tag locaryn-tag-soft">📅 {f.releaseDate}</span>
                        {(() => {
                          const c = classifyModel(`${f.name} ${f.id}`, {
                            uncensored: f.uncensored,
                          });
                          if (c.risk === "safe") return null;
                          return (
                            <span
                              className="locaryn-tag"
                              style={{
                                background: "rgba(204,125,114,0.2)",
                                color: "var(--danger)",
                                border: "1px solid rgba(204,125,114,0.4)",
                              }}
                              title={nsfwReason(`${f.name} ${f.id}`) ?? c.label}
                            >
                              {c.icon} {c.label}
                            </span>
                          );
                        })()}
                        {f.finetunable && (
                          <span className="locaryn-tag locaryn-tag-ft">🎯 LoRA Ready</span>
                        )}
                        {f.source === "huggingface" && (
                          <span
                            className="locaryn-tag"
                            title="Découvert automatiquement sur le Hub HuggingFace"
                            style={{
                              background: "rgba(255, 200, 87, 0.14)",
                              color: "#ffc857",
                              border: "1px solid rgba(255, 200, 87, 0.35)",
                            }}
                          >
                            🤗 HuggingFace
                          </span>
                        )}
                        {capBadges(f).map((c) => (
                          <span key={c} className="locaryn-tag">
                            {c}
                          </span>
                        ))}
                        <span className="locaryn-tag locaryn-tag-soft">{f.license}</span>
                        <span className="locaryn-model-chevron">{open ? "▾" : "▸"}</span>
                      </div>
                    </button>
                  </div>
                  <button
                    type="button"
                    onClick={() => toggleFavorite(f.id)}
                    aria-pressed={favorites.has(f.id)}
                    title={favorites.has(f.id) ? "Retirer des favoris" : "Ajouter aux favoris"}
                    style={{
                      background: "none",
                      border: "none",
                      cursor: "pointer",
                      fontSize: "16px",
                      lineHeight: 1,
                      padding: "10px 14px",
                      color: favorites.has(f.id) ? "#ffc857" : "var(--text-faint)",
                      flex: "none",
                    }}
                  >
                    {favorites.has(f.id) ? "★" : "☆"}
                  </button>
                </div>

                {open && (
                  <div className="locaryn-model-body">
                    <p className="locaryn-model-desc">{f.description}</p>
                    {f.variants.map((v) => {
                      const activeQuant = selectedQuants[v.tag] || v.quants[0] || "q4_K_M";
                      const targetTag = getQuantTag(v.tag, activeQuant);
                      const targetStorageGb = getQuantStorageGb(v.storageGb, activeQuant);
                      const isInstalled =
                        isVariantInstalled(targetTag, installedSet) ||
                        isVariantInstalled(v.tag, installedSet);
                      const progress = installProgress[targetTag] ?? installProgress[v.tag];
                      const isInstalling = progress !== undefined;
                      const isDeleting = deletingTag === targetTag || deletingTag === v.tag;
                      const compatV = variantCompat(targetStorageGb, hardwareSpec, airllmEnabled);

                      return (
                        <div key={v.tag} className="locaryn-variant">
                          <div className="locaryn-variant-top">
                            <span className="locaryn-variant-size">{v.size}</span>
                            <span className="locaryn-stat-vram">
                              💾 ~{targetStorageGb} Go Stockage
                            </span>
                            <span
                              className="locaryn-tag"
                              style={{ background: `${compatV.color}22`, color: compatV.color }}
                              title={compatV.label}
                            >
                              {compatV.icon} {compatV.short}
                            </span>
                            {compatV.level === "airllm" && (
                              <span
                                className="locaryn-tag"
                                style={{
                                  background: "rgba(167, 139, 250, 0.18)",
                                  color: "#a78bfa",
                                }}
                                title="Estimation AirLLM sur ce PC — débit réel selon VRAM / RAM / disque"
                              >
                                ⚡ ~
                                {fmtTokPerSec(
                                  estimateAirllmTokPerSec(targetStorageGb, hardwareSpec, v.size) ??
                                    0,
                                )}
                              </span>
                            )}
                            {isInstalled && (
                              <span className="locaryn-tag locaryn-tag-installed">Installé ✓</span>
                            )}
                            <code className="locaryn-variant-tag">{targetTag}</code>

                            <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                              {isInstalled ? (
                                <div style={{ display: "flex", gap: "4px", alignItems: "center" }}>
                                  {f.imageGen || looksLikeImageModel(targetTag) ? (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary"
                                      style={{ padding: "3px 8px", fontSize: "11px" }}
                                      onClick={() => onOpenImageGen?.()}
                                      title="Ouvrir la génération d'images avec ce modèle"
                                    >
                                      🎨 Générer
                                    </button>
                                  ) : (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary"
                                      style={{ padding: "3px 8px", fontSize: "11px" }}
                                      onClick={() => onSelectModelForChat?.(targetTag)}
                                      title="Utiliser ce modèle dans le Chat"
                                    >
                                      💬 Utiliser
                                    </button>
                                  )}
                                  <button
                                    type="button"
                                    className="locaryn-btn-ghost"
                                    style={{
                                      color: "var(--danger)",
                                      padding: "3px 8px",
                                      fontSize: "11px",
                                    }}
                                    onClick={() => handleDeleteModel(targetTag)}
                                    disabled={isDeleting}
                                    title="Supprimer ce modèle du disque dur local"
                                  >
                                    {isDeleting ? "..." : "Supprimer"}
                                  </button>
                                </div>
                              ) : isInstalling ? (
                                <button
                                  type="button"
                                  className="locaryn-btn-ghost"
                                  style={{
                                    color: "var(--danger)",
                                    border: "1px solid var(--danger)",
                                    padding: "3px 8px",
                                    fontSize: "11px",
                                  }}
                                  onClick={() => handleCancelInstall(targetTag)}
                                  title="Annuler le téléchargement en cours"
                                >
                                  ⛔ Annuler ({progress}%)
                                </button>
                              ) : compatV.level === "airllm" ? (
                                (() => {
                                  const entry = AIRLLM_MODELS[f.id];
                                  const installed = entry ? airllmInstalled.has(entry.repo) : false;
                                  const busy = entry ? airllmBusy[entry.repo] : false;
                                  if (!entry) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-ghost"
                                        style={{ border: "1px dashed #a78bfa", color: "#a78bfa" }}
                                        onClick={() => setAirllmModalOpen(true)}
                                        title="Cette architecture n'est pas encore supportée par AirLLM (Llama, Mistral, Qwen2…)"
                                      >
                                        🔮 Info AirLLM
                                      </button>
                                    );
                                  }
                                  if (installed) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-primary locaryn-variant-use"
                                        style={{ background: "#a78bfa", color: "#111" }}
                                        onClick={() => onLaunchAirllm?.(entry.repo)}
                                        title={`Lancer ${entry.repo} via le moteur AirLLM`}
                                      >
                                        🚀 Lancer avec AirLLM
                                      </button>
                                    );
                                  }
                                  return (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary locaryn-variant-use"
                                      style={{ background: "#a78bfa", color: "#111" }}
                                      disabled={busy}
                                      onClick={() => handleAirllmInstall(f)}
                                      title={`Télécharger ${entry.repo} (~${entry.sizeGb} Go fp16) puis lancer via AirLLM`}
                                    >
                                      {busy
                                        ? "⏳ AirLLM…"
                                        : `🚀 Installer via AirLLM (~${entry.sizeGb} Go)`}
                                    </button>
                                  );
                                })()
                              ) : (
                                <button
                                  type="button"
                                  className="locaryn-btn-primary locaryn-variant-use"
                                  onClick={() =>
                                    requestInstall(targetTag, f.name, Boolean(f.uncensored))
                                  }
                                  title={`Installer la quantisation ${activeQuant}`}
                                >
                                  Installer ({activeQuant})
                                </button>
                              )}
                            </div>
                          </div>
                          <div className="locaryn-quant-row">
                            <span className="locaryn-quant-label">quant:</span>
                            {v.quants.map((q) => {
                              const isSelected = activeQuant === q;
                              return (
                                <button
                                  key={q}
                                  type="button"
                                  className={`locaryn-quant-chip${isSelected ? " locaryn-quant-chip-active" : ""}`}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setSelectedQuants((prev) => ({ ...prev, [v.tag]: q }));
                                  }}
                                  title={`Choisir la quantisation ${q}`}
                                >
                                  {q}
                                </button>
                              );
                            })}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* AirLLM info modal — low-VRAM inference engine */}
      {airllmModalOpen && (
        <div
          className="locaryn-settings-backdrop"
          onClick={(e) => {
            if (e.target === e.currentTarget) setAirllmModalOpen(false);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") setAirllmModalOpen(false);
          }}
        >
          <div
            className="locaryn-card"
            style={{
              width: "560px",
              maxHeight: "85vh",
              overflowY: "auto",
              margin: "40px auto",
              border: "1px solid var(--border-strong)",
              boxShadow: "0 16px 40px rgba(0,0,0,0.85)",
            }}
          >
            <div className="locaryn-field-head" style={{ marginBottom: "14px" }}>
              <div>
                <h3 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
                  🔮 AirLLM — Gros modèles sur petit GPU
                </h3>
                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-faint)" }}>
                  Moteur d'inférence open-source : les gros modèles tournent sur un GPU 4 Go de VRAM
                  en chargeant les couches une par une.
                </span>
              </div>
              <button
                type="button"
                className="locaryn-icon-btn"
                onClick={() => setAirllmModalOpen(false)}
              >
                ✕
              </button>
            </div>
            <p style={{ fontSize: "var(--text-sm)", lineHeight: 1.6, margin: "0 0 12px" }}>
              Ce modèle est trop lourd pour les composants de ce PC (
              {hardwareSpec?.total_ram_gb ?? "?"} Go RAM
              {hardwareSpec?.total_vram_gb ? `, ${hardwareSpec.total_vram_gb} Go VRAM` : ""}). Avec
              le moteur AirLLM activé, il s'exécute quand même en local : chaque couche du
              transformer est chargée sur le GPU une par une (et chaque expert un par un pour les
              MoE), le reste restant sur le disque — un GPU 4 Go de VRAM suffit pour des modèles de
              70B et plus, comme Kimi K3.
            </p>
            <p
              style={{
                fontSize: "var(--text-sm)",
                lineHeight: 1.6,
                margin: "0 0 16px",
                color: "var(--text-dim)",
              }}
            >
              Architectures supportées par AirLLM : Llama 2/3/3.1, Mistral/Mixtral, Qwen/Qwen2. Les
              familles affichant « Info AirLLM » (GLM, Nemotron, Kimi… ) ne sont pas encore
              supportées par le moteur. Les modèles compatibles se téléchargent en pleine précision
              fp16 (≈2× la taille GGUF indiquée) puis se lancent d'un clic — l'inférence passe par
              le serveur AirLLM local (OpenAI-compatible) géré par Locaryn.
            </p>
            <div className="locaryn-field-actions" style={{ justifyContent: "flex-end" }}>
              <button
                type="button"
                className="locaryn-btn-primary"
                style={{ background: "#a78bfa", color: "#111" }}
                onClick={() => setAirllmModalOpen(false)}
              >
                Compris
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Hardware Benchmark Modal */}
      <HardwareBenchmarkModal
        isOpen={hardwareModalOpen}
        onClose={() => setHardwareModalOpen(false)}
        onApplyFilter={(onlyRec, hw) => {
          setOnlyRecommended(onlyRec);
          if (hw) setHardwareSpec(hw);
        }}
      />

      {/* NSFW / unfiltered model consent gate */}
      <ResponsibilityGate
        open={nsfwGateOpen}
        what="l'installation d'un modèle classé NSFW / sans garde-fous"
        onAccept={confirmNsfwInstall}
        onCancel={() => {
          setNsfwGateOpen(false);
          setPendingNsfwInstall(null);
        }}
      />

      {/* Obliterator Studio Modal */}
      <ModelObliterator
        isOpen={obliteratorOpen}
        onClose={() => setObliteratorOpen(false)}
        installedModels={installed}
        onModelAbliterated={async (newTag) => {
          await onInstall(newTag);
        }}
      />
    </div>
  );
}
