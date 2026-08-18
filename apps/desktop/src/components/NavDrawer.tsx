import { Icon, type IconName, isCapability, isIconName } from "@locaryn/ui-core";
import type { ConnectionMode, InstalledExtension, ProviderSummary } from "../lib/core";
import { ModalShell } from "./ModalShell";
import { getSlotContributions } from "./extensions/SlotRegistry";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  activeView: string;
  onSelectView: (view: string) => void;
  mode?: ConnectionMode;
  provider?: ProviderSummary | null;
  /** Active extension capabilities currently installed/enabled. */
  activeCapabilities?: string[];
  /** Extensions actives : leurs `nav_items` s'ajoutent au menu, sans jamais
   *  recouvrir une entrée native (une extension ne s'impose pas). */
  extensions?: InstalledExtension[];
};

type NavItem = {
  id: string;
  label: string;
  icon: IconName;
  desc: string;
  /** If set, item is only rendered when at least one required capability is present. */
  requiredCapabilities?: string[];
};

const BASE_NAV_ITEMS: NavItem[] = [
  {
    id: "chat",
    label: "Chat et agent",
    icon: "chat",
    desc: "Environnement de chat principal, exécution de code et prompts",
  },
  {
    id: "studio",
    label: "Studio de génération",
    icon: "studio",
    desc: "Image, vidéo, audio, musique, 3D et édition multimodale",
    requiredCapabilities: [
      "image-gen",
      "image-editor",
      "video-gen",
      "3d-gen",
      "voice-tts",
      "music-gen",
      "vision-ocr",
      "rag-qa",
      "translation",
      "text-analysis",
    ],
  },
  {
    id: "figures",
    label: "Figures",
    icon: "figures",
    desc: "Un rôle, ses consignes, ses conversations",
    requiredCapabilities: ["figures"],
  },
  {
    id: "installed",
    label: "Mes modèles installés",
    icon: "models",
    desc: "Gérer vos modèles locaux, ouvrir le dossier et sélection rapide",
  },
  {
    id: "models",
    label: "Catalogue de modèles",
    icon: "marketplace",
    desc: "Découverte et installation de modèles locaux & HuggingFace",
  },
  {
    id: "batch",
    label: "Batch API (-50%)",
    icon: "speed",
    desc: "Traitement par lots asynchrone à moitié prix",
    requiredCapabilities: ["text-analysis", "batch-api"],
  },
  {
    id: "training",
    label: "Entraînement et oblitération",
    icon: "shield",
    desc: "Studio d'entraînement LoRA et oblitération de modèles RepE",
    requiredCapabilities: ["model-training"],
  },
  {
    id: "extensions",
    label: "Extensions",
    icon: "extensions",
    desc: "Extensions Locaryn, plugins compatibles et noyaux",
  },
  {
    id: "connectors",
    label: "Connecteurs & MCP",
    icon: "server",
    desc: "Connexions SSH, bases de données et serveurs MCP",
  },
  {
    id: "settings",
    label: "Paramètres",
    icon: "settings",
    desc: "Configuration des moteurs d'inférence, thèmes et gouvernance",
  },
];

// Garde-fou : le socle ne référence que des capacités de la liste canonique
// (`packages/shared-types/capabilities.json`). Une entrée qui exigerait un mot
// inconnu ne pourrait jamais apparaître — autant le voir tout de suite.
const CAPACITES_HORS_CANONIQUE = BASE_NAV_ITEMS.flatMap((i) => i.requiredCapabilities ?? []).filter(
  (c) => !isCapability(c),
);
if (CAPACITES_HORS_CANONIQUE.length > 0) {
  console.warn(
    "capacités référencées par la navigation mais absentes de la liste canonique :",
    CAPACITES_HORS_CANONIQUE,
  );
}

/**
 * Les écrans qui n'existent que si une extension les apporte, et ce qu'ils
 * exigent. Dérivé de la même liste que le menu : une entrée cachée du menu et
 * un écran ouvert par un ancien clic doivent obéir à la même règle.
 */
