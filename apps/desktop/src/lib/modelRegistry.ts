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

/** Resolve capabilities for a model by looking it up in the TTS registry.
 *  Falls back to keyword heuristics when the model is not in the registry. */
export function getModelCapabilities(modelName: string): TTSCapabilities {
  const lower = modelName.toLowerCase();

  // 1. Exact registry match
  for (const family of TTS_MODELS) {
    if (lower.includes(family.id.toLowerCase())) {
      return buildCaps(family);
    }
    for (const variant of family.variants) {
      if (lower.includes(variant.tag.toLowerCase())) {
        return buildCaps(family);
      }
      // Also match against the repo path extracted from the variant URL,
      // normalized to the installed directory name format (double underscore
      // replaces slash, e.g. "Qwen__Qwen3-TTS-12Hz-1.7B-CustomVoice").
      const tag = variant.tag.toLowerCase();
      const repoPath = tag
        .replace(/^https?:\/\/huggingface\.co\//, "")
        .split("/resolve/")[0]
        .replace(/\/+/g, "__");
      if (repoPath && repoPath.length > 5 && lower.includes(repoPath)) {
        return buildCaps(family);
      }
    }
  }

  // 2. Heuristic fallback — scan description keywords
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

  // 2. Check specialized registries (TTS, Image, Video, Music, 3D)
  const specializedRegistries = [
    ...IMAGE_GEN_MODELS.map((f) => ({ family: f, flag: "image-gen" as const })),
    ...TTS_MODELS.map((f) => ({ family: f, flag: "tts" as const })),
    ...MUSIC_MODELS.map((f) => ({ family: f, flag: "music-gen" as const })),
    ...VIDEO_MODELS.map((f) => ({ family: f, flag: "video-gen" as const })),
    ...MODEL3D_MODELS.map((f) => ({ family: f, flag: "3d-gen" as const })),
  ];

  for (const { family, flag } of specializedRegistries) {
    if (lower.includes(family.id.toLowerCase())) {
      return { kind: flag, family };
    }
    for (const variant of family.variants) {
      if (lower.includes(variant.tag.toLowerCase().replace(/^https:\/\/huggingface.co\//, ""))) {
        return { kind: flag, family };
      }
    }
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

export type ModelCategory =
  | "all"
  | "chat"
  | "code"
  | "vision"
  | "reasoning"
  | "image-gen"
  | "speech-synthesis"
  | "video-generation"
  | "language-translation"
  | "3d-modeling"
  | "music-generation"
  | "object-detection"
  | "text-analysis"
  | "image-editing"
  | "question-answering"
  | "audio";

export const MODEL_CATEGORIES: { id: ModelCategory; label: string; icon: string }[] = [
  { id: "all", label: "Tous", icon: "models" },
  { id: "chat", label: "Chat / Instruct", icon: "chat" },
  { id: "code", label: "Code", icon: "cpu" },
  { id: "vision", label: "Vision", icon: "image" },
  { id: "reasoning", label: "Raisonnement", icon: "memory" },
  { id: "image-gen", label: "Image Gen", icon: "image" },
  { id: "speech-synthesis", label: "Synthèse vocale", icon: "mic" },
  { id: "video-generation", label: "Vidéo", icon: "video" },
  { id: "language-translation", label: "Traduction", icon: "translate" },
  { id: "3d-modeling", label: "3D", icon: "extensions" },
  { id: "music-generation", label: "Musique", icon: "music" },
  { id: "object-detection", label: "Détection", icon: "target" },
  { id: "text-analysis", label: "Analyse texte", icon: "chart" },
  { id: "image-editing", label: "Édition image", icon: "edit" },
  { id: "question-answering", label: "Q&R", icon: "question" },
];

// ── Constants ───────────────────────────────────────────────────────────────

const CACHE_KEY = "locaryn_model_registry_cache_v17";
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

// ── Image Generation Models ─────────────────────────────────────────────────

export const IMAGE_GEN_MODELS: ModelFamily[] = [
  {
    id: "krea-2-turbo",
    name: "Krea 2 Turbo (GGUF)",
    brand: "Krea",
    description:
      "Modèle Krea 2 Turbo en GGUF (quantisations Q2 à Q8). Rendu photographique, peu d'étapes. " +
      "Nécessite un VAE et un encodeur de texte séparés — le dépôt GGUF ne contient que le modèle de diffusion.",
    license: "Voir la licence Krea",
    contextWindow: "N/A",
    releaseDate: "2025-10",
    releaseYear: 2025,
    imageGen: true,
    variants: [
      {
        size: "Q4_K_M",
        params: 12,
        tag: "https://huggingface.co/vantagewithai/Krea-2-Turbo-GGUF/resolve/main/krea2_turbo-Q4_K_M.gguf",
        quants: ["Q4_K_M"],
        storageGb: 7.5,
      },
      {
        size: "Q6_K",
        params: 12,
        tag: "https://huggingface.co/vantagewithai/Krea-2-Turbo-GGUF/resolve/main/krea2_turbo-Q6_K.gguf",
        quants: ["Q6_K"],
        storageGb: 10.6,
      },
      {
        size: "Q8_0",
        params: 12,
        tag: "https://huggingface.co/vantagewithai/Krea-2-Turbo-GGUF/resolve/main/krea2_turbo-Q8_0.gguf",
        quants: ["Q8_0"],
        storageGb: 13.7,
      },
    ],
    source: "seed",
  },
  {
    id: "z-image-turbo",
    name: "Z-Image Turbo (GGUF)",
    brand: "Z-Image / LeeJet",
    description:
      "Modèle ultra-rapide (1 à 4 steps) en haute fidélité visuelle. Optimisé pour GPU & CPU.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2024-11",
    releaseYear: 2024,
    imageGen: true,
    variants: [
      {
        size: "6B",
        params: 6,
        tag: "https://huggingface.co/leejet/Z-Image-Turbo-GGUF/resolve/main/z_image_turbo-Q8_0.gguf",
        quants: ["Q8_0"],
        storageGb: 6.5,
      },
    ],
    source: "seed",
  },
  {
    id: "z-image-turbo-heretic",
    name: "Z-Image Turbo — sans limite (Heretic)",
    brand: "Z-Image / Heretic",
    description:
      "Z-Image Turbo avec encodeur de texte abliteré (méthode heretic) : le modèle ne refuse plus de prompt. " +
      "L'installation met en place automatiquement le VAE et l'encodeur sans garde-fous. " +
      "Vous êtes responsable de ce que vous générez.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2026-01",
    releaseYear: 2026,
    imageGen: true,
    uncensored: true,
    variants: [
      {
        size: "6B",
        params: 6,
        tag: "https://huggingface.co/leejet/Z-Image-Turbo-GGUF/resolve/main/z_image_turbo-Q8_0.gguf",
        quants: ["Q8_0"],
        storageGb: 9.0,
      },
    ],
    source: "seed",
  },
  {
    id: "sd-1-5",
    name: "Stable Diffusion 1.5 (GGUF)",
    brand: "RunwayML / SecondState",
    description:
      "Modèle d'image léger (1.5 Go) ultra-rapide. Fonctionne parfaitement sur tout GPU/CPU.",
    license: "OpenRail",
    contextWindow: "N/A",
    releaseDate: "2024-05",
    releaseYear: 2024,
    imageGen: true,
    uncensored: true,
    variants: [
      {
        size: "1B",
        params: 1,
        tag: "https://huggingface.co/second-state/stable-diffusion-v1-5-GGUF/resolve/main/stable-diffusion-v1-5-pruned-emaonly-Q4_0.gguf",
        quants: ["Q4_0", "Q8_0", "f16"],
        storageGb: 1.5,
      },
    ],
    source: "seed",
  },
  {
    id: "sdxl-turbo",
    name: "SDXL Turbo (GGUF)",
    brand: "Stability AI / SecondState",
    description: "Modèle SDXL temps réel sub-seconde (1-step generation) haute qualité 1024x1024.",
    license: "OpenRail",
    contextWindow: "N/A",
    releaseDate: "2024-01",
    releaseYear: 2024,
    imageGen: true,
    variants: [
      {
        size: "7B",
        params: 7,
        tag: "https://huggingface.co/second-state/SDXL-Turbo-GGUF/resolve/main/sdxl-turbo-Q4_0.gguf",
        quants: ["Q4_0", "Q8_0"],
        storageGb: 3.1,
      },
    ],
    source: "seed",
  },
  {
    id: "sdxl-base-1-0",
    name: "Stable Diffusion XL 1.0 (GGUF)",
    brand: "Stability AI / City96",
    description:
      "Le modèle phare SDXL 1024x1024 d'une qualité artistique professionnelle exceptionnelle.",
    license: "OpenRail",
    contextWindow: "N/A",
    releaseDate: "2024-03",
    releaseYear: 2024,
    imageGen: true,
    variants: [
      {
        size: "7B",
        params: 7,
        tag: "https://huggingface.co/city96/SDXL-1.0-gguf/resolve/main/sdxl-1.0-Q4_0.gguf",
        quants: ["Q4_0", "Q8_0"],
        storageGb: 3.8,
      },
    ],
    source: "seed",
  },
  {
    id: "flux1-schnell",
    name: "FLUX.1 Schnell (GGUF)",
    brand: "Black Forest Labs / City96",
    description: "Modèle de génération d'images haute résolution sub-seconde (4 steps).",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2024-08",
    releaseYear: 2024,
    imageGen: true,
    variants: [
      {
        size: "12B",
        params: 12,
        tag: "https://huggingface.co/city96/FLUX.1-schnell-gguf/resolve/main/flux1-schnell-Q4_0.gguf",
        quants: ["Q4_0", "Q5_0", "Q8_0"],
        storageGb: 6.7,
      },
    ],
    source: "seed",
  },
  {
    id: "flux1-dev",
    name: "FLUX.1 Dev (GGUF - 12B)",
    brand: "Black Forest Labs / City96",
    description:
      "Le modèle d'image d'art le plus puissant au monde. Fidélité photoréaliste ultime.",
    license: "Non-Commercial",
    contextWindow: "N/A",
    releaseDate: "2024-08",
    releaseYear: 2024,
    imageGen: true,
    variants: [
      {
        size: "12B",
        params: 12,
        tag: "https://huggingface.co/city96/FLUX.1-dev-gguf/resolve/main/flux1-dev-Q4_0.gguf",
        quants: ["Q4_0", "Q8_0"],
        storageGb: 7.2,
      },
    ],
    source: "seed",
  },
  {
    id: "sd-3-5-medium",
    name: "Stable Diffusion 3.5 Medium (GGUF)",
    brand: "Stability AI / City96",
    description:
      "La toute dernière architecture SD 3.5 (2.5B) avec rendu de texte parfait et anatomie corrigée.",
    license: "Community License",
    contextWindow: "N/A",
    releaseDate: "2024-10",
    releaseYear: 2024,
    imageGen: true,
    variants: [
      {
        size: "3B",
        params: 3,
        tag: "https://huggingface.co/city96/stable-diffusion-3.5-medium-gguf/resolve/main/sd3.5_medium-Q4_0.gguf",
        quants: ["Q4_0", "Q8_0"],
        storageGb: 2.1,
      },
    ],
    source: "seed",
  },
  {
    id: "flux1-schnell-uncensored",
    name: "FLUX.1 Schnell — sans limite",
    brand: "Black Forest Labs / Community",
    description: "Variante non-filtrée de FLUX.1 Schnell pour l'art libre sans restriction.",
    license: "Open Weights",
    contextWindow: "N/A",
    releaseDate: "2024-09",
    releaseYear: 2024,
    imageGen: true,
    uncensored: true,
    variants: [
      {
        size: "12B — sans limite",
        params: 12,
        tag: "https://huggingface.co/city96/FLUX.1-schnell-gguf/resolve/main/flux1-schnell-Q4_0.gguf",
        quants: ["Q4_0"],
        storageGb: 6.7,
      },
    ],
    source: "seed",
  },
];

// ── Speech Synthesis / TTS Models ──────────────────────────────────────────

export const TTS_MODELS: ModelFamily[] = [
  {
    id: "piper-tts",
    name: "Piper TTS",
    brand: "rhasspy / Piper",
    description:
      "Synthese vocale locale ultra-rapide et legere, fonctionne sur CPU. Ideale pour la voix naturelle en hors ligne.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2024-11",
    releaseYear: 2024,
    audio: true,
    tts: true,
    variants: [
      {
        size: "en_US-amy-medium",
        params: 0.05,
        tag: "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/medium/en_US-amy-medium.onnx",
        quants: ["onnx"],
        storageGb: 0.05,
      },
      {
        size: "en_GB-alan-medium",
        params: 0.05,
        tag: "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_GB/alan/medium/en_GB-alan-medium.onnx",
        quants: ["onnx"],
        storageGb: 0.05,
      },
      {
        size: "fr_FR-siwis-medium",
        params: 0.05,
        tag: "https://huggingface.co/rhasspy/piper-voices/resolve/main/fr/fr_FR/siwis/medium/fr_FR-siwis-medium.onnx",
        quants: ["onnx"],
        storageGb: 0.05,
      },
    ],
    source: "seed",
  },
  {
    id: "kokoro-82m",
    name: "Kokoro-82M",
    brand: "hexgrad",
    description:
      "TTS haute qualite 82M parametres, le plus telecharge sur HuggingFace (10M+). StyleTTS2-based, voix naturelles en anglais. Existe en version ONNX pour inference locale.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2024-12",
    releaseYear: 2024,
    audio: true,
    tts: true,
    variants: [
      {
        size: "82M (ONNX FP32)",
        params: 0.082,
        tag: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model.onnx",
        quants: ["onnx"],
        storageGb: 0.31,
      },
      {
        size: "82M (ONNX FP16)",
        params: 0.082,
        tag: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model_fp16.onnx",
        quants: ["onnx"],
        storageGb: 0.16,
      },
      {
        size: "82M (ONNX Q8)",
        params: 0.082,
        tag: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model_quantized.onnx",
        quants: ["onnx"],
        storageGb: 0.09,
      },
      {
        size: "82M (PyTorch .pth)",
        params: 0.082,
        tag: "https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/kokoro-v1_0.pth",
        quants: ["pth"],
        storageGb: 0.31,
      },
    ],
    source: "seed",
  },
  {
    id: "coqui-xtts",
    name: "Coqui XTTS v2 (clonage de voix)",
    brand: "Coqui",
    description:
      "TTS haute fidelite avec clonage de voix zero-shot a partir d un simple echantillon audio. 17 langues. Le second TTS le plus populaire au monde (9M+ telechargements). Depot complet requis.",
    license: "Coqui Public License",
    contextWindow: "N/A",
    releaseDate: "2023-10",
    releaseYear: 2023,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "v2 (repo complet)",
        params: 2,
        tag: "https://huggingface.co/coqui/XTTS-v2",
        quants: ["repo"],
        storageGb: 1.87,
      },
    ],
    source: "seed",
  },
  {
    id: "pocket-tts",
    name: "Pocket TTS (clonage de voix CPU)",
    brand: "Kyutai",
    description:
      "TTS 100M parametres, le plus leger jamais sorti avec clonage de voix zero-shot (20s d'audio suffisent). Tourne en temps reel sur CPU (~200ms de latence), 8 voix incluses. Anglais uniquement. Depot gate sur HuggingFace (licence CC-BY-4.0 a accepter avant telechargement).",
    license: "CC-BY-4.0",
    contextWindow: "N/A",
    releaseDate: "2025-09",
    releaseYear: 2025,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "100M (repo complet, ~225 Mo)",
        params: 0.1,
        tag: "https://huggingface.co/kyutai/pocket-tts",
        quants: ["repo"],
        storageGb: 0.23,
      },
    ],
    source: "seed",
  },
  {
    id: "chatterbox",
    name: "Chatterbox (clonage de voix multilingue)",
    brand: "ResembleAI",
    description:
      "TTS multilingue open-source (29 langues) avec clonage de voix zero-shot. Hautement expressif, qualite studio. 2.5M+ telechargements.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2025-04",
    releaseYear: 2025,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "repo complet",
        params: 0.5,
        tag: "https://huggingface.co/ResembleAI/chatterbox",
        quants: ["repo"],
        storageGb: 6.5,
      },
    ],
    source: "seed",
  },
  {
    id: "qwen3-tts-customvoice",
    name: "Qwen3-TTS 1.7B (Custom Voice)",
    brand: "Alibaba / Qwen",
    description:
      "TTS Qwen3 1.7B avec voix personnalisee et clonage zero-shot. Multilingue (zh, en, ja, ko, de, fr, ru, pt, es, it). Le plus recent TTS de Qwen.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2026-01",
    releaseYear: 2026,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "1.7B (repo complet)",
        params: 1.7,
        tag: "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
        quants: ["repo"],
        storageGb: 3.4,
      },
    ],
    source: "seed",
  },
  {
    id: "qwen3-tts-voicedesign",
    name: "Qwen3-TTS 1.7B (Voice Design)",
    brand: "Alibaba / Qwen",
    description:
      "Qwen3-TTS en mode Voice Design : genere des voix personnalisees a partir de descriptions textuelles. Multilingue.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2026-01",
    releaseYear: 2026,
    audio: true,
    tts: true,
    variants: [
      {
        size: "1.7B (repo complet)",
        params: 1.7,
        tag: "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign",
        quants: ["repo"],
        storageGb: 3.4,
      },
    ],
    source: "seed",
  },
  {
    id: "qwen3-tts-06b-base",
    name: "Qwen3-TTS 0.6B (Base + clonage)",
    brand: "Alibaba / Qwen",
    description:
      "Version legere 0.6B de Qwen3-TTS avec support du clonage de voix. Ideale pour GPU modestes.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2026-01",
    releaseYear: 2026,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "0.6B (repo complet)",
        params: 0.6,
        tag: "https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-Base",
        quants: ["repo"],
        storageGb: 1.2,
      },
    ],
    source: "seed",
  },
  {
    id: "voxcpm2",
    name: "VoxCPM2 (clonage + voice design)",
    brand: "OpenBMB",
    description:
      "TTS multilingue (36 langues) avec clonage de voix zero-shot et voice design par description. Architecture diffusion-based, tres expressif.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2026-04",
    releaseYear: 2026,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "repo complet",
        params: 1,
        tag: "https://huggingface.co/openbmb/VoxCPM2",
        quants: ["repo"],
        storageGb: 3.0,
      },
    ],
    source: "seed",
  },
  {
    id: "omnivoice",
    name: "OmniVoice (clonage multilingue)",
    brand: "k2-fsa",
    description:
      "TTS zero-shot multilingue (300+ langues) avec clonage de voix et voice design. Base sur Qwen3-0.6B, tres leger.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2026-03",
    releaseYear: 2026,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "repo complet",
        params: 0.6,
        tag: "https://huggingface.co/k2-fsa/OmniVoice",
        quants: ["repo"],
        storageGb: 1.2,
      },
    ],
    source: "seed",
  },
  {
    id: "f5-tts",
    name: "F5-TTS (clonage de voix)",
    brand: "SWivid",
    description:
      "TTS a diffusion flow-matching avec clonage de voix zero-shot. Tres haute qualite, anglais principalement. 766K+ telechargements.",
    license: "CC-BY-NC-4.0",
    contextWindow: "N/A",
    releaseDate: "2024-10",
    releaseYear: 2024,
    audio: true,
    tts: true,
    voiceCloning: true,
    variants: [
      {
        size: "repo complet",
        params: 0.3,
        tag: "https://huggingface.co/SWivid/F5-TTS",
        quants: ["repo"],
        storageGb: 1.5,
      },
    ],
    source: "seed",
  },
  {
    id: "parler-tts",
    name: "Parler-TTS (Voice Design)",
    brand: "Parler / Hugging Face",
    description:
      "TTS base sur la description de voix (voice design). Genere une voix dont le style est defini par un prompt textuel. English-first, zero-shot voice design.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2024-04",
    releaseYear: 2024,
    audio: true,
    tts: true,
    voiceDesign: true,
    variants: [
      {
        size: "mini 1.1B",
        params: 1.1,
        tag: "https://huggingface.co/parler-tts/parler_tts_mini_v0.1",
        quants: ["repo"],
        storageGb: 2.2,
      },
      {
        size: "large 2.3B",
        params: 2.3,
        tag: "https://huggingface.co/parler-tts/parler_tts_large_v0.1",
        quants: ["repo"],
        storageGb: 4.6,
      },
    ],
    source: "seed",
  },
  {
    id: "vibevoice",
    name: "VibeVoice Realtime 0.5B",
    brand: "Microsoft",
    description:
      "TTS streaming temps reel 0.5B pour generation de parole longue duree. Anglais. Base sur Qwen2.5-0.5B, optimise low-latency.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2025-12",
    releaseYear: 2025,
    audio: true,
    tts: true,
    variants: [
      {
        size: "0.5B (repo complet)",
        params: 0.5,
        tag: "https://huggingface.co/microsoft/VibeVoice-Realtime-0.5B",
        quants: ["repo"],
        storageGb: 1.0,
      },
    ],
    source: "seed",
  },
  {
    id: "moss-tts",
    name: "MOSS-TTS",
    brand: "OpenMOSS",
    description:
      "TTS multilingue (15 langues) avec architecture delay-based. Qualite naturelle, support du francais.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2026-02",
    releaseYear: 2026,
    audio: true,
    tts: true,
    variants: [
      {
        size: "repo complet",
        params: 1,
        tag: "https://huggingface.co/OpenMOSS-Team/MOSS-TTS",
        quants: ["repo"],
        storageGb: 2.5,
      },
    ],
    source: "seed",
  },
  {
    id: "higgs-tts",
    name: "Higgs TTS 3-4B",
    brand: "BosonAI",
    description:
      "TTS 4B multimodal controllable et expressif, 100+ langues. Architecture Qwen3-based avec controle fin de la voix.",
    license: "Other",
    contextWindow: "N/A",
    releaseDate: "2026-06",
    releaseYear: 2026,
    audio: true,
    tts: true,
    variants: [
      {
        size: "4B (repo complet)",
        params: 4,
        tag: "https://huggingface.co/bosonai/higgs-tts-3-4b",
        quants: ["repo"],
        storageGb: 8.0,
      },
    ],
    source: "seed",
  },
  {
    id: "melotts",
    name: "MeloTTS",
    brand: "MeloTTS / MyShell",
    description:
      "TTS multilingue leger (anglais, francais, espagnol, chinois, etc.) base sur Transformer, rapide et naturel.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2024-02",
    releaseYear: 2024,
    audio: true,
    tts: true,
    variants: [
      {
        size: "multi (repo complet)",
        params: 0.2,
        tag: "https://huggingface.co/myshell-ai/MeloTTS-English",
        quants: ["repo"],
        storageGb: 0.5,
      },
    ],
    source: "seed",
  },
];

