// Les trois modèles proposés à quelqu'un qui débute, choisis sur la machine.
//
// Pas de seuils codés en dur sur la mémoire : on soumet une échelle de modèles
// au même estimateur que le catalogue (poids, cache, bande passante) et on
// retient, pour chaque palier, ce qui tourne réellement à la vitesse voulue.
// Les étiquettes (tag) sont celles du catalogue : le téléchargement passe par
// le même chemin que le marketplace.

import { type LlmfitCatalogEntry, type LlmfitHardware, type ModelFit, core } from "./core";
import { MODEL_CATALOG } from "./modelCatalog";

export type ModelTier = "fast" | "balanced" | "smart";

export interface TierChoice {
  tier: ModelTier;
  tag: string;
  /** Taille du téléchargement (Go). */
  storageGb: number;
  /** Vitesse estimée sur cette machine ; `null` si l'estimation a échoué. */
  tokensPerSecond: number | null;
  /** Ne tient pas, ou trop lent pour servir. */
  tooHeavy: boolean;
}

export interface TierPlan {
  choices: TierChoice[];
  recommended: ModelTier;
  hardware: LlmfitHardware | null;
}

interface Rung {
  tag: string;
  params: number;
  storageGb: number;
}

/** Fluide : on peut suivre la réponse à la lecture. */
const FLUID_TPS = 10;
/** En dessous, l'attente devient pénible. */
const USABLE_TPS = 4;

/** L'échelle de modèles : une même famille, du plus petit au plus gros. */
function ladder(): Rung[] {
  const family = MODEL_CATALOG.find((f) => f.id === "qwen3");
  const rungs = (family?.variants ?? [])
    .filter((v) => /^qwen3:\d+(\.\d+)?b$/.test(v.tag) && v.params >= 1.5 && v.params <= 32)
    .map((v) => ({ tag: v.tag, params: v.params, storageGb: v.storageGb }));
  if (rungs.length >= 3) return rungs.sort((a, b) => a.params - b.params);
  return [
    { tag: "qwen3:1.7b", params: 1.7, storageGb: 1.4 },
    { tag: "qwen3:4b", params: 4, storageGb: 2.6 },
    { tag: "qwen3:14b", params: 14, storageGb: 9.3 },
  ];
}

function entryOf(r: Rung): LlmfitCatalogEntry {
  return { id: r.tag, parameters_b: r.params, quant: "Q4_K_M", size_gb: r.storageGb };
}

/** Un palier peut servir : il tient vraiment et ne rampe pas. */
function usable(fit: ModelFit | undefined): boolean {
  if (!fit) return false;
  if (fit.verdict === "refuse" || fit.verdict === "risque") return false;
  return fit.tokens_per_second >= USABLE_TPS;
}

/** Choisit les trois paliers dans l'échelle, d'après les estimations. */
export function pickTiers(rungs: Rung[], fits: (ModelFit | undefined)[]): TierPlan["choices"] {
  const tps = (i: number) => fits[i]?.tokens_per_second ?? null;
  const last = rungs.length - 1;
  // Sans aucune estimation, on garde l'échelle : petit, moyen, gros.
  if (fits.every((f) => !f)) {
    const picks: [ModelTier, number][] = [
      ["fast", 0],
      ["balanced", Math.min(1, last)],
      ["smart", Math.min(3, last)],
    ];
    return picks.map(([tier, i]) => ({
      tier,
      tag: rungs[i].tag,
      storageGb: rungs[i].storageGb,
      tokensPerSecond: null,
      tooHeavy: false,
    }));
  }
  // Rapide : le plus petit, toujours proposé, même sur une machine modeste.
  const fast = 0;
  // Équilibré : le plus gros qui reste fluide.
  let balanced = fast;
  rungs.forEach((_, i) => {
    if (usable(fits[i]) && (tps(i) ?? 0) >= FLUID_TPS) balanced = i;
  });
  // Intelligent : le plus gros qui sert encore, au moins un cran au-dessus.
  let smart = balanced;
  rungs.forEach((_, i) => {
    if (usable(fits[i])) smart = Math.max(smart, i);
  });
  if (smart === balanced) smart = Math.min(last, balanced + 1);
  // Trois cartes, trois modèles différents : sur une machine modeste, les
  // plus gros restent proposés, marqués trop lourds.
  if (balanced === fast) balanced = Math.min(1, last);
  if (smart <= balanced) smart = Math.min(last, balanced + 1);
  const make = (tier: ModelTier, i: number): TierChoice => ({
    tier,
    tag: rungs[i].tag,
    storageGb: rungs[i].storageGb,
    tokensPerSecond: tps(i),
    tooHeavy: !usable(fits[i]) && i !== fast,
  });
  return [make("fast", fast), make("balanced", balanced), make("smart", smart)];
}

/** Lit la machine, estime l'échelle, et rend le plan. Jamais d'exception : sans
 *  estimation, les trois paliers restent proposés sans vitesse. */
export async function loadTierPlan(): Promise<TierPlan> {
  const rungs = ladder();
  const [hardware, fits] = await Promise.all([
    core.llmfitHardware().catch((e) => {
      console.warn("[Accueil] matériel illisible :", e);
      return null;
    }),
    core.llmfitCatalog(rungs.map(entryOf)).catch((e) => {
      console.warn("[Accueil] estimation indisponible :", e);
      return [] as ModelFit[];
    }),
  ]);
  const choices = pickTiers(rungs, fits);
  const recommended: ModelTier = choices.find((c) => c.tier === "balanced")?.tooHeavy
    ? "fast"
    : "balanced";
  return { choices, recommended, hardware };
}
