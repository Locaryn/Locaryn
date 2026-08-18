import { useEffect, useRef, useState } from "react";
import { type Conversation, type PhoneProject, api } from "../lib/core";

type Props = {
  open: boolean;
  onClose: () => void;
  conversations: Conversation[] | null;
  currentId: string | null;
  onPick: (id: string) => void;
  onNew: () => void;
  /** Rafraîchir la liste après une action qui change le serveur. */
  onChanged: () => void;
};

/** Ce que la feuille du bas montre, selon qui l'a ouverte. */
type Menu =
  | { kind: "chat"; chat: Conversation; archived: boolean }
  | { kind: "move"; chat: Conversation }
  | { kind: "create" }
  | null;

/**
 * Une ligne de l'historique, avec son appui prolongé.
 *
 * Le long appui est un geste à part entière : il ouvre le menu (archiver,
 * déplacer), jamais la conversation. Le clic rapide, lui, l'ouvre. Les deux
 * partagent le même bouton : un minuteur de 480 ms départage, et le geste
 * système (menu contextuel de la vue web) fait la même chose que le minuteur.
 */
function ConversationRow({
  chat,
  isCurrent,
  archived,
  onOpen,
  onLongPress,
}: {
  chat: Conversation;
  isCurrent: boolean;
  archived: boolean;
  onOpen: () => void;
  onLongPress: () => void;
}) {
  const timer = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
    };
  }, []);

  function demarrer() {
    if (timer.current !== null) return;
    timer.current = window.setTimeout(onLongPress, 480);
  }
  function annuler() {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
  }

  return (
    <button
      type="button"
      className={`lo-drawer-item${isCurrent ? " lo-drawer-item-on" : ""}`}
      onClick={onOpen}
      onTouchStart={demarrer}
      onTouchEnd={annuler}
      onTouchMove={annuler}
      onMouseDown={demarrer}
      onMouseUp={annuler}
      onMouseLeave={annuler}
      onContextMenu={(e) => {
        e.preventDefault();
        annuler();
        onLongPress();
      }}
      aria-haspopup="menu"
    >
      {chat.title}
    </button>
  );
}

/**
 * L'historique, et rien d'autre — mais un historique qu'on range.
 *
 * Ce tiroir contient ce sur quoi on travaille : les conversations libres,
 * puis les projets, chacun dépliable sur les siennes. Un appui prolongé sur
 * une conversation propose de l'archiver ou de la ranger dans un projet, et
 * les projets se créent d'ici — le serveur est le même que celui de
 * l'ordinateur, le rangement fait ici se voit là-bas.
 */
