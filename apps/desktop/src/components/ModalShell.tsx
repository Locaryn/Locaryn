// Le fond sombre d'une modale, et ce qu'il doit garantir.
//
// Chaque modale de l'application réimplémentait le même couple : un fond qui
// ferme au clic, une carte qui arrête la propagation. Aucune ne fermait sur
// Échap — une fenêtre qu'on ne peut quitter qu'à la souris n'est pas une
// fenêtre, c'est un piège pour qui navigue au clavier.
//
// Un seul endroit porte donc la règle : Échap ferme, le clic dehors ferme, le
// clic dedans ne ferme pas, et la carte s'annonce comme un vrai dialogue.

import type { CSSProperties, ReactNode } from "react";
import { useEffect } from "react";

type Props = {
  onClose: () => void;
  children: ReactNode;
  /** Classe de la carte intérieure. Le fond, lui, ne change jamais. */
  className?: string;
  style?: CSSProperties;
  /** Ce que le lecteur d'écran annonce en entrant. */
  label: string;
  /** `alertdialog` pour une décision qu'on ne peut pas remettre à plus tard :
   *  les lecteurs d'écran l'annoncent avec plus d'insistance qu'un dialogue. */
  role?: "dialog" | "alertdialog";
  /** Le tiroir de navigation n'utilise pas le même voile que les cartes. */
  overlayClassName?: string;
};

export function ModalShell({
  onClose,
  children,
  className = "locaryn-card",
  style,
  label,
  role = "dialog",
  overlayClassName = "locaryn-settings-backdrop",
}: Props) {
  // Écoute au niveau du document : Échap doit fermer quel que soit l'élément
  // qui a le focus, y compris un champ de saisie à l'intérieur de la carte.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    // Comparer la cible au conteneur remplace le `stopPropagation` que chaque
    // modale plaçait sur sa carte : un clic né à l'intérieur a une autre
    // cible, donc ne ferme rien. Un gestionnaire de moins, et plus aucune
    // poignée de clic sur du contenu non interactif.
    <div
      className={overlayClassName}
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
    >
      {/* `dialog open` plutôt qu'un div portant role="dialog" : l'élément natif
          porte déjà la sémantique, et `open` le rend visible sans réclamer le
          fond natif — celui-ci est déjà dessiné au-dessus. */}
      <dialog
        open
        className={className}
        style={style}
        aria-label={label}
        aria-modal="true"
        // `dialog` est déjà le rôle natif de l'élément : ne le répéter que
        // pour le porter à `alertdialog`, qui n'a pas d'équivalent natif.
        role={role === "alertdialog" ? "alertdialog" : undefined}
      >
        {children}
      </dialog>
    </div>
  );
}
