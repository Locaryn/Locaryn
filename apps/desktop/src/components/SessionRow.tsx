import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import type { Session } from "../lib/core";
import { focusPremiereEntree, navigationMenu, useMenuUnique } from "../lib/menuUnique";
import { clicApresGlisser, suivreAppui } from "../lib/sessionDrag";

/** La pastille d'une conversation dans la liste. */
export type EtatSession = "attente" | "erreur" | "terminee" | null;

const PASTILLES: Record<Exclude<EtatSession, null>, { classe: string; titre: string }> = {
  attente: {
    classe: "locaryn-etat-attente",
    titre: "En pause : le modèle attend votre réponse",
  },
  erreur: {
    classe: "locaryn-etat-erreur",
    titre: "Quelque chose est cassé dans cette conversation",
  },
  terminee: {
    classe: "locaryn-etat-terminee",
    titre: "Une réponse est arrivée pendant que vous étiez ailleurs",
  },
};

type Props = {
  session: Session;
  label: string;
  active: boolean;
  /** Marqueur de tête : icône pour une conversation libre, point pour l'historique groupé. */
  bullet: "chat" | "dot" | "";
  /**
   * Ce que cette conversation attend, s'il y a quelque chose.
   *
   * `attente` : le modèle est en pause, il attend une réponse — c'est la seule
   * chose qui débloque le travail, et c'est pour cela qu'on la voit depuis la
   * liste plutôt qu'en ouvrant la conversation. `erreur` : quelque chose est
   * cassé et demande une action, pas une réponse. `terminee` : une réponse a
   * fini pendant qu'on regardait une autre conversation.
   */
  etat?: EtatSession;
  onSelect: () => void;
  onRename: (title: string) => void;
  /** Renommer avec le modèle chargé. Absent : l'action ne s'affiche pas. */
  onRenameAi?: () => Promise<void>;
  onArchive: () => void;
  /** Projets où la ranger. Vide : l'action ne s'affiche pas. */
  projects: { id: string; name: string }[];
  onMove: (projectId: string) => void;
  /** Vrai le temps de l'animation de départ, quand elle quitte la liste. */
  leaving: boolean;
  /** Elle est en main : sa place se referme, la liste se resserre, et elle
   *  revient si on la lâche dans le vide. */
  dragging?: boolean;
  /** Une conversation en main la survole, prête à y être versée. */
  dropHot?: boolean;
  /** Une autre conversation peut être déposée sur celle-ci pour les réunir
   *  (le dépôt est traité par la barre latérale). Faux : elle n'en accepte pas. */
  acceptsMerge?: boolean;
  /** Le mode sélection est ouvert : la ligne montre sa case et un clic la coche
   *  au lieu d'ouvrir la conversation. */
  selecting?: boolean;
  selected?: boolean;
  /** Cocher ou décocher. `range` : Maj enfoncée, tout ce qui sépare de la
   *  dernière case touchée. Un Ctrl+clic ou un Maj+clic ouvre aussi le mode. */
  onToggleSelect?: (range: boolean) => void;
};

/**
 * Une conversation dans la barre latérale.
 *
 * Elle se prend et se dépose (voir `lib/sessionDrag`) : dans un projet pour la
 * ranger, sur la corbeille pour l'archiver, sur une autre pour les réunir. Le clic droit ouvre les mêmes choix pour qui préfère un
 * menu, et le renommage se fait sur place — un titre écrit ici est définitif,
 * aucun modèle n'y revient.
 */
