import { Icon } from "@locaryn/ui-core";
import { useState } from "react";
import type { RiskLevel, RiskScope, ToolApprovalDecision, ToolApprovalRequest } from "../lib/core";

type Props = {
  approval: ToolApprovalRequest | null;
  onResolve: (decision: ToolApprovalDecision) => void;
};

const RISK_LABELS: Record<RiskLevel, string> = {
  low: "Lecture seule",
  medium: "Modification",
  high: "Commande système",
  critical: "Critique / Distant",
};

const RISK_CLASSES: Record<RiskLevel, string> = {
  low: "lo-risk-low",
  medium: "lo-risk-medium",
  high: "lo-risk-high",
  critical: "lo-risk-critical",
};

export function ToolApprovalModal({ approval, onResolve }: Props) {
  const [scope, setScope] = useState<RiskScope>("once");

  if (!approval) return null;

  const { call_id, tool, args, risk, reason, diff, is_remote } = approval;

  function handleAllow() {
    onResolve({
      call_id,
      tool,
      allow: true,
      scope,
    });
  }

  function handleDeny() {
    onResolve({
      call_id,
      tool,
      allow: false,
      scope: "once",
    });
  }

  const formattedArgs = args ? JSON.stringify(args, null, 2) : null;

  return (
    // Le fond ne ferme pas : une demande fermée sans réponse laisse l'agent
    // attendre jusqu'au délai. La croix, elle, refuse explicitement.
    <div className="lo-modal-backdrop">
      <div className="lo-modal" role="dialog" aria-modal="true">
        <div className="lo-modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className={`lo-risk-pill ${RISK_CLASSES[risk] ?? "lo-risk-medium"}`}>
              {RISK_LABELS[risk] ?? risk}
            </span>
            <span className="lo-modal-title">Demande d'autorisation</span>
          </div>
          <button
            type="button"
            className="lo-btn-ghost"
            aria-label="Refuser et fermer"
            style={{
              width: 44,
              minHeight: 44,
              padding: 0,
              border: "none",
              fontSize: 16,
            }}
            onClick={handleDeny}
          >
            <Icon name="close" size={18} />
          </button>
        </div>

        <div className="lo-modal-body">
          <div>
            <span className="lo-label">Outil sollicité</span>
            <div
              style={{
                fontSize: 15,
                fontWeight: 700,
                color: "var(--text)",
                wordBreak: "break-all",
              }}
            >
              <code>{tool}</code>{" "}
              {is_remote && (
                <span style={{ fontSize: 12, color: "var(--text-dim)" }}>(distant)</span>
              )}
            </div>
          </div>

          {reason && (
            <div>
              <span className="lo-label">Motif de l'action</span>
              <p className="lo-hint" style={{ color: "var(--text-dim)", fontSize: 14 }}>
                {reason}
              </p>
            </div>
          )}

          {diff && (
            <div>
              <span className="lo-label">Modifications prévues (Diff)</span>
              <pre className="lo-code-block">{diff}</pre>
            </div>
          )}

          {formattedArgs && !diff && (
            <div>
              <span className="lo-label">Arguments de l'appel</span>
              <pre className="lo-code-block">{formattedArgs}</pre>
            </div>
          )}

          <div>
            <span className="lo-label">Portée de l'autorisation</span>
            <div className="lo-chips" style={{ marginTop: 4 }}>
              <button
                type="button"
                className={`lo-chip ${scope === "once" ? "lo-chip-active" : ""}`}
                onClick={() => setScope("once")}
              >
                Cette fois seulement
              </button>
              <button
                type="button"
                className={`lo-chip ${scope === "session" ? "lo-chip-active" : ""}`}
                onClick={() => setScope("session")}
              >
                Pour cette session
              </button>
              <button
                type="button"
                className={`lo-chip ${scope === "always" ? "lo-chip-active" : ""}`}
                onClick={() => setScope("always")}
              >
                Toujours autoriser
              </button>
            </div>
          </div>
        </div>

        <div className="lo-modal-footer">
          <button type="button" className="lo-btn" onClick={handleAllow}>
            Autoriser l'exécution
          </button>
          <button
            type="button"
            className="lo-btn-ghost"
            style={{ color: "var(--danger)" }}
            onClick={handleDeny}
          >
            Refuser l'accès
          </button>
        </div>
      </div>
    </div>
  );
}
