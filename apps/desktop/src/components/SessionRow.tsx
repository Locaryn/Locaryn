import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import type { Session } from "../lib/core";

type Props = {
  session: Session;
  label: string;
  active: boolean;
  /** Marqueur de tête : icône pour une conversation libre, point pour l'historique groupé. */
  bullet: "chat" | "dot" | "";
  onSelect: () => void;
  onRename: (title: string) => void;
  onArchive: () => void;
  /** Projets où la ranger. Vide : l'action ne s'affiche pas. */
  projects: { id: string; name: string }[];
  onMove: (projectId: string) => void;
  /** Vrai le temps de l'animation de départ, quand elle quitte la liste. */
  leaving: boolean;
  /** Une autre conversation a été déposée sur celle-ci : les réunir. Absent,
   *  la ligne n'accepte pas de dépôt. */
  onMergeInto?: (sourceId: string) => void;
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
  onMergeInto,
}: Props) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [showSubmenu, setShowSubmenu] = useState(false);
  /** Une conversation survole celle-ci, prête à y être versée. */
  const [accueille, setAccueille] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(label);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const holdTimerRef = useRef<number | null>(null);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  useEffect(() => {
    return () => {
      if (holdTimerRef.current) window.clearTimeout(holdTimerRef.current);
    };
  }, []);

  function startHoldTimer() {
    if (editing) return;
    if (holdTimerRef.current) window.clearTimeout(holdTimerRef.current);
    holdTimerRef.current = window.setTimeout(() => {
      window.dispatchEvent(
        new CustomEvent("locaryn:session-drag-start", { detail: { id: session.id } }),
      );
    }, 280);
  }

  function clearHoldTimer() {
    if (holdTimerRef.current) {
      window.clearTimeout(holdTimerRef.current);
      holdTimerRef.current = null;
    }
  }

  // Un menu ouvert se ferme au premier clic ailleurs, ou sur Échap : sinon il
  // reste posé sur l'écran pendant qu'on fait autre chose.
  useEffect(() => {
    if (!menu) return;
    const close = () => {
      setMenu(null);
      setShowSubmenu(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setMenu(null);
        setShowSubmenu(false);
      }
    };
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
      className={`locaryn-session-row locaryn-drag-item${leaving ? " locaryn-leaving" : ""}${
        accueille ? " locaryn-session-merge" : ""
      }`}
      draggable={!editing}
      onPointerDown={startHoldTimer}
      onPointerUp={clearHoldTimer}
      onPointerCancel={clearHoldTimer}
      onMouseDown={startHoldTimer}
      onMouseUp={clearHoldTimer}
      onDragStart={(e) => {
        clearHoldTimer();
        e.dataTransfer.setData("application/locaryn-session", session.id);
        e.dataTransfer.effectAllowed = "move";
        window.dispatchEvent(
          new CustomEvent("locaryn:session-drag-start", { detail: { id: session.id } }),
        );
      }}
      onDragEnd={() => {
        clearHoldTimer();
        window.dispatchEvent(new CustomEvent("locaryn:session-drag-end"));
      }}
      // Déposer une conversation sur une autre les réunit. Le geste dit ce
      // qu'il fait : on met l'une dans l'autre, littéralement. Une ligne ne
      // s'accepte pas elle-même, et le survol se voit avant le lâcher —
      // sinon on découvre la fusion après coup.
      onDragOver={(e) => {
        if (!onMergeInto) return;
        if (!e.dataTransfer.types.includes("application/locaryn-session")) return;
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = "move";
        setAccueille(true);
      }}
      onDragLeave={() => setAccueille(false)}
      onDrop={(e) => {
        if (!onMergeInto) return;
        const source = e.dataTransfer.getData("application/locaryn-session");
        setAccueille(false);
        if (!source || source === session.id) return;
        e.preventDefault();
        e.stopPropagation();
        onMergeInto(source);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        setShowSubmenu(false);
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
          {bullet === "chat" ? (
            <Icon name="chat" size={14} />
          ) : bullet === "dot" ? (
            <span className="locaryn-history-session-bullet" aria-hidden="true" />
          ) : null}{" "}
          {label}
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
              setShowSubmenu(false);
              setDraft(label);
              setEditing(true);
            }}
          >
            Renommer
          </button>
          {projects.length > 0 && (
            <div
              className="locaryn-ctx-submenu-parent"
              onMouseEnter={() => setShowSubmenu(true)}
              onMouseLeave={() => setShowSubmenu(false)}
            >
              <button
                type="button"
                className={`locaryn-ctx-item locaryn-ctx-item-has-sub${showSubmenu ? " locaryn-active" : ""}`}
                onClick={(e) => {
                  e.stopPropagation();
                  setShowSubmenu((prev) => !prev);
                }}
              >
                <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                  <Icon name="project" size={14} />
                  <span>Ranger dans</span>
                </span>
                <span className="locaryn-ctx-arrow">▸</span>
              </button>
              {showSubmenu && (
                <div
                  className={`locaryn-ctx locaryn-ctx-sub${menu.x + 360 > window.innerWidth ? " locaryn-ctx-sub-left" : ""}`}
                  role="menu"
                >
                  <div className="locaryn-ctx-label" style={{ paddingBottom: "4px" }}>
                    Choisir un projet
                  </div>
                  {projects.map((p) => (
                    <button
                      key={p.id}
                      type="button"
                      className="locaryn-ctx-item"
                      onClick={(e) => {
                        e.stopPropagation();
                        setMenu(null);
                        setShowSubmenu(false);
                        onMove(p.id);
                      }}
                    >
                      <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                        <Icon name="project" size={13} />
                        <span
                          style={{
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {p.name}
                        </span>
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
          <button
            type="button"
            className="locaryn-ctx-item"
            onClick={() => {
              setMenu(null);
              setShowSubmenu(false);
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
