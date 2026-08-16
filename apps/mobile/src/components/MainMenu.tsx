/** Les grands espaces de l'application, ceux qui méritent leur propre écran. */
export type Destination = "studio" | "extensions" | "models" | "settings";

type Props = {
  open: boolean;
  onClose: () => void;
  /** Le Studio n'existe que si une extension apporte de quoi créer. */
  canCreate: boolean;
  onGo: (d: Destination) => void;
};

/**
 * Le menu principal.
 *
 * Séparé de l'historique, et pour la même raison que sur l'ordinateur : une
 * navigation fixe et une liste qui s'allonge sans fin ne tiennent pas dans la
 * même colonne. Une feuille qui monte du bas, quatre destinations, chacune sur
 * son écran.
 */
export function MainMenu({ open, onClose, canCreate, onGo }: Props) {
  if (!open) return null;

  const destinations: { id: Destination; label: string; icon: string; note: string }[] = [
    { id: "studio", label: "Studio", icon: "🎨", note: "Images et voix" },
    { id: "extensions", label: "Extensions", icon: "🧩", note: "Ce que le serveur sait faire" },
    { id: "models", label: "Modèles", icon: "📦", note: "Ce qui est installé" },
    { id: "settings", label: "Réglages", icon: "⚙", note: "Serveur, mémoire, mise à jour" },
  ];

  return (
    <>
      <button
        type="button"
        className="lo-sheet-veil"
        aria-label="Fermer le menu"
        onClick={onClose}
      />
      <div className="lo-sheet" role="menu">
        <div className="lo-sheet-grip" />
        {destinations
          .filter((d) => d.id !== "studio" || canCreate)
          .map((d) => (
            <button key={d.id} type="button" className="lo-sheet-item" onClick={() => onGo(d.id)}>
              <span className="lo-sheet-icon">{d.icon}</span>
              <span className="lo-sheet-text">
                <span className="lo-sheet-label">{d.label}</span>
                <span className="lo-hint">{d.note}</span>
              </span>
            </button>
          ))}
      </div>
    </>
  );
}
