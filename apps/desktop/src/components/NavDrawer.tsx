import type { ConnectionMode, ProviderSummary } from "../lib/core";

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
    id: "batch",
    label: "Batch API (-50%)",
    icon: "⚡",
    desc: "Traitement par lots asynchrone à moitié prix",
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
  if (!isOpen) return null;

  return (
    <div className="locaryn-nav-drawer-overlay" onClick={onClose}>
      <aside className="locaryn-nav-drawer" onClick={(e) => e.stopPropagation()}>
        <div className="locaryn-nav-drawer-head">
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <span className="locaryn-logo-dot" />
            <strong style={{ fontSize: "15px", letterSpacing: "-0.3px" }}>
              Locaryn Navigation
            </strong>
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
      </aside>
    </div>
  );
}