export function Drawer({
  open,
  onClose,
  conversations,
  currentId,
  onPick,
  onNew,
  onChanged,
}: Props) {
  const [projects, setProjects] = useState<PhoneProject[] | null>(null);
  const [openProject, setOpenProject] = useState<string | null>(null);
  const [projectChats, setProjectChats] = useState<Record<string, Conversation[]>>({});
  const [archived, setArchived] = useState<Conversation[] | null>(null);
  const [showArchived, setShowArchived] = useState(false);
  const [menu, setMenu] = useState<Menu>(null);
  const [newProjectName, setNewProjectName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reloadSide = () => {
    void api
      .listProjects()
      .then(setProjects)
      .catch(() => setProjects([]));
    void api
      .archivedConversations()
      .then(setArchived)
      .catch(() => setArchived([]));
  };

  // Les listes ne sont lues qu'à l'ouverture : un tiroir fermé n'a rien à
  // demander au serveur. Elles sont relues après chaque action. Les listes
  // elles-mêmes ne font pas partie des dépendances : c'est leur passage à
  // `null` (ou le retour à l'ouverture) qui décide, pas leur contenu.
  // biome-ignore lint/correctness/useExhaustiveDependencies: voir ci-dessus.
  useEffect(() => {
    if (!open) return;
    if (projects === null) {
      void api
        .listProjects()
        .then(setProjects)
        .catch(() => setProjects([]));
    }
    if (archived === null) {
      void api
        .archivedConversations()
        .then(setArchived)
        .catch(() => setArchived([]));
    }
  }, [open]);

  async function toggleProject(id: string) {
    if (openProject === id) {
      setOpenProject(null);
      return;
    }
    setOpenProject(id);
    // Relu à chaque ouverture : une conversation rangée dans le projet depuis
    // l'ordinateur doit apparaître ici, pas au prochain redémarrage.
    try {
      const list = await api.listProjectConversations(id);
      setProjectChats((prev) => ({ ...prev, [id]: list }));
    } catch {
      setProjectChats((prev) => ({ ...prev, [id]: [] }));
    }
  }

  async function archiver(chat: Conversation, archivedValue: boolean) {
    setBusy(true);
    setError(null);
    try {
      await api.archiveConversation(chat.id, archivedValue);
      setMenu(null);
      onChanged();
      reloadSide();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function deplacer(chat: Conversation, projectId: string) {
    setBusy(true);
    setError(null);
    try {
      await api.moveConversation(chat.id, projectId);
      setMenu(null);
      onChanged();
      reloadSide();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function creerProjet(depuisMenu: Menu) {
    const name = newProjectName.trim();
    if (!name || busy) return;
    setBusy(true);
    setError(null);
    try {
      const projet = await api.createProject(name);
      setNewProjectName("");
      reloadSide();
      // Créé depuis le menu d'une conversation : on l'y range tout de suite.
      if (depuisMenu?.kind === "move") {
        await api.moveConversation(depuisMenu.chat.id, projet.id);
      }
      setMenu(null);
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function rendreMenu() {
    if (!menu) return null;
    return (
      <>
        <button
          type="button"
          className="lo-sheet-veil"
          aria-label="Fermer le menu de la conversation"
          onClick={() => setMenu(null)}
        />
        <div className="lo-sheet" role="menu">
          <div className="lo-sheet-grip" />

          {menu.kind === "chat" && (
            <>
              <p className="lo-sheet-context">{menu.chat.title}</p>
              <button
                type="button"
                className="lo-sheet-item"
                disabled={busy}
                onClick={() => void archiver(menu.chat, !menu.archived)}
              >
                <span className="lo-sheet-icon">🗄</span>
                <span className="lo-sheet-text">
                  <span className="lo-sheet-label">{menu.archived ? "Restaurer" : "Archiver"}</span>
                  <span className="lo-hint">
                    {menu.archived
                      ? "Remet la conversation dans la liste"
                      : "Quitte la liste, rien n'est effacé"}
                  </span>
                </span>
              </button>
              {!menu.archived && (
                <button
                  type="button"
                  className="lo-sheet-item"
                  disabled={busy}
                  onClick={() => setMenu({ kind: "move", chat: menu.chat })}
                >
                  <span className="lo-sheet-icon">📁</span>
                  <span className="lo-sheet-text">
                    <span className="lo-sheet-label">Déplacer vers un projet…</span>
                    <span className="lo-hint">Range la conversation dans un projet</span>
                  </span>
                </button>
              )}
            </>
          )}

          {menu.kind === "move" && (
            <>
              <p className="lo-sheet-context">Déplacer « {menu.chat.title} » vers…</p>
              {projects === null && <p className="lo-sub lo-pad">Chargement…</p>}
              {projects?.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className="lo-sheet-item"
                  disabled={busy}
                  onClick={() => void deplacer(menu.chat, p.id)}
                >
                  <span className="lo-sheet-icon">📁</span>
                  <span className="lo-sheet-text">
                    <span className="lo-sheet-label">{p.name}</span>
                  </span>
                </button>
              ))}
              {projects?.length === 0 && (
                <p className="lo-sub lo-pad">Aucun projet. Créez-en un ci-dessous.</p>
              )}
              <div className="lo-sheet-create">
                <input
                  className="lo-input"
                  placeholder="Nom du nouveau projet"
                  value={newProjectName}
                  onChange={(e) => setNewProjectName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && void creerProjet(menu)}
                />
                <button
                  type="button"
                  className="lo-btn"
                  disabled={busy || !newProjectName.trim()}
                  onClick={() => void creerProjet(menu)}
                >
                  {busy ? "Création…" : "Créer et y déplacer"}
                </button>
              </div>
            </>
          )}

          {menu.kind === "create" && (
            <>
              <p className="lo-sheet-context">Nouveau projet</p>
              <div className="lo-sheet-create">
                <input
                  className="lo-input"
                  placeholder="Nom du projet"
                  value={newProjectName}
                  onChange={(e) => setNewProjectName(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && void creerProjet(menu)}
                />
                <button
                  type="button"
                  className="lo-btn"
                  disabled={busy || !newProjectName.trim()}
                  onClick={() => void creerProjet(menu)}
                >
                  {busy ? "Création…" : "Créer le projet"}
                </button>
              </div>
            </>
          )}

          {error && <p className="lo-error lo-pad">{error}</p>}

          <button
            type="button"
            className="lo-btn-small lo-sheet-close"
            onClick={() => setMenu(null)}
          >
            Annuler
          </button>
        </div>
      </>
    );
  }

  return (
    <>
      {open && (
        <button
          type="button"
          className="lo-drawer-veil"
          aria-label="Fermer l'historique"
          onClick={onClose}
        />
      )}
      <nav className={`lo-drawer${open ? " lo-drawer-open" : ""}`} aria-hidden={!open}>
        <div className="lo-drawer-head">
          <span className="lo-drawer-title">Historique</span>
        </div>

        <div className="lo-drawer-scroll">
          <button type="button" className="lo-drawer-cta" onClick={onNew}>
            Nouvelle conversation
          </button>

          {conversations === null && (
            <div className="lo-loading-row" role="status">
              <span className="lo-spinner" aria-hidden />
              <span>Chargement…</span>
            </div>
          )}
          {conversations?.length === 0 && <p className="lo-sub lo-pad">Rien pour l'instant.</p>}
          <ul className="lo-list">
            {conversations?.map((c) => (
              <ConversationRow
                key={c.id}
                chat={c}
                isCurrent={c.id === currentId}
                archived={false}
                onOpen={() => onPick(c.id)}
                onLongPress={() => setMenu({ kind: "chat", chat: c, archived: false })}
              />
            ))}
          </ul>

          {projects && projects.length > 0 && (
            <>
              <div className="lo-drawer-group-label">Projets</div>
              <ul className="lo-list">
                {projects.map((p) => (
                  <li key={p.id}>
                    <button
                      type="button"
                      className="lo-drawer-item lo-drawer-project"
                      onClick={() => void toggleProject(p.id)}
                      aria-expanded={openProject === p.id}
                    >
                      <span className="lo-drawer-caret">{openProject === p.id ? "▾" : "▸"}</span>
                      {p.name}
                    </button>
                    {openProject === p.id && (
                      <ul className="lo-list lo-list-nested">
                        {projectChats[p.id] === undefined && (
                          <li>
                            <div className="lo-loading-row" role="status">
                              <span className="lo-spinner" aria-hidden />
                              <span>Chargement…</span>
                            </div>
                          </li>
                        )}
                        {projectChats[p.id] !== undefined && projectChats[p.id].length === 0 && (
                          <li className="lo-sub lo-pad">Aucune conversation.</li>
                        )}
                        {projectChats[p.id]?.map((c) => (
                          <ConversationRow
                            key={c.id}
                            chat={c}
                            isCurrent={c.id === currentId}
                            archived={false}
                            onOpen={() => onPick(c.id)}
                            onLongPress={() => setMenu({ kind: "chat", chat: c, archived: false })}
                          />
                        ))}
                      </ul>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}

          <button
            type="button"
            className="lo-drawer-item lo-drawer-new-project"
            onClick={() => {
              setNewProjectName("");
              setError(null);
              setMenu({ kind: "create" });
            }}
          >
            + Nouveau projet
          </button>

          {archived && archived.length > 0 && (
            <>
              <button
                type="button"
                className="lo-drawer-item lo-drawer-project"
                onClick={() => setShowArchived((v) => !v)}
                aria-expanded={showArchived}
              >
                <span className="lo-drawer-caret">{showArchived ? "▾" : "▸"}</span>
                Archivées ({archived.length})
              </button>
              {showArchived && (
                <ul className="lo-list lo-list-nested">
                  {archived.map((c) => (
                    <ConversationRow
                      key={c.id}
                      chat={c}
                      isCurrent={c.id === currentId}
                      archived
                      onOpen={() => onPick(c.id)}
                      onLongPress={() => setMenu({ kind: "chat", chat: c, archived: true })}
                    />
                  ))}
                </ul>
              )}
            </>
          )}
        </div>
      </nav>

      {rendreMenu()}
    </>
  );
}
