import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import type { UseThemeReturn } from "../hooks/useTheme";
import { type InstalledExtension, type Project, type TrustLevel, core } from "../lib/core";
import { PerformancePanel } from "./PerformancePanel";

type Props = {
  theme: UseThemeReturn;
  /** Called after the active provider/model changes, so the app can refresh. */
  onProviderChanged?: () => void;
  /** Open the full-page application settings (everything, not just this chat). */
  onOpenFullSettings?: () => void;
  activeCapabilities?: string[];
  activeExtensions?: InstalledExtension[];
  /** Le projet de la conversation en cours — l'onglet Permissions s'applique à lui. */
  activeProject?: Project | null;
  onTrustLevelChange?: (level: TrustLevel) => void;
  /** Après archivage, pour que l'appelant retire le projet de ses listes. */
  onProjectArchived?: (project: Project) => void;
};

type Tab = "performance" | "permissions";

const TRUST_LABELS: Record<TrustLevel, { label: string; hint: string }> = {
  trusted: {
    label: "Confiance",
    hint: "Les outils peu et moyennement risqués s'exécutent sans confirmation.",
  },
  untrusted: {
    label: "Prudent",
    hint: "Seuls les outils en lecture s'exécutent sans confirmation.",
  },
  sandbox: {
    label: "Bac à sable",
    hint: "Chaque outil demande une confirmation explicite.",
  },
};

export function SettingsPanel({
  theme,
  onOpenFullSettings,
  activeProject,
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

            {tab === "permissions" &&
              (!activeProject ? (
                <p className="locaryn-field-hint">
                  Aucun projet n'est ouvert pour cette conversation : les permissions par projet ne
                  s'appliquent qu'à une conversation liée à un dossier.
                </p>
              ) : (
                <>
                  <div className="locaryn-field">
                    <label htmlFor="perm-trust" className="locaryn-field-label">
                      Niveau de confiance — {activeProject.name}
                    </label>
                    <select
                      id="perm-trust"
                      className="locaryn-select"
                      value={activeProject.trust_level}
                      onChange={(e) => onTrustLevelChange?.(e.target.value as TrustLevel)}
                    >
                      {(Object.keys(TRUST_LABELS) as TrustLevel[]).map((level) => (
                        <option key={level} value={level}>
                          {TRUST_LABELS[level].label}
                        </option>
                      ))}
                    </select>
                    <p className="locaryn-field-hint">
                      {TRUST_LABELS[activeProject.trust_level].hint} Définit l'autonomie accordée à
                      l'agent pour exécuter des commandes et modifier vos fichiers dans ce projet.
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
              ))}
          </div>
        </div>
      </dialog>
    </>
  );
}
