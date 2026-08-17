import type { ReactNode } from "react";
import { Icon } from "./Icon";

type Props = {
  title: string;
  onBack: () => void;
  /** Posé à droite de la barre : une action propre à l'écran. */
  action?: ReactNode;
  children: ReactNode;
};

/**
 * L'ossature commune des écrans secondaires.
 *
 * Une seule barre, une seule façon de revenir, un titre au même endroit.
 * Chaque écran la reprenait à sa manière, et le retour finissait décentré sur
 * l'un, absent sur l'autre.
 */
export function Screen({ title, onBack, action, children }: Props) {
  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <button type="button" className="lo-back" onClick={onBack} aria-label="Revenir">
          <Icon name="back" />
        </button>
        <span className="lo-bar-title">{title}</span>
        {action}
      </div>
      <div className="lo-page">{children}</div>
    </div>
  );
}
