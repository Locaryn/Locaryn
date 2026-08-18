import type { ExtensionUiSlotContribution, InstalledExtension } from "../../lib/core";

export interface ResolvedSlotContribution extends ExtensionUiSlotContribution {
  extensionId: string;
  extensionName: string;
}

/**
 * Registre universel des points d'extension (Slots) de l'interface.
 * Découvre et trie les contributions des extensions installées et actives.
 */
export function getSlotContributions(
  extensions: InstalledExtension[],
  slotName: string,
): ResolvedSlotContribution[] {
  const results: ResolvedSlotContribution[] = [];

  for (const ext of extensions) {
    if (!ext.enabled || !ext.ui) continue;

    // 1. Contributions explicites définies dans `ui.slots`
    if (ext.ui.slots && Array.isArray(ext.ui.slots)) {
      for (const slotContrib of ext.ui.slots) {
        if (slotContrib.slot === slotName) {
          results.push({
            ...slotContrib,
            order: slotContrib.order ?? 100,
            extensionId: ext.id,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }

    // 2. Rétro-compatibilité : mapper les anciennes déclarations déclaratives
    if (slotName === "composer.toolbar" && ext.ui.composer_actions) {
      for (const ca of ext.ui.composer_actions) {
        // Éviter les doublons si déjà déclaré dans `slots`
        if (!results.some((r) => r.id === ca.id && r.extensionId === ext.id)) {
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
            extensionId: ext.id,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }

    if (slotName === "studio.tabs" && ext.ui.studio_tabs) {
      for (const tab of ext.ui.studio_tabs) {
        if (!results.some((r) => r.id === tab.id && r.extensionId === ext.id)) {
          results.push({
            id: tab.id,
            slot: "studio.tabs",
            order: 100,
            type: "action",
            label: tab.label,
            icon: tab.icon,
            extensionId: ext.id,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }

    if (slotName === "nav.drawer" && ext.ui.nav_items) {
      for (const nav of ext.ui.nav_items) {
        if (!results.some((r) => r.id === nav.id && r.extensionId === ext.id)) {
          results.push({
            id: nav.id,
            slot: "nav.drawer",
            order: 100,
            type: "action",
            label: nav.label,
            icon: nav.icon,
            extensionId: ext.id,
            extensionName: ext.display_name || ext.name,
          });
        }
      }
    }
  }

  // Tri par ordre ascendant de priorité
  return results.sort((a, b) => (a.order ?? 100) - (b.order ?? 100));
}
