import type { InstalledExtension } from "../../lib/core";
import { DynamicPluginWidget } from "./DynamicPluginWidget";
import { getSlotContributions } from "./SlotRegistry";

/**
 * L'écran d'une extension, en pleine page.
 *
 * Une extension qui pose une entrée dans le menu (`nav.drawer`) déclare aussi
 * l'écran qui va avec : l'application n'a pas à connaître son nom pour
 * l'afficher. C'est ce qui permet à un studio entier — l'entraînement, par
 * exemple — de quitter la navigation native sans rien perdre.
 *
 * Rend `null` quand aucune extension active ne revendique cette vue : c'est à
 * l'appelant de décider quoi montrer à la place.
 */
export function ExtensionScreen({
  view,
  extensions,
}: {
  view: string;
  extensions: InstalledExtension[];
}) {
  const contribution = getSlotContributions(extensions, "nav.drawer").find((c) => c.id === view);
  if (!contribution) return null;

  return (
    <section className="locaryn-view-container locaryn-extension-screen">
      <div className="locaryn-view-header">
        <h2>{contribution.label || contribution.id}</h2>
        <p className="locaryn-view-desc">
          {contribution.hint || `Écran apporté par ${contribution.extensionName}.`}
        </p>
      </div>
      <DynamicPluginWidget contribution={contribution} className="locaryn-extension-screen-body" />
    </section>
  );
}

/** Vrai si une extension active revendique cette vue. */
export function isExtensionScreen(view: string, extensions: InstalledExtension[]): boolean {
  return getSlotContributions(extensions, "nav.drawer").some((c) => c.id === view);
}
