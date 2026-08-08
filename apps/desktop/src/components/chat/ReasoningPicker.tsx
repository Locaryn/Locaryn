import { useEffect, useRef, useState } from "react";
import type { ReasoningLevel } from "../../lib/core";

const LEVELS: { id: ReasoningLevel; label: string; hint: string }[] = [
  { id: "auto", label: "Auto", hint: "Laisse le modèle décider" },
  { id: "off", label: "Aucun", hint: "Réponse directe, le plus rapide" },
  { id: "low", label: "Bas", hint: "Réflexion courte" },
  { id: "medium", label: "Moyen", hint: "Équilibre vitesse / qualité" },
  { id: "high", label: "Élevé", hint: "Réfléchit longuement" },
  { id: "extreme", label: "Extrême", hint: "Réflexion illimitée, très lent" },
];

type Props = {
  value: ReasoningLevel;
  onChange: (v: ReasoningLevel) => void;
  disabled?: boolean;
};

/**
 * Reasoning-level chip with a themed popover. Replaces a native <select>, which
 * rendered as an unreadable white OS menu on the dark theme and gave no hint of
 * what each level actually does.
 */
export function ReasoningPicker({ value, onChange, disabled }: Props) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const current = LEVELS.find((l) => l.id === value) ?? LEVELS[0];

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    }
    function onEsc(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onEsc);
    };
  }, [open]);

  return (
    <div className="lochor-reason-wrap" ref={wrapRef}>
      <button
        type="button"
        className={`lochor-chip-btn${value !== "auto" ? " lochor-chip-on" : ""}`}
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="listbox"
        aria-expanded={open}
        title="Combien le modèle doit réfléchir avant de répondre"
      >
        <span aria-hidden="true">🧠</span> Réflexion
        <span className="lochor-chip-state">{current.label}</span>
      </button>

      {open && (
        <div className="lochor-reason-menu" role="listbox">
          {LEVELS.map((l) => (
            <button
              key={l.id}
              type="button"
              role="option"
              aria-selected={l.id === value}
              className={`lochor-reason-opt${l.id === value ? " selected" : ""}`}
              onClick={() => {
                onChange(l.id);
                setOpen(false);
              }}
            >
              <span className="lochor-reason-opt-mark">{l.id === value ? "✓" : ""}</span>
              <span className="lochor-reason-opt-body">
                <span className="lochor-reason-opt-label">{l.label}</span>
                <span className="lochor-reason-opt-hint">{l.hint}</span>
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
