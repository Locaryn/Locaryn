import type { ExtensionUiSlotContribution, PhoneExtension } from "../../lib/core";

export interface ResolvedSlotContribution extends ExtensionUiSlotContribution {
  extensionId: string;
  extensionName: string;
}

/** La surface sur laquelle cette interface tourne. */
export const SURFACE = "mobile";

/**
 * Cette contribution vise-t-elle la surface courante ?
 *
 * Sans `platforms`, oui. Une extension qui a conçu un grand panneau pour
 * l'ordinateur peut lui donner ici une forme à part — ou n'en donner aucune,
 * plutôt que de laisser un écran inutilisable sur un téléphone.
 */
export function targetsSurface(
  contribution: { platforms?: string[] },
  surface: string = SURFACE,
): boolean {
  const platforms = contribution.platforms;
  if (!platforms || platforms.length === 0) return true;
  return platforms.some((platform) => platform.trim().toLowerCase() === surface);
}

/**
 * Registre universel des points d'extension (Slots) pour l'application mobile.
 * Découvre et ordonne les contributions des extensions installées et actives.
 */
export function getSlotContributions(
  extensions: PhoneExtension[],
  slotName: string,
): ResolvedSlotContribution[] {
  const results: ResolvedSlotContribution[] = [];

  for (const ext of extensions) {
    if (!ext.enabled || !ext.ui) continue;

    // 1. Contributions explicites définies dans `ui.slots`
    if (ext.ui.slots && Array.isArray(ext.ui.slots)) {
      for (const slotContrib of ext.ui.slots) {
        if (slotContrib.slot === slotName && targetsSurface(slotContrib)) {
          results.push({
            ...slotContrib,
            order: slotContrib.order ?? 100,
            extensionId: ext.name,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }

    // 2. Rétro-compatibilité : mapper les anciennes déclarations
    if (slotName === "composer.toolbar" && ext.ui.composer_actions) {
      for (const ca of ext.ui.composer_actions) {
        if (!results.some((r) => r.id === ca.id && r.extensionId === ext.name)) {
          results.push({
            id: ca.id,
            slot: "composer.toolbar",
            order: 100,
            type: "button",
            label: ca.label,
            icon: ca.icon,
            hint: ca.hint,
            action: ca.action,
            value: ca.value,
            extensionId: ext.name,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }

    if (slotName === "studio.tabs" && ext.ui.studio_tabs) {
      for (const tab of ext.ui.studio_tabs) {
        if (!results.some((r) => r.id === tab.id && r.extensionId === ext.name)) {
          results.push({
            id: tab.id,
            slot: "studio.tabs",
            order: 100,
            type: "action",
            label: tab.label,
            icon: tab.icon,
            extensionId: ext.name,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }

    if (slotName === "nav.drawer" && ext.ui.nav_items) {
      for (const nav of ext.ui.nav_items) {
        if (!results.some((r) => r.id === nav.id && r.extensionId === ext.name)) {
          results.push({
            id: nav.id,
            slot: "nav.drawer",
            order: 100,
            type: "action",
            label: nav.label,
            icon: nav.icon,
            extensionId: ext.name,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }
  }

  return results.sort((a, b) => (a.order ?? 100) - (b.order ?? 100));
}
