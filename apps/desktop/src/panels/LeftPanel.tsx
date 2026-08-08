import { useEffect, useRef, useState } from "react";
import { type Project, type Session, core } from "../lib/core";
import { pickFolder } from "../lib/dialog";

type Props = {
  projects: Project[];
  sessions: Session[];
  standaloneSessions: Session[];
  activeProject: Project | null;
  activeSession: Session | null;
  onSelectProject: (p: Project | null) => void;
  onSelectSession: (s: Session) => void;
  onNewSession: (p: Project) => void;
  onNewStandaloneChat: () => void;
  onAddProject: (path: string, name: string) => void;
  onOpenProjectSettings?: (p: Project) => void;
  onDeleteSession?: (s: Session) => void;
  /** Called after a project is archived so the app can refresh its list. */
  onProjectArchived?: (p: Project) => void;
};

function sessionLabel(s: Session, index: number) {
  if (s.title) return s.title;
  const d = new Date(s.created_at);
  return `Chat ${index + 1} — ${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
}

export function LeftPanel({
  projects,
  sessions,
  standaloneSessions,
  activeProject,
  activeSession,
  onSelectProject,
  onSelectSession,
  onNewSession,
  onNewStandaloneChat,
  onAddProject,
  onOpenProjectSettings,
  onDeleteSession,
  onProjectArchived,
}: Props) {
  /** Project whose quick-actions menu is open (the ⚙ button), plus the anchor
   *  position. The menu renders `position: fixed` so the sidebar's own
   *  `overflow: auto` cannot clip it (it used to cut off the last action). */
  const [menuFor, setMenuFor] = useState<string | null>(null);
  const [menuPos, setMenuPos] = useState<{ top: number; right: number }>({ top: 0, right: 0 });
  const menuRef = useRef<HTMLDivElement | null>(null);

  function openMenu(e: React.MouseEvent, projectId: string) {
    e.stopPropagation();
    if (menuFor === projectId) {
      setMenuFor(null);
      return;
    }
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const MENU_H = 200; // approx; flip above the button when short on space
    const openUp = window.innerHeight - r.bottom < MENU_H;
    setMenuPos({
      top: openUp ? Math.max(8, r.top - MENU_H - 4) : r.bottom + 4,
      right: Math.max(8, window.innerWidth - r.right),
    });
    setMenuFor(projectId);
  }

  useEffect(() => {
    if (!menuFor) return;
    function onDown(e: MouseEvent) {
      if (!menuRef.current?.contains(e.target as Node)) setMenuFor(null);
    }
    function onEsc(e: KeyboardEvent) {
      if (e.key === "Escape") setMenuFor(null);
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onEsc);
    };
  }, [menuFor]);

  async function archive(p: Project) {
    setMenuFor(null);
    const ok = window.confirm(
      `Archiver « ${p.name} » ?\n\nLe projet disparaît de la liste. Ses conversations restent sur le disque.`,
    );
    if (!ok) return;
    try {
      await core.archiveProject(p.id);
      onProjectArchived?.(p);
    } catch (e) {
      window.alert(`Archivage impossible : ${String(e).replace(/^Error:\s*/, "")}`);
    }
  }

  async function promptAddProject() {
    const path = await pickFolder();
    if (!path) return;
    const name =
      window.prompt(
        "Nom du projet:",
        path
          .replace(/[\\/]+$/, "")
          .split(/[\\/]/)
          .pop() ?? "projet",
      ) ?? "projet";
    onAddProject(path, name);
  }

  return (
    <aside className="locaryn-left">
      <button type="button" className="locaryn-newchat-full" onClick={onNewStandaloneChat}>
        + Nouveau Chat Libre
      </button>

      {/* Standalone chats section */}
      <div className="locaryn-section-title">Conversations Libres</div>
      <ul className="locaryn-tree" style={{ marginBottom: "16px" }}>
        {standaloneSessions.length === 0 ? (
          <li className="locaryn-tree-empty">Aucune conversation libre</li>
        ) : (
          standaloneSessions.map((s, i) => (
            <li key={s.id} className="locaryn-session-row">
              <button
                type="button"
                className={`locaryn-tree-item${s.id === activeSession?.id ? " locaryn-active" : ""}`}
                style={{
                  flex: 1,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
                onClick={() => {
                  onSelectProject(null);
                  onSelectSession(s);
                }}
              >
                💬 {sessionLabel(s, i)}
              </button>
              {onDeleteSession && (
                <button
                  type="button"
                  className="locaryn-session-delete-btn"
                  title="Supprimer cette conversation libre"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteSession(s);
                  }}
                >
                  🗑
                </button>
              )}
            </li>
          ))
        )}
      </ul>

      {/* Projects section */}
      <div className="locaryn-section-title">Projets Code</div>
      <ul className="locaryn-tree">
        {projects.map((p) => {
          const isActive = p.id === activeProject?.id;
          return (
            <li key={p.id}>
              <div
                style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}
              >
                <button
                  type="button"
                  className={`locaryn-tree-item${isActive ? " locaryn-active" : ""}`}
                  style={{ flex: 1 }}
                  onClick={() => onSelectProject(p)}
                  title={p.path}
                >
                  <span className="locaryn-caret">{isActive ? "▾" : "▸"}</span> 📁 {p.name}
                </button>
                <div
                  className="locaryn-proj-menu-wrap"
                  ref={menuFor === p.id ? menuRef : undefined}
                >
                  <button
                    type="button"
                    className="locaryn-icon-btn"
                    style={{ padding: "2px 6px", fontSize: "12px" }}
                    title={`Actions sur ${p.name}`}
                    aria-haspopup="menu"
                    aria-expanded={menuFor === p.id}
                    onClick={(e) => openMenu(e, p.id)}
                  >
                    ⚙
                  </button>

                  {menuFor === p.id && (
                    <div
                      className="locaryn-proj-menu"
                      role="menu"
                      style={{ top: menuPos.top, right: menuPos.right }}
                    >
                      <div className="locaryn-proj-menu-head" title={p.path}>
                        {p.path}
                      </div>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setMenuFor(null);
                          onNewSession(p);
                        }}
                      >
                        💬 Nouvelle conversation
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setMenuFor(null);
                          core.openModelsFolder(p.path).catch(() => {});
                        }}
                      >
                        📂 Ouvrir le dossier
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setMenuFor(null);
                          navigator.clipboard?.writeText(p.path).catch(() => {});
                        }}
                      >
                        📋 Copier le chemin
                      </button>
                      {onOpenProjectSettings && (
                        <button
                          type="button"
                          role="menuitem"
                          onClick={() => {
                            setMenuFor(null);
                            onOpenProjectSettings(p);
                          }}
                        >
                          ⚙️ Paramètres du projet
                        </button>
                      )}
                      <div className="locaryn-proj-menu-sep" />
                      <button
                        type="button"
                        role="menuitem"
                        className="danger"
                        onClick={() => archive(p)}
                      >
                        🗄️ Archiver le projet
                      </button>
                    </div>
                  )}
                </div>
              </div>

              {isActive && (
                <ul>
                  {sessions.map((s, i) => (
                    <li key={s.id} className="locaryn-session-row">
                      <button
                        type="button"
                        className={`locaryn-tree-item${
                          s.id === activeSession?.id ? " locaryn-active" : ""
                        }`}
                        style={{
                          flex: 1,
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                        onClick={() => onSelectSession(s)}
                      >
                        • {sessionLabel(s, i)}
                      </button>
                      {onDeleteSession && (
                        <button
                          type="button"
                          className="locaryn-session-delete-btn"
                          title="Supprimer cette conversation"
                          onClick={(e) => {
                            e.stopPropagation();
                            onDeleteSession(s);
                          }}
                        >
                          🗑
                        </button>
                      )}
                    </li>
                  ))}
                  <li>
                    <button
                      type="button"
                      className="locaryn-tree-item locaryn-tree-new"
                      onClick={() => onNewSession(p)}
                    >
                      + session projet
                    </button>
                  </li>
                </ul>
              )}
            </li>
          );
        })}
      </ul>
      <button type="button" className="locaryn-add-btn" onClick={promptAddProject}>
        + Ajouter un projet
      </button>
    </aside>
  );
}