// ── Music Generation Models ─────────────────────────────────────────────
// These are Python-based text-to-music / text-to-audio models available
// on HuggingFace. Users install them via the Marketplace.

export const MUSIC_MODELS: ModelFamily[] = [
  {
    id: "musicgen-small",
    name: "MusicGen Small (1.5B)",
    brand: "Meta / FAIR",
    description:
      "MusicGen Small 1.5B de Meta. Génération musicale de haute qualité à partir de texte. Supporte la continuation mélodique (upload d'un fichier audio comme référence).",
    license: "CC-BY-NC-4.0",
    contextWindow: "N/A",
    releaseDate: "2024-06",
    releaseYear: 2024,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "1.5B (repo complet)",
        params: 1.5,
        tag: "https://huggingface.co/facebook/musicgen-small",
        quants: ["repo"],
        storageGb: 3.0,
      },
    ],
    source: "seed",
  },
  {
    id: "musicgen-medium",
    name: "MusicGen Medium (3.3B)",
    brand: "Meta / FAIR",
    description:
      "MusicGen Medium 3.3B. Version intermédiaire offrant un meilleur équilibre qualité/vitesse. Idéal pour GPU 6-8 Go VRAM.",
    license: "CC-BY-NC-4.0",
    contextWindow: "N/A",
    releaseDate: "2024-06",
    releaseYear: 2024,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "3.3B (repo complet)",
        params: 3.3,
        tag: "https://huggingface.co/facebook/musicgen-medium",
        quants: ["repo"],
        storageGb: 6.8,
      },
    ],
    source: "seed",
  },
  {
    id: "musicgen-large",
    name: "MusicGen Large (8.3B)",
    brand: "Meta / FAIR",
    description:
      "MusicGen Large 8.3B. Le plus grand modèle MusicGen, qualité maximale. Nécessite 12-16 Go VRAM pour l'inférence locale.",
    license: "CC-BY-NC-4.0",
    contextWindow: "N/A",
    releaseDate: "2024-06",
    releaseYear: 2024,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "8.3B (repo complet)",
        params: 8.3,
        tag: "https://huggingface.co/facebook/musicgen-large",
        quants: ["repo"],
        storageGb: 16.0,
      },
    ],
    source: "seed",
  },
  {
    id: "audioldm2",
    name: "AudioLDM 2 (Large)",
    brand: "Haohe Liu / Uni. Surrey",
    description:
      "AudioLDM 2 Large — synthèse audio/text-to-audio latente. Génère musique et effets sonores à partir de texte en langage naturel. Multilingue.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2024-05",
    releaseYear: 2024,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "Large (repo complet)",
        params: 0.5,
        tag: "https://huggingface.co/haoheliu/audioldm2-large",
        quants: ["repo"],
        storageGb: 4.0,
      },
    ],
    source: "seed",
  },
  {
    id: "audioldm2-music",
    name: "AudioLDM 2 (Music)",
    brand: "Haohe Liu / Uni. Surrey",
    description:
      "AudioLDM 2 spécialisé musique. Produit des échantillons musicaux de meilleure qualité que le modèle généraliste. 44.1 kHz.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2024-08",
    releaseYear: 2024,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "Music (repo complet)",
        params: 0.5,
        tag: "https://huggingface.co/haoheliu/audioldm2-music",
        quants: ["repo"],
        storageGb: 4.0,
      },
    ],
    source: "seed",
  },
  {
    id: "stable-audio-open",
    name: "Stable Audio Open 1.0",
    brand: "Stability AI",
    description:
      "Stable Audio Open 1.0 — génération audio (musique + effets) à partir de texte. Base sur VAE et transformer. 44.1 kHz, jusqu'à 114 secondes.",
    license: "Stability AI Community License",
    contextWindow: "N/A",
    releaseDate: "2024-08",
    releaseYear: 2024,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "1.0 (repo complet)",
        params: 1.2,
        tag: "https://huggingface.co/stabilityai/stable-audio-open-1.0",
        quants: ["repo"],
        storageGb: 2.8,
      },
    ],
    source: "seed",
  },
  {
    id: "riffusion",
    name: "Riffusion",
    brand: "Riffusion",
    description:
      "Riffusion — génération musicale via spectrogrammes (modèle de diffusion image vers audio). Génère des boucles et mélodies à partir de texte.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2023-11",
    releaseYear: 2023,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "v1 (repo complet)",
        params: 0.1,
        tag: "https://huggingface.co/riffusion/riffusion-model-v1/resolve/main/riffusion-model-v1.ckpt",
        quants: ["repo"],
        storageGb: 2.0,
      },
    ],
    source: "seed",
  },
  {
    id: "bark",
    name: "Bark (text-to-audio)",
    brand: "Suno",
    description:
      "Bark de Suno — modèle génératif audio transformeur qui peut produire de la parole, de la musique, des bruitages et des rires/chants. Multilingue.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2023-08",
    releaseYear: 2023,
    audio: true,
    musicGen: true,
    tts: true,
    variants: [
      {
        size: "v0 (repo complet)",
        params: 0.4,
        tag: "https://huggingface.co/suno/bark",
        quants: ["repo"],
        storageGb: 2.5,
      },
    ],
    source: "seed",
  },
  {
    id: "melody-musicgen",
    name: "MusicGen Melody (conditionné)",
    brand: "Meta / FAIR",
    description:
      "MusicGen avec support mélodique : fournissez un fichier audio en référence, le modèle génère une musique qui suit sa mélodie/style (continuation).",
    license: "CC-BY-NC-4.0",
    contextWindow: "N/A",
    releaseDate: "2024-06",
    releaseYear: 2024,
    audio: true,
    musicGen: true,
    variants: [
      {
        size: "Melody (même que Medium)",
        params: 3.3,
        tag: "https://huggingface.co/facebook/musicgen-medium",
        quants: ["repo"],
        storageGb: 6.8,
      },
    ],
    source: "seed",
  },
];

