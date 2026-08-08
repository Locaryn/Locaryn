import { useEffect, useRef, useState } from "react";
import type { ToolApprovalRequest, RiskLevel, RiskScope, ToolApprovalDecision } from "../lib/core";

type Props = {
  /** When `null`, the modal is closed. */
  approval: ToolApprovalRequest | null;
  /**
   * Called with the user-chosen verdict+scope on click of Allow/Deny.
   * Parent sends this to the Tauri `approve_tool_call` IPC.
   */
  onResolve: (decision: ToolApprovalDecision) => void;
  /** Esc / backdrop / X click — semantically = Deny. Ignored for Critical. */
  onCancel?: () => void;
  /** Visible label shown in the type-to-confirm prompt for Critical. */
  confirmTargetLabel?: string;
  /**
   * When `true`, the Allow button is disabled and re-labelled to "Blocked".
   * Used by the runtime to surface hard-block rules (e.g. Sandbox + mutating
   * tool) without removing the modal — the user still sees the reason + diff
   * and can copy them out before dismissing.
   */
  hardBlocked?: boolean;
};

const RISK_LABEL: Record<RiskLevel, string> = {
  low: "Read-only",
  medium: "Modifies the project",
  high: "Executes a command",
  critical: "Remote / Critical",
};

const RISK_ICON: Record<RiskLevel, string> = {
  low: "🟢",
  medium: "🟠",
  high: "🔴",
  critical: "⛔",
};

const SCOPE_LABEL: Record<RiskScope, string> = {
  once: "Once",
  session: "Session",
  project: "Project",
  always: "Always",
};

const SCOPE_TOOLTIP: Record<RiskScope, string> = {
  once: "Apply to this call only — you will be asked again next time.",
  session: "Apply for any further call of this tool until you close Lochor.",
  project: "Persist: every call in this project will auto-run.",
  always: "Whitelist globally. This is a strong commitment — use sparingly.",
};

function minimumAllowedScope(risk: RiskLevel): RiskScope {
  // RiskScope::minimum_for in Rust mirrors this:
  return "once";
}

