/** Les grands espaces de l'application, ceux qui méritent leur propre écran. */
export type Destination = "studio" | "extensions" | "models" | "settings" | "figures";

type Props = {
  open: boolean;
  onClose: () => void;
  /** Le Studio n'existe que si une extension apporte de quoi créer. */
  canCreate: boolean;
  /** L'écran Figures n'existe que si une extension apporte la capacité `figures`. */
  canFigures: boolean;
  onGo: (d: Destination) => void;
};

/**
 * Le menu principal — la même feuille que sur le téléphone, les mêmes noms.
 * Séparé du chat : une navigation fixe et une conversation ne tiennent pas
 * dans la même colonne.
 */
export function MainMenu({ open, onClose, canCreate, canFigures, onGo }: Props) {
  if (!open) return null;

  const destinations: { id: Destination; label: string; note: string }[] = [
    { id: "studio", label: "Studio", note: "Images et voix" },
    { id: "figures", label: "Figures", note: "Rôles et consignes" },
    { id: "extensions", label: "Extensions", note: "Ce que le serveur sait faire" },
    { id: "models", label: "Modèles", note: "Ce qui est installé" },
    { id: "settings", label: "Réglages", note: "Serveur, profil, mémoire" },
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
          .filter((d) => (d.id !== "studio" || canCreate) && (d.id !== "figures" || canFigures))
          .map((d) => (
            <button key={d.id} type="button" className="lo-sheet-item" onClick={() => onGo(d.id)}>
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
