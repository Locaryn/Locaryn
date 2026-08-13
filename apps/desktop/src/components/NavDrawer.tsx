import { useEffect, useRef, useState } from "react";
import type { ConnectionMode, ProviderSummary } from "../lib/core";
import { ModalShell } from "./ModalShell";

// Durée de l'animation de fermeture (CSS : slideLeft 0.2s + fadeOut 0.15s) :
// on garde le tiroir monté le temps qu'elle joue, puis on le démonte.
const CLOSE_ANIM_MS = 240;

type Props = {
  isOpen: boolean;
  onClose: () => void;
  activeView: string;
  onSelectView: (view: string) => void;
  mode?: ConnectionMode;
  provider?: ProviderSummary | null;
};

const NAV_ITEMS = [
  {
    id: "chat",
    label: "Chat & Assistant Agent",
    icon: "💬",
    desc: "Environnement de chat principal, execution de code et prompts",
  },
  {
    id: "studio",
    label: "Studio de génération",
    icon: "🎨",
    desc: "Image, vidéo, audio, musique, 3D et édition multimodale",
  },
  {
    id: "installed",
    label: "Mes Modèles Installés",
    icon: "💾",
    desc: "Gérer vos modèles locaux, ouvrir le dossier et sélection rapide",
  },
  {
    id: "models",
    label: "Marketplace Modèles",
    icon: "🛒",
    desc: "Découverte et installation de modèles locaux & HuggingFace",
  },
  {
    id: "training",
    label: "Entraînement & Oblitération",
    icon: "🔓",
    desc: "Studio d'entraînement LoRA et oblitération de modèles RepE",
  },
  {
    id: "connectors",
    label: "Connecteurs & MCP",
    icon: "🔌",
    desc: "Integrations serveur distant SSH, plugins et extensions",
  },
  {
    id: "settings",
    label: "Paramètres Système",
    icon: "⚙️",
    desc: "Configuration des moteurs d'inférence, thèmes et gouvernance",
  },
];

export function NavDrawer({ isOpen, onClose, activeView, onSelectView }: Props) {
  const [isClosing, setIsClosing] = useState(false);
  const wasOpen = useRef(false);

  useEffect(() => {
    if (isOpen) {
      wasOpen.current = true;
      setIsClosing(false);
      return;
    }
    // isOpen vient de passer à false : on joue l'animation inverse avant de
    // démonter. `wasOpen` évite de la lancer au montage initial.
    if (wasOpen.current) {
      wasOpen.current = false;
      setIsClosing(true);
      const t = setTimeout(() => setIsClosing(false), CLOSE_ANIM_MS);
      return () => clearTimeout(t);
    }
  }, [isOpen]);

  if (!isOpen && !isClosing) return null;
  const closingClass = !isOpen && isClosing ? " locaryn-nav-drawer-closing" : "";

  return (
    <ModalShell
      onClose={onClose}
      overlayClassName={`locaryn-nav-drawer-overlay${closingClass}`}
      className={`locaryn-nav-drawer${closingClass}`}
      label="Navigation"
    >
      <div className="locaryn-nav-drawer-head">
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <span className="locaryn-logo-dot" />
          <strong style={{ fontSize: "15px", letterSpacing: "-0.3px" }}>Locaryn Navigation</strong>
        </div>
        <button
          type="button"
          className="locaryn-icon-btn"
          onClick={onClose}
          title="Fermer le menu"
          style={{ fontSize: "16px" }}
        >
          ✕
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
          {NAV_ITEMS.map((item) => {
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
                <span className="locaryn-nav-drawer-icon">{item.icon}</span>
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
          Locaryn Agentic Platform v0.1.0
        </span>
      </div>
    </ModalShell>
  );
}