export function ToolApprovalModal({
  approval,
  onResolve,
  onCancel,
  confirmTargetLabel,
  hardBlocked = false,
}: Props) {
  const [scope, setScope] = useState<RiskScope>("once");
  const [understand, setUnderstand] = useState(false);
  const [confirmText, setConfirmText] = useState("");
  const [auditNote, setAuditNote] = useState("");
  const allowBtnRef = useRef<HTMLButtonElement>(null);
  const confirmInputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  // Reset state whenever a new approval arrives.
  useEffect(() => {
    if (!approval) return;
    setScope(minimumAllowedScope(approval.risk));
    setUnderstand(false);
    setConfirmText("");
    setAuditNote("");
  }, [approval?.call_id, approval?.risk]); // eslint-disable-line react-hooks/exhaustive-deps

  // Move focus on open + restore on close.
  useEffect(() => {
    if (!approval) return;
    const previous = document.activeElement as HTMLElement | null;
    const target =
      approval.risk === "critical" && confirmInputRef.current
        ? confirmInputRef.current
        : allowBtnRef.current;
    target?.focus();
    return () => previous?.focus();
  }, [approval?.call_id]); // eslint-disable-line react-hooks/exhaustive-deps

  // ESC = Deny for everything except Critical (which forces explicit click).
  useEffect(() => {
    if (!approval) return;
    function onKey(ev: KeyboardEvent) {
      if (ev.key !== "Escape") return;
      ev.preventDefault();
      if (approval?.risk !== "critical") handleDeny();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [approval?.call_id, approval?.risk]);

  if (!approval) return null;

  const isCritical = approval.risk === "critical";
  const isRemote = approval.is_remote;
  const targetNeedsTyping = isCritical && confirmTargetLabel;
  const confirmOk =
    !isCritical ||
    (understand && (!targetNeedsTyping || confirmText.trim() === confirmTargetLabel));
  // Hard-blocked calls (e.g. sandbox + mutating) refuse Allow entirely.
  const allowDisabled = hardBlocked || !confirmOk;

  function handleDeny() {
    onResolve({
      call_id: approval!.call_id,
      tool: approval!.tool,
      risk: approval!.risk,
      decision: "deny",
      scope: "once",
      note: auditNote.trim() || null,
    });
  }

  function handleAllow() {
    onResolve({
      call_id: approval!.call_id,
      tool: approval!.tool,
      risk: approval!.risk,
      decision: "allow",
      scope,
      note: auditNote.trim() || null,
    });
  }

  const scopeOptions: RiskScope[] = ["once", "session", "project", "always"];

  return (
    <div
      className="lochor-approval-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !isCritical && onCancel) onCancel();
      }}
    >
      <div
        ref={dialogRef}
        className={`lochor-approval lochor-approval-${approval.risk}${isRemote ? " lochor-approval-remote" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="lochor-approval-title"
        onKeyDown={(e) => {
          // Tab-loop inside the dialog (focus trap).
          if (e.key === "Tab" && dialogRef.current) {
            const focusables = dialogRef.current.querySelectorAll<HTMLElement>(
              'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
            );
            if (focusables.length === 0) return;
            const first = focusables[0];
            const last = focusables[focusables.length - 1];
            if (e.shiftKey && document.activeElement === first) {
              e.preventDefault();
              last.focus();
            } else if (!e.shiftKey && document.activeElement === last) {
              e.preventDefault();
              first.focus();
            }
          }
        }}
      >
        {/* ── Severity banner ─────────────────────────────────────── */}
        <header className={`lochor-approval-banner lochor-approval-banner-${approval.risk}`}>
          <div className="lochor-approval-pulse" aria-hidden="true" />
          <div className="lochor-approval-banner-text">
            <span id="lochor-approval-title" className="lochor-approval-banner-title">
              {isCritical
                ? "⚠ Remote execution — confirm explicitly"
                : isRemote
                  ? "↗ Remote target — escalated to Critical"
                  : `${RISK_ICON[approval.risk]} ${RISK_LABEL[approval.risk]}`}
            </span>
            <span className="lochor-approval-banner-sub">
              Risk: <strong>{approval.risk.toUpperCase()}</strong>
              {isRemote ? <> · Remote target</> : null}
            </span>
          </div>
          <button
            type="button"
            className="lochor-approval-close"
            aria-label="Cancel (denied)"
            disabled={isCritical}
            title={isCritical ? "Click Deny to close this dialog" : "Cancel"}
            onClick={onCancel}
          >
            ✕
          </button>
        </header>

        {/* ── Body ─────────────────────────────────────────────────── */}
        <section className="lochor-approval-body">
          <div className="lochor-approval-row">
            <span className="lochor-approval-label">Tool</span>
            <code className="lochor-approval-tool">{approval.tool}</code>
          </div>

          {approval.reason ? (
            <div className="lochor-approval-row">
              <span className="lochor-approval-label">Why</span>
              <p className="lochor-approval-reason">{approval.reason}</p>
            </div>
          ) : null}

          {approval.diff ? (
            <div className="lochor-approval-row">
              <span className="lochor-approval-label">Preview</span>
              <pre className="lochor-approval-diff">{approval.diff}</pre>
            </div>
          ) : (
            isCritical ? null : (
              <div className="lochor-approval-row">
                <span className="lochor-approval-label">Preview</span>
                <p className="lochor-approval-reason" style={{ fontStyle: "italic" }}>
                  This tool produces no previewable diff.
                </p>
              </div>
            )
          )}

          {/* Critical-only type-to-confirm + understand checkbox ──── */}
          {isCritical ? (
            <div className="lochor-approval-confirm">
              <label className="lochor-approval-checkbox">
                <input
                  type="checkbox"
                  checked={understand}
                  onChange={(e) => setUnderstand(e.target.checked)}
                />
                <span>
                  I understand the action runs on{" "}
                  <strong>{confirmTargetLabel ?? "the remote target"}</strong>.
                </span>
              </label>
          {targetNeedsTyping ? (
            <div className="lochor-approval-confirm-input">
              <label className="lochor-approval-confirm-input-label" htmlFor="lochor-approval-confirm-phrase">
                Type <code>{confirmTargetLabel}</code> to enable Allow:
              </label>
              <input
                ref={confirmInputRef}
                id="lochor-approval-confirm-phrase"
                className="lochor-input"
                type="text"
                value={confirmText}
                spellCheck={false}
                autoComplete="off"
                aria-required="true"
                aria-invalid={confirmText.trim() !== confirmTargetLabel}
                onChange={(e) => setConfirmText(e.target.value)}
              />
            </div>
          ) : null}
            </div>
          ) : null}

          {/* Scope chips ───────────────────────────────────────────── */}
          <div className="lochor-approval-row">
            <span className="lochor-approval-label">Apply to</span>
            <div className="lochor-approval-scopes" role="radiogroup" aria-label="Approval scope">
              {scopeOptions.map((s) => {
                const isDefault = s === minimumAllowedScope(approval.risk);
                return (
                  <button
                    type="button"
                    key={s}
                    role="radio"
                    aria-checked={scope === s}
                    className={`lochor-approval-chip${scope === s ? " is-selected" : ""}${isDefault ? " is-default" : ""}`}
                    onClick={() => setScope(s)}
                    title={SCOPE_TOOLTIP[s]}
                  >
                    {SCOPE_LABEL[s]}
                    {isDefault ? <span className="lochor-approval-chip-hint">min</span> : null}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Optional user note for the audit log ─────────────────── */}
          <div className="lochor-approval-row">
            <label className="lochor-approval-label" htmlFor="lochor-approval-note">
              Note <span style={{ color: "var(--text-faint)", fontWeight: 400 }}>(optional)</span>
            </label>
            <input
              id="lochor-approval-note"
              className="lochor-input"
              type="text"
              value={auditNote}
              maxLength={160}
              placeholder="Why is this OK?"
              onChange={(e) => setAuditNote(e.target.value)}
            />
          </div>
        </section>

        {/* ── Footer ──────────────────────────────────────────────── */}
        <footer className="lochor-approval-footer">
          <span className="lochor-approval-call-id" title={approval.call_id}>
            call {approval.call_id.slice(0, 8)}
          </span>
          <div className="lochor-approval-actions">
            <button
              type="button"
              className="lochor-btn-ghost"
              onClick={handleDeny}
            >
              Deny
            </button>
            <button
              ref={allowBtnRef}
              type="button"
              className="lochor-btn-primary lochor-approval-allow"
              onClick={handleAllow}
              disabled={allowDisabled}
              aria-disabled={allowDisabled}
              title={
                isCritical && !understand
                  ? "Check the 'I understand' box first"
                  : isCritical && targetNeedsTyping && confirmText.trim() !== confirmTargetLabel
                    ? `Type "${confirmTargetLabel}" to confirm`
                    : ""
              }
            >
              Allow ({SCOPE_LABEL[scope]})
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
