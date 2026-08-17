import type { ReactNode } from "react";

/** L'écran générique des destinations : un titre, un retour, le contenu. */
export function Screen({
  title,
  onBack,
  action,
  children,
}: {
  title: string;
  onBack: () => void;
  /** Un bouton à droite de la barre, comme « Nouvelle » sur le téléphone. */
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="lo-screen">
      <div className="lo-bar">
        <button type="button" className="lo-back" onClick={onBack}>
          ← Chat
        </button>
        <span className="lo-bar-title">{title}</span>
        <span className="lo-bar-spacer" />
        {action}
      </div>
      <div className="lo-screen-body">{children}</div>
    </div>
  );
}
