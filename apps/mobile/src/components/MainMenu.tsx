import { Icon, type IconName, isIconName } from "@locaryn/ui-core";
import type { PhoneExtension } from "../lib/core";
import { getSlotContributions } from "./extensions/SlotRegistry";

/**
 * Les grands espaces de l'application, ceux qui méritent leur propre écran.
 * Le chat est l'accueil, pas une destination : il n'apparaît pas dans le
 * menu, on y est déjà.
 */
export type Destination = "chat" | "studio" | "extensions" | "models" | "settings" | "figures";

export type ModelsTab = "installed" | "marketplace";

type Props = {
  open: boolean;
  onClose: () => void;
  /** Le Studio n'existe que si une extension apporte de quoi créer. */
  canCreate: boolean;
  /** L'écran Figures n'existe que si une extension apporte la capacité `figures`. */
  canFigures: boolean;
  onGo: (d: Destination | string, initialTab?: ModelsTab) => void;
  /** Extensions actives : leurs `nav_items` s'ajoutent au menu, sans jamais
   *  recouvrir une destination native. */
  extensions?: PhoneExtension[];
};

type Entree = {
  id: string;
  label: string;
  note: string;
  icon: IconName;
  /** Où aller : la destination réelle, si l'entrée n'est pas un écran. */
  destination?: Destination;
  /** L'onglet de l'écran Modèles, quand l'entrée en désigne un. */
  tab?: ModelsTab;
  /** "create" masque le Studio sans extension de création ; "figures" pareil. */
  gated?: "create" | "figures";
};

/**
 * Le menu principal — le même que celui de l'ordinateur, en plus court.
 *
 * Le bureau énumère ses vues dans un ordre précis, avec une phrase pour
 * chacune ; le téléphone reprend la même énumération, le même ordre, les mêmes
 * phrases, pour les vues qu'il a — sauf le chat, qui est l'accueil du
 * téléphone : le lister reviendrait à proposer de revenir là où on est déjà.
 * « Mes modèles installés » et « Catalogue de modèles », séparées sur le
 * bureau, sont ici deux entrées vers le même écran, qui s'ouvre sur l'onglet
 * correspondant. Les archives, les connecteurs et les réglages propres au
 * serveur vivent dans Paramètres, pas ici.
 */
export function MainMenu({ open, onClose, canCreate, canFigures, onGo, extensions = [] }: Props) {
  if (!open) return null;

  const natives: Entree[] = [
    {
      id: "studio",
      label: "Studio de génération",
      note: "Outils multimodaux fournis par les extensions",
      icon: "studio",
      gated: "create",
    },
    {
      id: "figures",
      label: "Figures",
      note: "Un rôle, ses consignes, ses conversations",
      icon: "figures",
      gated: "figures",
    },
    {
      id: "installed",
      label: "Mes modèles installés",
      note: "Gérer vos modèles locaux, ouvrir le dossier et sélection rapide",
      icon: "models",
      destination: "models",
      tab: "installed",
    },
    {
      id: "models",
      label: "Catalogue de modèles",
      note: "Découverte et installation de modèles locaux & HuggingFace",
      icon: "marketplace",
      destination: "models",
      tab: "marketplace",
    },
    {
      id: "extensions",
      label: "Extensions",
      note: "Extensions Locaryn, plugins compatibles et noyaux",
      icon: "extensions",
    },
    {
      id: "settings",
      label: "Paramètres",
      note: "Configuration des moteurs d'inférence, thèmes et gouvernance",
      icon: "settings",
    },
  ];

  const visibles = natives.filter(
    (e) => (e.gated !== "create" || canCreate) && (e.gated !== "figures" || canFigures),
  );

  const pris = new Set<string>(visibles.map((e) => e.id));
  const navSlots = getSlotContributions(extensions, "nav.drawer");
  const depuisSlots: Entree[] = navSlots.flatMap((slot) => {
    if (pris.has(slot.id)) return [];
    pris.add(slot.id);
    return [
      {
        id: slot.id,
        label: slot.label || slot.id,
        note: `Apporté par ${slot.extensionName}`,
        icon: (isIconName(slot.icon) ? slot.icon : "extensions") as IconName,
      },
    ];
  });
  const depuisExtensions: Entree[] = extensions.flatMap((ext) =>
    (ext.ui?.nav_items ?? []).flatMap((ni) => {
      if (pris.has(ni.id)) return [];
      pris.add(ni.id);
      return [
        {
          id: ni.id,
          label: ni.label,
          note: `Apporté par ${ext.display_name || ext.name}`,
          icon: (isIconName(ni.icon) ? ni.icon : "extensions") as IconName,
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
        {[...visibles, ...depuisSlots, ...depuisExtensions].map((d) => (
          <button
            key={d.id}
            type="button"
            className="lo-sheet-item"
            onClick={() => onGo(d.destination ?? d.id, d.tab)}
          >
            <span className="lo-sheet-icon">
              <Icon name={d.icon} />
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
