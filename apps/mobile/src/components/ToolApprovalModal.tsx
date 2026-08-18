import { useState } from "react";

export type RiskLevel = "low" | "medium" | "high" | "critical";
export type RiskScope = "once" | "session" | "always";

export interface ToolApprovalRequest {
  call_id: string;
  tool: string;
  args?: Record<string, unknown>;
  risk: RiskLevel;
  reason?: string;
  diff?: string;
  is_remote?: boolean;
}

export interface ToolApprovalDecision {
  call_id: string;
  allow: boolean;
  scope: RiskScope;
  audit_note?: string;
}

type Props = {
  approval: ToolApprovalRequest | null;
  onResolve: (decision: ToolApprovalDecision) => void;
  onCancel?: () => void;
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

export function ToolApprovalModal({ approval, onResolve, onCancel }: Props) {
  const [scope, setScope] = useState<RiskScope>("once");

  if (!approval) return null;

  const { call_id, tool, args, risk, reason, diff, is_remote } = approval;

  function handleAllow() {
    onResolve({
      call_id,
      allow: true,
      scope,
    });
  }

  function handleDeny() {
    onResolve({
      call_id,
      allow: false,
      scope: "once",
    });
  }

  const formattedArgs = args ? JSON.stringify(args, null, 2) : null;

  return (
    <div className="lo-modal-backdrop" onClick={onCancel}>
      <div className="lo-modal" onClick={(e) => e.stopPropagation()}>
        <div className="lo-modal-header">
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span className={`lo-risk-pill ${RISK_CLASSES[risk] ?? "lo-risk-medium"}`}>
              {RISK_LABELS[risk] ?? risk}
            </span>
            <span className="lo-modal-title">Demande d'autorisation</span>
          </div>
          {onCancel && (
            <button
              type="button"
              className="lo-btn-ghost"
              style={{
                width: "auto",
                minHeight: "auto",
                padding: "4px 8px",
                border: "none",
                fontSize: 16,
              }}
              onClick={onCancel}
            >
              ✕
            </button>
          )}
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
