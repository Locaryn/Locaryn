import type { InstalledExtension } from "../../lib/core";
import { NAVIGABLE_VIEWS } from "../NavDrawer";
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
 * Rend `null` quand aucune extension active ne revendique cette vue, et aussi
 * quand la vue est un identifiant natif (`chat`, `studio`, `models`, …) :
 * plusieurs morphs déclarent encore un `nav_items` avec l'id `studio`,
 * séquelle d'un contournement pour un bogue de gate côté hôte désormais
 * corrigé (Locaryn/Locaryn#13). Sans ce garde-fou, ce même id fait toujours
 * matcher `getSlotContributions` ici, et l'écran natif (`StudioView`) se
 * retrouvait affiché côte à côte avec ce doublon — Locaryn/Locaryn#7. Un
 * identifiant réservé par l'hôte ne peut plus jamais être repris par une
 * extension, quel que soit ce qu'elle déclare.
 */
export function ExtensionScreen({
  view,
  extensions,
}: {
  view: string;
  extensions: InstalledExtension[];
}) {
  if (NAVIGABLE_VIEWS.includes(view)) return null;
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

/** Vrai si une extension active revendique cette vue — jamais pour un
 *  identifiant réservé par l'hôte, voir le garde-fou plus haut. */
export function isExtensionScreen(view: string, extensions: InstalledExtension[]): boolean {
  if (NAVIGABLE_VIEWS.includes(view)) return false;
  return getSlotContributions(extensions, "nav.drawer").some((c) => c.id === view);
}
