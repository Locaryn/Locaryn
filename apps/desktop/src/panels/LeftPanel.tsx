import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { SessionRow } from "../components/SessionRow";
import { type Project, type Session, core } from "../lib/core";
import { pickFolder } from "../lib/dialog";

type Props = {
  projects: Project[];
  /** Conversations du projet actif, conservées pour le chat et le repli. */
  sessions: Session[];
  /** Toutes les conversations, indexées par projet, pour l'arbre historique. */
  sessionsByProject?: Record<string, Session[]>;
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
  /** Une conversation a été déposée sur une autre : les réunir. */
  onSessionsMerged?: (accueil: Session, sourceId: string) => void;
};

function sessionLabel(s: Session, index: number) {
  if (s.title) return s.title;
  const d = new Date(s.created_at);
  return `Chat ${index + 1} — ${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
}

export function LeftPanel({
  projects,
  sessions,
  sessionsByProject,
  standaloneSessions,
  activeProject,
  activeSession,
  onSelectProject,
  onSelectSession,
  onNewSession,
  onNewStandaloneChat,
  onAddProject,
  onOpenProjectSettings,
  onDeleteSession: _onDeleteSession,
  onProjectArchived,
  onSessionsMerged,
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
  /** Indique si une session est en cours de glisser/maintien pour transformer le bouton du haut. */
  const [isDraggingSession, setIsDraggingSession] = useState(false);

  useEffect(() => {
    const onDragStart = () => setIsDraggingSession(true);
    const onDragEnd = () => {
      window.setTimeout(() => {
        setIsDraggingSession(false);
        setOverBin(false);
      }, 100);
    };

    window.addEventListener("locaryn:session-drag-start", onDragStart);
    window.addEventListener("locaryn:session-drag-end", onDragEnd);
    window.addEventListener("dragend", onDragEnd);
    window.addEventListener("pointerup", onDragEnd);
    window.addEventListener("mouseup", onDragEnd);

    return () => {
      window.removeEventListener("locaryn:session-drag-start", onDragStart);
      window.removeEventListener("locaryn:session-drag-end", onDragEnd);
      window.removeEventListener("dragend", onDragEnd);
      window.removeEventListener("pointerup", onDragEnd);
      window.removeEventListener("mouseup", onDragEnd);
    };
  }, []);

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

  // Les menus de déplacement doivent voir les conversations de tous les
  // groupes, pas seulement celles du projet actuellement ouvert.
  const allKnownSessions = Array.from(
    new Map(
      [...standaloneSessions, ...Object.values(sessionsByProject ?? {}).flat(), ...sessions]
        .filter((s) => !s.ephemeral)
        .map((s) => [s.id, s]),
    ).values(),
  );

  /** Project whose quick-actions menu is open (the settings button), plus the anchor
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
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onEsc);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onEsc);
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
      {/* ── Bouton Nouveau / Zone de dépôt pour archiver en cas de glisser/maintien ── */}
      {isDraggingSession ? (
        <div
          className={`locaryn-newchat-full locaryn-bin${overBin ? " locaryn-bin-hot" : ""}`}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: "6px",
            background: overBin ? "rgba(239, 68, 68, 0.25)" : "rgba(239, 68, 68, 0.12)",
            borderColor: overBin ? "var(--danger)" : "rgba(239, 68, 68, 0.4)",
            color: "var(--danger)",
            cursor: "copy",
            transition: "all 0.15s ease",
          }}
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
            setIsDraggingSession(false);
            const id = sessionDeposee(e);
            const s = allKnownSessions.find((x) => x.id === id);
            if (s) partirPuis(s, () => onSessionArchived?.(s));
          }}
        >
          <Icon name="archive" size={15} />
          <span>Déposer ici pour archiver</span>
        </div>
      ) : (
        <button
          type="button"
          className="locaryn-newchat-full"
          onClick={onNewStandaloneChat}
          style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: "6px" }}
        >
          <Icon name="chat" size={15} />
          <span>+ Nouveau</span>
        </button>
      )}

      {/* ── Espaces de travail (Projets) ── */}
      <div className="locaryn-history-title">Espaces de travail</div>

      {/* Chaque projet est un groupe avec accès rapide pour démarrer une conversation */}
      <div className="locaryn-history-groups">
        {projects.map((p) => {
          const isActive = p.id === activeProject?.id;
          const projectSessions = (sessionsByProject?.[p.id] ?? (isActive ? sessions : [])).filter(
            (s) => !s.ephemeral,
          );
          return (
            <section key={p.id} className="locaryn-history-group" style={{ marginBottom: "4px" }}>
              <div className="locaryn-history-group-head">
                <button
                  type="button"
                  className={`locaryn-history-group-button${isActive ? " locaryn-active" : ""}${
                    overProject === p.id ? " locaryn-drop-target" : ""
                  }`}
                  onClick={() => onSelectProject(p)}
                  title={p.path}
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
                    const s = allKnownSessions.find((x) => x.id === id);
                    if (s && s.project_id !== p.id) partirPuis(s, () => onSessionMoved?.(s, p.id));
                  }}
                >
                  <Icon name="project" size={14} />
                  <span className="locaryn-history-group-label">{p.name}</span>
                  {projectSessions.length > 0 && (
                    <span
                      style={{
                        fontSize: "10px",
                        padding: "1px 5px",
                        borderRadius: "8px",
                        background: "rgba(255,255,255,0.06)",
                        color: "var(--text-faint)",
                        marginLeft: "auto",
                        marginRight: "4px",
                      }}
                    >
                      {projectSessions.length}
                    </span>
                  )}
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
                    <Icon name="settings" size={14} />
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
                        <Icon name="chat" size={15} /> Nouvelle conversation
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setMenuFor(null);
                          core.openModelsFolder(p.path).catch(() => {});
                        }}
                      >
                        <Icon name="project" size={15} /> Ouvrir le dossier
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setMenuFor(null);
                          navigator.clipboard?.writeText(p.path).catch(() => {});
                        }}
                      >
                        <Icon name="check" size={15} /> Copier le chemin
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
                          <Icon name="settings" size={15} /> Paramètres du projet
                        </button>
                      )}
                      <div className="locaryn-proj-menu-sep" />
                      <button
                        type="button"
                        role="menuitem"
                        className="danger"
                        onClick={() => archive(p)}
                      >
                        <Icon name="archive" size={15} /> Archiver le projet
                      </button>
                    </div>
                  )}
                </div>
              </div>

              {isActive && (
                <div style={{ paddingLeft: "12px", marginTop: "2px" }}>
                  <button
                    type="button"
                    className="locaryn-tree-item locaryn-tree-new"
                    style={{ fontSize: "11px", marginBottom: "3px" }}
                    onClick={() => onNewSession(p)}
                  >
                    + Nouvelle session projet
                  </button>
                  {projectSessions.length > 0 && (
                    <ul className="locaryn-tree" style={{ margin: 0, padding: 0 }}>
                      {projectSessions.map((s, idx) => (
                        <SessionRow
                          key={s.id}
                          session={s}
                          label={sessionLabel(s, idx)}
                          bullet="dot"
                          active={activeSession?.id === s.id}
                          leaving={leaving === s.id}
                          projects={projects.map((proj) => ({ id: proj.id, name: proj.name }))}
                          onSelect={() => onSelectSession(s)}
                          onRename={(t) => onSessionRenamed?.(s, t)}
                          onArchive={() => partirPuis(s, () => onSessionArchived?.(s))}
                          onMove={(pid) => partirPuis(s, () => onSessionMoved?.(s, pid))}
                          onMergeInto={
                            onSessionsMerged ? (source) => onSessionsMerged(s, source) : undefined
                          }
                        />
                      ))}
                    </ul>
                  )}
                </div>
              )}
            </section>
          );
        })}
      </div>

      <button type="button" className="locaryn-add-btn" onClick={promptAddProject}>
        + Ajouter un projet
      </button>

      {/* ── Conversations (affichées sous les projets) ── */}
      <div className="locaryn-history-title" style={{ marginTop: "16px" }}>
        Conversations ({standaloneSessions.filter((s) => !s.ephemeral).length})
      </div>

      <div
        className="locaryn-history-standalone"
        style={{ display: "flex", flexDirection: "column", gap: "2px", marginBottom: "16px" }}
      >
        {standaloneSessions.filter((s) => !s.ephemeral).length === 0 ? (
          <div
            style={{
              fontSize: "11px",
              color: "var(--text-faint)",
              fontStyle: "italic",
              padding: "4px 8px",
            }}
          >
            Aucune conversation
          </div>
        ) : (
          <ul className="locaryn-tree" style={{ margin: 0, padding: 0 }}>
            {standaloneSessions
              .filter((s) => !s.ephemeral)
              .map((s, idx) => (
                <SessionRow
                  key={s.id}
                  session={s}
                  label={sessionLabel(s, idx)}
                  bullet="chat"
                  active={activeSession?.id === s.id}
                  leaving={leaving === s.id}
                  projects={projects.map((p) => ({ id: p.id, name: p.name }))}
                  onSelect={() => onSelectSession(s)}
                  onRename={(t) => onSessionRenamed?.(s, t)}
                  onArchive={() => partirPuis(s, () => onSessionArchived?.(s))}
                  onMove={(pid) => partirPuis(s, () => onSessionMoved?.(s, pid))}
                  onMergeInto={
                    onSessionsMerged ? (source) => onSessionsMerged(s, source) : undefined
                  }
                />
              ))}
          </ul>
        )}
      </div>
    </aside>
  );
}
