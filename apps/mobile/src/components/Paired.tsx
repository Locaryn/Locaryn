import { useEffect } from "react";
import type { PairingResult } from "../lib/core";

type Props = {
  result: PairingResult;
  onDone: () => void;
};

/** A short scatter in the accent tones. Positions are fixed, not random:
 *  the same moment should look the same every time. */
const BITS = [
  { dx: -120, dy: -90, rot: 220, c: "var(--accent)" },
  { dx: 110, dy: -110, rot: -180, c: "var(--text-dim)" },
  { dx: -70, dy: -160, rot: 140, c: "var(--accent)" },
  { dx: 150, dy: -40, rot: -260, c: "var(--border-strong)" },
  { dx: -160, dy: -30, rot: 300, c: "var(--text-faint)" },
  { dx: 60, dy: -170, rot: -120, c: "var(--accent)" },
  { dx: 20, dy: -200, rot: 200, c: "var(--text-dim)" },
  { dx: -30, dy: -140, rot: -300, c: "var(--accent)" },
];

/**
 * The one moment worth celebrating.
 *
 * Not because a network setting changed — because the person can stop
 * thinking about it. It says which server, so they can see they landed in the
 * right place, and nothing else: no address, no port, no relay.
 */
export function Paired({ result, onDone }: Props) {
  // It clears itself. Making someone dismiss a success is asking them to
  // acknowledge good news.
  useEffect(() => {
    const t = window.setTimeout(onDone, 2600);
    return () => window.clearTimeout(t);
  }, [onDone]);

  return (
    <div className="lo-screen">
      <div className="lo-paired">
        <div className="lo-confetti" aria-hidden="true">
          {BITS.map((b, i) => (
            <i
              key={i}
              style={
                {
                  background: b.c,
                  animationDelay: `${i * 45}ms`,
                  "--dx": `${b.dx}px`,
                  "--dy": `${b.dy}px`,
                  "--rot": `${b.rot}deg`,
                } as React.CSSProperties
              }
            />
          ))}
        </div>

        <svg className="lo-check" viewBox="0 0 80 80" aria-hidden="true">
          <circle cx="40" cy="40" r="36" />
          <path d="M25 41l11 11 20-22" />
        </svg>

        <h1 className="lo-title">Vous êtes connecté</h1>
        <p className="lo-sub">{result.message}</p>

        <button type="button" className="lo-btn-ghost" onClick={onDone}>
          Continuer
        </button>
      </div>
    </div>
  );
}