// ── Video Generation Models ─────────────────────────────────────────────
// These are Python-based text-to-video / image-to-video models.
// Users install them via the Marketplace as HuggingFace repos.

export const VIDEO_MODELS: ModelFamily[] = [
  {
    id: "wan21-i2v",
    name: "Wan 2.1 I2V (Image-to-Video)",
    brand: "Wan / Alibaba",
    description:
      "Wan 2.1 I2V — modèle image-vers-vidéo puissant et open-source. Génère 5s de vidéo à 1024x576 à partir d'une image de référence. 14B paramètres, nécessite GPU 16-24 Go VRAM.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2025-04",
    releaseYear: 2025,
    videoGen: true,
    variants: [
      {
        size: "14B (repo complet)",
        params: 14,
        tag: "https://huggingface.co/Wan-AI/Wan2.1-I2V-14B-480P",
        quants: ["repo"],
        storageGb: 30.0,
      },
    ],
    source: "seed",
  },
  {
    id: "wan21-t2v",
    name: "Wan 2.1 T2V (Text-to-Video)",
    brand: "Wan / Alibaba",
    description:
      "Wan 2.1 T2V — génération vidéo directement à partir de texte. Résultats impressionnants pour un modèle local. 14B paramètres.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2025-04",
    releaseYear: 2025,
    videoGen: true,
    variants: [
      {
        size: "14B (repo complet)",
        params: 14,
        tag: "https://huggingface.co/Wan-AI/Wan2.1-T2V-14B",
        quants: ["repo"],
        storageGb: 30.0,
      },
    ],
    source: "seed",
  },
  {
    id: "ltx-video",
    name: "LTX Video 0.9.1",
    brand: "Lightricks",
    description:
      "LTX Video — modèle de diffusion vidéo rapide (4 steps). Génération texte-vers-vidéo en 768x512. Très léger pour un modèle vidéo (2B).",
    license: "LTX Video License",
    contextWindow: "N/A",
    releaseDate: "2024-11",
    releaseYear: 2024,
    videoGen: true,
    variants: [
      {
        size: "2B (repo complet)",
        params: 2,
        tag: "https://huggingface.co/Lightricks/LTX-Video",
        quants: ["repo"],
        storageGb: 4.0,
      },
    ],
    source: "seed",
  },
  {
    id: "svd",
    name: "Stable Video Diffusion (SVD)",
    brand: "Stability AI",
    description:
      "Stable Video Diffusion — modèle image-vers-vidéo de Stability AI. Génère 14-25 frames à partir d'une image initiale. 2.5B paramètres.",
    license: "Stability AI Community License",
    contextWindow: "N/A",
    releaseDate: "2024-06",
    releaseYear: 2024,
    videoGen: true,
    variants: [
      {
        size: "2.5B (repo complet)",
        params: 2.5,
        tag: "https://huggingface.co/stabilityai/stable-video-diffusion-img2vid-xt",
        quants: ["repo"],
        storageGb: 5.0,
      },
    ],
    source: "seed",
  },
  {
    id: "cogvideo",
    name: "CogVideoX 5B",
    brand: "Tsinghua / Zhipu AI",
    description:
      "CogVideoX — modèle de génération vidéo texte-vers-vidéo de Tsinghua/Zhipu. 5B paramètres, génère 720x480, supporte le fine-tuning LoRA.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2024-10",
    releaseYear: 2024,
    videoGen: true,
    variants: [
      {
        size: "5B (repo complet)",
        params: 5,
        tag: "https://huggingface.co/THUDM/CogVideoX-5b",
        quants: ["repo"],
        storageGb: 10.0,
      },
    ],
    source: "seed",
  },
  {
    id: "hunyuan-video",
    name: "Hunyuan Video",
    brand: "Tencent",
    description:
      "Hunyuan Video de Tencent — modèle vidéo text-to-video 13B. Haute qualité, supporte le contrôle de mouvement et de caméra. Version GGUF et repo complets.",
    license: "Hunyuan Video License",
    contextWindow: "N/A",
    releaseDate: "2025-02",
    releaseYear: 2025,
    videoGen: true,
    variants: [
      {
        size: "13B (repo complet)",
        params: 13,
        tag: "https://huggingface.co/Tencent/HunyuanVideo",
        quants: ["repo"],
        storageGb: 26.0,
      },
    ],
    source: "seed",
  },
  {
    id: "mochi-1",
    name: "Mochi 1 (Genmo)",
    brand: "Genmo",
    description:
      "Mochi 1 de Genmo — modèle vidéo open-source de pointe avec prompt following excellent. 10B paramètres, génère jusqu'à 6s à 480p.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2025-10",
    releaseYear: 2025,
    videoGen: true,
    variants: [
      {
        size: "10B (repo complet)",
        params: 10,
        tag: "https://huggingface.co/genmo/mochi-1",
        quants: ["repo"],
        storageGb: 20.0,
      },
    ],
    source: "seed",
  },
];

