import { useState } from "react";
import { type ApprovalScope, type ToolApprovalRequest, api } from "../lib/core";

const RISQUES: Record<ToolApprovalRequest["risk"], string> = {
  low: "Lecture seule",
  medium: "Modification",
  high: "Risque élevé",
  critical: "Critique",
};

const PORTEES: { value: ApprovalScope; label: string }[] = [
  { value: "once", label: "Cette fois" },
  { value: "session", label: "Cette conversation" },
  { value: "always", label: "Toujours" },
];

type Props = {
  request: ToolApprovalRequest;
  /** Appelé une fois la réponse partie, ou avec l'erreur du service. */
  onDone: (error: string | null) => void;
};

/**
 * Un outil attend l'accord de l'utilisateur. Le fond ne ferme pas : une
 * demande laissée sans réponse fait attendre l'agent jusqu'au délai.
 */
export function ApprovalDialog({ request, onDone }: Props) {
  const [scope, setScope] = useState<ApprovalScope>("once");
  const [envoi, setEnvoi] = useState(false);

  async function repondre(allow: boolean) {
    setEnvoi(true);
    try {
      await api.answerApproval(request, allow, allow ? scope : "once");
      onDone(null);
    } catch (e) {
      onDone(`Réponse non transmise : ${String(e)}`);
    }
  }

  const args = request.args ? JSON.stringify(request.args, null, 2) : null;

  return (
    <div className="lo-approval-backdrop">
      <div
        className="lo-approval"
        role="dialog"
        aria-modal="true"
        aria-labelledby="lo-approval-title"
      >
        <div className="lo-approval-head">
          <span className={`lo-approval-risk is-${request.risk}`}>{RISQUES[request.risk]}</span>
          <h2 id="lo-approval-title" className="lo-approval-title">
            Demande d'autorisation
          </h2>
        </div>
        <p className="lo-approval-tool">{request.tool}</p>
        {request.reason && <p className="lo-hint">{request.reason}</p>}
        {(request.diff ?? args) && <pre className="lo-approval-args">{request.diff ?? args}</pre>}
        <div className="lo-segmented" role="group" aria-label="Portée de l'accord">
          {PORTEES.map((p) => (
            <button
              key={p.value}
              type="button"
              className={scope === p.value ? "lo-segment-on" : undefined}
              onClick={() => setScope(p.value)}
              disabled={envoi}
            >
              {p.label}
            </button>
          ))}
        </div>
        <div className="lo-approval-actions">
          <button
            type="button"
            className="lo-btn-ghost"
            onClick={() => void repondre(false)}
            disabled={envoi}
          >
            Refuser
          </button>
          <button
            type="button"
            className="lo-btn"
            onClick={() => void repondre(true)}
            disabled={envoi}
          >
            Autoriser
          </button>
        </div>
      </div>
    </div>
  );
}
