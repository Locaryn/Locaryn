// ═════════════════════════════════════════════════════════════════════════════
// modelSafety.ts — Known NSFW / unfiltered diffusion checkpoints and LoRAs
//
// These lists come from the community taxonomy of models that are explicitly
// fine-tuned or merged to bypass safety filters. They are used for *labeling*
// and *consent gating*, not for silent deletion or censorship.
//
// Checkpoints are kept deliberately specific to avoid flagging every text model
// that happens to be labelled "uncensored". LoRAs/embeddings can be more
// generic because they are already a narrower category of adapter files.
// ═════════════════════════════════════════════════════════════════════════════

/** Specific diffusion checkpoints / merges known to be NSFW or unfiltered. */
const NSFW_CHECKPOINT_PATTERNS = [
  // SD 1.5 / SDXL community NSFW merges
  "realisticvision",
  "realistic_vision",
  "urpm",
  "uberrealistic",
  "uber_realistic",
  "ponydiffusion",
  "pony_diffusion",
  "abyssorangemix",
  "abyss_orange",
  "counterfeit",
  "chilloutmix",
  "chillout_mix",
  "majicmix",
  "majic_mix",
  // Flux derivatives marketed as uncensored/unfiltered
  "fluxuncensored",
  "flux_uncensored",
  "fluxunfiltered",
  "flux_unfiltered",
  "flux-nsfw",
  "fluxnsfw",
  // Video models with NSFW fine-tunes
  "hunyuanvideonsfw",
  "hunyuanvideo_nsfw",
  "hunyuanvideo-nsfw",
  "hunyuanvideo_nsfw",
  "wan2.1nsfw",
  "wan2.1_nsfw",
  "wan2.1-nsfw",
  "wan2.1_nsfw",
  "wan21nsfw",
];

/** Generic NSFW terms used for LoRA / embedding file names and user-supplied
 *  paths. These are appropriate for adapters because the file itself declares
 *  an NSFW intent more often than a base checkpoint does. */
const NSFW_LORA_PATTERNS = [
  "nsfw",
  "nude",
  "nudity",
  "porn",
  "porno",
  "sex",
  "sexual",
  "explicit",
  "erotic",
  "hentai",
  "furry-nsfw",
  "furrynsfw",
  "uncensored",
  "unfiltered",
  // Poses / anatomy LoRAs commonly used for adult content
  "spread_legs",
  "spreadlegs",
  "bent_over",
  "bentover",
  "ass_up",
  "assup",
  "doggy",
  "missionary",
  // Specific named NSFW LoRAs from the taxonomy
  "urpm",
  "realisticvision",
  "ponydiffusion",
  "abyssorangemix",
  "counterfeit",
];

function normalize(text: string): string {
  return text.toLowerCase().replace(/[\s_\-–—.\/\\]+/g, "");
}

/** Return true if the checkpoint/model name matches a known NSFW/unfiltered
 *  diffusion checkpoint. */
export function isNsfwCheckpoint(name: string): boolean {
  if (!name) return false;
  const norm = normalize(name);
  return NSFW_CHECKPOINT_PATTERNS.some((p) => norm.includes(p));
}

/** Return true if the LoRA/embedding path/name matches a known NSFW LoRA. */
export function isNsfwLora(name: string): boolean {
  if (!name) return false;
  const norm = normalize(name);
  return NSFW_LORA_PATTERNS.some((p) => norm.includes(p));
}

export type ModelRisk = "safe" | "uncensored" | "nsfw";

export interface ModelClassification {
  risk: ModelRisk;
  label: string;
  icon: string;
}

/** Centralized classifier for a model name/tag.
 *  - "uncensored": explicitly marketed as uncensored / heretic / abliterated.
 *  - "nsfw"      : known NSFW checkpoint / LoRA.
 *  - "safe"      : default.
 */
export function classifyModel(
  name: string,
  catalogFlags?: { uncensored?: boolean },
): ModelClassification {
  const n = name.toLowerCase();
  const isHereticOrUncensored =
    catalogFlags?.uncensored ||
    n.includes("heretic") ||
    n.includes("abliter") ||
    n.includes("uncensored") ||
    n.includes("unfiltered") ||
    n.includes("sans limite") ||
    n.includes("sans-garde-fous");

  if (isHereticOrUncensored) {
    return { risk: "uncensored", label: "Sans limite", icon: "🔓" };
  }
  if (isNsfwCheckpoint(name) || isNsfwLora(name)) {
    return { risk: "nsfw", label: "NSFW", icon: "" };
  }
  return { risk: "safe", label: "Safe", icon: "🛡️" };
}

/** Classify only by name (no catalog flag). Kept for backwards compat. */
export function classifyModelRisk(name: string): "nsfw" | "safe" {
  return isNsfwCheckpoint(name) || isNsfwLora(name) ? "nsfw" : "safe";
}

/** Human-readable label for the matched risk, or null. */
export function nsfwReason(name: string): string | null {
  if (isNsfwCheckpoint(name)) return "Checkpoint / modèle NSFW ou sans garde-fous";
  if (isNsfwLora(name)) return "LoRA / embedding NSFW";
  return null;
}
