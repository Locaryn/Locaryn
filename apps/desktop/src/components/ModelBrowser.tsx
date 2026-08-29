import { Icon, type IconName, LoSwitch, isIconName } from "@locaryn/ui-core";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  type HfModelCandidate,
  type HfModelSelection,
  type HfRepoInspection,
  type InstalledExtension,
  type LlmfitCatalogEntry,
  type ModelFit,
  type ModelMetric,
  core,
  formatBytes,
  getHfToken,
} from "../lib/core";
import {
  type ExtensionMarketplaceCatalog,
  loadExtensionMarketplaces,
} from "../lib/extensionMarketplace";
import {
  MODEL_CATEGORIES,
  type ModelCategory,
  type ModelCategoryDefinition,
  type ModelDownloadSource,
  type ModelFamily,
  SIZE_BUCKETS,
  clearRegistryCache,
  fetchFullRegistry,
  fetchHuggingFaceModels,
  isCloudOnlyFamily,
} from "../lib/modelRegistry";
import { classifyModel, nsfwReason } from "../lib/modelSafety";
import { HardwareBenchmarkModal } from "./HardwareBenchmarkModal";
import { ModelObliterator } from "./ModelObliterator";
import { ResponsibilityGate } from "./ResponsibilityGate";
import { SpeedBadge, findMetric } from "./SpeedBadge";

