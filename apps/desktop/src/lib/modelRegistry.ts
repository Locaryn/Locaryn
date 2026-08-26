// ═══════════════════════════════════════════════════════════════════════════════
// modelRegistry.ts — Dynamic model discovery (Ollama API + HuggingFace + seed)
// No more manual maintenance. Models are fetched at runtime from:
//   1. Ollama local API  (/api/tags)          → installed models
//   2. Ollama Library    (via Rust IPC proxy)  → all available models
//   3. HuggingFace Hub   (/api/models?gguf)    → community GGUF models
//   4. SEED_CATALOG      (hardcoded fallback)   → offline minimum
// ═══════════════════════════════════════════════════════════════════════════════

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

/** A Marketplace filter. Extensions append entries using `marketplace.catalogs`. */
export interface ModelCategoryDefinition {
  id: ModelCategory;
  label: string;
  icon: string;
  /** A family is included when it declares at least one matching capability. */
  matches: string[];
  /** Optional runtime capabilities required before the filter is visible. */
  requires?: string[];
}

export const MODEL_CATEGORIES: ModelCategoryDefinition[] = [
  { id: "all", label: "Tous", icon: "models", matches: [] },
  { id: "chat", label: "Chat / Instruct", icon: "chat", matches: ["chat"] },
  { id: "code", label: "Code", icon: "cpu", matches: ["code"] },
  { id: "vision", label: "Vision", icon: "image", matches: ["vision"] },
  { id: "reasoning", label: "Raisonnement", icon: "memory", matches: ["reasoning"] },
  {
    id: "speech-synthesis",
    label: "Synthèse vocale",
    icon: "mic",
    matches: ["voice-tts"],
    requires: ["voice-tts", "voice-cloning"],
  },
  {
    id: "video-generation",
    label: "Vidéo",
    icon: "video",
    matches: ["video-gen"],
    requires: ["video-gen"],
  },
  {
    id: "language-translation",
    label: "Traduction",
    icon: "translate",
    matches: ["translation"],
    requires: ["translation"],
  },
  {
    id: "3d-modeling",
    label: "3D",
    icon: "extensions",
    matches: ["3d-gen"],
    requires: ["3d-gen"],
  },
  {
    id: "music-generation",
    label: "Musique",
    icon: "music",
    matches: ["music-gen"],
    requires: ["music-gen"],
  },
  {
    id: "object-detection",
    label: "Détection",
    icon: "target",
    matches: ["vision-ocr"],
    requires: ["vision-ocr"],
  },
  {
    id: "text-analysis",
    label: "Analyse texte",
    icon: "chart",
    matches: ["text-analysis"],
    requires: ["text-analysis"],
  },
  {
    id: "question-answering",
    label: "Q&R",
    icon: "question",
    matches: ["rag-qa"],
    requires: ["rag-qa"],
  },
];

// ── Constants ───────────────────────────────────────────────────────────────

const CACHE_KEY = "locaryn_model_registry_cache_v18";
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

