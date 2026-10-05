import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { BrowserTab } from "../components/workspace/BrowserTab";
import { ChangesTab } from "../components/workspace/ChangesTab";
import { FilesTab } from "../components/workspace/FilesTab";
import { TerminalConsole } from "../components/workspace/TerminalConsole";
import { useSessionWorkspace } from "../hooks/useSessionWorkspace";
import {
  KINDS,
  LAUNCHER_ORDER,
  type WorkspaceKind,
  type WorkspaceState,
  type WorkspaceTab,
  activateTab,
  closeTab,
  getWorkspace,
  openTab,
  setWorkspaceOpen,
  subscribeWorkspace,
} from "../lib/workspace";
import { ModelConfigPanel } from "./ModelConfigPanel";
import { RunPanel } from "./RunPanel";

type Props = {
  sessionId: string | null;
  /** Dossier du projet actif, en attendant celui de la conversation. */
  projectPath: string | null;
};

export function useWorkspace(): WorkspaceState {
  const [state, setState] = useState(getWorkspace());
  useEffect(() => subscribeWorkspace(() => setState(getWorkspace())), []);
  return state;
}

/** Le titre d'un onglet : « Terminal 2 » quand il y en a plusieurs. */
function titre(tab: WorkspaceTab, tabs: WorkspaceTab[]): string {
  const memes = tabs.filter((t) => t.kind === tab.kind);
  const base = KINDS[tab.kind].label;
  return memes.length > 1 ? `${base} ${memes.indexOf(tab) + 1}` : base;
}

/** Le nouvel onglet : de quoi ouvrir chaque outil, avec ce qu'il fait. */
function Launcher() {
  return (
    <div className="lw-launcher">
      <p className="lw-launcher-title">Ouvrir dans l'espace de travail</p>
      <div className="lw-launcher-grid">
        {LAUNCHER_ORDER.map((kind: WorkspaceKind) => (
          <button
            key={kind}
            type="button"
            className="lw-launcher-card"
            onClick={() => openTab(kind, KINDS[kind].multiple)}
          >
            <span className="lw-launcher-icon">
              <Icon name={KINDS[kind].icon} size={18} />
            </span>
            <span className="lw-launcher-label">{KINDS[kind].label}</span>
            <span className="lw-launcher-desc">{KINDS[kind].description}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * L'espace de travail : les outils à côté de la conversation, en onglets.
 * Tous les onglets restent montés — un terminal garde sa sortie, le navigateur
 * sa page — et seul celui qu'on regarde est visible.
 */
export function WorkspacePanel({ sessionId, projectPath }: Props) {
  const { tabs, active } = useWorkspace();
  const dossier = useSessionWorkspace(sessionId, projectPath);

  function contenu(tab: WorkspaceTab, visible: boolean) {
    switch (tab.kind) {
      case "browser":
        return <BrowserTab visible={visible} />;
      case "files":
        // Une autre conversation repart de son propre dossier.
        return <FilesTab key={sessionId ?? "aucune"} sessionId={sessionId} visible={visible} />;
      case "terminal":
        return <TerminalConsole cwd={dossier} greeting={dossier ? `Dans ${dossier}` : undefined} />;
      case "changes":
        return <ChangesTab key={sessionId ?? "aucune"} sessionId={sessionId} visible={visible} />;
      case "preview":
        return <RunPanel />;
      case "model":
        return <ModelConfigPanel sessionId={sessionId} onClose={() => closeTab(tab.id)} />;
    }
  }

  return (
    <aside className="lw-panel" aria-label="Espace de travail">
      <div className="lw-tabs" role="tablist" aria-label="Onglets de l'espace de travail">
        <div className="lw-tabs-scroll">
          {tabs.map((tab) => (
            <div
              key={tab.id}
              className={`lw-tab${tab.id === active ? " lw-tab-active" : ""}`}
              role="presentation"
            >
              <button
                type="button"
                role="tab"
                aria-selected={tab.id === active}
                className="lw-tab-btn"
                onClick={() => activateTab(tab.id)}
                onAuxClick={(e) => {
                  // Clic du milieu : fermer, comme dans un navigateur.
                  if (e.button === 1) closeTab(tab.id);
                }}
              >
                <Icon name={KINDS[tab.kind].icon} size={14} />
                <span className="lw-tab-label">{titre(tab, tabs)}</span>
              </button>
              <button
                type="button"
                className="lw-tab-close"
                aria-label={`Fermer ${titre(tab, tabs)}`}
                title="Fermer l'onglet"
                onClick={() => closeTab(tab.id)}
              >
                <Icon name="x" size={12} />
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          className={`lw-icon-btn${active === null ? " lw-icon-btn-active" : ""}`}
          aria-label="Nouvel onglet"
          title="Nouvel onglet"
          onClick={() => activateTab(null)}
        >
          <Icon name="plus" size={15} />
        </button>
        <button
          type="button"
          className="lw-icon-btn"
          aria-label="Fermer l'espace de travail"
          title="Fermer l'espace de travail"
          onClick={() => setWorkspaceOpen(false)}
        >
          <Icon name="close" size={15} />
        </button>
      </div>
      <div className="lw-body">
        {active === null && <Launcher />}
        {tabs.map((tab) => (
          <div
            key={tab.id}
            className="lw-tab-panel"
            role="tabpanel"
            hidden={tab.id !== active}
            aria-label={titre(tab, tabs)}
          >
            {contenu(tab, tab.id === active)}
          </div>
        ))}
      </div>
    </aside>
  );
}
