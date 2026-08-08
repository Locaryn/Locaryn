import { useEffect, useRef, useState } from "react";
import { reasoningPeek, reasoningSummary } from "../../lib/reasoning";

type Props = {
  reasoning: string;
  /** The model is still deliberating: show a live peek instead of a summary. */
  inProgress: boolean;
};

/**
 * The model's scratchpad, collapsed.
 *
 * Reasoning models emit far more deliberation than answer. Printing all of it
 * pushes the reply off the screen and makes short questions look like essays,
 * so it stays folded: a single line while the model thinks, a summary once it
 * is done, and the full text only on request.
 */
export function ReasoningBlock({ reasoning, inProgress }: Props) {
  const [open, setOpen] = useState(false);
  const bodyRef = useRef<HTMLDivElement | null>(null);

  // Follow the tail while it streams, but stop fighting the user once they
  // scroll up to read something earlier.
  useEffect(() => {
    if (!open || !inProgress) return;
    const el = bodyRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    if (atBottom) el.scrollTop = el.scrollHeight;
  }, [reasoning, open, inProgress]);

  if (!reasoning.trim()) return null;

  const peek = reasoningPeek(reasoning);
  return (
    <div className={`locaryn-reason${inProgress ? " locaryn-reason-live" : ""}`}>
      <button
        type="button"
        className="locaryn-reason-head"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span className="locaryn-reason-caret" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span className="locaryn-reason-label">
          {inProgress ? "Réflexion…" : "Raisonnement"}
        </span>
        {!open && (
          <span className="locaryn-reason-peek">
            {inProgress ? peek : reasoningSummary(reasoning)}
          </span>
        )}
      </button>
      {open && (
        <div className="locaryn-reason-body" ref={bodyRef}>
          {reasoning}
        </div>
      )}
    </div>
  );
}
