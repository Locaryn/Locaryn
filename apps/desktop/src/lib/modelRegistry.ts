// ═══════════════════════════════════════════════════════════════════════════════
// modelRegistry.ts — Dynamic model discovery (Ollama API + HuggingFace + seed)
// No more manual maintenance. Models are fetched at runtime from:
//   1. Ollama local API  (/api/tags)          → installed models
//   2. Ollama Library    (via Rust IPC proxy)  → all available models
//   3. HuggingFace Hub   (/api/models?gguf)    → community GGUF models
//   4. SEED_CATALOG      (packages/shared-types/chat-models.json) → socle hors ligne
// ═══════════════════════════════════════════════════════════════════════════════

// Les modèles de conversation du socle vivent dans un fichier partagé : le
// service le sert au téléphone et au web, qui proposent ainsi les mêmes.
import catalogueSocle from "../../../../packages/shared-types/chat-models.json";

// ── Types ───────────────────────────────────────────────────────────────────

export interface ModelVariant {
  size: string;
  params: number;
  tag: string;
  quants: string[];
  storageGb: number;
  instruct?: boolean;
  /** Additional files declared by the extension that owns this catalogue entry. */
  downloads?: ModelDownloadSource[];
  /** La forme de l'attention, quand le catalogue la publie : sans elle,
   *  l'estimation devine le cache d'après le nombre de paramètres. */
  attention?: AttentionShape;
}

/** Couches qui gardent un cache clés-valeurs, têtes clés-valeurs, dimension
 *  d'une tête. Un modèle hybride (Qwen 3.5, 3.6) n'a qu'une couche d'attention
 *  pleine sur quatre : deviné, son cache paraît quatre fois plus gros. */
export interface AttentionShape {
  layers: number;
  kv_heads: number;
  head_dim: number;
}

/** One companion file in an extension-owned model installation plan. */
export interface ModelDownloadSource {
  url: string;
  file: string;
  label?: string;
}

export interface ModelFamily {
  id: string;
  name: string;
  brand: string;
  description: string;
  /** English rendering of `description`. Absent = French only (shown as-is).
   *  `name` and `brand` are product nouns and are never translated. */
  descriptionEn?: string;
  license: string;
  contextWindow?: string;
  releaseDate: string;
  releaseYear: number;
  vision?: boolean;
  audio?: boolean;
  code?: boolean;
  reasoning?: boolean;
  instruct?: boolean;
  finetunable?: boolean;
  uncensored?: boolean;
  imageGen?: boolean;
  /** True if the family performs text-to-speech synthesis. */
  tts?: boolean;
  /** True if the family supports voice cloning / style transfer from a reference sample. */
  voiceCloning?: boolean;
  /** True if the family can generate a voice from a text description / prompt. */
  voiceDesign?: boolean;
  videoGen?: boolean;
  translation?: boolean;
  model3d?: boolean;
  musicGen?: boolean;
  objectDetection?: boolean;
  textAnalysis?: boolean;
  imageEditing?: boolean;
  questionAnswering?: boolean;
  /** Data-driven capabilities used by extension-contributed Marketplace filters. */
  marketplaceCapabilities?: string[];
  /** Enabled extension that supplied this family. Absent for the native catalogue. */
  marketplaceOwner?: string;
  variants: ModelVariant[];
  /** Number of pulls from Ollama registry (for sorting by popularity). */
  pulls?: number;
  /** Source: "seed" | "ollama" | "huggingface" */
  source?: string;
}

/** True when every variant of a family is cloud-only. */
export function isCloudOnlyFamily(f: ModelFamily): boolean {
  return f.variants.length > 0 && f.variants.every((v) => v.quants.includes("cloud"));
}

// ── TTS Capabilities ───────────────────────────────────────────────────────

/** What a TTS model can do. Every flag gates a UI section. */
export interface TTSCapabilities {
  /** Upload a reference audio sample to clone a voice (zero-shot). */
  cloning: boolean;
  /** Generate a voice from a text description/prompt. */
  voiceDesign: boolean;
  /** The model exposes fine-grained control sliders (pitch, speed, energy). */
  expressiveness: boolean;
  /** Whether the model outputs audio in real-time chunks. */
  streaming: boolean;
  /** Specific languages supported (empty = unknown, ["all"] = multilingual). */
  languages: string[];
  /** Runtime format: "onnx" = single-file, "repo" = full HF repo,
   *  "gguf" = quantized single-file, "torch" = .pth checkpoint. */
  format: "onnx" | "repo" | "gguf" | "torch" | "unknown";
}