// ── 3D Generation Models ────────────────────────────────────────────────
// These are Python-based text-to-3D / image-to-3D models.
// Users install them via the Marketplace as HuggingFace repos.

export const MODEL3D_MODELS: ModelFamily[] = [
  {
    id: "shap-e",
    name: "Shape-E (text-to-3D)",
    brand: "OpenAI",
    description:
      "Shape-E — modèle text-to-3D d'OpenAI qui génère un maillage 3D (format .obj/.ply) à partir d'une description textuelle. Base sur un modèle de diffusion 3D. 300M paramètres.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2023-05",
    releaseYear: 2023,
    model3d: true,
    variants: [
      {
        size: "300M (repo complet)",
        params: 0.3,
        tag: "https://huggingface.co/openai/shap-e",
        quants: ["repo"],
        storageGb: 1.0,
      },
    ],
    source: "seed",
  },
  {
    id: "point-e",
    name: "Point-E (text-to-3D)",
    brand: "OpenAI",
    description:
      "Point-E — génération de nuages de points 3D à partir de texte. Convertit ensuite en maillage. Plus rapide que Shape-E mais moins détaillé.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2023-05",
    releaseYear: 2023,
    model3d: true,
    variants: [
      {
        size: "1B (repo complet)",
        params: 1,
        tag: "https://huggingface.co/openai/point-e",
        quants: ["repo"],
        storageGb: 2.0,
      },
    ],
    source: "seed",
  },
  {
    id: "triposr",
    name: "TripoSR (image-to-3D)",
    brand: "Stability AI / Tripo",
    description:
      "TripoSR — reconstruction 3D ultra-rapide à partir d'une seule image (< 1s). Produit un maillage texturé de haute qualité. Base sur transformer + diffusion.",
    license: "MIT",
    contextWindow: "N/A",
    releaseDate: "2024-03",
    releaseYear: 2024,
    model3d: true,
    variants: [
      {
        size: "v1.0 (repo complet)",
        params: 0.3,
        tag: "https://huggingface.co/stabilityai/TripoSR",
        quants: ["repo"],
        storageGb: 0.8,
      },
    ],
    source: "seed",
  },
  {
    id: "zero-1-to-3",
    name: "Zero-1-to-3 (image-to-3D)",
    brand: "CVPR / Columbia",
    description:
      "Zero-1-to-3 — génère des vues novel à partir d'une image unique. Utilisable pour reconstruction 3D via NeRF ou score distillation sampling (SDS).",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2023-09",
    releaseYear: 2023,
    model3d: true,
    variants: [
      {
        size: "v1.1 (repo complet)",
        params: 1,
        tag: "https://huggingface.co/cvlab/zero123-llama-3.2-3b",
        quants: ["repo"],
        storageGb: 7.0,
      },
    ],
    source: "seed",
  },
  {
    id: "threestudio-sd",
    name: "ThreeStudio SD (text-to-3D)",
    brand: "ThreeStudio / threestudio-project",
    description:
      "ThreeStudio — framework de génération 3D par diffusion 2D (Score Distillation Sampling). Supporte Stable Diffusion comme backbone. Produit des maillages texturés.",
    license: "Apache-2.0",
    contextWindow: "N/A",
    releaseDate: "2024-01",
    releaseYear: 2024,
    model3d: true,
    variants: [
      {
        size: "v1.0 (repo complet)",
        params: 1,
        tag: "https://huggingface.co/threestudio-project/threestudio",
        quants: ["repo"],
        storageGb: 2.0,
      },
    ],
    source: "seed",
  },
];

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
    const isImageGen = /image.*gen|text.*to.*image|diffusion|flux|stable/i.test(
      `${m.name} ${m.description}`,
    );
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
    const isImageEditing = /image.*edit|inpaint|outpaint|upscale|restoration/i.test(
      `${m.name} ${m.description}`,
    );
    const isQuestionAnswering = /question.*answer|qa|extractive.qa/i.test(
      `${m.name} ${m.description}`,
    );
    const isInstruct =
      /instruct|chat/i.test(`${m.name} ${m.description}`) ||
      (!isImageGen && !isTTS && !isVideoGen && !isMusicGen && !is3D);

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
      imageGen: isImageGen,
      tts: isTTS,
      videoGen: isVideoGen,
      musicGen: isMusicGen,
      model3d: is3D,
      translation: isTranslation,
      objectDetection: isObjectDetection,
      textAnalysis: isTextAnalysis,
      imageEditing: isImageEditing,
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
  if (n.startsWith("flux") || n.startsWith("z-image") || n.includes("image")) return "Image Gen";
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

