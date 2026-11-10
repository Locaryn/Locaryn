import { Icon } from "@locaryn/ui-core";
import { useEffect, useState } from "react";
import { BrowserTab } from "../components/workspace/BrowserTab";
import { ChangesTab } from "../components/workspace/ChangesTab";
import { FilesTab } from "../components/workspace/FilesTab";
import { PtyTerminal } from "../components/workspace/PtyTerminal";
import { TasksTab } from "../components/workspace/TasksTab";
import { TerminalConsole } from "../components/workspace/TerminalConsole";
import { useSessionWorkspace } from "../hooks/useSessionWorkspace";
import { isTauri } from "../lib/core";
import {
  KINDS,
  LAUNCHER_ORDER,
  type WorkspaceKind,
  type WorkspaceState,
  type WorkspaceTab,
  activateTab,
  closeTab,
  getVisitedWorkspaces,
  getWorkspace,
  getWorkspaceConversation,
  openTab,
  setWorkspaceOpen,
  subscribeWorkspace,
} from "../lib/workspace";
import { ModelConfigPanel } from "./ModelConfigPanel";
import { RunPanel } from "./RunPanel";

type Props = {
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

type OngletsProps = {
  /** La conversation à qui appartiennent ces onglets. */
  sessionId: string | null;
  projectPath: string | null;
  espace: WorkspaceState;
  /** Cette conversation est affichée et son panneau ouvert. */
  affichee: boolean;
};

/**
 * Les onglets d'une conversation. Ils restent montés quand on en change — un
 * terminal garde sa sortie, le navigateur sa page que le modèle continue de
 * lire — et seul l'onglet actif de la conversation affichée est visible.
 */
function OngletsConversation({ sessionId, projectPath, espace, affichee }: OngletsProps) {
  const dossier = useSessionWorkspace(sessionId, projectPath);

  function contenu(tab: WorkspaceTab, visible: boolean) {
    switch (tab.kind) {
      case "browser":
        return <BrowserTab visible={visible} sessionId={sessionId} />;
      case "files":
        return <FilesTab sessionId={sessionId} visible={visible} />;
      case "terminal":
        return isTauri ? (
          <PtyTerminal cwd={dossier} ongletId={tab.id} visible={visible} />
        ) : (
          <TerminalConsole cwd={dossier} greeting={dossier ? `Dans ${dossier}` : undefined} />
        );
      case "changes":
        return <ChangesTab sessionId={sessionId} visible={visible} />;
      case "preview":
        return <RunPanel />;
      case "model":
        return <ModelConfigPanel sessionId={sessionId} onClose={() => closeTab(tab.id)} />;
      case "tasks":
        return <TasksTab />;
    }
  }

  return (
    <>
      {espace.tabs.map((tab) => {
        const visible = affichee && tab.id === espace.active;
        return (
          <div
            key={tab.id}
            className="lw-tab-panel"
            role="tabpanel"
            hidden={!visible}
            aria-label={titre(tab, espace.tabs)}
          >
            {contenu(tab, visible)}
          </div>
        );
      })}
    </>
  );
}

/** L'espace de travail : les outils à côté de la conversation, en onglets. */
export function WorkspacePanel({ projectPath }: Props) {
  const { tabs, active, open } = useWorkspace();
  const affichee = getWorkspaceConversation();

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
        {getVisitedWorkspaces().map(([conversation, espace]) => (
          <OngletsConversation
            key={conversation}
            sessionId={conversation === "" ? null : conversation}
            projectPath={projectPath}
            espace={espace}
            affichee={open && conversation === affichee}
          />
        ))}
      </div>
    </aside>
  );
}