export const SEED_CATALOG: ModelFamily[] = [
  // Google
  {
    id: "gemma4",
    name: "Gemma 4",
    brand: "Google",
    description: "Multimodal encoder-free (texte, image, audio) avec function calling natif.",
    license: "Apache-2.0",
    contextWindow: "256k",
    releaseDate: "2026-04",
    releaseYear: 2026,
    vision: true,
    audio: true,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "E2B",
        params: 2,
        tag: "gemma4:e2b",
        quants: QUANTS_SMALL,
        storageGb: 1.6,
        instruct: true,
      },
      {
        size: "E4B",
        params: 4,
        tag: "gemma4:e4b",
        quants: QUANTS_SMALL,
        storageGb: 2.8,
        instruct: true,
      },
      {
        size: "12B",
        params: 12,
        tag: "gemma4:12b",
        quants: QUANTS_BIG,
        storageGb: 7.6,
        instruct: true,
      },
      {
        size: "26B MoE",
        params: 26,
        tag: "gemma4:26b",
        quants: QUANTS_BIG,
        storageGb: 16.0,
        instruct: true,
      },
      {
        size: "31B Dense",
        params: 31,
        tag: "gemma4:31b",
        quants: QUANTS_BIG,
        storageGb: 19.0,
        instruct: true,
      },
    ],
  },
  {
    id: "gemma2",
    name: "Gemma 2",
    brand: "Google",
    description: "Modèles légers ultra-performants (2B, 9B, 27B).",
    license: "Gemma Terms",
    contextWindow: "8k",
    releaseDate: "2024-06",
    releaseYear: 2024,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "2B",
        params: 2,
        tag: "gemma2:2b",
        quants: QUANTS_SMALL,
        storageGb: 1.6,
        instruct: true,
      },
      {
        size: "9B",
        params: 9,
        tag: "gemma2:9b",
        quants: QUANTS_SMALL,
        storageGb: 5.4,
        instruct: true,
      },
      {
        size: "27B",
        params: 27,
        tag: "gemma2:27b",
        quants: QUANTS_BIG,
        storageGb: 16.0,
        instruct: true,
      },
    ],
  },
  // Qwen
  {
    id: "qwen3",
    name: "Qwen3",
    brand: "Alibaba / Qwen",
    description: "Flagship multilingue (119+ langues), mode thinking hybride, dense et MoE.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2025-04",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "0.6B",
        params: 0.6,
        tag: "qwen3:0.6b",
        quants: QUANTS_SMALL,
        storageGb: 0.5,
        instruct: true,
      },
      {
        size: "1.7B",
        params: 1.7,
        tag: "qwen3:1.7b",
        quants: QUANTS_SMALL,
        storageGb: 1.1,
        instruct: true,
      },
      {
        size: "4B",
        params: 4,
        tag: "qwen3:4b",
        quants: QUANTS_SMALL,
        storageGb: 2.6,
        instruct: true,
      },
      {
        size: "8B",
        params: 8,
        tag: "qwen3:8b",
        quants: QUANTS_SMALL,
        storageGb: 4.9,
        instruct: true,
      },
      {
        size: "14B",
        params: 14,
        tag: "qwen3:14b",
        quants: QUANTS_BIG,
        storageGb: 9.0,
        instruct: true,
      },
      {
        size: "30B MoE",
        params: 30,
        tag: "qwen3:30b-a3b",
        quants: QUANTS_BIG,
        storageGb: 19.0,
        instruct: true,
      },
      {
        size: "32B",
        params: 32,
        tag: "qwen3:32b",
        quants: QUANTS_BIG,
        storageGb: 20.0,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen3-coder",
    name: "Qwen3-Coder",
    brand: "Alibaba / Qwen",
    description: "MoE agentic-coding 256k contexte.",
    license: "Apache-2.0",
    contextWindow: "256k",
    releaseDate: "2025-07",
    releaseYear: 2025,
    code: true,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "30B MoE",
        params: 30,
        tag: "qwen3-coder:30b",
        quants: QUANTS_BIG,
        storageGb: 19.0,
        instruct: true,
      },
    ],
  },
  {
    id: "qwq",
    name: "QwQ",
    brand: "Alibaba / Qwen",
    description: "Raisonnement profond : maths, logique, problèmes complexes.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2025-03",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "32B",
        params: 32,
        tag: "qwq:32b",
        quants: QUANTS_BIG,
        storageGb: 20.0,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen2.5-coder",
    name: "Qwen2.5 Coder",
    brand: "Alibaba / Qwen",
    description: "Spécialiste code et refactoring (0.5B à 32B).",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    code: true,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "7B",
        params: 7,
        tag: "qwen2.5-coder:7b",
        quants: QUANTS_SMALL,
        storageGb: 4.7,
        instruct: true,
      },
      {
        size: "14B",
        params: 14,
        tag: "qwen2.5-coder:14b",
        quants: QUANTS_BIG,
        storageGb: 9.0,
        instruct: true,
      },
      {
        size: "32B",
        params: 32,
        tag: "qwen2.5-coder:32b",
        quants: QUANTS_BIG,
        storageGb: 20.0,
        instruct: true,
      },
    ],
  },
  // DeepSeek
  {
    id: "deepseek-r1",
    name: "DeepSeek-R1",
    brand: "DeepSeek",
    description: "Raisonnement étape par étape (1.5B à 671B).",
    license: "MIT",
    contextWindow: "128k",
    releaseDate: "2025-01",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "1.5B",
        params: 1.5,
        tag: "deepseek-r1:1.5b",
        quants: QUANTS_SMALL,
        storageGb: 1.1,
        instruct: true,
      },
      {
        size: "7B",
        params: 7,
        tag: "deepseek-r1:7b",
        quants: QUANTS_SMALL,
        storageGb: 4.7,
        instruct: true,
      },
      {
        size: "8B",
        params: 8,
        tag: "deepseek-r1:8b",
        quants: QUANTS_SMALL,
        storageGb: 4.9,
        instruct: true,
      },
      {
        size: "14B",
        params: 14,
        tag: "deepseek-r1:14b",
        quants: QUANTS_BIG,
        storageGb: 9.0,
        instruct: true,
      },
      {
        size: "32B",
        params: 32,
        tag: "deepseek-r1:32b",
        quants: QUANTS_BIG,
        storageGb: 20.0,
        instruct: true,
      },
      {
        size: "70B",
        params: 70,
        tag: "deepseek-r1:70b",
        quants: QUANTS_BIG,
        storageGb: 42.0,
        instruct: true,
      },
    ],
  },
  {
    id: "deepseek-coder-v2",
    name: "DeepSeek-Coder-V2",
    brand: "DeepSeek",
    description: "MoE code (16B Lite & 236B Full).",
    license: "DeepSeek License",
    contextWindow: "128k",
    releaseDate: "2024-06",
    releaseYear: 2024,
    code: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "16B Lite",
        params: 16,
        tag: "deepseek-coder-v2:16b",
        quants: QUANTS_BIG,
        storageGb: 9.7,
        instruct: true,
      },
    ],
  },
  // Meta
  {
    id: "llama4",
    name: "Llama 4",
    brand: "Meta",
    description: "MoE multimodal (Scout 109B, Maverick 400B).",
    license: "Llama 4 Community",
    contextWindow: "10M",
    releaseDate: "2025-04",
    releaseYear: 2025,
    vision: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "Scout 109B MoE",
        params: 109,
        tag: "llama4:scout",
        quants: QUANTS_BIG,
        storageGb: 64.0,
        instruct: true,
      },
    ],
  },
  {
    id: "llama3.2",
    name: "Llama 3.2",
    brand: "Meta",
    description: "Ultra-rapides et légers (1B, 3B).",
    license: "Llama 3.2 Community",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "1B",
        params: 1,
        tag: "llama3.2:1b",
        quants: QUANTS_SMALL,
        storageGb: 1.3,
        instruct: true,
      },
      {
        size: "3B",
        params: 3,
        tag: "llama3.2:3b",
        quants: QUANTS_SMALL,
        storageGb: 2.0,
        instruct: true,
      },
    ],
  },
  // Mistral
  {
    id: "mistral",
    name: "Mistral 7B",
    brand: "Mistral AI",
    description: "Modèle 7B iconique.",
    license: "Apache-2.0",
    contextWindow: "32k",
    releaseDate: "2024-03",
    releaseYear: 2024,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "7B",
        params: 7,
        tag: "mistral:7b",
        quants: QUANTS_SMALL,
        storageGb: 4.1,
        instruct: true,
      },
    ],
  },
  {
    id: "codestral",
    name: "Codestral",
    brand: "Mistral AI",
    description: "22B pour 80+ langages.",
    license: "MNPL",
    contextWindow: "32k",
    releaseDate: "2024-05",
    releaseYear: 2024,
    code: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "22B",
        params: 22,
        tag: "codestral:22b",
        quants: QUANTS_BIG,
        storageGb: 13.0,
        instruct: true,
      },
    ],
  },
  // Microsoft
  {
    id: "phi4",
    name: "Phi-4",
    brand: "Microsoft",
    description: "14B raisonnement exceptionnel.",
    license: "MIT",
    contextWindow: "16k",
    releaseDate: "2024-12",
    releaseYear: 2024,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "14B",
        params: 14,
        tag: "phi4:14b",
        quants: QUANTS_BIG,
        storageGb: 9.1,
        instruct: true,
      },
    ],
  },
  {
    id: "phi4-mini",
    name: "Phi-4 Mini",
    brand: "Microsoft",
    description: "SLM 3.8B edge-ready avec function calling.",
    license: "MIT",
    contextWindow: "16k",
    releaseDate: "2025-02",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "3.8B",
        params: 3.8,
        tag: "phi4-mini",
        quants: QUANTS_SMALL,
        storageGb: 2.4,
        instruct: true,
      },
    ],
  },
  // GLM
  {
    id: "glm4",
    name: "GLM-4",
    brand: "Zhipu AI",
    description: "9B conversationnel chinois/anglais 128k.",
    license: "GLM-4 License",
    contextWindow: "128k",
    releaseDate: "2024-06",
    releaseYear: 2024,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "9B",
        params: 9,
        tag: "glm4:9b",
        quants: QUANTS_SMALL,
        storageGb: 5.5,
        instruct: true,
      },
    ],
  },
  // Thinking Machines
  {
    id: "inkling",
    name: "Inkling",
    brand: "Thinking Machines",
    description: "Modèle multimodal MoE de pointe (Inkling Small 12B).",
    license: "Open Weights",
    contextWindow: "128k",
    releaseDate: "2026-07",
    releaseYear: 2026,
    vision: true,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "12B Small",
        params: 12,
        tag: "inkling:12b",
        quants: QUANTS_BIG,
        storageGb: 7.6,
        instruct: true,
      },
    ],
  },
  // Xiaomi / MiMo
  {
    id: "mimo",
    name: "MiMo V2.5",
    brand: "Xiaomi AI / MiMo",
    description: "Famille MoE ultra-rapide pour raisonnement et agents autonomes.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2026-05",
    releaseYear: 2026,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "7B",
        params: 7,
        tag: "mimo:7b",
        quants: QUANTS_SMALL,
        storageGb: 4.5,
        instruct: true,
      },
      {
        size: "12B Flash",
        params: 12,
        tag: "mimo-v2.5:12b",
        quants: QUANTS_BIG,
        storageGb: 7.2,
        instruct: true,
      },
    ],
  },
  // NVIDIA
  {
    id: "nemotron-nano",
    name: "Nemotron-3 Nano",
    brand: "NVIDIA",
    description: "Hybrid MoE 30B (3.6B actifs) pour workflows agentiques 1M tokens.",
    license: "NVIDIA Open Model",
    contextWindow: "1M",
    releaseDate: "2025-12",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "30B MoE",
        params: 30,
        tag: "nemotron-nano",
        quants: QUANTS_BIG,
        storageGb: 19.0,
        instruct: true,
      },
    ],
  },
];

