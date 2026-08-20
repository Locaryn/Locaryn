import type { InstalledExtension } from "./core";
import type {
  ModelCategoryDefinition,
  ModelDownloadSource,
  ModelFamily,
  ModelVariant,
} from "./modelRegistry";

export interface ExtensionMarketplaceCatalog {
  categories: ModelCategoryDefinition[];
  models: ModelFamily[];
  /** Ce que chaque extension revendique parmi les poids déjà sur le disque. */
  claims: ExtensionWeightClaim[];
}

/** Les motifs par lesquels une extension reconnaît ses propres poids.
 *
 *  L'hôte énumère les fichiers que la conversation n'utilise pas sans savoir à
 *  quoi ils servent ; sans cette revendication, des modèles installés de
 *  longue date n'apparaissent nulle part. */
export interface ExtensionWeightClaim {
  extensionId: string;
  /** Fragments de nom, en minuscules. Un poids appartient à l'extension dès
   *  qu'il en contient un. */
  patterns: string[];
}

const EMPTY_CATALOG: ExtensionMarketplaceCatalog = { categories: [], models: [], claims: [] };

function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function number(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function downloads(value: unknown): ModelDownloadSource[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!item || typeof item !== "object") return [];
    const raw = item as Record<string, unknown>;
    const url = text(raw.url);
    const file = text(raw.file);
    if (
      !url ||
      !file ||
      !/^https:\/\//i.test(url) ||
      file.includes("/") ||
      file.includes("\\") ||
      file === "." ||
      file === ".." ||
      file.endsWith(".part")
    ) {
      return [];
    }
    return [{ url, file, label: text(raw.label) ?? undefined }];
  });
}

function variant(value: unknown): ModelVariant | null {
  if (!value || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  const size = text(raw.size);
  const tag = text(raw.tag);
  const params = number(raw.params);
  const storageGb = number(raw.storageGb);
  if (!size || !tag || !/^https:\/\//i.test(tag) || params == null || storageGb == null) {
    return null;
  }
  return {
    size,
    tag,
    params,
    storageGb,
    quants: strings(raw.quants),
    instruct: raw.instruct === true || undefined,
    downloads: downloads(raw.downloads),
  };
}

function flags(capabilities: string[]): Partial<ModelFamily> {
  const has = (capability: string) => capabilities.includes(capability);
  return {
    imageGen: has("image-gen") || undefined,
    imageEditing: has("image-editor") || undefined,
    tts: has("voice-tts") || undefined,
    voiceCloning: has("voice-cloning") || undefined,
    videoGen: has("video-gen") || undefined,
    musicGen: has("music-gen") || undefined,
    model3d: has("3d-gen") || undefined,
    translation: has("translation") || undefined,
    objectDetection: has("vision-ocr") || undefined,
    textAnalysis: has("text-analysis") || undefined,
    questionAnswering: has("rag-qa") || undefined,
    uncensored: has("uncensored") || undefined,
    vision: has("vision") || undefined,
    code: has("code") || undefined,
    reasoning: has("reasoning") || undefined,
    instruct: has("chat") || has("instruct") || undefined,
  };
}

/** Parse one data asset without executing extension code. Invalid rows are ignored. */
export function parseExtensionMarketplace(
  raw: string,
  extensionId: string,
): ExtensionMarketplaceCatalog {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return EMPTY_CATALOG;
  }
  if (!value || typeof value !== "object") return EMPTY_CATALOG;
  const root = value as Record<string, unknown>;
  if (root.schemaVersion !== 1) return EMPTY_CATALOG;

  const categories = Array.isArray(root.categories)
    ? root.categories.flatMap((item) => {
        if (!item || typeof item !== "object") return [];
        const category = item as Record<string, unknown>;
        const id = text(category.id);
        const label = text(category.label);
        const matches = strings(category.matches);
        if (!id || !label || matches.length === 0) return [];
        return [
          {
            id,
            label,
            icon: text(category.icon) ?? "extensions",
            matches,
            requires: strings(category.requires),
          },
        ];
      })
    : [];

  const models = Array.isArray(root.models)
    ? root.models.flatMap((item) => {
        if (!item || typeof item !== "object") return [];
        const model = item as Record<string, unknown>;
        const id = text(model.id);
        const name = text(model.name);
        const brand = text(model.brand);
        const description = text(model.description);
        const license = text(model.license);
        const releaseDate = text(model.releaseDate);
        const releaseYear = number(model.releaseYear);
        const capabilities = strings(model.capabilities);
        const variants = Array.isArray(model.variants)
          ? model.variants.map(variant).filter((item): item is ModelVariant => item != null)
          : [];
        if (
          !id ||
          !name ||
          !brand ||
          !description ||
          !license ||
          !releaseDate ||
          releaseYear == null ||
          capabilities.length === 0 ||
          variants.length === 0
        ) {
          return [];
        }
        return [
          {
            id: `${extensionId}:${id}`,
            name,
            brand,
            description,
            license,
            contextWindow: text(model.contextWindow) ?? undefined,
            releaseDate,
            releaseYear,
            finetunable: model.finetunable === true || undefined,
            marketplaceCapabilities: capabilities,
            marketplaceOwner: extensionId,
            variants,
            source: "extension",
            ...flags(capabilities),
          } satisfies ModelFamily,
        ];
      })
    : [];
  // `owns` complète les fichiers déclarés par les téléchargements : un poids
  // installé avant que l'extension existe n'est nommé nulle part ailleurs.
  const declaredFiles = models.flatMap((model) =>
    model.variants.flatMap((item) => (item.downloads ?? []).map((source) => source.file)),
  );
  const patterns = [...strings(root.owns), ...declaredFiles]
    .map((pattern) => pattern.trim().toLowerCase())
    .filter((pattern) => pattern.length >= 3);
  const claims: ExtensionWeightClaim[] =
    patterns.length > 0 ? [{ extensionId, patterns: [...new Set(patterns)] }] : [];
  return { categories, models, claims };
}

/** L'extension qui revendique ce poids, s'il en est une. */
export function claimantOf(weightName: string, claims: ExtensionWeightClaim[]): string | null {
  const lower = weightName.toLowerCase();
  return (
    claims.find((claim) => claim.patterns.some((pattern) => lower.includes(pattern)))
      ?.extensionId ?? null
  );
}

/** Load every enabled extension catalogue declared through a data slot.
 *
 *  `readAsset` est fourni par l'appelant : passez la variante qui suit
 *  l'adresse de rafraîchissement pour que le catalogue livré ne fige pas la
 *  liste des modèles à la version du paquet. */
export async function loadExtensionMarketplaces(
  extensions: InstalledExtension[],
  readAsset: (extensionId: string, path: string) => Promise<string>,
): Promise<ExtensionMarketplaceCatalog> {
  const slots = extensions.flatMap((extension) =>
    (extension.ui?.slots ?? [])
      .filter((slot) => slot.slot === "marketplace.catalogs" && slot.entry)
      .map((slot) => ({ extensionId: extension.id, entry: slot.entry as string })),
  );
  const catalogues = await Promise.all(
    slots.map(async ({ extensionId, entry }) => {
      try {
        return parseExtensionMarketplace(await readAsset(extensionId, entry), extensionId);
      } catch {
        return EMPTY_CATALOG;
      }
    }),
  );
  return {
    categories: catalogues.flatMap((catalogue) => catalogue.categories),
    models: catalogues.flatMap((catalogue) => catalogue.models),
    claims: catalogues.flatMap((catalogue) => catalogue.claims),
  };
}
