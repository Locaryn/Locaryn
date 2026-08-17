import { useEffect, useState } from "react";
import { type Conversation, type PhoneProject, api } from "../lib/core";

type Props = {
  open: boolean;
  onClose: () => void;
  conversations: Conversation[] | null;
  currentId: string | null;
  onPick: (id: string) => void;
  onNew: () => void;
};

/**
 * L'historique, et rien d'autre — le même tiroir que sur le téléphone, la
 * même langue, les mêmes noms. Reprendre une conversation ici la retrouve
 * sur l'ordinateur : c'est le même serveur, le même historique.
 *
 * Les conversations libres d'abord, puis les projets, chacun dépliable sur
 * les siennes — exactement comme sur le téléphone.
 */
export function Drawer({ open, onClose, conversations, currentId, onPick, onNew }: Props) {
  const [projects, setProjects] = useState<PhoneProject[] | null>(null);
  const [openProject, setOpenProject] = useState<string | null>(null);
  const [projectChats, setProjectChats] = useState<Record<string, Conversation[]>>({});

  // Les projets ne sont lus qu'à l'ouverture : un tiroir fermé n'a rien à
  // demander au serveur.
  useEffect(() => {
    if (!open || projects !== null) return;
    void api
      .listProjects()
      .then(setProjects)
      .catch(() => setProjects([]));
  }, [open, projects]);

  async function toggleProject(id: string) {
    if (openProject === id) {
      setOpenProject(null);
      return;
    }
    setOpenProject(id);
    if (projectChats[id]) return;
    try {
      const list = await api.listProjectConversations(id);
      setProjectChats((prev) => ({ ...prev, [id]: list }));
    } catch {
      setProjectChats((prev) => ({ ...prev, [id]: [] }));
    }
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

          {conversations === null && <p className="lo-sub lo-pad">Chargement…</p>}
          {conversations?.length === 0 && <p className="lo-sub lo-pad">Rien pour l'instant.</p>}
          <ul className="lo-list">
            {conversations?.map((c) => (
              <li key={c.id}>
                <button
                  type="button"
                  className={`lo-drawer-item${c.id === currentId ? " lo-drawer-item-on" : ""}`}
                  onClick={() => onPick(c.id)}
                >
                  {c.title}
                </button>
              </li>
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
                        {(projectChats[p.id] ?? []).length === 0 && (
                          <li className="lo-sub lo-pad">Aucune conversation.</li>
                        )}
                        {projectChats[p.id]?.map((c) => (
                          <li key={c.id}>
                            <button
                              type="button"
                              className={`lo-drawer-item${
                                c.id === currentId ? " lo-drawer-item-on" : ""
                              }`}
                              onClick={() => onPick(c.id)}
                            >
                              {c.title}
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      </nav>
    </>
  );
}