/** Default capabilities for models without explicit metadata. */
const DEFAULT_TTS_CAPABILITIES: TTSCapabilities = {
  cloning: false,
  voiceDesign: false,
  expressiveness: false,
  streaming: false,
  languages: [],
  format: "unknown",
};

export function getModelCapabilities(modelName: string): TTSCapabilities {
  const lower = modelName.toLowerCase();
  const caps = { ...DEFAULT_TTS_CAPABILITIES };
  if (/clone|xtts|coqui|zero.shot|custom.?voice|customvoice/i.test(lower)) caps.cloning = true;
  if (/design|prompt|description|text.to.voice|voice.?design|voicedesign/i.test(lower))
    caps.voiceDesign = true;
  if (/express|emotion|controllable|pitch/i.test(lower)) caps.expressiveness = true;
  if (/stream|realtime|low.latency/i.test(lower)) caps.streaming = true;
  if (/onnx/i.test(lower)) caps.format = "onnx";
  else if (/gguf/i.test(lower)) caps.format = "gguf";
  else if (/repo|huggingface/i.test(lower)) caps.format = "repo";
  else if (/\.pth|\.pt|torch/i.test(lower)) caps.format = "torch";
  return caps;
}

function buildCaps(f: ModelFamily): TTSCapabilities {
  return {
    cloning: f.voiceCloning === true,
    voiceDesign:
      f.voiceDesign === true || /voice.?design|text.?to.?voice/i.test(`${f.description} ${f.name}`),
    expressiveness: /express|emotion|control|controllable/i.test(f.description),
    streaming: /stream|realtime|real.time/i.test(f.description),
    languages: /multilingue|multilingual|\d+ .*lang/i.test(f.description) ? ["all"] : [],
    format: f.variants.some((v) => v.quants.includes("onnx"))
      ? "onnx"
      : f.variants.some((v) => v.quants.includes("repo"))
        ? "repo"
        : f.variants.some((v) => v.quants.includes("pth"))
          ? "torch"
          : f.variants.some((v) => v.tag.endsWith(".gguf"))
            ? "gguf"
            : "unknown",
  };
}

// ── Model classification ───────────────────────────────────────────────

/** All possible types a model can be. Components use this to filter pickers. */
export type ModelKind =
  | "chat" // GGUF text-in/text-out LLMs
  | "vision" // multimodal with image input
  | "code" // code-specialised
  | "reasoning" // chain-of-thought / deep reasoning
  | "image-gen" // diffusion / image generation
  | "tts" // text-to-speech (Piper, Kokoro, XTTS, etc.)
  | "music-gen" // text-to-music (MusicGen, AudioLDM, etc.)
  | "video-gen" // text-to-video (Wan2.1, LTX, SVD, etc.)
  | "3d-gen" // text-to-3D (Shape-E, TripoSR, etc.)
  | "object-detection"
  | "translation"
  | "text-analysis"
  | "image-editing"
  | "question-answering"
  | "unknown";

/** Classification result: the model kind plus the matched family (if any). */
export interface ModelClassification {
  kind: ModelKind;
  family: ModelFamily | null;
}

/**
 * Classify a model name into a ModelKind by scanning all registries
 * (TTS, image, video, music, 3D, seed catalog) and falling back to
 * keyword heuristics. This is the canonical classification used by
 * QuickModelSelector and every generation panel.
 */
