// Les trois modèles proposés à quelqu'un qui débute, et le calcul de celui qui
// convient à sa machine. Les étiquettes (tag) sont celles du catalogue : le
// téléchargement passe par le même chemin que le marketplace.

import type { LlmfitHardware } from "./core";
import { MODEL_CATALOG } from "./modelCatalog";

export type ModelTier = "fast" | "balanced" | "smart";

export interface TierModel {
  tier: ModelTier;
  tag: string;
  /** Taille sur disque (Go), lue dans le catalogue. */
  storageGb: number;
  /** Mémoire minimale conseillée (Go) pour qu'il tourne à l'aise. */
  needGb: number;
}

const TIER_TAGS: Record<ModelTier, { tag: string; fallbackGb: number }> = {
  fast: { tag: "qwen3:1.7b", fallbackGb: 1.4 },
  balanced: { tag: "qwen3:4b", fallbackGb: 2.6 },
  smart: { tag: "qwen3:14b", fallbackGb: 9.3 },
};

function sizeOf(tag: string, fallbackGb: number): number {
  for (const family of MODEL_CATALOG) {
    const v = family.variants.find((x) => x.tag === tag);
    if (v) return v.storageGb;
  }
  return fallbackGb;
}

export function tierModels(): TierModel[] {
  return (Object.keys(TIER_TAGS) as ModelTier[]).map((tier) => {
    const { tag, fallbackGb } = TIER_TAGS[tier];
    const storageGb = sizeOf(tag, fallbackGb);
    return { tier, tag, storageGb, needGb: Math.ceil(storageGb * 1.4) };
  });
}

/** La mémoire que la machine peut raisonnablement consacrer à un modèle. */
export function usableMemoryGb(hw: LlmfitHardware | null): number {
  if (!hw) return 8;
  if (hw.unified_memory) return hw.total_ram_gb * 0.7;
  return Math.max(hw.total_vram_gb, hw.total_ram_gb * 0.6);
}

/** Le palier à mettre en avant, et ceux qui sont trop lourds ici. */
export function judgeTiers(hw: LlmfitHardware | null): {
  recommended: ModelTier;
  tooHeavy: Set<ModelTier>;
} {
  const mem = usableMemoryGb(hw);
  const models = tierModels();
  const tooHeavy = new Set<ModelTier>(models.filter((m) => m.needGb > mem).map((m) => m.tier));
  // Le plus gros n'est conseillé que s'il a de la mémoire graphique dédiée :
  // sur processeur seul, il tient mais traîne.
  const gpuOk = hw ? hw.total_vram_gb >= 12 || hw.unified_memory : false;
  let recommended: ModelTier = "balanced";
  if (!tooHeavy.has("smart") && gpuOk) recommended = "smart";
  else if (tooHeavy.has("balanced")) recommended = "fast";
  return { recommended, tooHeavy };
}
