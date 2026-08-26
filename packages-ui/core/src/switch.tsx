/**
 * L'interrupteur du système visuel Locaryn.
 *
 * Il se clique, et il se **tire** : au-delà de 5px de glissement, c'est la
 * direction du geste qui décide, pas le point d'arrivée. C'est pour ça que les
 * gestionnaires sont `pointerdown` / `pointermove` / `pointerup` et non
 * `click` — un `click` ne sait rien du trajet, il ne voit que la fin.
 *
 * Le filet lumineux `--stroke-top` ne va pas ici : posé sur une pastille
 * pleine, il dessine une barre blanche parasite. Il est réservé aux surfaces
 * de chrome (barre du haut, panneaux, cartes).
 */

import { useCallback, useId, useRef } from "react";

/** Au-delà de ce déplacement, le geste est un tirage, pas un clic. */
const DRAG_THRESHOLD = 5;

export interface LoSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  /** Étiquette accessible, quand aucun texte n'accompagne l'interrupteur. */
  label?: string;
  /** Identifiant du texte qui sert d'étiquette, s'il y en a un à côté. */
  labelledBy?: string;
}

export function LoSwitch({ checked, onChange, disabled, label, labelledBy }: LoSwitchProps) {
  const id = useId();
  // Le geste en cours : d'où il part, et s'il a déjà dépassé le seuil.
  const gesture = useRef<{ x: number; dragged: boolean } | null>(null);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLButtonElement>) => {
      if (disabled) return;
      gesture.current = { x: e.clientX, dragged: false };
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [disabled],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLButtonElement>) => {
      const g = gesture.current;
      if (!g || disabled) return;
      const dx = e.clientX - g.x;
      if (Math.abs(dx) < DRAG_THRESHOLD) return;
      // Le seuil est franchi : la direction décide, et une seule fois par geste.
      g.dragged = true;
      const wanted = dx > 0;
      if (wanted !== checked) onChange(wanted);
      g.x = e.clientX;
    },
    [checked, disabled, onChange],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent<HTMLButtonElement>) => {
      const g = gesture.current;
      gesture.current = null;
      if (e.currentTarget.hasPointerCapture(e.pointerId)) {
        e.currentTarget.releasePointerCapture(e.pointerId);
      }
      if (disabled) return;
      // Un geste qui n'a pas bougé est un clic : il bascule.
      if (g && !g.dragged) onChange(!checked);
    },
    [checked, disabled, onChange],
  );

  return (
    <button
      type="button"
      id={id}
      role="switch"
      aria-checked={checked}
      aria-label={labelledBy ? undefined : label}
      aria-labelledby={labelledBy}
      disabled={disabled}
      className={`lo-switch${checked ? " lo-switch-on" : ""}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        gesture.current = null;
      }}
      // Le clavier n'a pas de trajet : Espace et Entrée basculent, point.
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          if (!disabled) onChange(!checked);
        }
      }}
    >
      <span className="lo-switch-thumb" />
    </button>
  );
}
