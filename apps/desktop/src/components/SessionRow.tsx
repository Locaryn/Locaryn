import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import type { Session } from "../lib/core";

type Props = {
  session: Session;
  label: string;
  active: boolean;
  /** Icône de tête : `chat` pour une conversation libre, rien dans un projet. */
  bullet: "chat" | "";
  onSelect: () => void;
  onRename: (title: string) => void;
  onArchive: () => void;
  /** Projets où la ranger. Vide : l'action ne s'affiche pas. */
  projects: { id: string; name: string }[];
  onMove: (projectId: string) => void;
  /** Vrai le temps de l'animation de départ, quand elle quitte la liste. */
  leaving: boolean;
};

/**
 * Une conversation dans la barre latérale.
 *
 * Elle se prend et se dépose : dans un projet pour la ranger, sur la corbeille
 * pour l'archiver. Le clic droit ouvre les mêmes choix pour qui préfère un
 * menu, et le renommage se fait sur place — un titre écrit ici est définitif,
 * aucun modèle n'y revient.
 */
export function SessionRow({
  session,
  label,
  active,
  bullet,
  onSelect,
  onRename,
  onArchive,
  projects,
  onMove,
  leaving,
}: Props) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(label);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  // Un menu ouvert se ferme au premier clic ailleurs, ou sur Échap : sinon il
  // reste posé sur l'écran pendant qu'on fait autre chose.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  function commitRename() {
    const t = draft.trim();
    setEditing(false);
    if (t && t !== label) onRename(t);
    else setDraft(label);
  }

  return (
    <li
      className={`locaryn-session-row locaryn-drag-item${leaving ? " locaryn-leaving" : ""}`}
      draggable={!editing}
      onDragStart={(e) => {
        e.dataTransfer.setData("application/locaryn-session", session.id);
        e.dataTransfer.effectAllowed = "move";
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY });
      }}
    >
      {editing ? (
        <input
          ref={inputRef}
          className="locaryn-input locaryn-session-rename"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commitRename}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitRename();
            if (e.key === "Escape") {
              setDraft(label);
              setEditing(false);
            }
          }}
        />
      ) : (
        <button
          type="button"
          className={`locaryn-tree-item${active ? " locaryn-active" : ""}`}
          style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
          onClick={onSelect}
          onDoubleClick={() => setEditing(true)}
          title={label}
        >
          {bullet === "chat" ? <Icon name="chat" size={14} /> : null} {label}
          {session.ephemeral && <span className="locaryn-ephemeral-dot" title="Éphémère" />}
        </button>
      )}

      {menu && (
        <div
          className="locaryn-ctx"
          style={{ top: menu.y, left: menu.x }}
          // Le menu ne doit pas se refermer sur son propre clic avant d'avoir
          // déclenché l'action qu'on vient de choisir.
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => e.stopPropagation()}
          role="menu"
          tabIndex={-1}
        >
          <button
            type="button"
            className="locaryn-ctx-item"
            onClick={() => {
              setMenu(null);
              setDraft(label);
              setEditing(true);
            }}
          >
            Renommer
          </button>
          {projects.length > 0 && (
            <>
              <div className="locaryn-ctx-label">Ranger dans</div>
              {projects.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className="locaryn-ctx-item"
                  onClick={() => {
                    setMenu(null);
                    onMove(p.id);
                  }}
                >
                  {p.name}
                </button>
              ))}
            </>
          )}
          <button
            type="button"
            className="locaryn-ctx-item"
            onClick={() => {
              setMenu(null);
              onArchive();
            }}
          >
            Archiver
          </button>
        </div>
      )}
    </li>
  );
}