// ── HuggingFace TTS auto-discovery ──────────────────────────────────────────
// The GGUF fetch above only covers chat models. TTS/voice models ship as
// safetensors repos, so they need their own query. This keeps the registry in
// sync with HuggingFace without manual seed maintenance: new TTS models show
// up automatically on the next refresh.

const KNOWN_HF_REPOS = new Set(
  [...TTS_MODELS, ...MUSIC_MODELS, ...VIDEO_MODELS, ...MODEL3D_MODELS, ...IMAGE_GEN_MODELS]
    .flatMap((f) => f.variants.map((v) => v.tag))
    .filter((t) => t.startsWith("https://huggingface.co/") && !t.includes("/resolve/"))
    .map((t) => t.replace("https://huggingface.co/", "").replace(/\/+$/, "").toLowerCase()),
);

/** Does this HF repo id look like a text-to-speech / voice model (and not ASR)? */
function isTtsRepoId(repoId: string): boolean {
  if (
    /speech.to.text|asr\b|recognition|whisper|transcri|stt\b|voice.?chat|chat.?voice|audio.?lm|speech.?lm|to.?text/i.test(
      repoId,
    )
  ) {
    return false;
  }
  return (
    /tts|text.to.speech|text.?to.?speech|kokoro|xtts|parler|chatterbox|melotts|voxcpm|omnivoice|higgs.?tts|moss.?tts|pocket.?tts|piper|read.?aloud/i.test(
      repoId,
    ) ||
    /voice.?clone|voice.?design|voice.?conversion|neural.?voice/i.test(repoId) ||
    (/voice|speech|vits|bark/i.test(repoId) && !/recognition|to.?text/i.test(repoId))
  );
}

