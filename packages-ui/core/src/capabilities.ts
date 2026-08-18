// La liste canonique des capacités d'extension.
//
// Un seul fichier fait foi : packages/shared-types/capabilities.json. Le
// daemon le lit côté Rust (include_str!), l'ordinateur et le téléphone le
// lisent ici via @locaryn/ui-core, et la documentation y renvoie — rien ne
// peut diverger.
import capabilitiesJson from "../../../packages/shared-types/capabilities.json";

export interface Capability {
  id: string;
  label: string;
  description: string;
}

/** Toutes les capacités reconnues, dans l'ordre du fichier canonique. */
export const CAPABILITIES: Capability[] = capabilitiesJson as Capability[];

/** Les ids canoniques, pour une recherche en O(1). */
export const CAPABILITY_IDS: ReadonlySet<string> = new Set(
  CAPABILITIES.map((c) => c.id),
);

/** `true` si `id` est une capacité reconnue. */
export function isCapability(id: string): boolean {
  return CAPABILITY_IDS.has(id);
}

/** Le label français d'une capacité ; l'id brut si elle n'est pas reconnue. */
export function capabilityLabel(id: string): string {
  const c = CAPABILITIES.find((c) => c.id === id);
  return c ? c.label : id;
}
