import { useEffect, useRef, useState } from "react";
import { SessionRow } from "../components/SessionRow";
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
  /** Rangée aux archives — la liste doit la retirer. */
  onSessionArchived?: (s: Session) => void;
  /** Déplacée dans un projet — les deux listes bougent. */
  onSessionMoved?: (s: Session, projectId: string) => void;
  /** Renommée à la main. */
  onSessionRenamed?: (s: Session, title: string) => void;
  /** Ouvrir une conversation dont rien ne sera gardé. */
  onNewEphemeralChat?: () => void;
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
  onSessionArchived,
  onSessionMoved,
  onSessionRenamed,
  onNewEphemeralChat,
}: Props) {
  /**
   * La conversation qui s'en va, le temps de l'animation.
   *
   * Sans ce délai, une conversation archivée disparaît d'un coup et on ne sait
   * pas ce qui vient de partir. Deux cents millisecondes suffisent à voir le
   * mouvement sans avoir à l'attendre.
   */
  const [leaving, setLeaving] = useState<string | null>(null);
  /** La corbeille s'allume quand on survole avec une conversation en main. */
  const [overBin, setOverBin] = useState(false);
  /** Le projet survolé pendant un glisser, pour montrer où ça va tomber. */
  const [overProject, setOverProject] = useState<string | null>(null);

  function partirPuis(s: Session, action: () => void) {
    setLeaving(s.id);
    window.setTimeout(() => {
      setLeaving(null);
      action();
    }, 200);
  }

  function sessionDeposee(e: React.DragEvent): string | null {
    const id = e.dataTransfer.getData("application/locaryn-session");
    return id || null;
  }

  const tousLesSessions = [...standaloneSessions, ...sessions];
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
            <SessionRow
              key={s.id}
              session={s}
              label={sessionLabel(s, i)}
              bullet="💬"
              active={s.id === activeSession?.id}
              leaving={leaving === s.id}
              projects={projects.map((p) => ({ id: p.id, name: p.name }))}
              onSelect={() => {
                onSelectProject(null);
                onSelectSession(s);
              }}
              onRename={(t) => onSessionRenamed?.(s, t)}
              onArchive={() => partirPuis(s, () => onSessionArchived?.(s))}
              onMove={(pid) => partirPuis(s, () => onSessionMoved?.(s, pid))}
            />
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
                  className={`locaryn-tree-item${isActive ? " locaryn-active" : ""}${
                    overProject === p.id ? " locaryn-drop-target" : ""
                  }`}
                  style={{ flex: 1 }}
                  onClick={() => onSelectProject(p)}
                  title={p.path}
                  // Déposer une conversation ici la range dans ce projet.
                  onDragOver={(e) => {
                    if (!e.dataTransfer.types.includes("application/locaryn-session")) return;
                    e.preventDefault();
                    e.dataTransfer.dropEffect = "move";
                    setOverProject(p.id);
                  }}
                  onDragLeave={() => setOverProject((cur) => (cur === p.id ? null : cur))}
                  onDrop={(e) => {
                    e.preventDefault();
                    setOverProject(null);
                    const id = sessionDeposee(e);
                    const s = tousLesSessions.find((x) => x.id === id);
                    if (s && s.project_id !== p.id) partirPuis(s, () => onSessionMoved?.(s, p.id));
                  }}
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
                    <SessionRow
                      key={s.id}
                      session={s}
                      label={sessionLabel(s, i)}
                      bullet="•"
                      active={s.id === activeSession?.id}
                      leaving={leaving === s.id}
                      projects={projects
                        .filter((x) => x.id !== p.id)
                        .map((x) => ({ id: x.id, name: x.name }))}
                      onSelect={() => onSelectSession(s)}
                      onRename={(t) => onSessionRenamed?.(s, t)}
                      onArchive={() => partirPuis(s, () => onSessionArchived?.(s))}
                      onMove={(pid) => partirPuis(s, () => onSessionMoved?.(s, pid))}
                    />
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

      {/*
        La corbeille archive, elle ne supprime pas. Retirer une conversation
        d'une liste n'est presque jamais vouloir la perdre : elle part aux
        archives, où elle reste consultable, et la suppression devient un
        second geste, pris là-bas.
      */}
      <div
        className={`locaryn-bin${overBin ? " locaryn-bin-hot" : ""}`}
        onDragOver={(e) => {
          if (!e.dataTransfer.types.includes("application/locaryn-session")) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          setOverBin(true);
        }}
        onDragLeave={() => setOverBin(false)}
        onDrop={(e) => {
          e.preventDefault();
          setOverBin(false);
          const id = sessionDeposee(e);
          const s = tousLesSessions.find((x) => x.id === id);
          if (s) partirPuis(s, () => onSessionArchived?.(s));
        }}
      >
        🗄 Déposer ici pour archiver
      </div>

      {onNewEphemeralChat && (
        <button
          type="button"
          className="locaryn-ephemeral-btn"
          onClick={onNewEphemeralChat}
          title="Rien de cette conversation ne sera gardé"
        >
          Conversation éphémère
        </button>
      )}
    </aside>
  );
}