/**
 * Fetch live TTS/voice models from the HuggingFace Hub (safetensors repos).
 * These ship as full repos rather than single GGUF files, so they need a
 * separate query from fetchHuggingFaceModels(). No manual curation needed:
 * anything that looks like a TTS repo and is not already seeded shows up here.
 */
export async function fetchHuggingFaceTTSModels(query = "tts"): Promise<ModelFamily[]> {
  try {
    const res = await fetch(
      `https://huggingface.co/api/models?search=${encodeURIComponent(
        query,
      )}&filter=safetensors&sort=downloads&direction=-1&limit=25`,
    );
    if (!res.ok) return [];
    const items: Array<{
      id: string;
      downloads?: number;
      lastModified?: string;
      gated?: string | boolean;
    }> = await res.json();

    const families: ModelFamily[] = [];

    for (const item of items) {
      const repoId = item.id.toLowerCase();
      if (KNOWN_HF_REPOS.has(repoId)) continue; // already curated in the seed catalog
      if (!isTtsRepoId(repoId)) continue; // not a voice model
      if (item.gated === "true" || item.gated === true) continue; // hard-gated, not downloadable

      const parts = item.id.split("/");
      const author = parts[0] || "HuggingFace";
      const repoName = parts[1] || item.id;

      // Params: prefer explicit "100M" / "1.7B" style markers in the repo name.
      let params = 0.5;
      const mB = repoName.match(/(\d+(?:\.\d+)?)\s*[mM]\b/);
      const bB = repoName.match(/(\d+(?:\.\d+)?)\s*[bB]\b/);
      if (mB) params = Number.parseFloat(mB[1]) / 1000;
      else if (bB) params = Number.parseFloat(bB[1]);

      const yearMatch = item.lastModified ? new Date(item.lastModified).getFullYear() : 2026;
      const dateStr = item.lastModified ? item.lastModified.slice(0, 7) : "2026-01";

      families.push({
        id: `hf-${author}-${repoName.toLowerCase()}`,
        name: repoName.replace(/[-_]+/g, " "),
        brand: author,
        description: `Modèle TTS / voix découvert automatiquement sur HuggingFace Hub (${(item.downloads || 0).toLocaleString()} téléchargements).`,
        license: "Voir la licence HuggingFace",
        contextWindow: "N/A",
        releaseDate: dateStr,
        releaseYear: yearMatch,
        audio: true,
        tts: true,
        voiceCloning: /clone|xtts|zero.?shot|voice|pocket|speaker/i.test(repoId),
        voiceDesign: /voice.?design|description|prompt|parler/i.test(repoId),
        finetunable: true,
        variants: [
          {
            size: `${params}B (repo complet)`,
            params,
            tag: `https://huggingface.co/${item.id}`,
            quants: ["repo"],
            storageGb: Math.max(0.2, Math.round(params * 2 * 10) / 10),
          },
        ],
        source: "huggingface",
      });
    }

    return families;
  } catch {
    return [];
  }
}

