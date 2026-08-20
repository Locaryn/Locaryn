import type { ExtensionUiSlotContribution, InstalledExtension } from "../../lib/core";

export interface ResolvedSlotContribution extends ExtensionUiSlotContribution {
  extensionId: string;
  extensionName: string;
  extensionVersion: string;
}

/** La surface sur laquelle cette interface tourne. */
export const SURFACE = "desktop";

/**
 * Cette contribution vise-t-elle la surface courante ?
 *
 * Sans `platforms`, oui — le cas courant reste une contribution unique qui
 * s'affiche partout. Une extension qui veut deux formes du même écran en
 * déclare deux, chacune ciblant sa surface : rien n'oblige un grand panneau
 * conçu pour un écran large à s'afficher tel quel sur un téléphone, et rien
 * n'oblige non plus à le priver du téléphone.
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
        if (slotContrib.slot === slotName && targetsSurface(slotContrib)) {
          results.push({
            ...slotContrib,
            order: slotContrib.order ?? 100,
            extensionId: ext.id,
            extensionName: ext.display_name || ext.name,
            extensionVersion: ext.version,
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
            extensionVersion: ext.version,
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
            extensionVersion: ext.version,
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
            extensionVersion: ext.version,
          });
        }
      }
    }
  }

  // Tri par ordre ascendant de priorité
  return results.sort((a, b) => (a.order ?? 100) - (b.order ?? 100));
}