type Props = {
  /** Download / Install model to local storage. */
  onInstall: (
    tag: string,
    onProgress?: (pct: number) => void,
    heretic?: boolean,
    consent?: boolean,
    selection?: HfModelSelection,
    downloads?: ModelDownloadSource[],
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
  /** Launch an AirLLM model: activates the AirLlm provider and opens Chat. */
  onLaunchAirllm?: (repo: string) => void;
  /** Active extension capabilities currently installed/enabled. */
  activeCapabilities?: string[];
  /** Enabled extensions may contribute data-only Marketplace catalogues. */
  activeExtensions?: InstalledExtension[];
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
  "deepseek-r1-70b-airllm": { repo: "deepseek-ai/DeepSeek-R1-Distill-Llama-70B", sizeGb: 140 },
  "deepseek-r1-70b-gguf": { repo: "deepseek-ai/DeepSeek-R1-Distill-Llama-70B", sizeGb: 140 },
  "deepseek-r1": { repo: "deepseek-ai/DeepSeek-R1-Distill-Llama-70B", sizeGb: 140 },
  deepseek_r1: { repo: "deepseek-ai/DeepSeek-R1-Distill-Llama-70B", sizeGb: 140 },
  "llama-3.3-70b-airllm": { repo: "meta-llama/Llama-3.3-70B-Instruct", sizeGb: 140 },
  "llama3.3": { repo: "meta-llama/Llama-3.3-70B-Instruct", sizeGb: 140 },
  "llama3.1": { repo: "meta-llama/Llama-3.1-70B-Instruct", sizeGb: 140 },
  llama4: { repo: "meta-llama/Llama-4-Scout-17B-16E-Instruct", sizeGb: 34 },
  "qwen2.5-72b-airllm": { repo: "Qwen/Qwen2.5-72B-Instruct", sizeGb: 145 },
  "qwen2.5-72b-gguf": { repo: "Qwen/Qwen2.5-72B-Instruct", sizeGb: 145 },
  "qwen2.5": { repo: "Qwen/Qwen2.5-72B-Instruct", sizeGb: 145 },
  qwen2_5: { repo: "Qwen/Qwen2.5-72B-Instruct", sizeGb: 145 },
  qwen3: { repo: "Qwen/Qwen3-235B-A22B-Instruct-2507", sizeGb: 470 },
  "mistral-nemo-airllm": { repo: "mistralai/Mistral-Nemo-Instruct-2407", sizeGb: 27 },
  "mistral-nemo": { repo: "mistralai/Mistral-Nemo-Instruct-2407", sizeGb: 27 },
  "mixtral-8x7b-airllm": { repo: "mistralai/Mixtral-8x7B-Instruct-v0.1", sizeGb: 90 },
  "command-r-airllm": { repo: "CohereForAI/c4ai-command-r-v01", sizeGb: 70 },
  "qwen2.5-coder-32b-airllm": { repo: "Qwen/Qwen2.5-Coder-32B-Instruct", sizeGb: 65 },
  mistral: { repo: "mistralai/Mistral-7B-Instruct-v0.3", sizeGb: 30 },
  "gemini-2-5-flash": { repo: "google/gemini-2.5-flash-distill-gguf", sizeGb: 28 },
  "gemini-nano": { repo: "google/gemini-nano-2", sizeGb: 16 },
  gemma4: { repo: "google/gemma-4-31b-it", sizeGb: 62 },
};

/** Find the AirLLM HuggingFace repo & size metadata for a model family. */
function getAirllmEntry(f: ModelFamily): { repo: string; sizeGb: number } | undefined {
  if (AIRLLM_MODELS[f.id]) return AIRLLM_MODELS[f.id];
  const v = f.variants.find((v) => v.tag.startsWith("airllm:"));
  if (v) return { repo: v.tag.replace(/^airllm:/, ""), sizeGb: v.storageGb };
  return undefined;
}

/**
 * Hides every specialized family unless an enabled extension owns the
 * corresponding capability. This is deliberately applied before cards,
 * brands and years are derived so uninstalling a plugin removes all of its
 * Marketplace traces in the same render.
 */
export function isFamilyAvailableForCapabilities(
  family: ModelFamily,
  activeCapabilities: string[],
): boolean {
  const has = (capability: string) => activeCapabilities.includes(capability);
  const hasTts = has("voice-tts") || has("voice-cloning");

  if (family.tts && !hasTts) return false;
  if (family.videoGen && !has("video-gen")) return false;
  if (family.musicGen && !has("music-gen")) return false;
  if (family.model3d && !has("3d-gen")) return false;
  if (family.objectDetection && !family.instruct && !has("vision-ocr")) return false;
  if (family.textAnalysis && !family.instruct && !has("text-analysis")) return false;
  if (family.questionAnswering && !family.instruct && !has("rag-qa")) return false;
  if (family.translation && !family.instruct && !has("translation")) return false;
  return true;
}

function familyMarketplaceCapabilities(family: ModelFamily): Set<string> {
  const capabilities = new Set(family.marketplaceCapabilities ?? []);
  if (family.instruct) capabilities.add("chat");
  if (family.code) capabilities.add("code");
  if (family.vision) capabilities.add("vision");
  if (family.reasoning) capabilities.add("reasoning");
  if (family.tts) capabilities.add("voice-tts");
  if (family.voiceCloning) capabilities.add("voice-cloning");
  if (family.videoGen) capabilities.add("video-gen");
  if (family.translation) capabilities.add("translation");
  if (family.model3d) capabilities.add("3d-gen");
  if (family.musicGen) capabilities.add("music-gen");
  if (family.objectDetection) capabilities.add("vision-ocr");
  if (family.textAnalysis) capabilities.add("text-analysis");
  if (family.questionAnswering) capabilities.add("rag-qa");
  if (family.audio) capabilities.add("audio");
  return capabilities;
}

const EMPTY_EXTENSION_MARKETPLACE: ExtensionMarketplaceCatalog = {
  categories: [],
  models: [],
  claims: [],
};

/** Une capacité annoncée : son icône et son nom. */
type Pastille = { icon: IconName; label: string };

function capBadges(f: ModelFamily, activeCapabilities: string[] = []): Pastille[] {
  const caps: Pastille[] = [];
  const a = (c: string) => activeCapabilities.includes(c);
  if (f.source === "airllm" || f.variants.some((v) => v.quants.includes("airllm"))) {
    caps.push({ icon: "star", label: "AirLLM" });
  }
  if (f.imageGen && a("image-gen")) caps.push({ icon: "image", label: "Image" });
  if (f.tts && a("voice-tts")) caps.push({ icon: "mic", label: "Voix" });
  if (f.voiceCloning && a("voice-cloning")) caps.push({ icon: "figures", label: "Clonage" });
  if (f.videoGen && a("video-gen")) caps.push({ icon: "video", label: "Vidéo" });
  if (f.musicGen && a("music-gen")) caps.push({ icon: "music", label: "Musique" });
  if (f.model3d && a("3d-gen")) caps.push({ icon: "cube", label: "3D" });
  if (f.translation && a("translation")) caps.push({ icon: "translate", label: "Traduction" });
  if (f.objectDetection && a("vision-ocr")) caps.push({ icon: "target", label: "Détection" });
  if (f.textAnalysis && a("text-analysis")) caps.push({ icon: "chart", label: "Texte" });
  if (f.imageEditing && a("image-editor")) caps.push({ icon: "edit", label: "Édition" });
  if (f.questionAnswering && a("rag-qa")) caps.push({ icon: "question", label: "Q&R" });
  if (f.vision && a("vision-ocr")) caps.push({ icon: "image", label: "Vision" });
  if (f.audio && a("voice-tts")) caps.push({ icon: "sound", label: "Audio" });
  if (f.code) caps.push({ icon: "cpu", label: "Code" });
  if (f.reasoning) caps.push({ icon: "memory", label: "Raisonnement" });
  if (f.instruct) caps.push({ icon: "chat", label: "Instruct" });
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
type Compat = { level: CompatLevel; label: string; short: string; color: string };

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
function variantCompat(
  storageGb: number,
  hw: HwSpec | null,
  airllm = false,
  fit?: ModelFit,
): Compat {
  // L'estimation native prime : elle tient compte du contexte réglé et de la
  // mémoire réellement libre, là où la taille du fichier ne dit rien des deux.
  if (fit) return compatFromFit(fit, airllm);
  if (storageGb === 0) {
    return {
      level: "unknown",
      label: "Modèle local",
      short: "Local",
      color: "var(--info)",
    };
  }
  if (!hw) {
    return {
      level: "unknown",
      label: "Analyse PC requise pour estimer",
      short: "?",
      color: "var(--text-faint)",
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
      color: "var(--accent-300)",
    };
  }
  if (need <= ram * 0.85) {
    return {
      level: "offload",
      label: "Trop gros pour la VRAM, mais tourne via la RAM (offload CPU, plus lent)",
      short: "OK via RAM",
      color: "var(--warn)",
    };
  }
  if (airllm) {
    return {
      level: "airllm",
      label:
        "Trop lourd pour la VRAM/RAM de ce PC, mais exécutable localement via AirLLM " +
        "(chargement des couches une par une — un GPU 4 Go de VRAM suffit)",
      short: "AirLLM",
      color: "var(--info)",
    };
  }
  return {
    level: "heavy",
    label: "Dépasse la mémoire de ce PC — non recommandé",
    short: "Trop lourd",
    color: "var(--danger)",
  };
}

/**
 * Deux jeux d'estimations disent-ils la même chose ?
 *
 * Le tri des familles dépend des estimations, et les estimations sont
 * recalculées quand la liste change : sans cette comparaison, chaque réponse
 * relancerait un calcul identique en boucle. Le message porte les chiffres
 * libres du moment, donc une vraie évolution de la mémoire se voit ici.
 */
function sameFits(a: Record<string, ModelFit>, b: Record<string, ModelFit>): boolean {
  const keys = Object.keys(a);
  if (keys.length !== Object.keys(b).length) return false;
  return keys.every((key) => a[key]?.message === b[key]?.message);
}

/**
 * Clé d'estimation.
 *
 * À paramètres, quantification et taille égaux, la réponse est la même : deux
 * cents lignes de catalogue ne demandent qu'une poignée de calculs distincts.
 */
function fitKey(params: number, quant: string, storageGb: number): string {
  return `${params}|${quant}|${storageGb}`;
}

/**
 * Le verdict natif, traduit en pastille.
 *
 * L'estimation connaît ce que la taille du fichier ignore : le cache
 * d'attention pour le contexte réglé, le nombre de couches qui tiennent
 * vraiment sur le GPU, et le débit qui en découle. La pastille dit donc ce qui
 * va se passer, pas seulement si ça rentre.
 */
function compatFromFit(fit: ModelFit, airllm: boolean): Compat {
  const speed = fit.tokens_per_second > 0 ? ` — ~${fmtTokPerSec(fit.tokens_per_second)}` : "";
  if (fit.placement === "gpu") {
    return {
      level: "gpu",
      label: fit.message,
      short: `Fluide GPU${speed}`,
      color: "var(--accent-300)",
    };
  }
  if (fit.placement === "partage") {
    return {
      level: "offload",
      label: fit.message,
      short: `${fit.gpu_layers}/${fit.total_layers} couches GPU${speed}`,
      color: "var(--warn)",
    };
  }
  if (fit.placement === "ram") {
    return {
      level: "offload",
      label: fit.message,
      short: `RAM${speed}`,
      color: "var(--warn)",
    };
  }
  if (airllm) {
    return {
      level: "airllm",
      label: `${fit.message} AirLLM charge les couches une par une : le modèle tourne quand même, beaucoup plus lentement.`,
      short: "AirLLM",
      color: "var(--info)",
    };
  }
  return {
    level: "heavy",
    label: fit.message,
    short: "Trop lourd",
    color: "var(--danger)",
  };
}

function familyBestCompat(
  variants: { storageGb: number; params?: number; quants?: string[] }[],
  hw: HwSpec | null,
  airllm = false,
  fits?: Record<string, ModelFit>,
): Compat {
  if (variants.length === 0) return variantCompat(0, hw, airllm);
  const best = variants.reduce((a, b) => (a.storageGb <= b.storageGb ? a : b));
  const quant = best.quants?.[0];
  const fit =
    fits && best.params !== undefined && quant
      ? fits[fitKey(best.params, quant, best.storageGb)]
      : undefined;
  return variantCompat(best.storageGb, hw, airllm, fit);
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

function isVariantInstalled(tag: string, installedSet: Set<string>, variantHint?: string): boolean {
  if (!tag) return false;
  if (installedSet.has(tag)) return true;
  if (installedSet.has(`${tag}:latest`)) return true;
  const fileName = tag.startsWith("http") ? tag.split("/").pop()! : tag;
  if (installedSet.has(fileName)) return true;

  // HuggingFace repo URLs (e.g. https://huggingface.co/coqui/XTTS-v2, deliberate-v2, etc.)
  if (tag.startsWith("https://huggingface.co/")) {
    const repoPart = tag.replace("https://huggingface.co/", "").replace(/\/+$/, "");
    const dirName = repoPart.replace("/", "__");
    if (installedSet.has(dirName)) return true;
    const hint = variantHint?.toLowerCase().replace(/[^a-z0-9]+/g, "");
    if (hint) {
      for (const inst of installedSet) {
        const normalized = inst.toLowerCase().replace(/[^a-z0-9]+/g, "");
        if (
          inst.toLowerCase().startsWith(`${dirName.toLowerCase()}/`) &&
          normalized.includes(hint)
        ) {
          return true;
        }
      }
    }
  }

  // Exact or normalized filename match (case-insensitive)
  const tagLower = tag.toLowerCase();
  const fileLower = fileName.toLowerCase();
  for (const inst of installedSet) {
    const instLower = inst.toLowerCase();
    if (instLower === fileLower || instLower === tagLower) {
      return true;
    }
  }
  return false;
}

/** La carte ne quitte l'état qu'au bout de ce délai, pour que la grille ne
 *  saute pas avant qu'on ait vu ce qui partait. */
const SHATTER_MS = 620;

/** Le nombre d'éclats. Assez pour que ça se disloque, pas assez pour ramer. */
const SHARD_COUNT = 28;

/**
 * Les éclats d'une suppression : ils divergent depuis le centre de la ligne,
 * dans les tons de l'accent. Purement décoratifs, donc muets.
 */
function Shards() {
  return (
    <span className="locaryn-shards" aria-hidden="true">
      {Array.from({ length: SHARD_COUNT }, (_, i) => {
        const angle = (i / SHARD_COUNT) * Math.PI * 2;
        const reach = 40 + (i % 5) * 14;
        return (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: les éclats n'ont pas d'identité — leur seule différence est leur rang, qui fixe l'angle.
            key={i}
            className="locaryn-shard"
            style={
              {
                "--tx": `${Math.cos(angle) * reach}px`,
                "--ty": `${Math.sin(angle) * reach}px`,
                "--rot": `${(i % 2 ? 1 : -1) * (90 + (i % 5) * 40)}deg`,
                animationDelay: `${(i % 7) * 12}ms`,
              } as React.CSSProperties
            }
          />
        );
      })}
    </span>
  );
}

/**
 * L'éditeur d'une famille, sans le repackager.
 *
 * Le catalogue écrit la marque comme un chemin : « Alibaba / Qwen / unsloth »
 * désigne un modèle de Qwen réempaqueté par unsloth. La maquette n'affiche que
 * l'éditeur — c'est lui qu'on cherche, pas qui a refait l'archive.
 */
function editeur(marque: string): string {
  return marque.split(" / ")[0].trim();
}

export function ModelBrowser({
  onInstall,
  onCancelInstall,
  onDelete,
  onOpenTraining,
  onSelectModelForChat,
  onLaunchAirllm,
  installed = [],
  activeCapabilities = [],
  activeExtensions = [],
}: Props) {
  const [query, setQuery] = useState("");
  /**
   * Vitesses relevées sur cette machine. Un modèle jamais lancé ici n'en a
   * pas : la carte reste muette plutôt que d'afficher une estimation.
   */
  const [metrics, setMetrics] = useState<ModelMetric[]>([]);
  useEffect(() => {
    void core
      .listModelMetrics()
      .then(setMetrics)
      .catch(() => setMetrics([]));
  }, []);
  const [customTagInput, setCustomTagInput] = useState("");
  const [addMenuOpen, setAddMenuOpen] = useState(false);
  const [customDownloadModalOpen, setCustomDownloadModalOpen] = useState(false);
  const addMenuRef = useRef<HTMLDivElement | null>(null);
  const addMenuBtnRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!addMenuOpen) return;
    function onDocDown(e: MouseEvent) {
      const t = e.target as Node;
      if (addMenuRef.current?.contains(t) || addMenuBtnRef.current?.contains(t)) return;
      setAddMenuOpen(false);
    }
    document.addEventListener("mousedown", onDocDown);
    return () => document.removeEventListener("mousedown", onDocDown);
  }, [addMenuOpen]);

  const [extensionMarketplace, setExtensionMarketplace] = useState<ExtensionMarketplaceCatalog>(
    EMPTY_EXTENSION_MARKETPLACE,
  );

  useEffect(() => {
    let cancelled = false;
    // Remove stale extension rows synchronously when a plugin is disabled or
    // uninstalled; the replacement catalogue is loaded immediately after.
    setExtensionMarketplace(EMPTY_EXTENSION_MARKETPLACE);
    void loadExtensionMarketplaces(activeExtensions, core.refreshExtensionAsset).then(
      (catalogue) => {
        if (!cancelled) setExtensionMarketplace(catalogue);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [activeExtensions]);

  const marketplaceCategories = useMemo(() => {
    const categories = new Map<string, ModelCategoryDefinition>();
    for (const item of [...MODEL_CATEGORIES, ...extensionMarketplace.categories]) {
      if (!categories.has(item.id)) categories.set(item.id, item);
    }
    return [...categories.values()];
  }, [extensionMarketplace.categories]);

  // An extension's filters exist only while its data slot is enabled.
  const visibleCategories = useMemo(() => {
    return marketplaceCategories.filter((category) => {
      if (!category.requires?.length) return true;
      return category.requires.some((capability) => activeCapabilities.includes(capability));
    });
  }, [activeCapabilities, marketplaceCategories]);

  const [category, setCategory] = useState<ModelCategory>("all");

  useEffect(() => {
    if (category !== "all" && !visibleCategories.some((c) => c.id === category)) {
      setCategory("all");
    }
  }, [visibleCategories, category]);

  const [brand, setBrand] = useState("all");
  const [size, setSize] = useState("all");
  const [yearFilter, setYearFilter] = useState("all");
  const [sortBy, setSortBy] = useState<"compat" | "newest" | "name" | "pulls">("compat");
  const [onlyFinetunable, setOnlyFinetunable] = useState(false);
  const [riskFilter, setRiskFilter] = useState<"all" | "safe" | "uncensored" | "nsfw">("all");
  const [onlyRecommended, setOnlyRecommended] = useState(false);
  // AirLLM : basculer l'affichage pour afficher et exécuter les très gros modèles
  // via le moteur AirLLM (chargement des couches une par une sur petit GPU).
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
  } | null>(null);
  /** Estimations natives, par clé « paramètres | quantification | taille ». */
  const [fits, setFits] = useState<Record<string, ModelFit>>({});
  const [nsfwGateOpen, setNsfwGateOpen] = useState(false);
  const [pendingNsfwInstall, setPendingNsfwInstall] = useState<{
    tag: string;
    heretic: boolean;
    downloads?: ModelDownloadSource[];
  } | null>(null);
  /** Repository inspection is deliberately explicit: a HF repo may contain
   * Q3/Q4/Q8, instruct/base and several sharded checkpoints. */
  const [repoInspection, setRepoInspection] = useState<HfRepoInspection | null>(null);
  const [repoInstallContext, setRepoInstallContext] = useState<{
    source: string;
    familyName: string;
    heretic: boolean;
    consent: boolean;
    downloads?: ModelDownloadSource[];
  } | null>(null);
  const [repoCandidateId, setRepoCandidateId] = useState<string>("");
  const [repoInspecting, setRepoInspecting] = useState(false);
  /** Quand la fenêtre de choix s'est ouverte, en millisecondes. Le clic qui
   *  l'ouvre se termine après elle : sans ce délai, il retombe sur le fond de
   *  la nouvelle fenêtre et la referme aussitôt. */
  const repoInspectionOpenedAt = useRef(0);
  const [repoInspectionError, setRepoInspectionError] = useState<string | null>(null);
  const [showQuantGuideDetails, setShowQuantGuideDetails] = useState(false);
  const [quantFilter, setQuantFilter] = useState<"all" | "recommended" | "light" | "quality">(
    "all",
  );

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
        if (active && hw) {
          const vram =
            hw.total_vram_gb > 128 ? Math.round(hw.total_vram_gb / 1024) : hw.total_vram_gb;
          setHardwareSpec({ total_ram_gb: hw.total_ram_gb, total_vram_gb: vram });
        }
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

  const capabilityFamilies = useMemo(
    () =>
      [...registryModels, ...liveApiModels, ...extensionMarketplace.models].filter((family) =>
        isFamilyAvailableForCapabilities(family, activeCapabilities),
      ),
    [registryModels, liveApiModels, extensionMarketplace.models, activeCapabilities],
  );

  const allBrands = useMemo(() => {
    return Array.from(new Set(capabilityFamilies.map((f) => f.brand)))
      .filter(
        (b) => b && !b.toLowerCase().includes("claude") && !b.toLowerCase().includes("anthropic"),
      )
      .sort();
  }, [capabilityFamilies]);

  /**
   * Les marques mises en puces : les plus fournies, et rien d'autre.
   *
   * Le catalogue en compte plus de deux cents, parce qu'une marque y est
   * parfois un chemin complet (« Alibaba / Qwen / AirLLM »). Toutes en puces,
   * la barre faisait 1800px de haut. Les autres restent dans la liste
   * déroulante, sous « Avancé », qui les porte toutes.
   */
  const topBrands = useMemo(() => {
    const compte = new Map<string, number>();
    for (const f of capabilityFamilies) {
      if (!f.brand) continue;
      const b = f.brand.toLowerCase();
      if (b.includes("claude") || b.includes("anthropic")) continue;
      const nom = editeur(f.brand);
      compte.set(nom, (compte.get(nom) ?? 0) + 1);
    }
    return [...compte.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], "fr"))
      .slice(0, 10)
      .map(([nom]) => nom);
  }, [capabilityFamilies]);

  const years = useMemo(() => {
    const ySet = new Set(capabilityFamilies.map((f) => f.releaseYear));
    return Array.from(ySet).sort((a, b) => b - a);
  }, [capabilityFamilies]);

  useEffect(() => {
    if (brand !== "all" && !allBrands.includes(brand) && !topBrands.includes(brand)) {
      setBrand("all");
    }
  }, [brand, allBrands, topBrands]);

  useEffect(() => {
    if (yearFilter !== "all" && !years.includes(Number(yearFilter))) setYearFilter("all");
  }, [yearFilter, years]);

  const families = useMemo(() => {
    const q = query.trim().toLowerCase();
    const bucket = SIZE_BUCKETS.find((b) => b.id === size);
    const catalogSource = capabilityFamilies;
    const activeCategory = visibleCategories.find((item) => item.id === category);

    return catalogSource
      .map((f) => {
        // Exclude any cloud-only or remote-hosted model entirely, as well as Claude / Anthropic.
        if (
          isCloudOnlyFamily(f) ||
          f.variants.some((v) => v.quants.includes("cloud") || v.tag.includes(":cloud")) ||
          f.brand.toLowerCase().includes("claude") ||
          f.brand.toLowerCase().includes("anthropic") ||
          f.name.toLowerCase().includes("claude")
        ) {
          return null;
        }

        const matchingVariants = f.variants.filter((v) => {
          if (v.quants.includes("cloud") || v.tag.includes(":cloud")) return false;
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

        if (
          activeCategory &&
          activeCategory.id !== "all" &&
          !activeCategory.matches.some((capability) =>
            familyMarketplaceCapabilities(f).has(capability),
          )
        ) {
          return null;
        }

        if (brand !== "all" && f.brand !== brand && editeur(f.brand) !== brand) return null;
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
          const ra =
            COMPAT_RANK[familyBestCompat(a.variants, hardwareSpec, airllmEnabled, fits).level];
          const rb =
            COMPAT_RANK[familyBestCompat(b.variants, hardwareSpec, airllmEnabled, fits).level];
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
    capabilityFamilies,
    visibleCategories,
    hardwareSpec,
    airllmEnabled,
    fits,
  ]);

  // Estimation native de toute la liste visible, en un appel.
  //
  // Sans elle, la pastille de compatibilité ne connaîtrait que la taille du
  // fichier : elle ignorerait le cache d'attention, qui dépasse les poids sur
  // les contextes longs, et annoncerait « fluide GPU » pour un modèle qui
  // débordera au chargement. Le calcul est dédoublonné, et retardé le temps
  // que l'utilisateur finisse de taper.
  useEffect(() => {
    let active = true;
    const entries = new Map<string, LlmfitCatalogEntry>();
    for (const family of families) {
      for (const variant of family.variants) {
        const quants = variant.quants.length > 0 ? variant.quants : ["q4_K_M"];
        for (const quant of quants) {
          const storageGb = getQuantStorageGb(variant.storageGb, quant);
          const key = fitKey(variant.params, quant, storageGb);
          if (!entries.has(key)) {
            entries.set(key, {
              id: key,
              parameters_b: variant.params,
              quant,
              size_gb: storageGb,
            });
          }
        }
      }
    }
    if (entries.size === 0) {
      setFits({});
      return;
    }
    const timer = setTimeout(() => {
      const keys = [...entries.keys()];
      core
        .llmfitCatalog([...entries.values()])
        .then((reports) => {
          if (!active) return;
          const next: Record<string, ModelFit> = {};
          keys.forEach((key, index) => {
            const report = reports[index];
            if (report) next[key] = report;
          });
          setFits((prev) => (sameFits(prev, next) ? prev : next));
        })
        .catch(() => {
          // Estimation indisponible : les pastilles retombent sur la taille du
          // fichier, qui reste une réponse — approximative, mais pas fausse.
        });
    }, 250);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [families]);

  function toggleCardExpand(id: string) {
    setExpandedCards((prev) => ({ ...prev, [id]: !prev[id] }));
  }

  async function handleFetchLiveApiModels() {
    setIsFetchingLive(true);
    try {
      const q = query.trim() || "gguf";
      const gguf = await fetchHuggingFaceModels(q);
      setLiveApiModels(gguf);
    } finally {
      setIsFetchingLive(false);
    }
  }

  interface QuantAdvice {
    badge: string;
    badgeStyle: { background: string; color: string; border: string };
    advice: string;
    detail: string;
    priority: number;
    isRecommended: boolean;
    category: "recommended" | "high" | "max" | "light" | "raw" | "default";
  }

  function getQuantizationAdvice(quant?: string | null, format?: string | null): QuantAdvice {
    const q = (quant || "").toUpperCase();

    // --- Q4 Variants ---
    if (q.includes("Q4_K_M")) {
      return {
        badge: "Recommandé (K-Quant Medium)",
        badgeStyle: {
          background: "rgba(101, 211, 145, 0.15)",
          color: "var(--accent-300)",
          border: "1px solid rgba(101, 211, 145, 0.35)",
        },
        advice:
          "Standard d'or recommandé : compresse intelligemment chaque couche selon son importance. Meilleur équilibre vitesse / mémoire / qualité.",
        detail: "Standard universel pour 90% des usages.",
        priority: 100,
        isRecommended: true,
        category: "recommended",
      };
    }
    if (q.includes("Q4_K_S")) {
      return {
        badge: "K-Quant Compact (Small)",
        badgeStyle: {
          background: "rgba(101, 211, 145, 0.15)",
          color: "var(--accent-300)",
          border: "1px solid rgba(101, 211, 145, 0.35)",
        },
        advice:
          "Variante 4-bit plus légère que Q4_K_M : gagne ~10% de RAM avec une qualité quasi identique.",
        detail: "Idéal si vous êtes un peu juste en mémoire.",
        priority: 96,
        isRecommended: true,
        category: "recommended",
      };
    }
    if (q.includes("Q4_K_L") || q.includes("Q4_K_XL")) {
      return {
        badge: "K-Quant Large",
        badgeStyle: {
          background: "rgba(101, 211, 145, 0.15)",
          color: "var(--accent-300)",
          border: "1px solid rgba(101, 211, 145, 0.35)",
        },
        advice: "K-Quant renforcé en précision sur les couches d'attention critiques.",
        detail: "Légèrement plus lourd que Q4_K_M pour un gain subtil.",
        priority: 94,
        isRecommended: true,
        category: "recommended",
      };
    }
    if (q.includes("Q4_0_4_4") || q.includes("Q4_0_8_8") || q.includes("Q4_0_4_8")) {
      return {
        badge: "⚡ 4-bit Vectorisé (ARM / NPU)",
        badgeStyle: {
          background: "rgba(110, 168, 254, 0.15)",
          color: "var(--info)",
          border: "1px solid rgba(110, 168, 254, 0.35)",
        },
        advice:
          "4-bit structuré par blocs, optimisé pour l'accélération matricielle parallèle sur puces ARM / Apple Silicon.",
        detail: "Excellente vitesse d'inférence sur puces ARM.",
        priority: 88,
        isRecommended: false,
        category: "recommended",
      };
    }
    if (q === "Q4_0" || q === "Q4_K") {
      return {
        badge: "⚡ 4-bit Uniforme (Legacy)",
        badgeStyle: {
          background: "rgba(251, 191, 36, 0.15)",
          color: "var(--warn)",
          border: "1px solid rgba(251, 191, 36, 0.35)",
        },
        advice:
          "Quantification 4-bit uniforme simple. Très rapide et universelle, mais un peu moins fine que Q4_K_M.",
        detail: "Format classique universel.",
        priority: 85,
        isRecommended: false,
        category: "recommended",
      };
    }
    if (q === "Q4_1") {
      return {
        badge: "⚡ 4-bit Uniforme + Offset",
        badgeStyle: {
          background: "rgba(251, 191, 36, 0.15)",
          color: "var(--warn)",
          border: "1px solid rgba(251, 191, 36, 0.35)",
        },
        advice: "4-bit uniforme avec offset pour une fidélité légèrement supérieure à Q4_0.",
        detail: "Variante de Q4_0.",
        priority: 84,
        isRecommended: false,
        category: "recommended",
      };
    }

    // --- Q5 Variants ---
    if (q.includes("Q5_K_M")) {
      return {
        badge: "5-bit K-Quant Medium",
        badgeStyle: {
          background: "rgba(110, 168, 254, 0.15)",
          color: "var(--info)",
          border: "1px solid rgba(110, 168, 254, 0.35)",
        },
        advice:
          "Raisonnement accru (logique, code, maths). Qualité quasi indiscernable du modèle complet.",
        detail: "Idéal si vous avez 16-32 Go de mémoire.",
        priority: 92,
        isRecommended: false,
        category: "high",
      };
    }
    if (q.includes("Q5_K_S") || q.includes("Q5_0") || q.includes("Q5_1") || q.includes("Q5_K")) {
      return {
        badge: "5-bit Compact",
        badgeStyle: {
          background: "rgba(110, 168, 254, 0.15)",
          color: "var(--info)",
          border: "1px solid rgba(110, 168, 254, 0.35)",
        },
        advice: "Excellente fidélité avec ~15% de mémoire en moins que Q6_K.",
        detail: "Alternative 5-bit équilibrée.",
        priority: 89,
        isRecommended: false,
        category: "high",
      };
    }

    // --- Q6 & Q8 Variants ---
    if (q.includes("Q6_K")) {
      return {
        badge: "6-bit K-Quant (Qualité / RAM)",
        badgeStyle: {
          background: "rgba(192, 132, 252, 0.15)",
          color: "var(--info)",
          border: "1px solid rgba(192, 132, 252, 0.35)",
        },
        advice:
          "99.5% de la qualité de Q8_0 tout en économisant ~20% de RAM. Souvent le meilleur choix haut de gamme.",
        detail: "Privilégiez Q6_K face à Q8_0 pour économiser 20% d'espace sans perte perceptible.",
        priority: 82,
        isRecommended: false,
        category: "max",
      };
    }
    if (q.includes("Q8_0") || q.includes("Q8_1") || q.includes("Q8_K")) {
      return {
        badge: "8-bit Pleine Fidélité (Sans perte)",
        badgeStyle: {
          background: "rgba(192, 132, 252, 0.15)",
          color: "var(--info)",
          border: "1px solid rgba(192, 132, 252, 0.35)",
        },
        advice:
          "Reproduction parfaite et sans aucune altération du modèle original. Exige beaucoup de RAM/VRAM.",
        detail: "À choisir uniquement si votre machine dispose d'une grande marge de mémoire.",
        priority: 78,
        isRecommended: false,
        category: "max",
      };
    }

    // --- IQ (Importance Matrix) Variants ---
    if (q.includes("IQ4")) {
      return {
        badge: "I-Matrix 4-bit Calibré",
        badgeStyle: {
          background: "rgba(101, 211, 145, 0.15)",
          color: "var(--accent-300)",
          border: "1px solid rgba(101, 211, 145, 0.35)",
        },
        advice:
          "Compression 4-bit guidée par matrice d'importance (imatrix) pour une précision supérieure à Q4_0.",
        detail: "Excellente qualité compressée par calibration.",
        priority: 91,
        isRecommended: false,
        category: "recommended",
      };
    }
    if (q.includes("IQ3")) {
      return {
        badge: "I-Matrix 3-bit Calibré",
        badgeStyle: {
          background: "rgba(251, 191, 36, 0.15)",
          color: "var(--warn)",
          border: "1px solid rgba(251, 191, 36, 0.35)",
        },
        advice:
          "Quantification 3-bit calibrée par imatrix : préserve la cohérence textuelle bien mieux que Q3_K classique.",
        detail: "Idéal pour faire tourner un gros modèle avec peu de mémoire.",
        priority: 72,
        isRecommended: false,
        category: "light",
      };
    }
    if (q.includes("IQ2") || q.includes("IQ1")) {
      return {
        badge: "I-Matrix Ultra-Compact",
        badgeStyle: {
          background: "rgba(251, 191, 36, 0.15)",
          color: "var(--warn)",
          border: "1px solid rgba(251, 191, 36, 0.35)",
        },
        advice:
          "Compression extrême (1 à 2 bits) calibrée par imatrix pour faire tourner de gros modèles sur 4-6 Go de RAM.",
        detail: "Permet de tester de grands modèles sur très petit matériel.",
        priority: 50,
        isRecommended: false,
        category: "light",
      };
    }

    // --- Q3 Variants ---
    if (q.includes("Q3_K_M")) {
      return {
        badge: "⚡ 3-bit K-Quant Medium",
        badgeStyle: {
          background: "rgba(251, 191, 36, 0.15)",
          color: "var(--warn)",
          border: "1px solid rgba(251, 191, 36, 0.35)",
        },
        advice:
          "Compromis pour réduire l'empreinte mémoire sous les 8 Go tout en gardant des phrases bien structurées.",
        detail: "Pour PC portables ou configurations 8 Go.",
        priority: 65,
        isRecommended: false,
        category: "light",
      };
    }
    if (
      q.includes("Q3_K_S") ||
      q.includes("Q3_K_L") ||
      q.includes("Q3_K_XL") ||
      q.includes("Q3_K")
    ) {
      return {
        badge: "⚡ 3-bit K-Quant",
        badgeStyle: {
          background: "rgba(251, 191, 36, 0.15)",
          color: "var(--warn)",
          border: "1px solid rgba(251, 191, 36, 0.35)",
        },
        advice: "Quantification 3-bit ultra-légère pour machines très contraintes en mémoire.",
        detail: "Très économe en RAM.",
        priority: 62,
        isRecommended: false,
        category: "light",
      };
    }

    // --- Q2 Variants ---
    if (q.includes("Q2_K")) {
      return {
        badge: "⚡ 2-bit Extrême",
        badgeStyle: {
          background: "rgba(251, 191, 36, 0.15)",
          color: "var(--warn)",
          border: "1px solid rgba(251, 191, 36, 0.35)",
        },
        advice:
          "Compression maximale en 2-bit (dégradation possible, réservé aux tests sur matériel très limité).",
        detail: "Très faible mémoire mais perte de précision possible.",
        priority: 45,
        isRecommended: false,
        category: "light",
      };
    }

    // --- Full Precision / Raw ---
    if (q.includes("F16") || q.includes("FP16") || q.includes("BF16") || q.includes("FP32")) {
      return {
        badge: "Poids bruts (Non compressé)",
        badgeStyle: {
          background: "rgba(255, 255, 255, 0.08)",
          color: "var(--text-faint)",
          border: "1px solid var(--border)",
        },
        advice:
          "Modèle 16/32-bit d'origine. Très lourd et inutilement gourmand pour une exécution locale standard.",
        detail: "À éviter sauf besoin spécifique d'archivage ou conversion.",
        priority: 20,
        isRecommended: false,
        category: "raw",
      };
    }

    return {
      badge: (format || "Modèle").toUpperCase(),
      badgeStyle: {
        background: "rgba(255, 255, 255, 0.06)",
        color: "var(--text-dim)",
        border: "1px solid var(--border)",
      },
      advice: "Variante standard disponible dans le dépôt.",
      detail: "Fichier de poids du modèle.",
      priority: 50,
      isRecommended: false,
      category: "default",
    };
  }

  function estimateRamUsage(bytes: number): string {
    const gb = bytes / (1024 * 1024 * 1024);
    const minRam = (gb + 0.8).toFixed(1);
    return `~${minRam} Go RAM min`;
  }

  function hfRepoSource(tag: string): string | null {
    const trimmed = tag.trim();
    if (!trimmed) return null;
    let source = trimmed.startsWith("hf.co/")
      ? `https://huggingface.co/${trimmed.slice(6)}`
      : trimmed;
    if (
      !source.startsWith("http") &&
      !source.includes(":") &&
      (source.match(/\//g) || []).length === 1
    ) {
      source = `https://huggingface.co/${source}`;
    }
    if (!source.startsWith("https://huggingface.co/")) return null;
    if (source.includes("/resolve/") || source.includes("/blob/")) return null;
    return source.replace(/\/+$/, "");
  }

  function makeSelection(
    inspection: HfRepoInspection,
    candidate: HfModelCandidate,
  ): HfModelSelection {
    const supportFiles = Array.from(
      new Set([...inspection.support_files, ...candidate.support_files]),
    );
    return {
      repo: inspection.repo,
      files: candidate.files,
      support_files: supportFiles,
      label: candidate.label,
    };
  }

  function requestInstall(
    tag: string,
    familyName: string,
    heretic: boolean,
    downloads?: ModelDownloadSource[],
  ) {
    if (classifyModel(`${familyName} ${tag}`).risk !== "safe") {
      setPendingNsfwInstall({ tag, heretic, downloads });
      setNsfwGateOpen(true);
      return;
    }
    void beginModelInstall(tag, familyName, heretic, false, undefined, downloads);
  }

  async function beginModelInstall(
    tag: string,
    familyName: string,
    heretic: boolean,
    consent: boolean,
    selection?: HfModelSelection,
    downloads?: ModelDownloadSource[],
    // « Installer le dépôt complet » est déjà une décision prise devant la
    // liste. Sans ce drapeau, l'appel repassait par l'inspection, qui rouvrait
    // la même fenêtre : le bouton ne pouvait rien installer.
    skipInspection = false,
  ) {
    const chosenSelection = selection;
    const repo = !chosenSelection && !skipInspection ? hfRepoSource(tag) : null;
    if (repo) {
      setRepoInspecting(true);
      setRepoInspectionError(null);
      repoInspectionOpenedAt.current = Date.now();
      setRepoInspection({
        repo: repo.replace("https://huggingface.co/", ""),
        candidates: [],
        support_files: [],
        total_bytes: 0,
        warning: null,
        suggested_repo: null,
      });
      setRepoInstallContext({ source: repo, familyName, heretic, consent, downloads });
      try {
        const inspection = await core.inspectHuggingFaceRepo(repo, getHfToken());
        const sortedCandidates = [...inspection.candidates].sort((a, b) => {
          const adviceA = getQuantizationAdvice(a.quantization, a.format);
          const adviceB = getQuantizationAdvice(b.quantization, b.format);
          return adviceB.priority - adviceA.priority;
        });
        const bestCandidate =
          sortedCandidates.find(
            (c) => getQuantizationAdvice(c.quantization, c.format).isRecommended,
          ) ?? sortedCandidates[0];

        // Le dépôt est présenté, toujours. La condition d'avant exigeait
        // *plusieurs* variantes : un dépôt qui n'en expose qu'une — ou aucune,
        // comme un paquet TTS multi-fichiers — refermait la fenêtre au bout
        // d'une demi-seconde et téléchargeait tout sans rien demander. Ce que
        // l'utilisateur choisit, il doit le voir avant que ça descende.
        setRepoInspection({ ...inspection, candidates: sortedCandidates });
        setRepoInstallContext({ source: repo, familyName, heretic, consent, downloads });
        setRepoCandidateId(bestCandidate?.id ?? "");
        setQuantFilter("all");
        return;
      } catch (e) {
        setRepoInspectionError(String(e).replace(/^Error:\s*/, ""));
        setRepoInspection({
          repo: repo.replace("https://huggingface.co/", ""),
          candidates: [],
          support_files: [],
          total_bytes: 0,
          warning: null,
          suggested_repo: null,
        });
        setRepoInstallContext({ source: repo, familyName, heretic, consent, downloads });
        return;
      } finally {
        setRepoInspecting(false);
      }
    }

    setRepoInspection(null);
    setRepoInstallContext(null);
    await handleInstallModel(tag, heretic, consent, chosenSelection, downloads);
  }

  async function handleInstallModel(
    tag: string,
    heretic?: boolean,
    consent = false,
    selection?: HfModelSelection,
    downloads?: ModelDownloadSource[],
  ) {
    // One global pull is allowed by the desktop shell; keep the progress key
    // equal to the repository/file tag so the card remains in sync after the
    // user chose a candidate in the modal.
    const progressKey = tag;
    setInstallProgress((prev) => ({ ...prev, [progressKey]: 0 }));
    try {
      await onInstall(
        tag,
        (pct) => {
          setInstallProgress((prev) => ({ ...prev, [progressKey]: pct }));
        },
        heretic,
        consent,
        selection,
        downloads,
      );
    } finally {
      setInstallProgress((prev) => {
        const copy = { ...prev };
        delete copy[progressKey];
        return copy;
      });
    }
  }

  function confirmNsfwInstall() {
    setNsfwGateOpen(false);
    if (pendingNsfwInstall) {
      const { tag, heretic, downloads } = pendingNsfwInstall;
      setPendingNsfwInstall(null);
      void beginModelInstall(tag, tag, heretic, true, undefined, downloads);
    }
  }

  function confirmRepoCandidate() {
    if (!repoInspection || !repoInstallContext) return;
    const candidate = repoInspection.candidates.find((item) => item.id === repoCandidateId);
    if (!candidate) return;
    const { source, familyName, heretic, consent, downloads } = repoInstallContext;
    void beginModelInstall(
      source,
      familyName,
      heretic,
      consent,
      makeSelection(repoInspection, candidate),
      downloads,
    );
  }

  function inspectSuggestedRepo() {
    if (!repoInspection?.suggested_repo || !repoInstallContext) return;
    const { familyName, heretic, consent, downloads } = repoInstallContext;
    const source = `https://huggingface.co/${repoInspection.suggested_repo}`;
    setRepoInspection(null);
    setRepoInstallContext(null);
    void beginModelInstall(source, familyName, heretic, consent, undefined, downloads);
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
    // La ligne se disloque d'abord, elle ne quitte la liste qu'ensuite : sans
    // ce délai la grille saute avant qu'on ait vu ce qui partait.
    await new Promise((resolve) => setTimeout(resolve, SHATTER_MS));
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
    const entry = getAirllmEntry(f);
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
      setAirllmLog((prev) => [...prev, `${repo} prêt — démarrage du moteur AirLLM…`]);
      await onLaunchAirllm?.(repo);
    } catch (e) {
      setAirllmError(String(e));
    } finally {
      setAirllmBusy((prev) => ({ ...prev, [repo]: false }));
    }
  }

  // Ce que la ligne de chiffres annonce, sur les familles réellement listées :
  // le compte doit décrire ce qu'on voit, pas le catalogue entier.
  const catalogueCounts = useMemo(() => {
    const tailles = families.reduce((n, f) => n + f.variants.length, 0);
    const quants = families.reduce(
      (n, f) => n + f.variants.reduce((m, v) => m + (v.quants?.length ?? 0), 0),
      0,
    );
    return { familles: families.length, tailles, quants };
  }, [families]);

  // Les réglages avancés sont repliés : la maquette n'a que deux rangées de
  // commandes avant la première carte, et ce moteur ne concerne que les petits
  // GPU. Il reste à un clic, il n'occupe plus la page de tout le monde.
  const [advancedOpen, setAdvancedOpen] = useState(false);

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
        <div className="locaryn-view-stats" style={{ marginBottom: "var(--space-2)" }}>
          <span>
            {catalogueCounts.familles} famille{catalogueCounts.familles > 1 ? "s" : ""}
          </span>
          <span>
            {catalogueCounts.tailles} taille{catalogueCounts.tailles > 1 ? "s" : ""}
          </span>
          <span>{catalogueCounts.quants} quantifications</span>
        </div>
        <div className="locaryn-models-search-row">
          <input
            className="locaryn-input locaryn-input-text"
            placeholder="Rechercher parmi les modèles (Gemma 4, Kimi K3, MiMo, GLM 5.2, DeepSeek-R1, Qwen2.5...)"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {/* Marque, année, tri : la maquette n'en a aucun sur la rangée de
              recherche. Ils restent, repliés — trois listes déroulantes en
              travers du champ, c'est ce qui rendait la barre illisible. */}
          {advancedOpen && (
            <>
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
                onChange={(e) =>
                  setSortBy(e.target.value as "compat" | "newest" | "name" | "pulls")
                }
                aria-label="Trier les modèles"
              >
                {/* Un `<option>` ne contient que du texte : ni icône, ni pastille.
                  Le navigateur les refuse, et React le signale. */}
                <option value="compat">Compatibles d'abord</option>
                <option value="newest">Plus récents</option>
                <option value="pulls">Plus populaires</option>
                <option value="name">Nom (A → Z)</option>
              </select>
            </>
          )}

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
          </div>

          {/* Bouton Plus en haut à droite — Menu Ajouter / Dépôt */}
          <div style={{ position: "relative" }}>
            <button
              ref={addMenuBtnRef}
              type="button"
              className="locaryn-btn-primary"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                height: "100%",
                padding: "0 12px",
                whiteSpace: "nowrap",
                fontSize: "12px",
              }}
              onClick={() => setAddMenuOpen((v) => !v)}
              title="Ajouter un modèle"
              aria-label="Ajouter un modèle"
            >
              <Icon name="plus" size={15} />
              <span>Ajouter</span>
            </button>
            {addMenuOpen && (
              <div
                ref={addMenuRef}
                style={{
                  position: "absolute",
                  top: "calc(100% + 4px)",
                  right: 0,
                  minWidth: "250px",
                  background: "var(--panel, #161816)",
                  border: "1px solid var(--border-strong)",
                  borderRadius: "var(--radius-sm)",
                  boxShadow: "0 10px 28px rgba(0,0,0,0.6)",
                  padding: "4px",
                  zIndex: 300,
                  display: "flex",
                  flexDirection: "column",
                  gap: "2px",
                }}
              >
                <button
                  type="button"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "8px",
                    width: "100%",
                    padding: "8px 10px",
                    background: "none",
                    border: "none",
                    borderRadius: "var(--radius-xs)",
                    color: "var(--text)",
                    fontSize: "12px",
                    cursor: "pointer",
                    textAlign: "left",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background =
                      "var(--surface-hover, rgba(255,255,255,0.06))";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "none";
                  }}
                  onClick={() => {
                    setAddMenuOpen(false);
                    setCustomDownloadModalOpen(true);
                  }}
                >
                  <Icon name="download" size={15} />
                  <div>
                    <div>Ajouter depuis un dépôt</div>
                    <div style={{ fontSize: "10px", color: "var(--text-faint)" }}>
                      HuggingFace, Ollama ou lien direct .gguf
                    </div>
                  </div>
                </button>
                <button
                  type="button"
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "8px",
                    width: "100%",
                    padding: "8px 10px",
                    background: "none",
                    border: "none",
                    borderRadius: "var(--radius-xs)",
                    color: "var(--text)",
                    fontSize: "12px",
                    cursor: "pointer",
                    textAlign: "left",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background =
                      "var(--surface-hover, rgba(255,255,255,0.06))";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background = "none";
                  }}
                  onClick={() => {
                    setAddMenuOpen(false);
                    handleFetchLiveApiModels();
                  }}
                  disabled={isFetchingLive}
                >
                  <Icon name="refresh" size={15} />
                  <div>
                    <div>
                      {isFetchingLive ? "Recherche en cours…" : "Chercher sur HuggingFace Hub"}
                    </div>
                    <div style={{ fontSize: "10px", color: "var(--text-faint)" }}>
                      Découvrir les derniers modèles en direct
                    </div>
                  </div>
                </button>
              </div>
            )}
          </div>
        </div>

        <button
          type="button"
          className={`locaryn-chip${advancedOpen ? " locaryn-chip-on" : ""}`}
          style={{ alignSelf: "flex-start", marginTop: "8px" }}
          aria-expanded={advancedOpen}
          onClick={() => setAdvancedOpen((v) => !v)}
          title="Moteur AirLLM : exécuter localement des modèles trop lourds pour ce PC"
        >
          <Icon name="sliders" size={14} /> Avancé
        </button>

        {/* Réglages avancés, repliés par défaut.
            La maquette n'a que deux rangées de commandes avant la première
            carte. Ce moteur ne sert qu'à qui a un petit GPU : il reste à un
            clic, il n'occupe plus la page de tout le monde. */}
        {advancedOpen && (
          <>
            {/* AirLLM toggle — converts too-heavy models into low-VRAM executable ones */}
            <div className="locaryn-airllm-bar">
              <span
                className="locaryn-airllm-switch"
                title="Basculer le moteur AirLLM : les modèles trop lourds pour ce PC deviennent exécutables localement (chargement des couches une par une, un GPU 4 Go de VRAM suffit)"
              >
                <LoSwitch
                  checked={airllmEnabled}
                  onChange={setAirllmEnabled}
                  labelledBy="locaryn-airllm-label"
                />
                <span
                  className={`locaryn-airllm-label${airllmEnabled ? " locaryn-airllm-on" : ""}`}
                  id="locaryn-airllm-label"
                >
                  <Icon name="star" size={15} /> AirLLM — Gros modèles sur petit GPU
                </span>
              </span>
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
                  background: airllmError
                    ? "rgba(204, 125, 114, 0.08)"
                    : "rgba(167, 139, 250, 0.06)",
                  fontFamily: "var(--font-mono)",
                  fontSize: "11px",
                  color: airllmError ? "var(--danger)" : "var(--text-dim)",
                  maxHeight: "110px",
                  overflowY: "auto",
                  whiteSpace: "pre-wrap",
                }}
              >
                {airllmError ? airllmError : airllmLog.slice(-6).join("\n")}
              </div>
            )}
          </>
        )}

        {/* Les catégories : la maquette filtre par marque, pas par usage.
            Elles restent accessibles, repliées avec le reste. */}
        {advancedOpen && (
          <>
            {/* Category Filter Bar */}
            <div style={{ display: "flex", gap: "6px", flexWrap: "wrap", marginTop: "8px" }}>
              {visibleCategories.map((cat) => (
                <button
                  key={cat.id}
                  type="button"
                  className={`locaryn-chip${category === cat.id ? " locaryn-chip-on" : ""}`}
                  onClick={() => setCategory(cat.id)}
                >
                  {isIconName(cat.icon) && <Icon name={cat.icon} size={14} />} {cat.label}
                </button>
              ))}
              <button
                type="button"
                className="locaryn-btn-ghost"
                style={{
                  fontSize: "11px",
                  marginLeft: "auto",
                  padding: "4px 10px",
                  display: "inline-flex",
                  alignItems: "center",
                  gap: "6px",
                }}
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
                <span
                  style={{ display: "inline-flex" }}
                  className={isLoadingRegistry ? "locaryn-spin" : undefined}
                >
                  <Icon name="refresh" size={14} />
                </span>
                <span>{isLoadingRegistry ? "Chargement…" : "Rafraîchir le catalogue"}</span>
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
                  MAJ {new Date(lastUpdated).toLocaleTimeString()}
                </span>
              )}
            </div>
          </>
        )}

        <div className="locaryn-brand-chips">
          <button
            type="button"
            className={`locaryn-chip${brand === "all" ? " locaryn-chip-on" : ""}`}
            onClick={() => setBrand("all")}
          >
            Toutes les marques
          </button>
          {topBrands.map((b) => (
            <button
              key={b}
              type="button"
              className={`locaryn-chip${brand === b ? " locaryn-chip-on" : ""}`}
              onClick={() => setBrand(b)}
            >
              {b}
            </button>
          ))}
          {/* Une marque choisie dans la liste complète reste visible ici, même
              si elle n'est pas parmi les dix : sinon rien ne dit ce qui filtre. */}
          {brand !== "all" && !topBrands.includes(brand) && (
            <button
              type="button"
              className="locaryn-chip locaryn-chip-on"
              onClick={() => setBrand("all")}
            >
              {brand}
            </button>
          )}
        </div>

        <div className="locaryn-models-toolbar" style={{ marginTop: "8px" }}>
          {/* Recommandés, sûreté, réentraînables, favoris, et l'analyse du PC.
              La maquette ne met que les tailles ici — le reste est replié. */}
          {advancedOpen && (
            <div className="locaryn-chips-extra">
              <button
                type="button"
                className={`locaryn-chip locaryn-chip-ft${onlyRecommended ? " locaryn-chip-on" : ""}`}
                style={
                  onlyRecommended
                    ? {
                        background: "rgba(100, 200, 120, 0.2)",
                        borderColor: "var(--accent-300)",
                        color: "var(--accent-300)",
                      }
                    : {}
                }
                onClick={() => setOnlyRecommended((prev) => !prev)}
                title="Filtrer uniquement les modèles adaptés aux composants de votre PC"
              >
                <span className="locaryn-dot locaryn-dot-ok" /> Recommandés pour mon PC
              </button>

              <button
                type="button"
                className="locaryn-btn-ghost"
                style={{ fontSize: "11px", marginLeft: "auto", padding: "2px 8px" }}
                onClick={() => setHardwareModalOpen(true)}
                title="Analyser les composants de mon PC pour adapter les recommandations"
              >
                <Icon name="settings" size={15} /> Analyser mon PC
              </button>

              <button
                type="button"
                className={`locaryn-chip locaryn-chip-ft${onlyFinetunable ? " locaryn-chip-on" : ""}`}
                onClick={() => setOnlyFinetunable((prev) => !prev)}
                title="Afficher uniquement les modèles réentraînables via Fine-Tuning / LoRA"
              >
                <Icon name="target" size={15} /> Réentraînable / LoRA
              </button>
              <button
                type="button"
                className={`locaryn-chip locaryn-chip-ft${riskFilter === "safe" ? " locaryn-chip-on" : ""}`}
                style={
                  riskFilter === "safe"
                    ? {
                        background: "rgba(90, 168, 106, 0.2)",
                        borderColor: "var(--accent-300)",
                        color: "var(--accent-300)",
                      }
                    : {}
                }
                onClick={() => setRiskFilter((prev) => (prev === "safe" ? "all" : "safe"))}
                title="Afficher uniquement les modèles classiques avec garde-fous"
              >
                <Icon name="shield" size={15} /> Safe
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
                <Icon name="lock" size={15} /> Sans limite
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
                NSFW
              </button>
              <button
                type="button"
                className={`locaryn-chip locaryn-chip-ft${onlyFavorites ? " locaryn-chip-on" : ""}`}
                style={
                  onlyFavorites
                    ? {
                        background: "rgba(255, 200, 87, 0.18)",
                        borderColor: "var(--warn)",
                        color: "var(--warn)",
                      }
                    : {}
                }
                onClick={() => setOnlyFavorites((prev) => !prev)}
                title="Afficher uniquement les modèles marqués comme favoris"
              >
                <Icon name="star" size={15} /> Favoris
                {favorites.size > 0 ? ` (${favorites.size})` : ""}
              </button>
            </div>
          )}

          {/* Analyse du PC, studio d'oblitération, bascule grille/liste :
              des actions et un réglage de vue, pas des filtres. La maquette
              n'en a aucun sur cette barre. */}
          {advancedOpen && (
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
                <Icon name="chart" size={15} /> Analyse Perf PC
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
                <Icon name="lock" size={15} /> Studio d'Oblitération RepE
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
          )}
        </div>
      </div>

      {/* Intuitive hardware compatibility banner — no filter needed */}
      {hardwareSpec ? (
        (() => {
          const counts = families.reduce(
            (acc, f) => {
              acc[familyBestCompat(f.variants, hardwareSpec, airllmEnabled, fits).level]++;
              return acc;
            },
            { cloud: 0, gpu: 0, offload: 0, airllm: 0, heavy: 0, unknown: 0 } as Record<
              CompatLevel,
              number
            >,
          );
          return (
            <div className="locaryn-hw-banner">
              <span className="locaryn-hw-banner-pc">
                <Icon name="server" size={15} /> Votre PC&nbsp;:{" "}
                <b>{hardwareSpec.total_ram_gb} Go RAM</b>
                {hardwareSpec.total_vram_gb > 0 && (
                  <>
                    {" "}
                    · <b>{hardwareSpec.total_vram_gb} Go VRAM</b>
                  </>
                )}
              </span>
              <span className="locaryn-hw-banner-counts">
                {airllmEnabled ? (
                  <>
                    <span style={{ color: "var(--info)" }}>
                      <span className="locaryn-dot" style={{ background: "var(--info)" }} />{" "}
                      {counts.airllm} via AirLLM
                    </span>
                    <span style={{ color: "var(--accent-300)" }}>
                      <span className="locaryn-dot" style={{ background: "var(--accent-300)" }} />{" "}
                      {counts.gpu} fluides GPU
                    </span>
                    <span style={{ color: "var(--warn)" }}>
                      <span className="locaryn-dot" style={{ background: "var(--warn)" }} />{" "}
                      {counts.offload} via RAM
                    </span>
                    <span className="locaryn-hw-banner-note">
                      — AirLLM actif : chaque modèle est exécutable en local
                    </span>
                  </>
                ) : (
                  <>
                    <span style={{ color: "var(--accent-300)" }}>
                      <span className="locaryn-dot" style={{ background: "var(--accent-300)" }} />{" "}
                      {counts.gpu} fluides GPU
                    </span>
                    <span style={{ color: "var(--warn)" }}>
                      <span className="locaryn-dot" style={{ background: "var(--warn)" }} />{" "}
                      {counts.offload} via RAM
                    </span>
                    <span style={{ color: "var(--danger)" }}>
                      <span className="locaryn-dot" style={{ background: "var(--danger)" }} />{" "}
                      {counts.heavy} trop lourds
                    </span>
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
            <Icon name="server" size={15} /> Analyse du PC en cours… la compatibilité de chaque
            modèle s'affichera automatiquement.
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
            const compat = familyBestCompat(f.variants, hardwareSpec, airllmEnabled, fits);
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
                    getAirllmEntry(f)?.sizeGb,
                  )
                : null;

            return (
              <div key={f.id} className={`locaryn-box-card locaryn-compat-${compat.level}`}>
                <div className="locaryn-box-head">
                  <div style={{ minWidth: 140, flex: "1 1 55%" }}>
                    <span className="locaryn-box-brand" title={f.brand}>
                      {f.brand}
                    </span>
                    <h3 className="locaryn-box-name" title={f.name}>
                      {f.name}
                    </h3>
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
                      <span className="locaryn-dot" style={{ background: compat.color }} />{" "}
                      {compat.short}
                    </span>
                    {familyAirSpeed && (
                      <span
                        className="locaryn-tag"
                        style={{ background: "var(--accent-fill)", color: "var(--info)" }}
                        title="Estimation AirLLM sur ce PC — débit réel selon VRAM / RAM / disque"
                      >
                        <Icon name="speed" size={13} /> ~{fmtTokPerSec(familyAirSpeed)}
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
                      <Icon name="calendar" size={13} /> {f.releaseDate}
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
                          {isIconName(c.icon) && <Icon name={c.icon} size={13} />} {c.label}
                        </span>
                      );
                    })()}
                    {f.finetunable && (
                      <span
                        className="locaryn-tag locaryn-tag-ft"
                        title="Modèle prêt pour le fine-tuning LoRA"
                      >
                        <Icon name="target" size={15} /> LoRA
                      </span>
                    )}
                    {f.source === "huggingface" && (
                      <span
                        className="locaryn-tag"
                        title="Découvert automatiquement sur le Hub HuggingFace"
                        style={{
                          background: "rgba(255, 200, 87, 0.14)",
                          color: "var(--warn)",
                          border: "1px solid rgba(255, 200, 87, 0.35)",
                        }}
                      >
                        <Icon name="marketplace" size={15} /> HuggingFace
                      </span>
                    )}
                    {capBadges(f, activeCapabilities).map((c) => (
                      <span key={c.label} className="locaryn-tag">
                        <Icon name={c.icon} size={13} /> {c.label}
                      </span>
                    ))}
                    <SpeedBadge metric={findMetric(metrics, f.id)} />
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
                      color: favorites.has(f.id) ? "var(--warn)" : "var(--text-faint)",
                      flex: "none",
                    }}
                  >
                    <Icon name="star" size={15} />
                  </button>
                </div>

                <p className="locaryn-box-desc">{f.description}</p>

                {/* Contexte, licence, date : de la fiche, pas de la vignette.
                    La maquette n'en met aucun sur la carte. */}
                {isExpanded && (
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
                )}

                <div className="locaryn-box-foot">
                  <span>
                    {f.variants.length} taille{f.variants.length > 1 ? "s" : ""} · {cleanSizeRange}
                  </span>
                  <button
                    type="button"
                    className="locaryn-box-detail"
                    onClick={() => toggleCardExpand(f.id)}
                    aria-expanded={isExpanded}
                  >
                    {isExpanded ? "Replier" : "Détail"}
                    <Icon name={isExpanded ? "chevron" : "arrow-right"} size={13} />
                  </button>
                </div>

                {isExpanded && (
                  <div className="locaryn-box-variants" style={{ marginTop: "12px" }}>
                    <span className="locaryn-box-variants-title">Variantes & Quantisations :</span>
                    {f.variants.map((v) => {
                      const activeQuant = selectedQuants[v.tag] || v.quants[0] || "q4_K_M";
                      const targetTag = getQuantTag(v.tag, activeQuant);
                      const targetStorageGb = getQuantStorageGb(v.storageGb, activeQuant);
                      const isInstalled =
                        isVariantInstalled(targetTag, installedSet, activeQuant) ||
                        isVariantInstalled(v.tag, installedSet, activeQuant);
                      const progress = installProgress[targetTag] ?? installProgress[v.tag];
                      const isInstalling = progress !== undefined;
                      const isDeleting = deletingTag === targetTag || deletingTag === v.tag;
                      const fitV = fits[fitKey(v.params, activeQuant, targetStorageGb)];
                      const compatV = variantCompat(
                        targetStorageGb,
                        hardwareSpec,
                        airllmEnabled,
                        fitV,
                      );

                      return (
                        <div
                          key={v.tag}
                          className={`locaryn-box-variant-row${isDeleting ? " locaryn-shatter" : ""}`}
                          style={{ flexDirection: "column", alignItems: "stretch", gap: "6px" }}
                        >
                          {isDeleting && <Shards />}
                          <div
                            style={{
                              display: "flex",
                              justifyContent: "space-between",
                              alignItems: "center",
                            }}
                          >
                            <div className="locaryn-box-variant-info">
                              <span className="locaryn-variant-size">{v.size}</span>
                              <span className="locaryn-stat-vram">
                                <Icon name="models" size={13} /> ~{targetStorageGb} Go
                              </span>
                              <span
                                className="locaryn-tag"
                                style={{ background: `${compatV.color}22`, color: compatV.color }}
                                title={compatV.label}
                              >
                                <span
                                  className="locaryn-dot"
                                  style={{ background: compatV.color }}
                                />{" "}
                                {compatV.short}
                              </span>
                              {compatV.level === "airllm" && (
                                <span
                                  className="locaryn-tag"
                                  style={{
                                    background: "var(--accent-fill)",
                                    color: "var(--info)",
                                  }}
                                  title="Estimation AirLLM sur ce PC — débit réel selon VRAM / RAM / disque"
                                >
                                  <Icon name="speed" size={13} /> ~
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
                                <span className="locaryn-tag locaryn-tag-installed">Installé</span>
                              )}
                            </div>

                            <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                              {isInstalled ? (
                                <div style={{ display: "flex", gap: "4px", alignItems: "center" }}>
                                  <button
                                    type="button"
                                    className="locaryn-btn-primary"
                                    style={{ padding: "3px 8px", fontSize: "11px" }}
                                    onClick={() => onSelectModelForChat?.(targetTag)}
                                    title="Utiliser ce modèle dans le Chat"
                                  >
                                    <Icon name="chat" size={15} /> Utiliser
                                  </button>
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
                                  <Icon name="close" size={15} /> Annuler ({progress}%)
                                </button>
                              ) : compatV.level === "airllm" ? (
                                (() => {
                                  const entry = getAirllmEntry(f);
                                  const installed = entry ? airllmInstalled.has(entry.repo) : false;
                                  const busy = entry ? airllmBusy[entry.repo] : false;
                                  if (!entry) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-ghost"
                                        style={{
                                          border: "1px dashed var(--info)",
                                          color: "var(--info)",
                                        }}
                                        onClick={() => setAirllmModalOpen(true)}
                                        title="Cette architecture n'est pas encore supportée par AirLLM (Llama, Mistral, Qwen2…)"
                                      >
                                        <Icon name="star" size={15} /> Info AirLLM
                                      </button>
                                    );
                                  }
                                  if (installed) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-primary locaryn-variant-use"
                                        style={{
                                          background: "var(--info)",
                                          color: "var(--on-accent)",
                                        }}
                                        onClick={() => onLaunchAirllm?.(entry.repo)}
                                        title={`Lancer ${entry.repo} via le moteur AirLLM`}
                                      >
                                        <Icon name="speed" size={15} /> Lancer avec AirLLM
                                      </button>
                                    );
                                  }
                                  return (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary locaryn-variant-use"
                                      style={{
                                        background: "var(--info)",
                                        color: "var(--on-accent)",
                                      }}
                                      disabled={busy}
                                      onClick={() => handleAirllmInstall(f)}
                                      title={`Télécharger ${entry.repo} (~${entry.sizeGb} Go fp16) puis lancer via AirLLM`}
                                    >
                                      {busy
                                        ? "⏳ AirLLM…"
                                        : `Installer via AirLLM (~${entry.sizeGb} Go)`}
                                    </button>
                                  );
                                })()
                              ) : (
                                <button
                                  type="button"
                                  className="locaryn-btn-primary locaryn-variant-use"
                                  onClick={() =>
                                    requestInstall(
                                      targetTag,
                                      f.name,
                                      Boolean(f.uncensored),
                                      v.downloads,
                                    )
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
                        <span className="locaryn-model-name" title={f.name}>
                          {f.name}
                        </span>
                        <span className="locaryn-model-brand" title={f.brand}>
                          {f.brand}
                        </span>
                      </div>
                      <div className="locaryn-model-badges">
                        <span className="locaryn-tag locaryn-tag-soft">
                          <Icon name="calendar" size={13} /> {f.releaseDate}
                        </span>
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
                              {isIconName(c.icon) && <Icon name={c.icon} size={13} />} {c.label}
                            </span>
                          );
                        })()}
                        {f.finetunable && (
                          <span className="locaryn-tag locaryn-tag-ft">
                            <Icon name="target" size={15} /> LoRA Ready
                          </span>
                        )}
                        {f.source === "huggingface" && (
                          <span
                            className="locaryn-tag"
                            title="Découvert automatiquement sur le Hub HuggingFace"
                            style={{
                              background: "rgba(255, 200, 87, 0.14)",
                              color: "var(--warn)",
                              border: "1px solid rgba(255, 200, 87, 0.35)",
                            }}
                          >
                            <Icon name="marketplace" size={15} /> HuggingFace
                          </span>
                        )}
                        {capBadges(f, activeCapabilities).map((c) => (
                          <span key={c.label} className="locaryn-tag">
                            <Icon name={c.icon} size={13} /> {c.label}
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
                      color: favorites.has(f.id) ? "var(--warn)" : "var(--text-faint)",
                      flex: "none",
                    }}
                  >
                    <Icon name="star" size={15} />
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
                        isVariantInstalled(targetTag, installedSet, activeQuant) ||
                        isVariantInstalled(v.tag, installedSet, activeQuant);
                      const progress = installProgress[targetTag] ?? installProgress[v.tag];
                      const isInstalling = progress !== undefined;
                      const isDeleting = deletingTag === targetTag || deletingTag === v.tag;
                      const fitV = fits[fitKey(v.params, activeQuant, targetStorageGb)];
                      const compatV = variantCompat(
                        targetStorageGb,
                        hardwareSpec,
                        airllmEnabled,
                        fitV,
                      );

                      return (
                        <div
                          key={v.tag}
                          className={`locaryn-variant${isDeleting ? " locaryn-shatter" : ""}`}
                        >
                          {isDeleting && <Shards />}
                          <div className="locaryn-variant-top">
                            <span className="locaryn-variant-size">{v.size}</span>
                            <span className="locaryn-stat-vram">
                              <Icon name="models" size={13} /> ~{targetStorageGb} Go
                            </span>
                            <span
                              className="locaryn-tag"
                              style={{ background: `${compatV.color}22`, color: compatV.color }}
                              title={compatV.label}
                            >
                              <span className="locaryn-dot" style={{ background: compatV.color }} />{" "}
                              {compatV.short}
                            </span>
                            {compatV.level === "airllm" && (
                              <span
                                className="locaryn-tag"
                                style={{
                                  background: "var(--accent-fill)",
                                  color: "var(--info)",
                                }}
                                title="Estimation AirLLM sur ce PC — débit réel selon VRAM / RAM / disque"
                              >
                                <Icon name="speed" size={13} /> ~
                                {fmtTokPerSec(
                                  estimateAirllmTokPerSec(targetStorageGb, hardwareSpec, v.size) ??
                                    0,
                                )}
                              </span>
                            )}
                            {isInstalled && (
                              <span className="locaryn-tag locaryn-tag-installed">Installé</span>
                            )}
                            <code className="locaryn-variant-tag">{targetTag}</code>

                            <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                              {isInstalled ? (
                                <div style={{ display: "flex", gap: "4px", alignItems: "center" }}>
                                  <button
                                    type="button"
                                    className="locaryn-btn-primary"
                                    style={{ padding: "3px 8px", fontSize: "11px" }}
                                    onClick={() => onSelectModelForChat?.(targetTag)}
                                    title="Utiliser ce modèle dans le Chat"
                                  >
                                    <Icon name="chat" size={15} /> Utiliser
                                  </button>
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
                                  <Icon name="close" size={15} /> Annuler ({progress}%)
                                </button>
                              ) : compatV.level === "airllm" ? (
                                (() => {
                                  const entry = getAirllmEntry(f);
                                  const installed = entry ? airllmInstalled.has(entry.repo) : false;
                                  const busy = entry ? airllmBusy[entry.repo] : false;
                                  if (!entry) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-ghost"
                                        style={{
                                          border: "1px dashed var(--info)",
                                          color: "var(--info)",
                                        }}
                                        onClick={() => setAirllmModalOpen(true)}
                                        title="Cette architecture n'est pas encore supportée par AirLLM (Llama, Mistral, Qwen2…)"
                                      >
                                        <Icon name="star" size={15} /> Info AirLLM
                                      </button>
                                    );
                                  }
                                  if (installed) {
                                    return (
                                      <button
                                        type="button"
                                        className="locaryn-btn-primary locaryn-variant-use"
                                        style={{
                                          background: "var(--info)",
                                          color: "var(--on-accent)",
                                        }}
                                        onClick={() => onLaunchAirllm?.(entry.repo)}
                                        title={`Lancer ${entry.repo} via le moteur AirLLM`}
                                      >
                                        <Icon name="speed" size={15} /> Lancer avec AirLLM
                                      </button>
                                    );
                                  }
                                  return (
                                    <button
                                      type="button"
                                      className="locaryn-btn-primary locaryn-variant-use"
                                      style={{
                                        background: "var(--info)",
                                        color: "var(--on-accent)",
                                      }}
                                      disabled={busy}
                                      onClick={() => handleAirllmInstall(f)}
                                      title={`Télécharger ${entry.repo} (~${entry.sizeGb} Go fp16) puis lancer via AirLLM`}
                                    >
                                      {busy
                                        ? "⏳ AirLLM…"
                                        : `Installer via AirLLM (~${entry.sizeGb} Go)`}
                                    </button>
                                  );
                                })()
                              ) : (
                                <button
                                  type="button"
                                  className="locaryn-btn-primary locaryn-variant-use"
                                  onClick={() =>
                                    requestInstall(
                                      targetTag,
                                      f.name,
                                      Boolean(f.uncensored),
                                      v.downloads,
                                    )
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
                  <Icon name="star" size={15} /> AirLLM — Gros modèles sur petit GPU
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
                <Icon name="close" size={16} />
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
                style={{ background: "var(--info)", color: "var(--on-accent)" }}
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

      {/* HuggingFace variant selector. A repository is not a model: it can
          contain many quantisations and several checkpoints. */}
      {repoInspection && repoInstallContext && (
        <div
          className="locaryn-settings-backdrop"
          onClick={(e) => {
            if (Date.now() - repoInspectionOpenedAt.current < 400) return;
            if (e.target === e.currentTarget && !repoInspecting) {
              setRepoInspection(null);
              setRepoInstallContext(null);
            }
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape" && !repoInspecting) {
              setRepoInspection(null);
              setRepoInstallContext(null);
            }
          }}
        >
          <div
            className="locaryn-card"
            style={{
              width: "680px",
              maxWidth: "94vw",
              maxHeight: "86vh",
              overflowY: "auto",
              margin: "48px auto",
              border: "1px solid var(--border-strong)",
              boxShadow: "0 16px 40px rgba(0,0,0,0.85)",
            }}
          >
            <div className="locaryn-field-head" style={{ marginBottom: "12px" }}>
              <div>
                <h3 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
                  <Icon name="models" size={17} /> Choisir le modèle à installer
                </h3>
                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-faint)" }}>
                  {repoInspection.repo}
                  {repoInspection.suggested_repo
                    ? " — conversion requise pour llama.cpp"
                    : " — une seule variante sera téléchargée, pas tout le dépôt."}
                </span>
              </div>
              <button
                type="button"
                className="locaryn-icon-btn"
                onClick={() => {
                  if (!repoInspecting) {
                    setRepoInspection(null);
                    setRepoInstallContext(null);
                  }
                }}
                disabled={repoInspecting}
                aria-label="Fermer"
              >
                <Icon name="close" size={16} />
              </button>
            </div>

            {repoInspecting ? (
              <div style={{ padding: "24px 8px", color: "var(--text-dim)", textAlign: "center" }}>
                <span
                  className="locaryn-spin"
                  style={{ display: "inline-flex", marginRight: "8px" }}
                >
                  <Icon name="refresh" size={16} />
                </span>
                Analyse des fichiers et des quantifications HuggingFace…
              </div>
            ) : repoInspectionError ? (
              <div
                style={{
                  padding: "12px",
                  border: "1px solid var(--danger)",
                  borderRadius: "var(--radius-sm)",
                  color: "var(--danger)",
                  whiteSpace: "pre-wrap",
                }}
              >
                {repoInspectionError}
              </div>
            ) : repoInspection.suggested_repo ? (
              <div style={{ display: "grid", gap: "12px" }}>
                <div
                  style={{
                    padding: "12px",
                    border: "1px solid var(--warning, #d6a45c)",
                    borderRadius: "var(--radius-sm)",
                    background: "rgba(214, 164, 92, 0.08)",
                    color: "var(--text)",
                    lineHeight: 1.5,
                  }}
                >
                  {repoInspection.warning}
                </div>
                <div style={{ color: "var(--text-dim)", lineHeight: 1.5 }}>
                  Le dépôt d'origine contient {repoInspection.candidates.length || 1} checkpoint(s)
                  Transformers et ne sera pas installé. Locaryn peut ouvrir automatiquement{" "}
                  <strong>{repoInspection.suggested_repo}</strong>, puis proposer les variantes GGUF
                  compatibles et leur projecteur vision.
                </div>
              </div>
            ) : repoInspection.candidates.length === 0 ? (
              <div style={{ color: "var(--text-dim)", lineHeight: 1.5 }}>
                Aucun fichier de poids standard n'a été identifié. Ce dépôt semble être un paquet
                multi-fichiers (par exemple un modèle TTS). Installer le dépôt complet téléchargera
                aussi ses fichiers de configuration.
              </div>
            ) : (
              <div>
                {/* Educational Guide Box with Expandable Suffix Decoder */}
                <div
                  style={{
                    padding: "12px 14px",
                    background: "rgba(110, 168, 254, 0.08)",
                    border: "1px solid rgba(110, 168, 254, 0.22)",
                    borderRadius: "var(--radius-sm)",
                    marginBottom: "12px",
                    fontSize: "12px",
                    lineHeight: 1.5,
                    color: "var(--text)",
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      marginBottom: "4px",
                      flexWrap: "wrap",
                      gap: "6px",
                    }}
                  >
                    <div
                      style={{
                        fontWeight: 700,
                        color: "var(--accent, #6ea8fe)",
                        display: "flex",
                        alignItems: "center",
                        gap: "6px",
                      }}
                    >
                      <Icon name="question" size={15} /> Guide de choix de la quantification
                    </div>
                    <button
                      type="button"
                      onClick={() => setShowQuantGuideDetails(!showQuantGuideDetails)}
                      style={{
                        background: "none",
                        border: "none",
                        color: "var(--accent, #6ea8fe)",
                        fontSize: "11px",
                        fontWeight: 600,
                        cursor: "pointer",
                        textDecoration: "underline",
                        padding: 0,
                      }}
                    >
                      {showQuantGuideDetails
                        ? "Masquer les détails ▲"
                        : "Comprendre les suffixes (_K_M, _0, IQ, Q6_K vs Q8_0) ▼"}
                    </button>
                  </div>
                  La quantification compresse le modèle pour tourner localement avec fluidité sans
                  saturer votre mémoire vive (RAM) ou carte graphique (VRAM).
                  <div
                    style={{
                      marginTop: "6px",
                      display: "grid",
                      gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
                      gap: "6px",
                      fontSize: "11px",
                    }}
                  >
                    <div
                      style={{
                        padding: "6px 8px",
                        background: "rgba(0,0,0,0.25)",
                        borderRadius: "4px",
                      }}
                    >
                      <strong style={{ color: "var(--accent-300)" }}>Q4_K_M (Recommandé)</strong>
                      <div style={{ color: "var(--text-dim)" }}>
                        Équilibre parfait vitesse / intelligence.
                      </div>
                    </div>
                    <div
                      style={{
                        padding: "6px 8px",
                        background: "rgba(0,0,0,0.25)",
                        borderRadius: "4px",
                      }}
                    >
                      <strong style={{ color: "var(--warn)" }}>⚡ Q3_K / IQ (Ultra léger)</strong>
                      <div style={{ color: "var(--text-dim)" }}>
                        Pour PC modestes ou cartes 4-8 Go.
                      </div>
                    </div>
                    <div
                      style={{
                        padding: "6px 8px",
                        background: "rgba(0,0,0,0.25)",
                        borderRadius: "4px",
                      }}
                    >
                      <strong style={{ color: "var(--info)" }}>Q6_K / Q8_0 (Qualité max)</strong>
                      <div style={{ color: "var(--text-dim)" }}>
                        Fidélité maximale, requiert plus de RAM.
                      </div>
                    </div>
                  </div>
                  {showQuantGuideDetails && (
                    <div
                      style={{
                        marginTop: "10px",
                        padding: "10px 12px",
                        background: "rgba(0,0,0,0.35)",
                        borderRadius: "6px",
                        border: "1px solid rgba(255,255,255,0.08)",
                        display: "flex",
                        flexDirection: "column",
                        gap: "6px",
                        fontSize: "11px",
                        lineHeight: 1.45,
                      }}
                    >
                      <div>
                        <strong style={{ color: "var(--accent-300)" }}>• Q4_K_M vs Q4_0 :</strong>{" "}
                        <code>_K_M</code> (K-Quant adaptatif) compresse intelligemment chaque couche
                        selon son importance, préservant la précision sur l'attention critique.{" "}
                        <code>Q4_0</code> applique une compression 4-bit uniforme (plus ancienne et
                        un peu moins précise).
                      </div>
                      <div>
                        <strong style={{ color: "var(--info)" }}>• Q6_K vs Q8_0 :</strong>{" "}
                        <code>Q6_K</code> offre 99.5% de la qualité de <code>Q8_0</code> tout en
                        économisant ~20% d'espace mémoire. Si vous voulez la qualité maximale,{" "}
                        <code>Q6_K</code> est souvent le meilleur choix pragmatique.
                      </div>
                      <div>
                        <strong style={{ color: "var(--warn)" }}>• IQ (Importance Matrix) :</strong>{" "}
                        Ces versions (ex: <code>IQ3_M</code>, <code>IQ4_XS</code>) utilisent une
                        matrice de calibration pour préserver un maximum de cohérence tout en
                        réduisant drastiquement le poids.
                      </div>
                      <div>
                        <strong style={{ color: "var(--info)" }}>• _K_S / _K_M / _K_L :</strong>{" "}
                        <code>S</code> = Small (légèrement allégé), <code>M</code> = Medium
                        (recommandé), <code>L</code> = Large (précision renforcée).
                      </div>
                    </div>
                  )}
                </div>

                {/* Filter chips */}
                {repoInspection.candidates.length > 4 && (
                  <div
                    style={{ display: "flex", gap: "6px", marginBottom: "12px", flexWrap: "wrap" }}
                  >
                    <button
                      type="button"
                      className={`locaryn-chip ${quantFilter === "all" ? "locaryn-chip-on" : ""}`}
                      onClick={() => setQuantFilter("all")}
                      style={{ fontSize: "11px", padding: "4px 9px" }}
                    >
                      Toutes ({repoInspection.candidates.length})
                    </button>
                    {repoInspection.candidates.filter(
                      (c) => getQuantizationAdvice(c.quantization, c.format).isRecommended,
                    ).length > 0 && (
                      <button
                        type="button"
                        className={`locaryn-chip ${quantFilter === "recommended" ? "locaryn-chip-on" : ""}`}
                        onClick={() => setQuantFilter("recommended")}
                        style={{ fontSize: "11px", padding: "4px 9px" }}
                      >
                        Recommandées (
                        {
                          repoInspection.candidates.filter(
                            (c) => getQuantizationAdvice(c.quantization, c.format).isRecommended,
                          ).length
                        }
                        )
                      </button>
                    )}
                    {repoInspection.candidates.filter(
                      (c) => getQuantizationAdvice(c.quantization, c.format).category === "light",
                    ).length > 0 && (
                      <button
                        type="button"
                        className={`locaryn-chip ${quantFilter === "light" ? "locaryn-chip-on" : ""}`}
                        onClick={() => setQuantFilter("light")}
                        style={{ fontSize: "11px", padding: "4px 9px" }}
                      >
                        ⚡ Légères (Q2/Q3/IQ)
                      </button>
                    )}
                    {repoInspection.candidates.filter((c) => {
                      const cat = getQuantizationAdvice(c.quantization, c.format).category;
                      return cat === "high" || cat === "max";
                    }).length > 0 && (
                      <button
                        type="button"
                        className={`locaryn-chip ${quantFilter === "quality" ? "locaryn-chip-on" : ""}`}
                        onClick={() => setQuantFilter("quality")}
                        style={{ fontSize: "11px", padding: "4px 9px" }}
                      >
                        Haute fidélité (Q5/Q6/Q8)
                      </button>
                    )}
                  </div>
                )}

                {/* Candidate Cards */}
                <div
                  style={{
                    display: "grid",
                    gap: "8px",
                    maxHeight: "380px",
                    overflowY: "auto",
                    paddingRight: "4px",
                  }}
                >
                  {repoInspection.candidates
                    .filter((candidate) => {
                      if (quantFilter === "all") return true;
                      const advice = getQuantizationAdvice(
                        candidate.quantization,
                        candidate.format,
                      );
                      if (quantFilter === "recommended") return advice.isRecommended;
                      if (quantFilter === "light") return advice.category === "light";
                      if (quantFilter === "quality")
                        return advice.category === "high" || advice.category === "max";
                      return true;
                    })
                    .map((candidate) => {
                      const selected = candidate.id === repoCandidateId;
                      const advice = getQuantizationAdvice(
                        candidate.quantization,
                        candidate.format,
                      );
                      return (
                        <button
                          key={candidate.id}
                          type="button"
                          onClick={() => setRepoCandidateId(candidate.id)}
                          style={{
                            textAlign: "left",
                            display: "grid",
                            gridTemplateColumns: "20px 1fr auto",
                            gap: "12px",
                            alignItems: "center",
                            padding: "12px 14px",
                            borderRadius: "var(--radius-sm)",
                            border: selected
                              ? "1px solid var(--accent, #6ea8fe)"
                              : "1px solid var(--border-strong, rgba(255,255,255,0.12))",
                            background: selected
                              ? "rgba(110, 168, 254, 0.12)"
                              : "var(--surface, rgba(255,255,255,0.03))",
                            color: "var(--text)",
                            cursor: "pointer",
                            transition: "all 0.15s ease",
                          }}
                        >
                          <span
                            aria-hidden="true"
                            style={{
                              width: "16px",
                              height: "16px",
                              borderRadius: "50%",
                              border: selected
                                ? "5px solid var(--accent)"
                                : "1px solid var(--border-strong)",
                              boxSizing: "border-box",
                              background: selected ? "var(--surface)" : "transparent",
                            }}
                          />
                          <span
                            style={{
                              minWidth: 0,
                              display: "flex",
                              flexDirection: "column",
                              gap: "4px",
                            }}
                          >
                            <div
                              style={{
                                display: "flex",
                                alignItems: "center",
                                gap: "8px",
                                flexWrap: "wrap",
                              }}
                            >
                              <strong style={{ fontSize: "13px", overflowWrap: "anywhere" }}>
                                {candidate.label}
                              </strong>
                              <span
                                style={{
                                  fontSize: "10px",
                                  fontWeight: 700,
                                  padding: "2px 7px",
                                  borderRadius: "99px",
                                  ...advice.badgeStyle,
                                }}
                              >
                                {advice.badge}
                              </span>
                              {candidate.quantization && (
                                <span
                                  style={{
                                    fontSize: "10px",
                                    padding: "2px 6px",
                                    borderRadius: "4px",
                                    background: "rgba(255,255,255,0.06)",
                                    color: "var(--text-dim)",
                                  }}
                                >
                                  {candidate.quantization}
                                </span>
                              )}
                            </div>
                            <span
                              style={{
                                fontSize: "11px",
                                color: "var(--text)",
                                lineHeight: 1.4,
                              }}
                            >
                              {advice.advice}
                            </span>
                            <div
                              style={{
                                display: "flex",
                                alignItems: "center",
                                gap: "10px",
                                fontSize: "10px",
                                color: "var(--text-faint)",
                                flexWrap: "wrap",
                              }}
                            >
                              <span style={{ color: "var(--accent, #6ea8fe)", fontWeight: 600 }}>
                                {estimateRamUsage(candidate.total_bytes)}
                              </span>
                              {(candidate.files.length > 1 ||
                                candidate.support_files.length > 0) && (
                                <span>
                                  {candidate.files.length > 1
                                    ? `${candidate.files.length} shards de poids `
                                    : ""}
                                  {candidate.support_files.length > 0
                                    ? `· ${candidate.support_files.length} fichier(s) compagnon(s)`
                                    : ""}
                                </span>
                              )}
                            </div>
                          </span>
                          <div style={{ textAlign: "right" }}>
                            <span
                              style={{
                                fontSize: "13px",
                                fontWeight: 700,
                                color: "var(--text)",
                                whiteSpace: "nowrap",
                              }}
                            >
                              {formatBytes(candidate.total_bytes)}
                            </span>
                          </div>
                        </button>
                      );
                    })}
                </div>
              </div>
            )}

            <div
              className="locaryn-field-actions"
              style={{ justifyContent: "space-between", gap: "8px", marginTop: "16px" }}
            >
              <span style={{ fontSize: "11px", color: "var(--text-faint)" }}>
                {repoInspection.suggested_repo
                  ? "Les poids Safetensors existants ne seront ni relancés ni retéléchargés."
                  : repoInspection.candidates.length > 0
                    ? `${repoInspection.candidates.length} variante(s) détectée(s) · ${formatBytes(repoInspection.total_bytes)} au total dans le dépôt`
                    : "Les fichiers alternatifs ne seront pas ciblés automatiquement."}
              </span>
              <div style={{ display: "flex", gap: "8px" }}>
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => {
                    setRepoInspection(null);
                    setRepoInstallContext(null);
                  }}
                  disabled={repoInspecting}
                >
                  Annuler
                </button>
                {repoInspection.suggested_repo ? (
                  <button
                    type="button"
                    className="locaryn-btn-primary"
                    onClick={inspectSuggestedRepo}
                    disabled={repoInspecting}
                  >
                    <Icon name="refresh" size={14} /> Ouvrir la version GGUF compatible
                  </button>
                ) : repoInspection.candidates.length > 0 ? (
                  <button
                    type="button"
                    className="locaryn-btn-primary"
                    onClick={confirmRepoCandidate}
                    disabled={!repoCandidateId || repoInspecting}
                  >
                    <Icon name="download" size={14} /> Installer cette variante
                  </button>
                ) : (
                  <button
                    type="button"
                    className="locaryn-btn-primary"
                    onClick={() => {
                      const { source, familyName, heretic, consent, downloads } =
                        repoInstallContext;
                      setRepoInspection(null);
                      setRepoInstallContext(null);
                      void beginModelInstall(
                        source,
                        familyName,
                        heretic,
                        consent,
                        undefined,
                        downloads,
                        true,
                      );
                    }}
                    disabled={repoInspecting || Boolean(repoInspectionError)}
                  >
                    <Icon name="download" size={14} /> Installer le dépôt complet
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Custom Model / HuggingFace download modal */}
      {customDownloadModalOpen && (
        <div
          className="locaryn-settings-backdrop"
          onClick={(e) => {
            if (e.target === e.currentTarget) setCustomDownloadModalOpen(false);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") setCustomDownloadModalOpen(false);
          }}
        >
          <div
            className="locaryn-card"
            style={{
              width: "520px",
              maxWidth: "92vw",
              maxHeight: "85vh",
              overflowY: "auto",
              margin: "60px auto",
              border: "1px solid var(--border-strong)",
              boxShadow: "0 16px 40px rgba(0,0,0,0.85)",
            }}
          >
            <div className="locaryn-field-head" style={{ marginBottom: "14px" }}>
              <div>
                <h3 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
                  <Icon name="download" size={16} /> Ajouter depuis un dépôt
                </h3>
                <span style={{ fontSize: "var(--text-xs)", color: "var(--text-faint)" }}>
                  Téléchargez n'importe quel modèle depuis HuggingFace, Ollama ou une URL directe.
                </span>
              </div>
              <button
                type="button"
                className="locaryn-icon-btn"
                onClick={() => setCustomDownloadModalOpen(false)}
                aria-label="Fermer"
              >
                <Icon name="close" size={16} />
              </button>
            </div>

            <form
              onSubmit={(e) => {
                e.preventDefault();
                const trimmed = customTagInput.trim();
                if (!trimmed) return;
                setCustomDownloadModalOpen(false);
                setCustomTagInput("");
                const repo = hfRepoSource(trimmed);
                if (repo) {
                  void beginModelInstall(repo, trimmed, false, false);
                } else {
                  requestInstall(trimmed, trimmed, false);
                }
              }}
            >
              <div style={{ marginBottom: "16px" }}>
                <label
                  htmlFor="custom-model-tag-input"
                  style={{
                    display: "block",
                    fontSize: "var(--text-xs)",
                    color: "var(--text-dim)",
                    marginBottom: "6px",
                  }}
                >
                  Nom du modèle, identifiant HuggingFace ou URL :
                </label>
                <input
                  id="custom-model-tag-input"
                  className="locaryn-input"
                  style={{ width: "100%", fontSize: "13px" }}
                  placeholder="ex: gemma4:2b, kimi-k3:8b, mimo:7b, hf.co/user/repo, https://huggingface.co/..."
                  value={customTagInput}
                  onChange={(e) => setCustomTagInput(e.target.value)}
                  autoFocus
                />
                <div
                  style={{
                    fontSize: "11px",
                    color: "var(--text-faint)",
                    marginTop: "6px",
                    lineHeight: 1.4,
                  }}
                >
                  Formats acceptés : tag Ollama (<code>gemma4:2b</code>), alias HuggingFace (
                  <code>hf.co/organisation/modele</code>) ou lien HTTP direct vers un fichier{" "}
                  <code>.gguf</code>.
                </div>
              </div>

              <div
                className="locaryn-field-actions"
                style={{ justifyContent: "flex-end", gap: "8px" }}
              >
                <button
                  type="button"
                  className="locaryn-btn-ghost"
                  onClick={() => setCustomDownloadModalOpen(false)}
                >
                  Annuler
                </button>
                <button
                  type="submit"
                  className="locaryn-btn-primary"
                  disabled={!customTagInput.trim()}
                >
                  <Icon name="download" size={14} /> Télécharger ce modèle
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

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