// ── Merge & Deduplication ───────────────────────────────────────────────────

function mergeFamilies(...sources: ModelFamily[][]): ModelFamily[] {
  const byId = new Map<string, ModelFamily>();

  for (const list of sources) {
    for (const f of list) {
      // Normalize IDs for dedup: remove "ollama-" prefix if seed already has it
      const normalId = f.id.replace(/^ollama-/, "");

      if (byId.has(normalId)) {
        // Merge variants from duplicate
        const existing = byId.get(normalId)!;
        const existingTags = new Set(existing.variants.map((v) => v.tag));
        for (const v of f.variants) {
          if (!existingTags.has(v.tag)) {
            existing.variants.push(v);
          }
        }
        // Take higher pulls count
        if ((f.pulls || 0) > (existing.pulls || 0)) {
          existing.pulls = f.pulls;
        }
        // The seed catalog is manually curated, so it always wins on metadata,
        // capability flags and description. Only non-seed duplicates may enrich
        // the description if the Ollama-provided one is longer.
        if (
          f.source === "ollama" &&
          existing.source !== "seed" &&
          f.description.length > existing.description.length
        ) {
          existing.description = f.description;
        }
      } else {
        byId.set(normalId, { ...f, id: normalId });
      }
    }
  }

  // Sort: seed first, then by pulls desc, then by name
  return Array.from(byId.values()).sort((a, b) => {
    if (a.source === "seed" && b.source !== "seed") return -1;
    if (b.source === "seed" && a.source !== "seed") return 1;
    return (b.pulls || 0) - (a.pulls || 0) || a.name.localeCompare(b.name);
  });
}

