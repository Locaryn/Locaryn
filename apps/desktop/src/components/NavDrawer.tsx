import { Icon, type IconName, isCapability, isIconName } from "@locaryn/ui-core";
import { useState } from "react";
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

type NavCategory = "workspace" | "models" | "extensibility" | "system";

type NavItem = {
  id: string;
  label: string;
  icon: IconName;
  desc: string;
  category: NavCategory;
  badge?: string;
  tooltip?: string;
  /** If set, item is only rendered when at least one required capability is present. */
  requiredCapabilities?: string[];
};

const BASE_NAV_ITEMS: NavItem[] = [
  {
    id: "chat",
    label: "Chat et agent",
    icon: "chat",
    category: "workspace",
    desc: "Environnement de chat principal, exécution de code et prompts",
    tooltip: "Conversation avec les modèles d'IA, exécution d'ordres et d'outils",
  },
  {
    id: "studio",
    label: "Studio de génération",
    icon: "studio",
    category: "workspace",
    desc: "Image, vidéo, audio, musique, 3D et édition multimodale",
    badge: "Multimodal",
    tooltip: "Espace de création multimédia apporté par vos plugins d'IA actifs",
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
    tooltip: "Personnalités et agents spécialisés configurés",
    requiredCapabilities: ["figures"],
  },
  {
    id: "batch",
    label: "Batch API (-50%)",
    icon: "speed",
    category: "workspace",
    desc: "Traitement par lots asynchrone à moitié prix",
    tooltip: "Exécution de requêtes par lots pour réduire le coût et la latence",
    requiredCapabilities: ["text-analysis", "batch-api"],
  },
  {
    id: "training",
    label: "Entraînement & Oblitération",
    icon: "shield",
    category: "workspace",
    desc: "Studio d'entraînement LoRA et oblitération de modèles RepE",
    tooltip: "Fine-tuning local et modification ciblée des représentations de modèles",
    requiredCapabilities: ["model-training"],
  },
  {
    id: "models",
    label: "Catalogue de modèles",
    icon: "marketplace",
    category: "models",
    desc: "Découverte et installation de modèles locaux & HuggingFace",
    tooltip: "Recherchez et installez des modèles GGUF et spécialisés certifiés",
  },
  {
    id: "installed",
    label: "Mes modèles installés",
    icon: "models",
    category: "models",
    desc: "Gérer vos modèles locaux, ouvrir le dossier et sélection rapide",
    tooltip: "Gestion du stockage local, suppression et choix des modèles résidents",
  },
  {
    id: "extensions",
    label: "Extensions",
    icon: "extensions",
    category: "extensibility",
    badge: "Locaryn & Tierces",
    desc: "Extensions officielles, modules spécialisés, règles et compétences",
    tooltip:
      "• Extensions Locaryn : modules officiels ajoutant des fonctionnalités UI, des moteurs d'inférence (image, 3D, voix, SSH) et des compétences d'agent.\n• Packs compatibles : règles et skills Claude Code, Gemini CLI, OpenCode.\n• Noyaux : mémoires et agents alternatifs.",
  },
  {
    id: "connectors",
    label: "Connecteurs & MCP",
    icon: "server",
    category: "extensibility",
    badge: "Outils MCP",
    desc: "Serveurs MCP, bases de données et outils externes",
    tooltip:
      "• Connecteurs & MCP : passerelles techniques (STDIO/HTTP) exposant des outils et fonctions aux modèles d'IA sans modifier l'interface utilisateur.",
  },
  {
    id: "settings",
    label: "Paramètres",
    icon: "settings",
    category: "system",
    desc: "Configuration des moteurs d'inférence, thèmes et gouvernance",
    tooltip: "Réglages de performance, GPU, stockage, langues et sécurité",
  },
];

const CATEGORY_TITLES: Record<NavCategory, string> = {
  workspace: "ESPACES DE TRAVAIL & STUDIO",
  models: "MODÈLES & INTELLIGENCE",
  extensibility: "EXTENSIBILITÉ & INTÉGRATIONS",
  system: "SYSTÈME & RÉGLAGES",
};

export const NAVIGABLE_VIEWS: string[] = BASE_NAV_ITEMS.map((item) => item.id);

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
  const [hoveredTooltip, setHoveredTooltip] = useState<string | null>(null);

  if (!isOpen) return null;

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
        category: "extensibility",
        badge: "Plugin",
        tooltip: `Fonctionnalité fournie par le plugin ${ni.extensionName}`,
      },
    ];
  });

  const itemsAffiches = [...visibleItems, ...depuisSlots];

  const categoriesOrder: NavCategory[] = ["workspace", "models", "extensibility", "system"];

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
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  fontSize: "11px",
                  letterSpacing: "0.6px",
                  color: "var(--text-faint)",
                }}
              >
                <span>{CATEGORY_TITLES[cat]}</span>
                {cat === "extensibility" && (
                  <span style={{ fontSize: "10px", color: "var(--accent)" }}>
                    Plugins · Extensions · MCP
                  </span>
                )}
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
                      onMouseEnter={() => setHoveredTooltip(item.tooltip || item.desc)}
                      onMouseLeave={() => setHoveredTooltip(null)}
                      title={item.tooltip || item.desc}
                      style={{ position: "relative" }}
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
                        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                          <span className="locaryn-nav-drawer-label">{item.label}</span>
                          {item.badge && (
                            <span
                              style={{
                                fontSize: "10px",
                                padding: "1px 6px",
                                borderRadius: "10px",
                                background: "var(--surface-3, var(--border))",
                                color: "var(--text-dim)",
                                fontWeight: 500,
                              }}
                            >
                              {item.badge}
                            </span>
                          )}
                        </div>
                        <span className="locaryn-nav-drawer-desc">{item.desc}</span>
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          );
        })}

        {hoveredTooltip && (
          <div
            className="locaryn-card"
            style={{
              padding: "10px 12px",
              marginTop: "8px",
              fontSize: "12px",
              lineHeight: 1.4,
              color: "var(--text-dim)",
              background: "var(--surface-2)",
              border: "1px solid var(--border)",
              borderRadius: "var(--radius-sm)",
              whiteSpace: "pre-line",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: "6px", marginBottom: "4px" }}>
              <Icon name="extensions" size={13} />
              <strong style={{ color: "var(--text)", fontSize: "11px" }}>Aperçu &amp; Rôle</strong>
            </div>
            {hoveredTooltip}
          </div>
        )}
      </div>
    </ModalShell>
  );
}
