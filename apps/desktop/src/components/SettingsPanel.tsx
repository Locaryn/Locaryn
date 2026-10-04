import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import type { UseThemeReturn } from "../hooks/useTheme";
import {
  type InstalledExtension,
  type Project,
  type Session,
  type TrustLevel,
  core,
} from "../lib/core";
import { TRUST_LEVELS, trustInfo } from "../lib/trust";
import { PerformancePanel } from "./PerformancePanel";
import { SessionTrustControl } from "./SessionTrustControl";

type Props = {
  theme: UseThemeReturn;
  /** Called after the active provider/model changes, so the app can refresh. */
  onProviderChanged?: () => void;
  /** Open the full-page application settings (everything, not just this chat). */
  onOpenFullSettings?: () => void;
  activeCapabilities?: string[];
  activeExtensions?: InstalledExtension[];
  /** Le projet de la conversation en cours, quand il y en a un. */
  activeProject?: Project | null;
  /** La conversation ouverte : ses permissions se règlent dans l'onglet
   *  Permissions, avec ou sans projet. */
  activeSession?: Session | null;
  onTrustLevelChange?: (level: TrustLevel) => void;
  /** Après archivage, pour que l'appelant retire le projet de ses listes. */
  onProjectArchived?: (project: Project) => void;
};

type Tab = "performance" | "permissions";

export function SettingsPanel({
  theme,
  onOpenFullSettings,
  activeProject,
  activeSession,
  onTrustLevelChange,
  onProjectArchived,
}: Props) {
  const { settingsOpen, setSettingsOpen } = theme;
  const [tab, setTab] = useState<Tab>("performance");
  const [archiving, setArchiving] = useState(false);

  async function archive() {
    if (!activeProject) return;
    const ok = window.confirm(
      `Archiver « ${activeProject.name} » ?\n\nLe projet disparaît de la liste. Ses conversations restent sur le disque.`,
    );
    if (!ok) return;
    setArchiving(true);
    try {
      await core.archiveProject(activeProject.id);
      onProjectArchived?.(activeProject);
      setSettingsOpen(false);
    } catch (e) {
      window.alert(`Archivage impossible : ${String(e).replace(/^Error:\s*/, "")}`);
    } finally {
      setArchiving(false);
    }
  }

  if (!settingsOpen) return null;

  return (
    <>
      {/* Ce fond n'a aucun enfant : le panneau est positionné à part. Il ferme
          au clic, et Échap ferme quel que soit l'élément qui a le focus. */}
      <div
        className="locaryn-settings-backdrop"
        role="presentation"
        onClick={() => setSettingsOpen(false)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setSettingsOpen(false);
        }}
      />
      <dialog
        open
        className="locaryn-settings-modal"
        aria-modal="true"
        aria-label="Paramètres du chat"
      >
        <div className="locaryn-settings-header">
          <span className="locaryn-settings-title">Paramètres du chat</span>
          <button
            type="button"
            className="locaryn-settings-close"
            onClick={() => setSettingsOpen(false)}
            aria-label="Fermer les paramètres"
          >
            <Icon name="close" size={16} />
          </button>
        </div>

        <div className="locaryn-settings-main">
          <nav className="locaryn-settings-nav">
            <button
              type="button"
              className={`locaryn-nav-item${tab === "performance" ? " locaryn-active" : ""}`}
              onClick={() => setTab("performance")}
            >
              <Icon name="speed" size={15} /> Performance
            </button>
            <button
              type="button"
              className={`locaryn-nav-item${tab === "permissions" ? " locaryn-active" : ""}`}
              onClick={() => setTab("permissions")}
            >
              <Icon name="shield" size={15} /> Permissions
            </button>
            {onOpenFullSettings && (
              <button
                type="button"
                className="locaryn-settings-all"
                onClick={() => {
                  setSettingsOpen(false);
                  onOpenFullSettings();
                }}
                title="Moteur, projets, extensions, apparence, stockage…"
              >
                Tous les paramètres →
              </button>
            )}
          </nav>

          <div className="locaryn-settings-pane">
            {tab === "performance" && <PerformancePanel />}

            {tab === "permissions" && (
              <>
                <div className="locaryn-field">
                  <div className="locaryn-field-label">Cette conversation</div>
                  {activeSession ? (
                    <SessionTrustControl sessionId={activeSession.id} />
                  ) : (
                    <p className="locaryn-field-hint">
                      Aucune conversation ouverte. Envoyez un premier message : elle est créée, et
                      ses permissions se règlent ici. Les nouvelles conversations démarrent avec le
                      réglage de Réglages → Compte.
                    </p>
                  )}
                </div>

                {activeProject && (
                  <>
                    <div className="locaryn-field">
                      <label htmlFor="perm-trust" className="locaryn-field-label">
                        Niveau de confiance du projet — {activeProject.name}
                      </label>
                      <select
                        id="perm-trust"
                        className="locaryn-select"
                        value={activeProject.trust_level}
                        onChange={(e) => onTrustLevelChange?.(e.target.value as TrustLevel)}
                      >
                        {TRUST_LEVELS.map((n) => (
                          <option key={n.value} value={n.value}>
                            {n.label}
                          </option>
                        ))}
                      </select>
                      <p className="locaryn-field-hint">
                        {trustInfo(activeProject.trust_level).hint} C'est le réglage des
                        conversations de ce projet, sauf exception posée sur l'une d'elles.
                      </p>
                    </div>

                    <div className="locaryn-settings-danger-zone">
                      <div className="locaryn-field-label">Zone dangereuse</div>
                      <p className="locaryn-field-hint">
                        Archiver retire le projet de la liste ; ses conversations restent sur le
                        disque et rien n'est supprimé.
                      </p>
                      <button
                        type="button"
                        className="locaryn-btn-ghost locaryn-btn-danger"
                        disabled={archiving}
                        onClick={() => void archive()}
                      >
                        <Icon name="archive" size={15} /> Archiver « {activeProject.name} »
                      </button>
                    </div>
                  </>
                )}
              </>
            )}
          </div>
        </div>
      </dialog>
    </>
  );
}
