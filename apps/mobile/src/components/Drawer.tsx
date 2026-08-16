import type { Conversation } from "../lib/core";

type Props = {
  open: boolean;
  onClose: () => void;
  conversations: Conversation[] | null;
  currentId: string | null;
  onPick: (id: string) => void;
  onNew: () => void;
  /** Absent tant qu'aucune extension n'apporte de quoi créer. */
  onStudio: (() => void) | null;
  onSettings: () => void;
};

/**
 * Le tiroir de navigation.
 *
 * Même organisation que sur l'ordinateur : les conversations d'abord, puis ce
 * qu'on peut ouvrir. Elles viennent du serveur, donc ce sont exactement celles
 * de l'ordinateur — en toucher une la reprend là où elle en était, quel que
 * soit l'appareil qui l'a commencée.
 *
 * Fermé, il ne coûte rien : le panneau est décalé hors de l'écran plutôt que
 * démonté, pour que l'ouverture ne recharge pas la liste.
 */
export function Drawer({
  open,
  onClose,
  conversations,
  currentId,
  onPick,
  onNew,
  onStudio,
  onSettings,
}: Props) {
  return (
    <>
      {open && (
        <button
          type="button"
          className="lo-drawer-veil"
          aria-label="Fermer le menu"
          onClick={onClose}
        />
      )}
      <nav className={`lo-drawer${open ? " lo-drawer-open" : ""}`} aria-hidden={!open}>
        <div className="lo-drawer-head">
          <span className="lo-drawer-title">Locaryn</span>
          <button type="button" className="lo-bar-action" onClick={onNew}>
            Nouvelle
          </button>
        </div>

        <div className="lo-drawer-scroll">
          {conversations === null && <p className="lo-sub lo-pad">Chargement…</p>}
          {conversations?.length === 0 && (
            <p className="lo-sub lo-pad">Aucune conversation pour l'instant.</p>
          )}
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
        </div>

        <div className="lo-drawer-foot">
          {onStudio && (
            <button type="button" className="lo-drawer-item" onClick={onStudio}>
              Studio
            </button>
          )}
          <button type="button" className="lo-drawer-item" onClick={onSettings}>
            Réglages
          </button>
        </div>
      </nav>
    </>
  );
}
