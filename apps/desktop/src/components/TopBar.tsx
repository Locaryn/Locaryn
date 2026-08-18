import { Icon } from "@locaryn/ui-core";
import type { ConnectionMode, ProviderSummary } from "../lib/core";
import { ExtensionSlot } from "./extensions/ExtensionSlot";

type Props = {
  activeView: string;
  onSelectView: (view: string) => void;
  mode: ConnectionMode;
  demo: boolean;
  project: string;
  provider: ProviderSummary | null;
  showPreview: boolean;
  showBottom: boolean;
  showModelConfig: boolean;
  onToggleNavDrawer: () => void;
  onTogglePreview: () => void;
  onToggleBottom: () => void;
  onToggleModelConfig: () => void;
  onSettingsClick?: () => void;
  /** Chat-scoped settings popup (model, performance). */
  onChatSettingsClick?: () => void;
  /** Start an ephemeral conversation. */
  onNewEphemeralChat?: () => void;
  /** True when current conversation is ephemeral. */
  isEphemeral?: boolean;
};

const MODE_LABEL: Record<ConnectionMode, string> = {
  auto: "Auto",
  remote: "Remote",
  local: "Local",
};

const VIEW_TITLES: Record<string, string> = {
  models: "Marketplace Modèles",
  installed: "Mes Modèles Installés",
  batch: "Batch API Studio (-50%)",
  training: "Entraînement & Oblitération",
  connectors: "Connecteurs & MCP",
  extensions: "Extensions",
  settings: "Paramètres Système",
  account: "Compte Utilisateur",
};

export function TopBar({
  activeView,
  mode,
  demo,
  project,
  provider,
  showPreview,
  showBottom,
  showModelConfig,
  onToggleNavDrawer,
  onTogglePreview,
  onToggleBottom,
  onToggleModelConfig,
  onSettingsClick,
  onChatSettingsClick,
  onNewEphemeralChat,
  isEphemeral = false,
}: Props) {
  const isChatView = activeView === "chat";

  return (
    <header className="locaryn-topbar">
      <div className="locaryn-topbar-left">
        <button
          type="button"
          className="locaryn-icon-btn locaryn-topbar-action locaryn-menu-btn"
          title="Ouvrir le menu de navigation (Marketplace, Batch API, Entraînement...)"
          aria-label="Ouvrir le menu de navigation"
          onClick={onToggleNavDrawer}
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <line x1="3" y1="12" x2="21" y2="12" />
            <line x1="3" y1="6" x2="21" y2="6" />
            <line x1="3" y1="18" x2="21" y2="18" />
          </svg>
        </button>

        <span className="locaryn-logo">
          <span className="locaryn-logo-dot" aria-hidden="true" />
          Locaryn
        </span>
        <span className="locaryn-sep" aria-hidden="true">
          /
        </span>
        <span className="locaryn-project">
          {isChatView ? project : VIEW_TITLES[activeView] || "Navigation"}
        </span>
        {demo && <span className="locaryn-demo-badge">demo</span>}
      </div>

      {/* Render chat-specific right controls ONLY when in Chat view */}
      {isChatView && (
        <div className="locaryn-topbar-right">
          {onNewEphemeralChat && (
            <button
              type="button"
              className={`locaryn-ephemeral-topbar-btn${isEphemeral ? " locaryn-ephemeral-active" : ""}`}
              onClick={onNewEphemeralChat}
              title={
                isEphemeral
                  ? "Quitter le mode éphémère et ouvrir une conversation normale"
                  : "Démarrer une conversation éphémère (rien ne sera conservé)"
              }
              style={
                isEphemeral
                  ? {
                      background: "rgba(239, 68, 68, 0.18)",
                      borderColor: "var(--danger)",
                      color: "var(--danger)",
                    }
                  : undefined
              }
            >
              <Icon name="private" size={13} />
              <span>{isEphemeral ? "Éphémère actif (cliquer pour quitter)" : "Éphémère"}</span>
            </button>
          )}

          <span className="locaryn-provider-badge" title={provider?.endpoint ?? "no model"}>
            <span
              className={`locaryn-health-dot ${provider ? "locaryn-health-ok" : "locaryn-health-off"}`}
              aria-hidden="true"
            />
            <span className="locaryn-provider-label">
              {provider
                ? `${MODE_LABEL[mode]}${provider.model ? ` · ${provider.model}` : ""}`
                : "no model"}
            </span>
          </span>

          <div className="locaryn-topbar-toggles">
            {/* Slot pour les actions ajoutées par les extensions en haut à droite */}
            <ExtensionSlot name="topbar.actions" />

            {/* Terminal / Logs icon */}
            <button
              type="button"
              className={`locaryn-icon-btn locaryn-topbar-action${showBottom ? " locaryn-icon-btn-active" : ""}`}
              title="Terminal / Journaux"
              aria-label="Ouvrir le terminal et les journaux"
              aria-pressed={showBottom}
              onClick={onToggleBottom}
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                aria-hidden="true"
              >
                <polyline points="4 17 10 11 4 5" />
                <line x1="12" y1="19" x2="20" y2="19" />
              </svg>
            </button>

            {/* Preview / Artifacts icon */}
            <button
              type="button"
              className={`locaryn-icon-btn locaryn-topbar-action${showPreview ? " locaryn-icon-btn-active" : ""}`}
              title="Aperçu des Artefacts"
              aria-label="Ouvrir l'aperçu des artefacts"
              aria-pressed={showPreview}
              onClick={onTogglePreview}
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                aria-hidden="true"
              >
                <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
                <line x1="15" y1="3" x2="15" y2="21" />
              </svg>
            </button>

            {/* Model Parameters icon */}
            <button
              type="button"
              className={`locaryn-icon-btn locaryn-topbar-action${showModelConfig ? " locaryn-icon-btn-active" : ""}`}
              title="Paramètres du modèle et de la conversation"
              aria-label="Ouvrir les paramètres du modèle et du chat"
              aria-pressed={showModelConfig}
              onClick={onChatSettingsClick ?? onToggleModelConfig}
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                aria-hidden="true"
              >
                <line x1="4" y1="21" x2="4" y2="14" />
                <line x1="4" y1="10" x2="4" y2="3" />
                <line x1="12" y1="21" x2="12" y2="12" />
                <line x1="12" y1="8" x2="12" y2="3" />
                <line x1="20" y1="21" x2="20" y2="16" />
                <line x1="20" y1="12" x2="20" y2="3" />
                <line x1="1" y1="14" x2="7" y2="14" />
                <line x1="9" y1="8" x2="15" y2="8" />
                <line x1="17" y1="16" x2="23" y2="16" />
              </svg>
            </button>

            {/* Chat Permissions Settings icon */}
            <button
              type="button"
              className="locaryn-icon-btn locaryn-topbar-action"
              title="Gouvernance et Autorisations du Chat"
              aria-label="Ouvrir la gouvernance et les autorisations du chat"
              onClick={onSettingsClick}
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                aria-hidden="true"
              >
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
            </button>
          </div>
        </div>
      )}
    </header>
  );
}