export function classifyModel(modelName: string): ModelClassification {
  const lower = modelName.toLowerCase();

  // 1. Check heuristics for specialized modalities FIRST
  // (Prevents generic LLM family ids like "qwen3" from incorrectly capturing "qwen3-tts" or "nomic-embed")
  if (/embed|embedding|bge-|all-minilm|text2vec|gte-|e5-/i.test(lower)) {
    return { kind: "unknown", family: null };
  }
  if (
    /shap.?e|point.?e|triposr|tripo.?sr|zero.?1.?to.?3|zero123|threestudio|3d.*model|mesh/i.test(
      lower,
    )
  ) {
    return { kind: "3d-gen", family: null };
  }
  if (
    /wan.?2|ltx.?video|svd|stable.?video|cogvideo|hunyuan.?video|mochi|genmo|video.?diffusion/i.test(
      lower,
    )
  ) {
    return { kind: "video-gen", family: null };
  }
  if (/musicgen|audioldm|stable.?audio|riffusion|bark|text.?to.?music/i.test(lower)) {
    return { kind: "music-gen", family: null };
  }
  if (
    /piper|kokoro|xtts|tts|coqui|chatterbox|qwen.*tts|voxcpm|omnivoice|parler|vibevoice|moss.?tts|higgs.?tts|melotts|voice.?clone|voice.?design|text.?to.?speech|f5.?tts|e2.?tts/i.test(
      lower,
    )
  ) {
    return { kind: "tts", family: null };
  }
  if (
    /flux|stable.?diffusion|sdxl|sd-|\bsd15\b|\bsd3\b|z.?image|krea|dreamshaper|realistic|inpainting|controlnet|image.?gen/i.test(
      lower,
    )
  ) {
    return { kind: "image-gen", family: null };
  }
  if (/yolo|detr|sam|object.?detect/i.test(lower)) {
    return { kind: "object-detection", family: null };
  }
  if (/nllb|m2m|opus.?mt|translate/i.test(lower)) {
    return { kind: "translation", family: null };
  }

  // 3. Exclude non-GGUF weights that aren't LLMs (safetensors / onnx / pt files)
  if (/\.onnx$|\.safetensors$|\.pth$|\.bin$|\.pt$/i.test(lower)) {
    return { kind: "unknown", family: null };
  }

  // 4. Match SEED_CATALOG variants & families
  for (const family of SEED_CATALOG) {
    let flag: ModelKind = "chat";
    if (family.code) flag = "code";
    else if (family.reasoning) flag = "reasoning";
    else if (family.vision) flag = "vision";
    else if (family.instruct) flag = "chat";

    for (const variant of family.variants) {
      if (lower.includes(variant.tag.toLowerCase().replace(/^https:\/\/huggingface.co\//, ""))) {
        return { kind: flag, family };
      }
    }

    if (lower.includes(family.id.toLowerCase())) {
      return { kind: flag, family };
    }
  }

  // 5. LLM Heuristic Sub-classification
  if (
    /deepseek-r1|r1-distill|qwq|deepseek.?reasoner|marco-o1|\bo1-|\bo3-|skywork-o1|reasoning/i.test(
      lower,
    )
  ) {
    return { kind: "reasoning", family: null };
  }
  if (/coder|starcoder|codellama|deepseek-coder|codegeex|qwen.*coder/i.test(lower)) {
    return { kind: "code", family: null };
  }
  if (/vl|vision|llava|minicpm-v|qwen.*vl|pixtral|molmo|florence|paligemma/i.test(lower)) {
    return { kind: "vision", family: null };
  }

  if (/\.gguf$/i.test(lower)) {
    return { kind: "chat", family: null };
  }

  return { kind: "chat", family: null };
}

/** True when a model is suitable for the chat model picker (not TTS/image/video/music/3D/embedding). */
export function isChatModel(name: string): boolean {
  const lower = name.toLowerCase();
  // Exclude embedding models
  if (/embed|embedding|bge-|all-minilm|text2vec|gte-|e5-/i.test(lower)) return false;
  // Exclude audio / TTS / music / video / image diffusion files
  if (
    /tts|voice|kokoro|piper|parler|chatterbox|coqui|speech|music|video|diffusion|flux|inpainting/i.test(
      lower,
    )
  ) {
    return false;
  }
  // Exclude raw safetensors/onnx/pth non-chat files
  if (/\.onnx$|\.safetensors$|\.pth$|\.bin$|\.pt$/i.test(lower)) return false;

  const { kind } = classifyModel(name);
  return kind === "chat" || kind === "vision" || kind === "code" || kind === "reasoning";
}

/** True when a model is a text-to-image model. */
export function isImageGenModel(name: string): boolean {
  return classifyModel(name).kind === "image-gen";
}

/** True when a model is a text-to-speech / voice model. */
export function isTTSModel(name: string): boolean {
  return classifyModel(name).kind === "tts";
}

/** True when a model is a text-to-music model. */
export function isMusicGenModel(name: string): boolean {
  return classifyModel(name).kind === "music-gen";
}

/** True when a model is a text-to-video model. */
export function isVideoGenModel(name: string): boolean {
  return classifyModel(name).kind === "video-gen";
}

/** True when a model is a text-to-3D model. */
export function isModel3DGenModel(name: string): boolean {
  return classifyModel(name).kind === "3d-gen";
}

/**
 * Group a list of model paths by their classified kind.
 * Returns a map keyed by ModelKind.
 */
export function groupModelsByKind(models: string[]): Record<ModelKind, string[]> {
  const groups: Record<string, string[]> = {};
  for (const m of models) {
    const { kind } = classifyModel(m);
    if (!groups[kind]) groups[kind] = [];
    groups[kind].push(m);
  }
  return groups as Record<ModelKind, string[]>;
}

export interface SizeBucket {
  id: string;
  label: string;
  test: (p: number) => boolean;
}

export const SIZE_BUCKETS: SizeBucket[] = [
  { id: "tiny", label: "≤ 3B", test: (p) => p <= 3 },
  { id: "small", label: "3–14B", test: (p) => p > 3 && p <= 14 },
  { id: "mid", label: "14–35B", test: (p) => p > 14 && p <= 35 },
  { id: "large", label: "35–100B", test: (p) => p > 35 && p <= 100 },
  { id: "frontier", label: "> 100B", test: (p) => p > 100 },
];

// ── Category filter ─────────────────────────────────────────────────────────

export type ModelCategory = string;

/**
 * A Marketplace filter. The core only knows what its chat engine does; every
 * specialised filter comes from an active morph through `marketplace.catalogs`.
 */
export interface ModelCategoryDefinition {
  id: ModelCategory;
  label: string;
  /** English rendering of `label`. Absent = shown as-is. */
  labelEn?: string;
  icon: string;
  /** A family is included when it declares at least one matching capability. */
  matches: string[];
  /** Optional runtime capabilities required before the filter is visible. */
  requires?: string[];
}

export const MODEL_CATEGORIES: ModelCategoryDefinition[] = [
  { id: "all", label: "Tous", labelEn: "All", icon: "models", matches: [] },
  { id: "chat", label: "Chat / Instruct", icon: "chat", matches: ["chat"] },
  { id: "code", label: "Code", icon: "code", matches: ["code"] },
  { id: "vision", label: "Vision", icon: "image", matches: ["vision"] },
  { id: "reasoning", label: "Raisonnement", labelEn: "Reasoning", icon: "memory", matches: ["reasoning"] },
];


/** Display language for catalogue data. */
export type DisplayLang = "fr" | "en";

/** Localised family description. `name` and `brand` are product nouns:
 *  they are never translated, only the description is. */
export function familyDescription(
  f: Pick<ModelFamily, "description" | "descriptionEn" | "source">,
  lang: DisplayLang,
): string {
  if (lang === "en") {
    if (f.descriptionEn) return f.descriptionEn;
    const hf = hfDiscoveredDescriptionEn(f.description);
    if (hf) return hf;
  }
  return f.description;
}

/** English rendering of an auto-generated HuggingFace discovery blurb
 *  ("Poids GGUF publies sur HuggingFace par …"), or null when the text is
 *  not one of those. Lets live-discovered models read English without
 *  storing a translation per repository. */
function hfDiscoveredDescriptionEn(fr: string): string | null {
  const m = /^Poids GGUF [\s\S]*? sur HuggingFace par (.+?)(?:, ([\d\s.,]+) [\s\S]*?chargements)?\.$/.exec(
    fr.trim(),
  );
  if (!m) return null;
  const author = m[1];
  const digits = m[2]?.replace(/[^\d]/g, "");
  if (digits) {
    const n = Number(digits);
    const social = Number.isFinite(n) ? n.toLocaleString("en-US") : m[2];
    return `GGUF weights published on HuggingFace by ${author}, ${social} downloads.`;
  }
  return `GGUF weights published on HuggingFace by ${author}.`;
}

/** Localised category label. Category ids are never translated. */
export function categoryLabelText(
  c: Pick<ModelCategoryDefinition, "label" | "labelEn">,
  lang: DisplayLang,
): string {
  return lang === "en" ? (c.labelEn ?? c.label) : c.label;
}

// ── Constants ───────────────────────────────────────────────────────────────

const CACHE_KEY = "locaryn_model_registry_cache_v19";
const CACHE_TTL_MS = 60 * 60 * 1000; // 1 hour

const QUANTS_SMALL = ["q4_K_M", "q5_K_M", "q8_0", "fp16"];
const QUANTS_BIG = ["q4_K_M", "q5_K_M", "q6_K", "q8_0"];

/** Does this tag/filename designate an IMAGE model (diffusion checkpoint)?
 *
 *  Mirrors `is_image_asset` in the Rust backend. Needed because the catalogue's
 *  `imageGen` flag is only set on curated entries — models discovered through
 *  the live HuggingFace search have no flag, and were therefore offered as chat
 *  models and loaded into the conversation. */
export function looksLikeImageModel(tagOrName: string): boolean {
  const n = tagOrName.toLowerCase();
  const diffusion = [
    "stable-diffusion",
    "stable_diffusion",
    "sd_xl",
    "sdxl",
    "sd15",
    "sd-v1",
    "sd_v1",
    "sd3",
    "z_image",
    "z-image",
    "z_img",
    "zimg",
    "flux",
    "krea",
    "dreamshaper",
    "juggernaut",
    "pony",
    "playground-v",
    "kolors",
    "hunyuan-dit",
    "pixart",
  ];
  const aux = ["mmproj-", "vae", "clip", "t5xxl", "text_encoder", "text-encoder"];
  return diffusion.some((p) => n.includes(p)) && !aux.some((p) => n.includes(p));
}

// ── Seed Catalog (offline fallback — top ~20 families) ──────────────────────

export const SEED_CATALOG: ModelFamily[] = catalogueSocle.families as ModelFamily[];

// ── Curated AirLLM-ready large models (layer-by-layer low VRAM inference) ──

export const AIRLLM_CATALOG_MODELS: ModelFamily[] = [
  {
    id: "deepseek-r1-70b-airllm",
    name: "DeepSeek-R1 Distill Llama 70B (AirLLM)",
    brand: "DeepSeek / AirLLM",
    description:
      "Modèle de raisonnement 70B exécutable sur petit GPU (4 Go VRAM) grâce au chargement couche par couche AirLLM.",
      descriptionEn: "70B reasoning model runnable on a small GPU (4GB VRAM) thanks to AirLLM layer-by-layer loading.",
    license: "MIT",
    contextWindow: "128k",
    releaseDate: "2025-01",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    source: "airllm",
    variants: [
      {
        size: "70B (fp16 AirLLM)",
        params: 70,
        tag: "airllm:deepseek-ai/DeepSeek-R1-Distill-Llama-70B",
        quants: ["airllm"],
        storageGb: 140,
        instruct: true,
      },
    ],
  },
  {
    id: "llama-3.3-70b-airllm",
    name: "Llama 3.3 70B Instruct (AirLLM)",
    brand: "Meta / AirLLM",
    description:
      "Flagship 70B de Meta exécutable en local sur machine modeste via le streaming de couches AirLLM.",
      descriptionEn: "Meta's 70B flagship runnable locally on a modest machine via AirLLM layer streaming.",
    license: "Llama 3.3 Community",
    contextWindow: "128k",
    releaseDate: "2024-12",
    releaseYear: 2024,
    instruct: true,
    reasoning: true,
    source: "airllm",
    variants: [
      {
        size: "70B (fp16 AirLLM)",
        params: 70,
        tag: "airllm:unsloth/Llama-3.3-70B-Instruct",
        quants: ["airllm"],
        storageGb: 140,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen2.5-72b-airllm",
    name: "Qwen 2.5 72B Instruct (AirLLM)",
    brand: "Alibaba / Qwen / AirLLM",
    description:
      "Le plus puissant modèle Qwen 2.5 de 72B exécutable localement avec ~4 Go de VRAM.",
      descriptionEn: "The most powerful 72B Qwen 2.5 model runnable locally with ~4GB of VRAM.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    instruct: true,
    reasoning: true,
    source: "airllm",
    variants: [
      {
        size: "72B (fp16 AirLLM)",
        params: 72,
        tag: "airllm:Qwen/Qwen2.5-72B-Instruct",
        quants: ["airllm"],
        storageGb: 145,
        instruct: true,
      },
    ],
  },
  {
    id: "mistral-nemo-airllm",
    name: "Mistral NeMo 24B (AirLLM)",
    brand: "Mistral AI / AirLLM",
    description:
      "Modèle compact et rapide 24B pour le code et le chat, ultra-fluide avec AirLLM sur GPU 2-4 Go.",
      descriptionEn: "Compact, fast 24B model for code and chat, ultra-smooth with AirLLM on 2-4GB GPUs.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-07",
    releaseYear: 2024,
    instruct: true,
    code: true,
    source: "airllm",
    variants: [
      {
        size: "24B (fp16 AirLLM)",
        params: 24,
        tag: "airllm:mistralai/Mistral-Nemo-Instruct-2407",
        quants: ["airllm"],
        storageGb: 27,
        instruct: true,
      },
    ],
  },
  {
    id: "mixtral-8x7b-airllm",
    name: "Mixtral 8x7B Instruct (AirLLM)",
    brand: "Mistral AI / AirLLM",
    description:
      "Architecture MoE 47B paramètres (8 experts) tournant couche par couche sur 4 Go de VRAM.",
      descriptionEn: "47B-parameter MoE architecture (8 experts) running layer by layer on 4GB of VRAM.",
    license: "Apache-2.0",
    contextWindow: "32k",
    releaseDate: "2024-01",
    releaseYear: 2024,
    instruct: true,
    source: "airllm",
    variants: [
      {
        size: "8x7B MoE (AirLLM)",
        params: 47,
        tag: "airllm:mistralai/Mixtral-8x7B-Instruct-v0.1",
        quants: ["airllm"],
        storageGb: 90,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen2.5-coder-32b-airllm",
    name: "Qwen 2.5 Coder 32B (AirLLM)",
    brand: "Alibaba / Qwen / AirLLM",
    description: "Le modèle de code 32B spécialisé pour le développement logiciel long-horizon.",
    descriptionEn: "The 32B code model specialised for long-horizon software development.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-11",
    releaseYear: 2024,
    code: true,
    instruct: true,
    source: "airllm",
    variants: [
      {
        size: "32B (AirLLM)",
        params: 32,
        tag: "airllm:Qwen/Qwen2.5-Coder-32B-Instruct",
        quants: ["airllm"],
        storageGb: 65,
        instruct: true,
      },
    ],
  },
];

// ── Large local models for professional GPUs ────────────────────────────────
// These single-file GGUFs are meant for workstations with big GPUs/VRAM
// (Blackwell, multi-GPU, etc.) and are always offered as local downloads.

const LARGE_LOCAL_MODELS: ModelFamily[] = catalogueSocle.largeLocal as ModelFamily[];

// ── Cache helpers ───────────────────────────────────────────────────────────

interface CacheEntry {
  timestamp: number;
  families: ModelFamily[];
}

function loadCache(): CacheEntry | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const entry: CacheEntry = JSON.parse(raw);
    if (Date.now() - entry.timestamp > CACHE_TTL_MS) {
      localStorage.removeItem(CACHE_KEY);
      return null;
    }
    return entry;
  } catch {
    return null;
  }
}

function saveCache(families: ModelFamily[]): void {
  try {
    const entry: CacheEntry = { timestamp: Date.now(), families };
    localStorage.setItem(CACHE_KEY, JSON.stringify(entry));
  } catch {
    // localStorage full or unavailable — ignore
  }
}

function guessBrand(name: string): string {
  const n = name.toLowerCase();
  if (n.startsWith("gemma") || n.startsWith("codegemma")) return "Google";
  if (n.startsWith("qwen") || n.startsWith("qwq")) return "Alibaba / Qwen";
  if (n.startsWith("deepseek")) return "DeepSeek";
  if (n.startsWith("llama")) return "Meta";
  if (n.startsWith("mistral") || n.startsWith("codestral") || n.startsWith("mixtral"))
    return "Mistral AI";
  if (n.startsWith("phi")) return "Microsoft";
  if (n.startsWith("glm") || n.startsWith("chatglm")) return "Zhipu AI";
  if (n.startsWith("nemotron")) return "NVIDIA";
  if (n.startsWith("kimi")) return "Moonshot AI";
  if (n.startsWith("starcoder")) return "Hugging Face";
  if (n.startsWith("yi")) return "01.AI";
  if (n.startsWith("internlm")) return "Shanghai AI Lab";
  if (n.startsWith("command")) return "Cohere";
  if (n.startsWith("aya")) return "Cohere";
  if (n.startsWith("vicuna") || n.startsWith("tinyllama")) return "Community";
  return "Community";
}

// ── HuggingFace Hub API ─────────────────────────────────────────────────────

export async function fetchHuggingFaceModels(query = "gguf"): Promise<ModelFamily[]> {
  try {
    const urls: string[] = [];
    const qLower = query.trim().toLowerCase();

    if (!query || qLower === "gguf" || qLower === "all") {
      // Parallel queries across major open families, trending, and latest releases
      urls.push(
        "https://huggingface.co/api/models?search=qwen&filter=gguf&sort=downloads&direction=-1&limit=40",
        "https://huggingface.co/api/models?search=deepseek&filter=gguf&sort=downloads&direction=-1&limit=30",
        "https://huggingface.co/api/models?search=llama&filter=gguf&sort=downloads&direction=-1&limit=30",
        "https://huggingface.co/api/models?search=mistral&filter=gguf&sort=downloads&direction=-1&limit=30",
        "https://huggingface.co/api/models?search=gemma&filter=gguf&sort=downloads&direction=-1&limit=20",
        "https://huggingface.co/api/models?filter=gguf&sort=trending&direction=-1&limit=40",
        "https://huggingface.co/api/models?filter=gguf&sort=lastModified&direction=-1&limit=40",
        "https://huggingface.co/api/models?filter=gguf&sort=downloads&direction=-1&limit=40",
      );
    } else {
      urls.push(
        `https://huggingface.co/api/models?search=${encodeURIComponent(query)}&filter=gguf&sort=downloads&direction=-1&limit=40`,
        `https://huggingface.co/api/models?search=${encodeURIComponent(query)}&filter=gguf&sort=trending&direction=-1&limit=40`,
        `https://huggingface.co/api/models?search=${encodeURIComponent(query)}&filter=gguf&sort=lastModified&direction=-1&limit=40`,
      );
    }

    // `gated` n'est renvoyé que s'il est demandé, et `expand` remplace alors
    // toute la liste des champs par défaut : sans lui, le filtre des dépôts
    // verrouillés ne voyait jamais rien, et ces modèles échouaient à
    // l'installation faute de jeton.
    const champs = [
      "gated",
      "downloads",
      "likes",
      "lastModified",
      "createdAt",
      "tags",
      "pipeline_tag",
    ]
      .map((c) => `&expand%5B%5D=${c}`)
      .join("");
    const responses = await Promise.allSettled(
      urls.map((u) =>
        fetch(`${u}${champs}`)
          .then((r) => (r.ok ? r.json() : []))
          .catch(() => []),
      ),
    );

    const rawItems: Array<{
      id: string;
      downloads?: number;
      likes?: number;
      lastModified?: string;
      createdAt?: string;
      tags?: string[];
      pipeline_tag?: string;
      gated?: string | boolean;
    }> = [];

    const seenIds = new Set<string>();
    for (const r of responses) {
      if (r.status === "fulfilled" && Array.isArray(r.value)) {
        for (const item of r.value) {
          if (item?.id && !seenIds.has(item.id)) {
            seenIds.add(item.id);
            rawItems.push(item);
          }
        }
      }
    }

    const familyMap: Record<string, ModelFamily> = {};

    for (const item of rawItems) {
      // `false`, ou le mode d'accès : "auto", "manual".
      if (item.gated && item.gated !== "false") continue;
      const parts = item.id.split("/");
      const author = parts[0] || "HuggingFace";
      const repoName = parts[1] || item.id;
      const fullText =
        `${item.id} ${(item.tags || []).join(" ")} ${item.pipeline_tag || ""}`.toLowerCase();

      let sizeLabel = "GGUF";
      let paramsNum = 7;
      const paramMatch = repoName.match(/(\d+(?:\.\d+)?)\s*[bB]\b/);
      if (paramMatch) {
        paramsNum = Number.parseFloat(paramMatch[1]);
        sizeLabel = `${paramsNum}B`;
      }

      const isInstruct = /instruct|chat|conversational/i.test(fullText);
      const isVision = /vision|vl|multimodal|image-text-to-text/i.test(fullText);
      const isAudio = /audio|voice|speech/i.test(fullText);
      const isCode = /code|coder/i.test(fullText);
      const isReasoning = /reason|thinking|r1|qwq/i.test(fullText);
      const isUncensored = /uncensored|abliterated|heretic|decensored/i.test(fullText);

      const guessed = guessBrand(repoName);
      const brand = guessed !== "Community" ? `${guessed} / ${author}` : author;
      const familyId = `hf-${author}-${repoName.toLowerCase()}`;
      const dateRaw = item.createdAt || item.lastModified;
      const yearMatch = dateRaw ? new Date(dateRaw).getFullYear() : 2026;
      const dateStr = dateRaw ? dateRaw.slice(0, 7) : "2026-08";
      // La popularite ne merite une phrase que si elle en a une. Les depots
      // frais remontent a zero telechargement : « (0 telechargements) » dans
      // le tiroir de detail lit comme une erreur plutot que comme une donnee.
      const social = item.downloads || item.likes || 0;

      if (!familyMap[familyId]) {
        familyMap[familyId] = {
          id: familyId,
          name: repoName.replace(/-GGUF$/i, ""),
          brand,
          description:
            social > 0
              ? `Poids GGUF publiés sur HuggingFace par ${author}, ${social.toLocaleString("fr-FR")} téléchargements.`
              : `Poids GGUF publiés sur HuggingFace par ${author}.`,
          license: "Open Weights",
          contextWindow: "128k",
          releaseDate: dateStr,
          releaseYear: yearMatch,
          finetunable: true,
          instruct: isInstruct || !isVision,
          vision: isVision,
          audio: isAudio,
          code: isCode,
          reasoning: isReasoning,
          uncensored: isUncensored,
          variants: [
            {
              size: sizeLabel + (isInstruct ? " Instruct" : ""),
              params: paramsNum,
              tag: `hf.co/${item.id}`,
              quants: ["q4_K_M", "q5_K_M", "q8_0"],
              storageGb: Math.round(paramsNum * 0.65 * 10) / 10 || 4.5,
              instruct: isInstruct,
            },
          ],
          pulls: item.downloads || 0,
          source: "huggingface",
        };
      }
    }

    return Object.values(familyMap);
  } catch {
    return [];
  }
}

export interface RegistryResult {
  families: ModelFamily[];
  ttsModels: ModelFamily[];
  brands: string[];
  loading: boolean;
  lastFetched: number;
}

function mergeFamilies(...sources: (ModelFamily[] | ModelFamily)[]): ModelFamily[] {
  const map = new Map<string, ModelFamily>();
  for (const s of sources) {
    const list = Array.isArray(s) ? s : [s];
    for (const f of list) {
      if (f && !map.has(f.id)) {
        map.set(f.id, f);
      }
    }
  }
  return Array.from(map.values());
}

export async function fetchFullRegistry(): Promise<RegistryResult> {
  // 1. Check cache
  const cached = loadCache();
  if (cached) {
    const all = mergeFamilies(
      SEED_CATALOG,
      LARGE_LOCAL_MODELS,
      AIRLLM_CATALOG_MODELS,
      cached.families,
    );
    const brands = Array.from(new Set(all.map((f) => f.brand))).sort();
    return {
      families: all,
      ttsModels: [],
      brands,
      loading: false,
      lastFetched: cached.timestamp,
    };
  }

  // 2. Une seule source distante : HuggingFace. La bibliotheque Ollama etait
  //    interrogee ici et ne rendait jamais rien — sa commande renvoyait une
  //    liste vide, faute d'implementation.
  const hfModels = await fetchHuggingFaceModels("gguf").catch(() => [] as ModelFamily[]);

  // 3. Toutes les sources : socle + gros modeles locaux + airllm + HF (GGUF)
  const allFamilies = mergeFamilies(
    SEED_CATALOG,
    LARGE_LOCAL_MODELS,
    AIRLLM_CATALOG_MODELS,
    hfModels,
  );

  // 5. Cache
  saveCache(allFamilies);

  const brands = Array.from(new Set(allFamilies.map((f) => f.brand))).sort();

  return {
    families: allFamilies,
    ttsModels: [],
    brands,
    loading: false,
    lastFetched: Date.now(),
  };
}

export function clearRegistryCache(): void {
  localStorage.removeItem(CACHE_KEY);
}

// ── Re-export for backward compat ───────────────────────────────────────────

/** @deprecated Use fetchFullRegistry() instead */
export const MODEL_CATALOG = [...SEED_CATALOG, ...LARGE_LOCAL_MODELS, ...AIRLLM_CATALOG_MODELS];
export const BRANDS = Array.from(new Set(MODEL_CATALOG.map((f) => f.brand))).sort();