// ── Main API ────────────────────────────────────────────────────────────────

export interface RegistryResult {
  families: ModelFamily[];
  imageGenModels: ModelFamily[];
  ttsModels: ModelFamily[];
  brands: string[];
  loading: boolean;
  lastFetched: number | null;
}

/**
 * Fetch all models from Ollama library (via Rust IPC), HuggingFace, and seed.
 * Uses localStorage cache (1h TTL).
 *
 * @param searchOllamaLibrary - The Rust IPC function from CoreApi
 */
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
      IMAGE_GEN_MODELS,
      TTS_MODELS,
      cached.families,
    );
    const imageGen = all.filter((f) => f.imageGen);
    const tts = all.filter((f) => f.tts);
    const brands = Array.from(new Set(all.map((f) => f.brand))).sort();
    return {
      families: all,
      imageGenModels: imageGen,
      ttsModels: tts,
      brands,
      loading: false,
      lastFetched: cached.timestamp,
    };
  }

  // 2. Fetch in parallel
  const [ollamaModels, hfModels, hfTtsModels] = await Promise.all([
    searchOllamaLibrary
      ? searchOllamaLibrary("", undefined).catch(() => [] as OllamaLibraryModel[])
      : Promise.resolve([] as OllamaLibraryModel[]),
    fetchHuggingFaceModels("gguf").catch(() => [] as ModelFamily[]),
    fetchHuggingFaceTTSModels("tts").catch(() => [] as ModelFamily[]),
  ]);

  // 3. Convert ollama library results
  const ollamaFamilies = ollamaLibraryToFamilies(ollamaModels);

  // 4. Merge all sources: seed + large local + airllm + image gen + tts + ollama + HF (GGUF + TTS)
  const allFamilies = mergeFamilies(
    SEED_CATALOG,
    LARGE_LOCAL_MODELS,
    AIRLLM_CATALOG_MODELS,
    IMAGE_GEN_MODELS,
    TTS_MODELS,
    ollamaFamilies,
    hfModels,
    hfTtsModels,
  );

  // 5. Cache
  saveCache(allFamilies);

  const imageGen = allFamilies.filter((f) => f.imageGen);
  const tts = allFamilies.filter((f) => f.tts);
  const brands = Array.from(new Set(allFamilies.map((f) => f.brand))).sort();

  return {
    families: allFamilies,
    imageGenModels: imageGen,
    ttsModels: tts,
    brands,
    loading: false,
    lastFetched: Date.now(),
  };
}

/**
 * Force refresh the registry (bypasses cache).
 */
export function clearRegistryCache(): void {
  localStorage.removeItem(CACHE_KEY);
}

/**
 * Check if any image generation model is installed locally.
 */
export function findInstalledImageGenModel(installedTags: string[]): string | null {
  const imageGenTags = IMAGE_GEN_MODELS.flatMap((f) => f.variants.map((v) => v.tag));
  const installed = new Set(installedTags.map((t) => t.replace(/:latest$/, "")));
  for (const tag of imageGenTags) {
    if (installed.has(tag) || installed.has(`${tag}:latest`)) return tag;
  }
  return null;
}

// ── Re-export for backward compat ───────────────────────────────────────────

/** @deprecated Use fetchFullRegistry() instead */
export const MODEL_CATALOG = [
  ...SEED_CATALOG,
  ...LARGE_LOCAL_MODELS,
  ...AIRLLM_CATALOG_MODELS,
  ...IMAGE_GEN_MODELS,
];
export const BRANDS = Array.from(new Set(MODEL_CATALOG.map((f) => f.brand))).sort();
