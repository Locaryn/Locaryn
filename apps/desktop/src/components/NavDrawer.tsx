import { Icon, type IconName } from "@locaryn/ui-core";
import type { ConnectionMode, ProviderSummary } from "../lib/core";
import { ModalShell } from "./ModalShell";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  activeView: string;
  onSelectView: (view: string) => void;
  mode?: ConnectionMode;
  provider?: ProviderSummary | null;
  /** Active extension capabilities currently installed/enabled. */
  activeCapabilities?: string[];
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
    id: "connectors",
    label: "Extensions et MCP",
    icon: "extensions",
    desc: "Intégrations serveurs distants, plugins et extensions",
  },
  {
    id: "settings",
    label: "Paramètres",
    icon: "settings",
    desc: "Configuration des moteurs d'inférence, thèmes et gouvernance",
  },
];

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
}: Props) {
  if (!isOpen) return null;

  const visibleItems = BASE_NAV_ITEMS.filter((item) => {
    if (!item.requiredCapabilities || item.requiredCapabilities.length === 0) {
      return true;
    }
    return item.requiredCapabilities.some((cap) => activeCapabilities.includes(cap));
  });

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
          {visibleItems.map((item) => {
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
