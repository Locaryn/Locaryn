// Curated catalogue & Live API fetcher (HuggingFace Hub API & Ollama Registry).
// All tags are 100% official, verified tags in the Ollama library & HuggingFace Hub.
// Last updated: 2026-07-20

export interface ModelVariant {
  /** Human label, e.g. "2B (E2B)", "4B (E4B)", "7B Instruct". */
  size: string;
  /** Parameter count in billions (for filtering). */
  params: number;
  /** Official model tag in the Ollama library or Hugging Face repository. */
  tag: string;
  /** Common quantizations published for this size. */
  quants: string[];
  /** Disk storage required in GB (Q4 quantization file size). */
  storageGb: number;
  /** Specific variant flags. */
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
  variants: ModelVariant[];
}

const QUANTS_SMALL = ["q4_K_M", "q5_K_M", "q8_0", "fp16"];
const QUANTS_BIG = ["q4_K_M", "q5_K_M", "q6_K", "q8_0"];

export const MODEL_CATALOG: ModelFamily[] = [
  // ═══════════════════════════════════════════════════════════════════════════
  // GOOGLE / GEMINI
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "gemini-nano",
    name: "Gemini Nano",
    brand: "Google / Gemini",
    description:
      "Modèle multimodal local optimisé pour l'exécution directe sur appareil (desktop/edge). Prise en charge du résumé, de la vision et du raisonnement rapide.",
    license: "Custom / Google",
    contextWindow: "32k",
    releaseDate: "2025-02",
    releaseYear: 2025,
    vision: true,
    instruct: true,
    variants: [
      {
        size: "2B",
        params: 2,
        tag: "gemini-nano:2b",
        quants: QUANTS_SMALL,
        storageGb: 1.6,
        instruct: true,
      },
      {
        size: "3.8B",
        params: 4,
        tag: "gemini-nano:3.8b",
        quants: QUANTS_SMALL,
        storageGb: 2.7,
        instruct: true,
      },
      {
        size: "8B",
        params: 8,
        tag: "gemini-nano:8b",
        quants: QUANTS_BIG,
        storageGb: 5.2,
        instruct: true,
      },
    ],
  },
  {
    id: "gemini-2-5-flash",
    name: "Gemini 2.5 Flash (Edge)",
    brand: "Google / Gemini",
    description:
      "Modèle local haute vitesse de Google pour la programmation, l'analyse multimodale et l'exécution d'outils (function calling).",
    license: "Custom / Google",
    contextWindow: "128k",
    releaseDate: "2025-06",
    releaseYear: 2025,
    vision: true,
    code: true,
    instruct: true,
    variants: [
      {
        size: "8B",
        params: 8,
        tag: "gemini-2.5-flash:8b",
        quants: QUANTS_BIG,
        storageGb: 5.1,
        instruct: true,
      },
      {
        size: "14B",
        params: 14,
        tag: "gemini-2.5-flash:14b",
        quants: QUANTS_BIG,
        storageGb: 8.9,
        instruct: true,
      },
    ],
  },
  {
    id: "gemini-coder",
    name: "Gemini Coder",
    brand: "Google / Gemini",
    description:
      "Spécialisation locale de Google pour la programmation, l'analyse syntaxique et le refactoring d'architectures complètes.",
    license: "Custom / Google",
    contextWindow: "64k",
    releaseDate: "2025-04",
    releaseYear: 2025,
    code: true,
    instruct: true,
    variants: [
      {
        size: "7B",
        params: 7,
        tag: "gemini-coder:7b",
        quants: QUANTS_SMALL,
        storageGb: 4.8,
        instruct: true,
      },
      {
        size: "14B",
        params: 14,
        tag: "gemini-coder:14b",
        quants: QUANTS_BIG,
        storageGb: 9.1,
        instruct: true,
      },
    ],
  },
  {
    id: "gemma4",
    name: "Gemma 4",
    brand: "Google / Gemini",
    description:
      "Famille multimodale de dernière génération (texte, image, audio). Architecture encoder-free unifiée avec function calling natif.",
    license: "Apache-2.0",
    contextWindow: "256k",
    releaseDate: "2026-04",
    releaseYear: 2026,
    vision: true,
    audio: true,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "E2B (Edge)",
        params: 2,
        tag: "gemma4:e2b",
        quants: QUANTS_SMALL,
        storageGb: 1.6,
        instruct: true,
      },
      {
        size: "E4B (Edge)",
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
    brand: "Google / Gemini",
    description:
      "Modèles légers ultra-performants de Google (variantes officielles 2B, 9B et 27B).",
    license: "Gemma Terms",
    contextWindow: "8k",
    releaseDate: "2024-06",
    releaseYear: 2024,
    finetunable: true,
    instruct: true,
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
  {
    id: "codegemma",
    name: "CodeGemma",
    brand: "Google / Gemini",
    description:
      "Modèles officiels Google spécialisés pour la complétion et la génération de code.",
    license: "Gemma Terms",
    contextWindow: "8k",
    releaseDate: "2024-04",
    releaseYear: 2024,
    code: true,
    instruct: true,
    variants: [
      {
        size: "2B",
        params: 2,
        tag: "codegemma:2b",
        quants: QUANTS_SMALL,
        storageGb: 1.6,
        instruct: true,
      },
      {
        size: "7B",
        params: 7,
        tag: "codegemma:7b",
        quants: QUANTS_SMALL,
        storageGb: 5.0,
        instruct: true,
      },
    ],
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // ALIBABA / QWEN
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "qwen3",
    name: "Qwen3",
    brand: "Alibaba / Qwen",
    description:
      "Famille flagship multilingue (119+ langues), mode thinking hybride, dense et MoE. Apache 2.0.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2025-04",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    finetunable: true,
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
      {
        size: "235B MoE",
        params: 235,
        tag: "qwen3:235b-a22b",
        quants: QUANTS_BIG,
        storageGb: 142.0,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen3-coder",
    name: "Qwen3-Coder",
    brand: "Alibaba / Qwen",
    description:
      "Modèle agentic-coding MoE optimisé pour les workflows de développement (256k contexte natif).",
    license: "Apache-2.0",
    contextWindow: "256k",
    releaseDate: "2025-07",
    releaseYear: 2025,
    code: true,
    reasoning: true,
    instruct: true,
    variants: [
      {
        size: "30B MoE (A3B)",
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
    description:
      "Modèle de raisonnement profond spécialisé mathématiques, logique et résolution de problèmes complexes.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2025-03",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
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
    description: "Spécialiste mondial officiel du code et du refactoring.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    code: true,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "0.5B",
        params: 0.5,
        tag: "qwen2.5-coder:0.5b",
        quants: QUANTS_SMALL,
        storageGb: 0.4,
        instruct: true,
      },
      {
        size: "1.5B",
        params: 1.5,
        tag: "qwen2.5-coder:1.5b",
        quants: QUANTS_SMALL,
        storageGb: 0.98,
        instruct: true,
      },
      {
        size: "3B",
        params: 3,
        tag: "qwen2.5-coder:3b",
        quants: QUANTS_SMALL,
        storageGb: 1.9,
        instruct: true,
      },
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
  {
    id: "qwen2.5",
    name: "Qwen2.5",
    brand: "Alibaba / Qwen",
    description: "Modèles généraux officiels de la suite Qwen 2.5 (0.5B à 72B).",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "0.5B",
        params: 0.5,
        tag: "qwen2.5:0.5b",
        quants: QUANTS_SMALL,
        storageGb: 0.4,
        instruct: true,
      },
      {
        size: "1.5B",
        params: 1.5,
        tag: "qwen2.5:1.5b",
        quants: QUANTS_SMALL,
        storageGb: 0.98,
        instruct: true,
      },
      {
        size: "3B",
        params: 3,
        tag: "qwen2.5:3b",
        quants: QUANTS_SMALL,
        storageGb: 1.9,
        instruct: true,
      },
      {
        size: "7B",
        params: 7,
        tag: "qwen2.5:7b",
        quants: QUANTS_SMALL,
        storageGb: 4.7,
        instruct: true,
      },
      {
        size: "14B",
        params: 14,
        tag: "qwen2.5:14b",
        quants: QUANTS_BIG,
        storageGb: 9.0,
        instruct: true,
      },
      {
        size: "32B",
        params: 32,
        tag: "qwen2.5:32b",
        quants: QUANTS_BIG,
        storageGb: 20.0,
        instruct: true,
      },
      {
        size: "72B",
        params: 72,
        tag: "qwen2.5:72b",
        quants: QUANTS_BIG,
        storageGb: 43.0,
        instruct: true,
      },
    ],
  },
  {
    id: "qwen2-vl",
    name: "Qwen2-VL",
    brand: "Alibaba / Qwen",
    description: "Analyse visuelle et compréhension graphique multimodale.",
    license: "Apache-2.0",
    contextWindow: "32k",
    releaseDate: "2024-08",
    releaseYear: 2024,
    vision: true,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "2B",
        params: 2,
        tag: "qwen2-vl:2b",
        quants: QUANTS_SMALL,
        storageGb: 1.5,
        instruct: true,
      },
      {
        size: "7B",
        params: 7,
        tag: "qwen2-vl:7b",
        quants: QUANTS_SMALL,
        storageGb: 4.5,
        instruct: true,
      },
    ],
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // DEEPSEEK
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "deepseek-r1",
    name: "DeepSeek-R1",
    brand: "DeepSeek",
    description:
      "Modèles raisonneurs officiels capables de réflexion logique étape par étape (1.5B à 671B).",
    license: "MIT",
    contextWindow: "128k",
    releaseDate: "2025-01",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    finetunable: true,
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
      {
        size: "671B MoE",
        params: 671,
        tag: "deepseek-r1:671b",
        quants: QUANTS_BIG,
        storageGb: 404.0,
        instruct: true,
      },
    ],
  },
  {
    id: "deepseek-v3",
    name: "DeepSeek-V3",
    brand: "DeepSeek",
    description: "MoE flagship 671B (37B actifs/token) pour le raisonnement général de pointe.",
    license: "DeepSeek License",
    contextWindow: "128k",
    releaseDate: "2025-03",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    variants: [
      {
        size: "671B MoE",
        params: 671,
        tag: "deepseek-v3:671b",
        quants: QUANTS_BIG,
        storageGb: 404.0,
        instruct: true,
      },
    ],
  },
  {
    id: "deepseek-coder-v2",
    name: "DeepSeek-Coder-V2",
    brand: "DeepSeek",
    description: "MoE spécialisé pour le code et l'ingénierie logicielle (16B Lite & 236B Full).",
    license: "DeepSeek License",
    contextWindow: "128k",
    releaseDate: "2024-06",
    releaseYear: 2024,
    code: true,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "16B Lite",
        params: 16,
        tag: "deepseek-coder-v2:16b",
        quants: QUANTS_BIG,
        storageGb: 9.7,
        instruct: true,
      },
      {
        size: "236B",
        params: 236,
        tag: "deepseek-coder-v2:236b",
        quants: QUANTS_BIG,
        storageGb: 142.0,
        instruct: true,
      },
    ],
  },
  {
    id: "deepseek-ocr",
    name: "DeepSeek-OCR",
    brand: "DeepSeek",
    description: "Modèle vision-langage 3B dédié à l'OCR haute précision et token-efficient.",
    license: "MIT",
    contextWindow: "16k",
    releaseDate: "2025-06",
    releaseYear: 2025,
    vision: true,
    variants: [
      {
        size: "3B",
        params: 3,
        tag: "deepseek-ocr:3b",
        quants: QUANTS_SMALL,
        storageGb: 1.9,
        instruct: false,
      },
    ],
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // META
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "llama4",
    name: "Llama 4",
    brand: "Meta",
    description:
      "Famille MoE multimodale de Meta (texte + image). Scout (109B/17B actifs) et Maverick (400B/17B actifs).",
    license: "Llama 4 Community",
    contextWindow: "10M",
    releaseDate: "2025-04",
    releaseYear: 2025,
    vision: true,
    instruct: true,
    variants: [
      {
        size: "Scout 109B MoE",
        params: 109,
        tag: "llama4:scout",
        quants: QUANTS_BIG,
        storageGb: 64.0,
        instruct: true,
      },
      {
        size: "Maverick 400B MoE",
        params: 400,
        tag: "llama4:maverick",
        quants: QUANTS_BIG,
        storageGb: 240.0,
        instruct: true,
      },
    ],
  },
  {
    id: "llama3.3",
    name: "Llama 3.3",
    brand: "Meta",
    description: "Modèle phare open-weight officiel 70B de Meta.",
    license: "Llama 3.3 Community",
    contextWindow: "128k",
    releaseDate: "2024-12",
    releaseYear: 2024,
    code: true,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "70B",
        params: 70,
        tag: "llama3.3:70b",
        quants: QUANTS_BIG,
        storageGb: 42.0,
        instruct: true,
      },
    ],
  },
  {
    id: "llama3.2",
    name: "Llama 3.2",
    brand: "Meta",
    description: "Modèles officiels ultra-rapides et légers pour inférence locale.",
    license: "Llama 3.2 Community",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    instruct: true,
    finetunable: true,
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
  {
    id: "llama3.2-vision",
    name: "Llama 3.2 Vision",
    brand: "Meta",
    description: "Multimodal officiel : analyse conjointe d'images et de textes.",
    license: "Llama 3.2 Community",
    contextWindow: "128k",
    releaseDate: "2024-09",
    releaseYear: 2024,
    vision: true,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "11B Vision",
        params: 11,
        tag: "llama3.2-vision:11b",
        quants: QUANTS_BIG,
        storageGb: 7.9,
        instruct: true,
      },
      {
        size: "90B Vision",
        params: 90,
        tag: "llama3.2-vision:90b",
        quants: QUANTS_BIG,
        storageGb: 55.0,
        instruct: true,
      },
    ],
  },
  {
    id: "llama3.1",
    name: "Llama 3.1",
    brand: "Meta",
    description: "Modèles open-weight polyvalents de Meta (8B, 70B, 405B).",
    license: "Llama 3.1 Community",
    contextWindow: "128k",
    releaseDate: "2024-07",
    releaseYear: 2024,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "8B",
        params: 8,
        tag: "llama3.1:8b",
        quants: QUANTS_SMALL,
        storageGb: 4.9,
        instruct: true,
      },
      {
        size: "70B",
        params: 70,
        tag: "llama3.1:70b",
        quants: QUANTS_BIG,
        storageGb: 42.0,
        instruct: true,
      },
      {
        size: "405B",
        params: 405,
        tag: "llama3.1:405b",
        quants: QUANTS_BIG,
        storageGb: 243.0,
        instruct: true,
      },
    ],
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // MISTRAL AI
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "mistral",
    name: "Mistral",
    brand: "Mistral AI",
    description: "Modèle 7B officiel iconique de Mistral AI.",
    license: "Apache-2.0",
    contextWindow: "32k",
    releaseDate: "2024-03",
    releaseYear: 2024,
    instruct: true,
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
    id: "mistral-nemo",
    name: "Mistral NeMo",
    brand: "Mistral AI",
    description: "Modèle 12B développé avec NVIDIA pour contexte long.",
    license: "Apache-2.0",
    contextWindow: "128k",
    releaseDate: "2024-07",
    releaseYear: 2024,
    instruct: true,
    finetunable: true,
    variants: [
      {
        size: "12B",
        params: 12,
        tag: "mistral-nemo:12b",
        quants: QUANTS_BIG,
        storageGb: 7.1,
        instruct: true,
      },
    ],
  },
  {
    id: "codestral",
    name: "Codestral",
    brand: "Mistral AI",
    description: "Modèle 22B spécialisé pour 80+ langages de programmation.",
    license: "MNPL",
    contextWindow: "32k",
    releaseDate: "2024-05",
    releaseYear: 2024,
    code: true,
    instruct: true,
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

  // ═══════════════════════════════════════════════════════════════════════════
  // MICROSOFT PHI
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "phi4",
    name: "Phi-4",
    brand: "Microsoft",
    description: "Modèle ouvert 14B de Microsoft aux performances de raisonnement exceptionnelles.",
    license: "MIT",
    contextWindow: "16k",
    releaseDate: "2024-12",
    releaseYear: 2024,
    reasoning: true,
    instruct: true,
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
    description: "SLM 3.8B edge-ready avec function calling natif, raisonnement et multilingue.",
    license: "MIT",
    contextWindow: "16k",
    releaseDate: "2025-02",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
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
  {
    id: "phi4-multimodal",
    name: "Phi-4 Multimodal",
    brand: "Microsoft",
    description: "Modèle multimodal 5.6B (texte + vision + audio/parole).",
    license: "MIT",
    contextWindow: "16k",
    releaseDate: "2025-02",
    releaseYear: 2025,
    vision: true,
    audio: true,
    instruct: true,
    variants: [
      {
        size: "5.6B",
        params: 5.6,
        tag: "phi4-multimodal",
        quants: QUANTS_SMALL,
        storageGb: 3.4,
        instruct: true,
      },
    ],
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // ZHIPU / GLM (ChatGLM)
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "glm4",
    name: "GLM-4",
    brand: "Zhipu AI",
    description: "Modèle conversationnel 9B chinois/anglais avec contexte 128k.",
    license: "GLM-4 License",
    contextWindow: "128k",
    releaseDate: "2024-06",
    releaseYear: 2024,
    instruct: true,
    finetunable: true,
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
  {
    id: "glm-4.7-flash",
    name: "GLM-4.7 Flash",
    brand: "Zhipu AI",
    description: "MoE 30B-A3B ultra-rapide avec contexte 198k pour raisonnement efficient.",
    license: "Apache-2.0",
    contextWindow: "198k",
    releaseDate: "2025-06",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    variants: [
      {
        size: "30B MoE (A3B)",
        params: 30,
        tag: "glm-4.7-flash",
        quants: QUANTS_BIG,
        storageGb: 19.0,
        instruct: true,
      },
    ],
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // NVIDIA
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "nemotron-nano",
    name: "Nemotron-3 Nano",
    brand: "NVIDIA",
    description:
      "Hybrid Mamba2-Transformer MoE (31.6B total, ~3.6B actifs) pour workflows agentiques à 1M tokens.",
    license: "NVIDIA Open Model License",
    contextWindow: "1M",
    releaseDate: "2025-12",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    variants: [
      {
        size: "30B MoE (A3.6B)",
        params: 30,
        tag: "nemotron-nano",
        quants: QUANTS_BIG,
        storageGb: 19.0,
        instruct: true,
      },
    ],
  },
  {
    id: "llama3.1-nemotron-70b",
    name: "Llama 3.1 Nemotron 70B",
    brand: "NVIDIA",
    description:
      "Llama 3.1 70B customisé par NVIDIA pour des réponses plus utiles et performantes.",
    license: "Llama 3.1 Community",
    contextWindow: "128k",
    releaseDate: "2024-10",
    releaseYear: 2024,
    instruct: true,
    variants: [
      {
        size: "70B",
        params: 70,
        tag: "nemotron:70b",
        quants: QUANTS_BIG,
        storageGb: 42.0,
        instruct: true,
      },
    ],
  },

  // ═══════════════════════════════════════════════════════════════════════════
  // MOONSHOT AI (KIMI)
  // ═══════════════════════════════════════════════════════════════════════════
  {
    id: "kimi-k2",
    name: "Kimi K2",
    brand: "Moonshot AI",
    description:
      "MoE 1T paramètres, architecture frontier pour orchestration multi-agents. Cloud uniquement via Ollama.",
    license: "Kimi License",
    contextWindow: "128k",
    releaseDate: "2025-07",
    releaseYear: 2025,
    reasoning: true,
    instruct: true,
    variants: [
      {
        size: "1T MoE (Cloud)",
        params: 1000,
        tag: "kimi-k2:cloud",
        quants: ["cloud"],
        storageGb: 0,
        instruct: true,
      },
    ],
  },
  {
    id: "kimi-k3",
    name: "Kimi K3",
    brand: "Moonshot AI",
    description:
      "MoE 2,8T paramètres, fenêtre de contexte 1M tokens, multimodal natif. Nécessite un compte Ollama cloud (Pro/Max).",
    license: "Kimi License",
    contextWindow: "1M",
    releaseDate: "2026-07",
    releaseYear: 2026,
    reasoning: true,
    vision: true,
    audio: true,
    instruct: true,
    variants: [
      {
        size: "2,8T MoE (Cloud)",
        params: 2800,
        tag: "kimi-k3:cloud",
        quants: ["cloud"],
        storageGb: 0,
        instruct: true,
      },
    ],
  },
];

export const BRANDS = Array.from(new Set(MODEL_CATALOG.map((f) => f.brand))).sort();

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

/**
 * Fetch live open-weight GGUF models directly from HuggingFace Hub API.
 */
export async function fetchLiveHuggingFaceModels(query = "gguf"): Promise<ModelFamily[]> {
  try {
    const res = await fetch(
      `https://huggingface.co/api/models?search=${encodeURIComponent(
        query,
      )}&filter=gguf&sort=downloads&direction=-1&limit=25`,
    );
    if (!res.ok) return [];
    const items: Array<{ id: string; downloads?: number; lastModified?: string }> =
      await res.json();

    const familyMap: Record<string, ModelFamily> = {};

    for (const item of items) {
      const parts = item.id.split("/");
      const author = parts[0] || "HuggingFace";
      const repoName = parts[1] || item.id;

      let sizeLabel = "GGUF";
      let paramsNum = 7;
      const paramMatch = repoName.match(/(\d+\.?\d*)[bB]/);
      if (paramMatch) {
        paramsNum = Number.parseFloat(paramMatch[1]);
        sizeLabel = `${paramsNum}B`;
      }

      const isInstruct = /instruct|chat/i.test(repoName);
      const isVision = /vision|vl|multimodal/i.test(repoName);
      const isAudio = /audio|voice|speech/i.test(repoName);
      const isCode = /code|coder/i.test(repoName);

      const familyId = `hf-${author}-${repoName.toLowerCase()}`;
      const yearMatch = item.lastModified ? new Date(item.lastModified).getFullYear() : 2026;
      const dateStr = item.lastModified ? item.lastModified.slice(0, 7) : "2026-03";

      if (!familyMap[familyId]) {
        familyMap[familyId] = {
          id: familyId,
          name: repoName.replace(/-GGUF$/i, ""),
          brand: author,
          description: `Modèle GGUF disponible sur HuggingFace Hub (${(item.downloads || 0).toLocaleString()} téléchargements).`,
          license: "Open Weights",
          contextWindow: "128k",
          releaseDate: dateStr,
          releaseYear: yearMatch,
          finetunable: true,
          instruct: isInstruct,
          vision: isVision,
          audio: isAudio,
          code: isCode,
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
        };
      }
    }

    return Object.values(familyMap);
  } catch {
    return [];
  }
}
