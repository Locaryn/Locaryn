/**
 * Les étapes d'un appairage, à côté du QR pendant qu'il se fabrique.
 *
 * Roue sur l'étape courante, coche sur les précédentes, texte éteint sur les
 * suivantes. Les étapes ne sont pas décoratives : ce sont celles que le
 * service enchaîne réellement. Elles s'égrènent sur un rythme fixe, mais le QR
 * n'apparaît que lorsque le service a répondu *et* que le compte est terminé —
 * jamais avant, pour ne pas afficher un code sous une étape non franchie.
 */

import { Icon, LoSpinner } from "@locaryn/ui-core";
import { useEffect, useState } from "react";

export interface PairingStep {
  label: string;
  /** Durée de cette étape, en millisecondes. */
  ms: number;
}

/** Réseau local : mDNS évite la saisie, le reste est de la cryptographie. */
export const LOCAL_STEPS: PairingStep[] = [
  { label: "Découverte sur le réseau", ms: 500 },
  { label: "Paire de clés", ms: 600 },
  { label: "Signature", ms: 700 },
];

/** Accès distant : il faut d'abord savoir si le port répond. */
export const REMOTE_STEPS: PairingStep[] = [
  { label: "Vérification du port 7443", ms: 900 },
  { label: "Paire de clés", ms: 700 },
  { label: "Signature du certificat", ms: 900 },
  { label: "Publication du point d'accès", ms: 900 },
];

/**
 * L'index de l'étape en cours, qui avance seul jusqu'au bout de la liste.
 * Rend `steps.length` quand tout est franchi.
 */
export function useStepProgress(steps: PairingStep[], running: boolean): number {
  const [index, setIndex] = useState(0);

  useEffect(() => {
    if (!running) {
      setIndex(0);
      return;
    }
    let cancelled = false;
    let timer = 0;
    const advance = (i: number) => {
      if (cancelled || i >= steps.length) return;
      timer = window.setTimeout(() => {
        if (cancelled) return;
        setIndex(i + 1);
        advance(i + 1);
      }, steps[i].ms);
    };
    setIndex(0);
    advance(0);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [steps, running]);

  return index;
}

export function PairingStepList({ steps, current }: { steps: PairingStep[]; current: number }) {
  return (
    <ol className="locaryn-pair-steps" aria-live="polite">
      {steps.map((s, i) => {
        const done = i < current;
        const now = i === current;
        return (
          <li
            key={s.label}
            className={`locaryn-pair-step${done ? " is-done" : ""}${now ? " is-now" : ""}`}
          >
            <span className="locaryn-pair-step-mark">
              {done ? <Icon name="check" size={13} /> : now ? <LoSpinner size="sm" /> : "·"}
            </span>
            {s.label}
          </li>
        );
      })}
    </ol>
  );
}

/**
 * La grille du QR pendant qu'il se fabrique : un damier qui pulse en vagues.
 * Le délai de chaque cellule vient de sa position, ce qui fait courir la vague
 * en diagonale plutôt que de faire clignoter tout le carré ensemble.
 */
export function PairingCheckerboard({ size = 13 }: { size?: number }) {
  return (
    <div
      className="locaryn-pair-grid"
      style={{ gridTemplateColumns: `repeat(${size}, 1fr)` }}
      aria-hidden="true"
    >
      {Array.from({ length: size * size }, (_, i) => {
        const row = Math.floor(i / size);
        const col = i % size;
        return (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: la grille est un damier fixe — la position EST l'identité de la cellule.
            key={i}
            className="locaryn-pair-cell"
            style={{ animationDelay: `${(row + col) * 45}ms` }}
          />
        );
      })}
    </div>
  );
}
