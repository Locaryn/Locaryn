import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import type { RiskLevel, RiskScope, ToolApprovalDecision, ToolApprovalRequest } from "../lib/core";

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
  low: "Lecture seule",
  medium: "Modifie le projet",
  high: "Exécute une commande",
  critical: "Distant / Critique",
};

const RISK_ICON: Record<RiskLevel, string> = {
  low: "🟢",
  medium: "🟠",
  high: "🔴",
  critical: "⛔",
};

const SCOPE_LABEL: Record<RiskScope, string> = {
  once: "Cette fois",
  session: "Session",
  project: "Ce projet",
  always: "Toujours",
};

const SCOPE_TOOLTIP: Record<RiskScope, string> = {
  once: "Pour cet appel seulement — la question reviendra la prochaine fois.",
  session: "Pour tous les appels de cet outil, jusqu'à la fermeture de Locaryn.",
  project: "Retenu : tout appel de cet outil dans ce projet passera sans question.",
  always: "Partout, sans limite de durée. Engagement fort — à réserver aux outils sûrs.",
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

  // Les effets ci-dessous ne dépendent que de ces deux valeurs primitives : se lier
  // à `approval` les relancerait à chaque nouvel objet produit par le parent, ce qui
  // effacerait la saisie en cours et volerait le focus sans qu'aucune demande n'ait changé.
  const callId = approval?.call_id ?? null;
  const risk = approval?.risk ?? null;

  // Reset state whenever a new approval arrives.
  useEffect(() => {
    if (!callId || !risk) return;
    setScope(minimumAllowedScope(risk));
    setUnderstand(false);
    setConfirmText("");
    setAuditNote("");
  }, [callId, risk]);

  // Move focus on open + restore on close.
  useEffect(() => {
    if (!callId) return;
    const previous = document.activeElement as HTMLElement | null;
    const target =
      risk === "critical" && confirmInputRef.current
        ? confirmInputRef.current
        : allowBtnRef.current;
    target?.focus();
    return () => previous?.focus();
  }, [callId, risk]);

  // ESC = Deny for everything except Critical (which forces explicit click).
  useEffect(() => {
    if (!callId) return;
    function onKey(ev: KeyboardEvent) {
      if (ev.key !== "Escape") return;
      ev.preventDefault();
      if (risk !== "critical") handleDeny();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [callId, risk]);

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
      className="locaryn-approval-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !isCritical && onCancel) onCancel();
      }}
    >
      <div
        ref={dialogRef}
        className={`locaryn-approval locaryn-approval-${approval.risk}${isRemote ? " locaryn-approval-remote" : ""}`}
        // biome-ignore lint/a11y/useSemanticElements: un <dialog> n'est modal que via showModal(), ce qui le pousse dans la couche supérieure avec son propre ::backdrop et les styles par défaut du navigateur — le fond assombri et la boîte .locaryn-approval ci-dessus cesseraient de s'appliquer
        role="dialog"
        aria-modal="true"
        aria-labelledby="locaryn-approval-title"
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
        <header className={`locaryn-approval-banner locaryn-approval-banner-${approval.risk}`}>
          <div className="locaryn-approval-pulse" aria-hidden="true" />
          <div className="locaryn-approval-banner-text">
            <span id="locaryn-approval-title" className="locaryn-approval-banner-title">
              {isCritical
                ? "⚠ Remote execution — confirm explicitly"
                : isRemote
                  ? "↗ Remote target — escalated to Critical"
                  : `${RISK_ICON[approval.risk]} ${RISK_LABEL[approval.risk]}`}
            </span>
            <span className="locaryn-approval-banner-sub">
              Risk: <strong>{approval.risk.toUpperCase()}</strong>
              {isRemote ? <> · Remote target</> : null}
            </span>
          </div>
          <button
            type="button"
            className="locaryn-approval-close"
            aria-label="Cancel (denied)"
            disabled={isCritical}
            title={isCritical ? "Utilisez Refuser pour fermer cette fenêtre" : "Annuler"}
            onClick={onCancel}
          >
            <Icon name="close" size={16} />
          </button>
        </header>

        {/* ── Body ─────────────────────────────────────────────────── */}
        <section className="locaryn-approval-body">
          <div className="locaryn-approval-row">
            <span className="locaryn-approval-label">Tool</span>
            <code className="locaryn-approval-tool">{approval.tool}</code>
          </div>

          {approval.reason ? (
            <div className="locaryn-approval-row">
              <span className="locaryn-approval-label">Why</span>
              <p className="locaryn-approval-reason">{approval.reason}</p>
            </div>
          ) : null}

          {approval.diff ? (
            <div className="locaryn-approval-row">
              <span className="locaryn-approval-label">Preview</span>
              <pre className="locaryn-approval-diff">{approval.diff}</pre>
            </div>
          ) : isCritical ? null : (
            <div className="locaryn-approval-row">
              <span className="locaryn-approval-label">Preview</span>
              <p className="locaryn-approval-reason" style={{ fontStyle: "italic" }}>
                This tool produces no previewable diff.
              </p>
            </div>
          )}

          {/* Critical-only type-to-confirm + understand checkbox ──── */}
          {isCritical ? (
            <div className="locaryn-approval-confirm">
              <label className="locaryn-approval-checkbox">
                <input
                  type="checkbox"
                  checked={understand}
                  onChange={(e) => setUnderstand(e.target.checked)}
                />
                <span>
                  Je comprends que l'action s'exécute sur{" "}
                  <strong>{confirmTargetLabel ?? "the remote target"}</strong>.
                </span>
              </label>
              {targetNeedsTyping ? (
                <div className="locaryn-approval-confirm-input">
                  <label
                    className="locaryn-approval-confirm-input-label"
                    htmlFor="locaryn-approval-confirm-phrase"
                  >
                    Type <code>{confirmTargetLabel}</code> to enable Allow:
                  </label>
                  <input
                    ref={confirmInputRef}
                    id="locaryn-approval-confirm-phrase"
                    className="locaryn-input"
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
          <div className="locaryn-approval-row">
            <span className="locaryn-approval-label">Apply to</span>
            <div className="locaryn-approval-scopes" role="radiogroup" aria-label="Approval scope">
              {scopeOptions.map((s) => {
                const isDefault = s === minimumAllowedScope(approval.risk);
                return (
                  <button
                    type="button"
                    key={s}
                    // biome-ignore lint/a11y/useSemanticElements: la pastille tire toute son apparence de .locaryn-approval-chip, défini pour un bouton dans la feuille de styles globale ; un <input type="radio"> dessinerait en plus le contrôle natif, et le masquer supprimerait l'anneau de focus dont dépend la navigation au clavier dans cette modale
                    role="radio"
                    aria-checked={scope === s}
                    className={`locaryn-approval-chip${scope === s ? " is-selected" : ""}${isDefault ? " is-default" : ""}`}
                    onClick={() => setScope(s)}
                    title={SCOPE_TOOLTIP[s]}
                  >
                    {SCOPE_LABEL[s]}
                    {isDefault ? <span className="locaryn-approval-chip-hint">min</span> : null}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Optional user note for the audit log ─────────────────── */}
          <div className="locaryn-approval-row">
            <label className="locaryn-approval-label" htmlFor="locaryn-approval-note">
              Note <span style={{ color: "var(--text-faint)", fontWeight: 400 }}>(optional)</span>
            </label>
            <input
              id="locaryn-approval-note"
              className="locaryn-input"
              type="text"
              value={auditNote}
              maxLength={160}
              placeholder="Pourquoi est-ce acceptable ?"
              onChange={(e) => setAuditNote(e.target.value)}
            />
          </div>
        </section>

        {/* ── Footer ──────────────────────────────────────────────── */}
        <footer className="locaryn-approval-footer">
          <span className="locaryn-approval-call-id" title={approval.call_id}>
            call {approval.call_id.slice(0, 8)}
          </span>
          <div className="locaryn-approval-actions">
            <button type="button" className="locaryn-btn-ghost" onClick={handleDeny}>
              Refuser
            </button>
            <button
              ref={allowBtnRef}
              type="button"
              className="locaryn-btn-primary locaryn-approval-allow"
              onClick={handleAllow}
              disabled={allowDisabled}
              aria-disabled={allowDisabled}
              title={
                isCritical && !understand
                  ? "Cochez d'abord la case de confirmation"
                  : isCritical && targetNeedsTyping && confirmText.trim() !== confirmTargetLabel
                    ? `Type "${confirmTargetLabel}" to confirm`
                    : ""
              }
            >
              Autoriser ({SCOPE_LABEL[scope]})
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