export function SessionRow({
  session,
  label,
  active,
  bullet,
  etat = null,
  onSelect,
  onRename,
  onRenameAi,
  onArchive,
  projects,
  onMove,
  leaving,
  dragging = false,
  dropHot = false,
  acceptsMerge = false,
  selecting = false,
  selected = false,
  onToggleSelect,
}: Props) {
  // Un seul menu ouvert dans toute l'application : voir `lib/menuUnique`.
  const { menu, ouvrir, fermer } = useMenuUnique<{ x: number; y: number }>();
  const [showSubmenu, setShowSubmenu] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(label);
  const [nommage, setNommage] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  // Le sous-menu se ferme avec son menu.
  useEffect(() => {
    if (!menu) setShowSubmenu(false);
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
        dragging ? " locaryn-drag-out" : ""
      }${dropHot ? " locaryn-session-merge" : ""}`}
      // Déposer une conversation sur une autre les réunit : on met l'une dans
      // l'autre, littéralement. Le survol se voit avant le lâcher, sinon on
      // découvre la fusion après coup.
      data-drop={acceptsMerge && !dragging ? "merge" : undefined}
      data-drop-id={acceptsMerge && !dragging ? session.id : undefined}
      onPointerDown={(e) => {
        if (e.button !== 0 || editing || selecting) return;
        suivreAppui(e, session.id, label);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        if (selecting) return;
        ouvrir({ x: e.clientX, y: e.clientY });
      }}
    >
      {selecting && (
        <button
          type="button"
          role="checkbox"
          aria-checked={selected}
          aria-label={`Sélectionner « ${label} »`}
          className={`locaryn-session-check${selected ? " locaryn-session-check-on" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            onToggleSelect?.(e.shiftKey);
          }}
        >
          {selected && <Icon name="check" size={12} />}
        </button>
      )}
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
          className={`locaryn-tree-item${active ? " locaryn-active" : ""}${
            selected ? " locaryn-session-selected" : ""
          }`}
          style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
          onClick={(e) => {
            // Le clic qui termine un glisser n'ouvre rien.
            if (clicApresGlisser()) return;
            // Un clic ouvre la conversation, sauf en mode sélection ou avec Ctrl,
            // Cmd ou Maj : alors il la coche.
            if (onToggleSelect && (selecting || e.ctrlKey || e.metaKey || e.shiftKey)) {
              e.preventDefault();
              onToggleSelect(e.shiftKey);
              return;
            }
            onSelect();
          }}
          onDoubleClick={() => setEditing(true)}
          title={label}
        >
          {/* Une conversation a la même icône partout, qu'elle vive seule ou
              dans un projet, sur ce poste ou lancée depuis le téléphone : deux
              dessins pour la même chose se lisaient comme deux sortes. */}
          {bullet !== "" && <Icon name="chat" size={14} />}{" "}
          <span
            className={`locaryn-session-label${nommage ? " locaryn-session-label-nommage" : ""}`}
            title={label}
          >
            {nommage ? "Nommage…" : label}
          </span>
          {session.ephemeral && <span className="locaryn-ephemeral-dot" title="Éphémère" />}
          {etat && (
            <span
              className={`locaryn-etat-pastille ${PASTILLES[etat].classe}`}
              title={PASTILLES[etat].titre}
            />
          )}
        </button>
      )}

      {menu && (
        <div
          ref={focusPremiereEntree}
          className="locaryn-ctx"
          style={{ top: menu.y, left: menu.x }}
          onClick={(e) => e.stopPropagation()}
          onKeyDown={navigationMenu}
          role="menu"
          tabIndex={-1}
        >
          <button
            type="button"
            role="menuitem"
            className="locaryn-ctx-item"
            onClick={() => {
              fermer();
              setDraft(label);
              setEditing(true);
            }}
          >
            Renommer
          </button>
          {onRenameAi && (
            <button
              type="button"
              role="menuitem"
              className="locaryn-ctx-item"
              disabled={nommage}
              onClick={() => {
                fermer();
                setNommage(true);
                void onRenameAi().finally(() => setNommage(false));
              }}
            >
              <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                <Icon name="sparkle" size={14} />
                <span>Renommer avec l'IA</span>
              </span>
            </button>
          )}
          {projects.length > 0 && (
            <div
              className="locaryn-ctx-submenu-parent"
              onMouseEnter={() => setShowSubmenu(true)}
              onMouseLeave={() => setShowSubmenu(false)}
            >
              <button
                type="button"
                role="menuitem"
                aria-haspopup="menu"
                aria-expanded={showSubmenu}
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
                      role="menuitem"
                      className="locaryn-ctx-item"
                      onClick={(e) => {
                        e.stopPropagation();
                        fermer();
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
            role="menuitem"
            className="locaryn-ctx-item"
            onClick={() => {
              fermer();
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
