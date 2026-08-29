import { Icon, type IconName, isIconName } from "@locaryn/ui-core";
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

type NavCategory = "workspace" | "models" | "system";

export type NavItem = {
  id: string;
  label: string;
  icon: IconName;
  desc: string;
  category: NavCategory;
  /** If set, item is only rendered when at least one required capability is present. */
  requiredCapabilities?: string[];
};

/**
 * Les destinations natives.
 *
 * Le studio d'entraînement n'en fait volontairement pas partie : c'est une
 * extension qui déclare son écran (`nav.drawer`), et il se rejoint aussi
 * depuis le catalogue de modèles, sur lequel il agit. L'application n'a pas à
 * connaître son nom.
 */
const BASE_NAV_ITEMS: NavItem[] = [
  {
    id: "chat",
    label: "Chat et agent",
    icon: "chat",
    category: "workspace",
    desc: "Environnement de chat principal, exécution de code et prompts",
  },
  {
    id: "studio",
    label: "Studio de génération",
    icon: "studio",
    category: "workspace",
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
    category: "workspace",
    desc: "Un rôle, ses consignes, ses conversations",
    requiredCapabilities: ["figures"],
  },
  {
    id: "batch",
    label: "Batch API (-50%)",
    icon: "speed",
    category: "workspace",
    desc: "Traitement par lots asynchrone à moitié prix",
    requiredCapabilities: ["text-analysis", "batch-api"],
  },
  {
    id: "models",
    label: "Catalogue de modèles",
    icon: "marketplace",
    category: "models",
    desc: "Découverte et installation de modèles locaux & HuggingFace",
  },
  {
    id: "installed",
    label: "Mes modèles installés",
    icon: "models",
    category: "models",
    desc: "Gérer vos modèles locaux, ouvrir le dossier et sélection rapide",
  },
  {
    id: "settings",
    label: "Paramètres & Profil",
    icon: "settings",
    category: "system",
    desc: "Morphs, Skills, MCP, configuration moteurs, thèmes et sécurité",
  },
];

const CATEGORY_TITLES: Record<NavCategory, string> = {
  workspace: "ESPACES DE TRAVAIL & STUDIO",
  models: "MODÈLES & INTELLIGENCE",
  system: "SYSTÈME & PROFIL",
};

export const NAVIGABLE_VIEWS: string[] = BASE_NAV_ITEMS.map((item) => item.id);

export const CAPABILITY_GATED_VIEWS: Record<string, string[]> = Object.fromEntries(
  BASE_NAV_ITEMS.filter((i) => i.requiredCapabilities?.length).map((i) => [
    i.id,
    i.requiredCapabilities as string[],
  ]),
);

/**
 * Les destinations réellement offertes : les natives dont la capacité est
 * présente, puis celles qu'une extension déclare.
 *
 * Vit ici plutôt que dans le rail parce que c'est ici que la liste native est
 * décrite ; le rail l'appelle, il ne la redéclare pas.
 */
export function visibleNavItems(activeCapabilities: string[], extensions: InstalledExtension[]) {
  const natives = BASE_NAV_ITEMS.filter(
    (item) =>
      !item.requiredCapabilities?.length ||
      item.requiredCapabilities.some((cap) => activeCapabilities.includes(cap)),
  );
  const pris = new Set(natives.map((i) => i.id));
  const depuisSlots: NavItem[] = getSlotContributions(extensions, "nav.drawer").flatMap((ni) => {
    if (pris.has(ni.id)) return [];
    pris.add(ni.id);
    return [
      {
        id: ni.id,
        label: ni.label || ni.id,
        icon: (isIconName(ni.icon) ? ni.icon : "extensions") as IconName,
        desc: ni.hint || `Apporté par ${ni.extensionName}`,
        category: "workspace" as NavCategory,
      },
    ];
  });
  return [...natives, ...depuisSlots];
}

export function NavDrawer({
  isOpen,
  onClose,
  activeView,
  onSelectView,
  activeCapabilities = [],
  extensions = [],
}: Props) {
  if (!isOpen) return null;

  const itemsAffiches = visibleNavItems(activeCapabilities, extensions);
  const categoriesOrder: NavCategory[] = ["workspace", "models", "system"];

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
          <strong style={{ fontSize: "15px", letterSpacing: "-0.3px" }}>Navigation Locaryn</strong>
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
        {categoriesOrder.map((cat) => {
          const itemsInCat = itemsAffiches.filter((i) => i.category === cat);
          if (itemsInCat.length === 0) return null;

          return (
            <div key={cat} style={{ marginBottom: "16px" }}>
              <span
                className="locaryn-box-variants-title"
                style={{
                  marginBottom: "8px",
                  display: "block",
                  fontSize: "11px",
                  letterSpacing: "0.6px",
                  color: "var(--text-faint)",
                }}
              >
                {CATEGORY_TITLES[cat]}
              </span>

              <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                {itemsInCat.map((item) => {
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
                      title={item.desc}
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
                          flex: 1,
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
          );
        })}
      </div>
    </ModalShell>
  );
}