// ── Curated AirLLM-ready large models (layer-by-layer low VRAM inference) ──

export const AIRLLM_CATALOG_MODELS: ModelFamily[] = [
  {
    id: "deepseek-r1-70b-airllm",
    name: "DeepSeek-R1 Distill Llama 70B (AirLLM)",
    brand: "DeepSeek / AirLLM",
    description:
      "Modèle de raisonnement 70B exécutable sur petit GPU (4 Go VRAM) grâce au chargement couche par couche AirLLM.",
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
        tag: "airllm:meta-llama/Llama-3.3-70B-Instruct",
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
    id: "command-r-airllm",
    name: "Command R 35B (AirLLM)",
    brand: "Cohere / AirLLM",
    description:
      "Modèle conversationnel et RAG 35B de Cohere exécutable via AirLLM en streaming de couches.",
    license: "CC-BY-NC-4.0",
    contextWindow: "128k",
    releaseDate: "2024-03",
    releaseYear: 2024,
    instruct: true,
    source: "airllm",
    variants: [
      {
        size: "35B (AirLLM)",
        params: 35,
        tag: "airllm:CohereForAI/c4ai-command-r-v01",
        quants: ["airllm"],
        storageGb: 70,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen2.5-coder-32b-airllm",
    name: "Qwen 2.5 Coder 32B (AirLLM)",
    brand: "Alibaba / Qwen / AirLLM",
    description: "Le modèle de code 32B spécialisé pour le développement logiciel long-horizon.",
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

const LARGE_LOCAL_MODELS: ModelFamily[] = [
  {
    id: "qwen3-gguf",
    name: "Qwen3 (GGUF local)",
    brand: "Alibaba / Qwen / ggml-org",
    description:
      "Versions GGUF vérifiées de Qwen3 pour llama.cpp. Le 4B privilégie la vitesse, le 8B est le meilleur compromis sur un GPU 6 Go et le 14B utilise aussi la RAM pour gagner en qualité.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2025-04",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "4B · rapide",
        params: 4,
        tag: "https://huggingface.co/Qwen/Qwen3-4B-GGUF",
        quants: ["Q4_K_M", "Q5_K_M", "Q8_0"],
        storageGb: 2.33,
        instruct: true,
      },
      {
        size: "8B · recommandé",
        params: 8,
        tag: "https://huggingface.co/Qwen/Qwen3-8B-GGUF",
        quants: ["Q4_K_M", "Q5_K_M", "Q8_0"],
        storageGb: 4.68,
        instruct: true,
      },
      {
        size: "14B · qualité",
        params: 14,
        tag: "https://huggingface.co/ggml-org/Qwen3-14B-GGUF",
        quants: ["Q4_K_M", "Q8_0", "F16"],
        storageGb: 8.38,
        instruct: true,
      },
    ],
  },
  {
    id: "gemma3-gguf",
    name: "Gemma 3 Vision (GGUF local)",
    brand: "Google / ggml-org",
    description:
      "Modèles multimodaux GGUF avec projecteur vision installé automatiquement. Le 4B tient confortablement sur une configuration 6 Go, le 12B fonctionne en mode hybride RAM + GPU.",
    license: "Gemma",
    contextWindow: "128k",
    releaseDate: "2025-03",
    releaseYear: 2025,
    vision: true,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "4B · vision rapide",
        params: 4,
        tag: "https://huggingface.co/ggml-org/gemma-3-4b-it-GGUF",
        quants: ["Q4_K_M", "Q8_0", "F16"],
        storageGb: 3.11,
        instruct: true,
      },
      {
        size: "12B · vision qualité",
        params: 12,
        tag: "https://huggingface.co/ggml-org/gemma-3-12b-it-GGUF",
        quants: ["Q4_K_M", "Q8_0", "F16"],
        storageGb: 7.6,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen3.8-27b-gguf",
    name: "Qwen3.8 27B (GGUF + Vision)",
    brand: "Alibaba / Qwen / ggml-org",
    description:
      "Qwen3.8 27B multimodal et raisonnant, proposé dans le format GGUF compatible avec le moteur local llama.cpp. Le projecteur vision adapté est installé automatiquement.",
    license: "Apache-2.0",
    contextWindow: "262k",
    releaseDate: "2026-08",
    releaseYear: 2026,
    vision: true,
    reasoning: true,
    instruct: true,
    finetunable: true,
    source: "seed",
    variants: [
      {
        size: "27B",
        params: 27,
        tag: "https://huggingface.co/ggml-org/Qwen3.8-27B-GGUF",
        quants: ["Q4_K_M", "Q8_0", "BF16"],
        storageGb: 18.3,
        instruct: true,
      },
    ],
  },
  {
    id: "deepseek-r1-70b-gguf",
    name: "DeepSeek-R1 Distill Llama 70B (GGUF)",
    brand: "DeepSeek / bartowski",
    description:
      "Raisonnement étape par étape dans un distillat Llama 70B prêt pour GPU haut de gamme. Format GGUF local.",
    license: "MIT",
    contextWindow: "128k",
    releaseDate: "2025-01",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    source: "seed",
    variants: [
      {
        size: "70B",
        params: 70,
        tag: "https://huggingface.co/bartowski/DeepSeek-R1-Distill-Llama-70B-GGUF/resolve/main/DeepSeek-R1-Distill-Llama-70B-Q4_K_M.gguf",
        quants: ["Q4_K_M"],
        storageGb: 40.0,
      },
    ],
  },
  {
    id: "qwen2.5-72b-gguf",
    name: "Qwen2.5 72B Instruct (GGUF)",
    brand: "Alibaba / Qwen / bartowski",
    description:
      "Le plus grand modèle de la famille Qwen2.5 Instruct, optimisé pour le dialogue et le travail long en GGUF local.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    instruct: true,
    reasoning: true,
    source: "seed",
    variants: [
      {
        size: "72B",
        params: 72,
        tag: "https://huggingface.co/bartowski/Qwen2.5-72B-Instruct-GGUF/resolve/main/Qwen2.5-72B-Instruct-Q4_K_M.gguf",
        quants: ["Q4_K_M"],
        storageGb: 42.0,
      },
    ],
  },
];

// ── Ollama Library Model (from Rust IPC search) ─────────────────────────────

export interface OllamaLibraryModel {
  name: string;
  description: string;
  tags: string[];
  pulls: number;
  updated: string;
}

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

// ── Parse Ollama /api/tags response into installed tags ──────────────────────

export interface OllamaLocalModel {
  name: string;
  model: string;
  size: number;
  digest: string;
  modified_at: string;
  details?: {
    parent_model?: string;
    format?: string;
    family?: string;
    families?: string[];
    parameter_size?: string;
    quantization_level?: string;
  };
}

export async function fetchInstalledModels(endpoint: string): Promise<OllamaLocalModel[]> {
  try {
    const url = `${endpoint.replace(/\/+$/, "")}/api/tags`;
    const resp = await fetch(url);
    if (!resp.ok) return [];
    const body = await resp.json();
    return (body.models || []) as OllamaLocalModel[];
  } catch {
    return [];
  }
}

// ── Parse Ollama library search results into ModelFamilies ──────────────────

function ollamaLibraryToFamilies(models: OllamaLibraryModel[]): ModelFamily[] {
  const families: ModelFamily[] = [];

  for (const m of models) {
    const tags = m.tags || [];
    if (tags.length === 0) continue;

    const isVision = /vision|vl|multimodal/i.test(`${m.name} ${m.description}`);
    const isCode = /code|coder/i.test(`${m.name} ${m.description}`);
    const isReasoning = /reason|thinking|r1|qwq/i.test(`${m.name} ${m.description}`);
    const isTTS = /tts|text.to.speech|synthes|voice|speech|piper|xtts|melotts/i.test(
      `${m.name} ${m.description}`,
    );
    const isAudio = isTTS || /audio|voice|whisper|omni/i.test(`${m.name} ${m.description}`);
    const isVideoGen = /video.*gen|text.to.video|video.diffusion|wan2.1|sora|ltx/i.test(
      `${m.name} ${m.description}`,
    );
    const isMusicGen = /music.*gen|audio.*gen|sound.gen|musicgen|audiogen/i.test(
      `${m.name} ${m.description}`,
    );
    const is3D = /3d.*model|mesh|shap|threestudio/i.test(`${m.name} ${m.description}`);
    const isTranslation = /translation|translate|nllb|m2m|opus/i.test(`${m.name} ${m.description}`);
    const isObjectDetection = /object.*detect|detection|yolo/i.test(`${m.name} ${m.description}`);
    const isTextAnalysis = /sentiment|classification|analysis|ner|embed|semantic/i.test(
      `${m.name} ${m.description}`,
    );
    const isQuestionAnswering = /question.*answer|qa|extractive.qa/i.test(
      `${m.name} ${m.description}`,
    );
    const isInstruct =
      /instruct|chat/i.test(`${m.name} ${m.description}`) ||
      (!isTTS && !isVideoGen && !isMusicGen && !is3D);

    const variants: ModelVariant[] = tags
      .filter((t) => !t.toLowerCase().includes("cloud"))
      .map((t) => {
        const paramMatch = t.match(/(\d+\.?\d*)[bB]/);
        const params = paramMatch ? Number.parseFloat(paramMatch[1]) : 7;
        const sizeLabel = paramMatch ? `${params}B` : t.split(":")[1] || "default";
        return {
          size: sizeLabel,
          params,
          tag: t.includes(":") ? t : `${m.name}:${t}`,
          quants: params > 14 ? QUANTS_BIG : QUANTS_SMALL,
          storageGb: Math.round(params * 0.65 * 10) / 10 || 4.0,
          instruct: isInstruct,
        };
      });

    // Deduplicate variants by tag
    const seen = new Set<string>();
    const dedupedVariants = variants.filter((v) => {
      if (seen.has(v.tag)) return false;
      seen.add(v.tag);
      return true;
    });

    const year = m.updated ? new Date(m.updated).getFullYear() : 2025;
    const dateStr = m.updated ? m.updated.slice(0, 7) : "2025-01";

    families.push({
      id: `ollama-${m.name}`,
      name: m.name,
      brand: guessBrand(m.name),
      description:
        m.description ||
        `Modèle ${m.name} disponible sur Ollama (${(m.pulls || 0).toLocaleString()} pulls).`,
      license: "Open Weights",
      contextWindow: "128k",
      releaseDate: dateStr,
      releaseYear: year,
      vision: isVision,
      audio: isAudio,
      code: isCode,
      reasoning: isReasoning,
      instruct: isInstruct,
      tts: isTTS,
      videoGen: isVideoGen,
      musicGen: isMusicGen,
      model3d: is3D,
      translation: isTranslation,
      objectDetection: isObjectDetection,
      textAnalysis: isTextAnalysis,
      questionAnswering: isQuestionAnswering,
      finetunable: true,
      variants: dedupedVariants,
      pulls: m.pulls || 0,
      source: "ollama",
    });
  }

  return families;
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

    const responses = await Promise.allSettled(
      urls.map((u) =>
        fetch(u)
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
      if (item.gated === "true" || item.gated === true) continue;
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

      if (!familyMap[familyId]) {
        familyMap[familyId] = {
          id: familyId,
          name: repoName.replace(/-GGUF$/i, ""),
          brand,
          description: `GGUF HuggingFace Hub (${(item.downloads || item.likes || 0).toLocaleString()} téléchargements / likes).`,
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

export async function fetchFullRegistry(
  searchOllamaLibrary?: (query: string, category?: string) => Promise<OllamaLibraryModel[]>,
): Promise<RegistryResult> {
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

  // 2. Fetch in parallel
  const [ollamaModels, hfModels] = await Promise.all([
    searchOllamaLibrary
      ? searchOllamaLibrary("", undefined).catch(() => [] as OllamaLibraryModel[])
      : Promise.resolve([] as OllamaLibraryModel[]),
    fetchHuggingFaceModels("gguf").catch(() => [] as ModelFamily[]),
  ]);

  // 3. Convert ollama library results
  const ollamaFamilies = ollamaLibraryToFamilies(ollamaModels);

  // 4. Merge all sources: seed + large local + airllm + ollama + HF (GGUF)
  const allFamilies = mergeFamilies(
    SEED_CATALOG,
    LARGE_LOCAL_MODELS,
    AIRLLM_CATALOG_MODELS,
    ollamaFamilies,
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