export const CAPABILITY_GATED_VIEWS: Record<string, string[]> = Object.fromEntries(
  BASE_NAV_ITEMS.filter((i) => i.requiredCapabilities?.length).map((i) => [
    i.id,
    i.requiredCapabilities as string[],
  ]),
);

export function NavDrawer({
  isOpen,
  onClose,
  activeView,
  onSelectView,
  activeCapabilities = [],
  extensions = [],
}: Props) {
  if (!isOpen) return null;

  // Le socle d'abord : les entrées natives, filtrées par capacités. Puis ce
  // que les extensions actives déclarent — une extension ne recouvre jamais
  // un id natif, elle s'ajoute à côté.
  const visibleItems = BASE_NAV_ITEMS.filter((item) => {
    if (!item.requiredCapabilities || item.requiredCapabilities.length === 0) {
      return true;
    }
    return item.requiredCapabilities.some((cap) => activeCapabilities.includes(cap));
  });

  const pris = new Set(visibleItems.map((i) => i.id));
  const slotNavItems = getSlotContributions(extensions, "nav.drawer");
  const depuisSlots: NavItem[] = slotNavItems.flatMap((ni) => {
    if (pris.has(ni.id)) return [];
    pris.add(ni.id);
    return [
      {
        id: ni.id,
        label: ni.label || ni.id,
        icon: (isIconName(ni.icon) ? ni.icon : "extensions") as IconName,
        desc: ni.hint || `Apporté par ${ni.extensionName}`,
      },
    ];
  });

  const depuisExtensions: NavItem[] = extensions.flatMap((ext) =>
    (ext.ui?.nav_items ?? []).flatMap((ni) => {
      if (pris.has(ni.id)) return [];
      pris.add(ni.id);
      return [
        {
          id: ni.id,
          label: ni.label,
          icon: isIconName(ni.icon) ? ni.icon : "extensions",
          desc: `Apporté par ${ext.display_name || ext.name}`,
        },
      ];
    }),
  );
  const itemsAffiches = [...visibleItems, ...depuisSlots, ...depuisExtensions];

  return (
    <ModalShell
      onClose={onClose}
      overlayClassName="locaryn-nav-drawer-overlay"
      className="locaryn-nav-drawer"
      label="Navigation"
    >
      <div className="locaryn-nav-drawer-head">
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <span className="locaryn-logo-dot" />
          <strong style={{ fontSize: "15px", letterSpacing: "-0.3px" }}>Navigation</strong>
        </div>
        <button
          type="button"
          className="locaryn-icon-btn"
          onClick={onClose}
          title="Fermer le menu"
          style={{ fontSize: "16px" }}
        >
          <Icon name="close" size={16} />
        </button>
      </div>

      <div className="locaryn-nav-drawer-body">
        <span
          className="locaryn-box-variants-title"
          style={{ marginBottom: "8px", display: "block" }}
        >
          VUES PRINCIPALES
        </span>

        <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
          {itemsAffiches.map((item) => {
            const isActive = activeView === item.id;
            return (
              <button
                key={item.id}
                type="button"
                className={`locaryn-nav-drawer-item${isActive ? " locaryn-active" : ""}`}
                onClick={() => {
                  onSelectView(item.id);
                  onClose();
                }}
              >
                <span className="locaryn-nav-drawer-icon">
                  <Icon name={item.icon} />
                </span>
                <div
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "flex-start",
                    textAlign: "left",
                  }}
                >
                  <span className="locaryn-nav-drawer-label">{item.label}</span>
                  <span className="locaryn-nav-drawer-desc">{item.desc}</span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <div className="locaryn-nav-drawer-foot">
        <span style={{ fontSize: "11px", color: "var(--text-faint)" }}>
          Locaryn — la version exacte est dans Paramètres → À propos
        </span>
      </div>
    </ModalShell>
  );
}
