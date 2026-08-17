import { Icon, isIconName } from "@locaryn/ui-core";
import { type PhoneExtension } from "../lib/core";

/** Les grands espaces de l'application, ceux qui méritent leur propre écran. */
export type Destination = "studio" | "extensions" | "models" | "settings" | "figures";

type Props = {
  open: boolean;
  onClose: () => void;
  /** Le Studio n'existe que si une extension apporte de quoi créer. */
  canCreate: boolean;
  /** L'écran Figures n'existe que si une extension apporte la capacité `figures`. */
  canFigures: boolean;
  onGo: (d: Destination | string) => void;
  /** Extensions actives : leurs `nav_items` s'ajoutent au menu, sans jamais
   *  recouvrir une destination native. */
  extensions?: PhoneExtension[];
};

/**
 * Le menu principal.
 *
 * Séparé de l'historique, et pour la même raison que sur l'ordinateur : une
 * navigation fixe et une liste qui s'allonge sans fin ne tiennent pas dans la
 * même colonne. Une feuille qui monte du bas, quatre destinations, chacune sur
 * son écran.
 */
export function MainMenu({
  open,
  onClose,
  canCreate,
  canFigures,
  onGo,
  extensions = [],
}: Props) {
  if (!open) return null;

  // Le socle d'abord : les destinations natives. Puis ce que les extensions
  // actives déclarent — une extension ne recouvre jamais un id natif.
  const natives: { id: Destination; label: string; note: string }[] = [
    { id: "studio", label: "Studio", note: "Images et voix" },
    { id: "figures", label: "Figures", note: "Rôles et consignes" },
    { id: "extensions", label: "Extensions", note: "Ce que le serveur sait faire" },
    { id: "models", label: "Modèles", note: "Ce qui est installé" },
    { id: "settings", label: "Réglages", note: "Serveur, mémoire, mise à jour" },
  ];
  const visibles = natives.filter(
    (d) => (d.id !== "studio" || canCreate) && (d.id !== "figures" || canFigures),
  );

  const pris = new Set<string>(visibles.map((d) => d.id));
  const depuisExtensions = extensions.flatMap((ext) =>
    (ext.ui?.nav_items ?? []).flatMap((ni) => {
      if (pris.has(ni.id)) return [];
      pris.add(ni.id);
      return [
        {
          id: ni.id,
          label: ni.label,
          note: `Apporté par ${ext.display_name || ext.name}`,
        },
      ];
    }),
  );

  return (
    <>
      <button
        type="button"
        className="lo-sheet-veil"
        aria-label="Fermer le menu"
        onClick={onClose}
      />
      <div className="lo-sheet" role="menu">
        <div className="lo-sheet-grip" />
        {[...visibles, ...depuisExtensions].map((d) => (
          <button key={d.id} type="button" className="lo-sheet-item" onClick={() => onGo(d.id)}>
            <span className="lo-sheet-icon">
              <Icon name={isIconName(d.id) ? d.id : "extensions"} />
            </span>
            <span className="lo-sheet-text">
              <span className="lo-sheet-label">{d.label}</span>
              <span className="lo-hint">{d.note}</span>
            </span>
          </button>
        ))}
      </div>
    </>
  );
}
