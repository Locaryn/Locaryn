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
  low: "ok",
  medium: "warn",
  high: "bad",
  critical: "bad",
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
  /** Les détails (modifications, portée) sont repliés : la demande tient sur
   *  une ligne au-dessus du composeur, et le motif reste lisible. */
  const [open, setOpen] = useState(false);
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
    setOpen(false);
  }, [callId, risk]);

  // Le focus ne se vole plus.
  //
  // Cette demande n'est plus un pop-up : on peut la laisser attendre, changer
  // d'écran, finir sa phrase. Prendre le focus arracherait le curseur au milieu
  // d'une saisie — et rien ne presse, puisque l'outil ne s'exécute pas sans
  // réponse. Le bandeau et la pastille orange suffisent à se faire voir.
  //
  // Le seul cas où l'on insiste est le risque critique : la demande y exige de
  // recopier une cible, et amener le curseur dans ce champ évite de chercher où
  // taper.
  useEffect(() => {
    if (!callId || risk !== "critical") return;
    confirmInputRef.current?.focus();
  }, [callId, risk]);

  // Échap refuse, mais depuis le bandeau seulement — l'écoute est posée sur
  // lui, plus bas.
  //
  // Elle couvrait la fenêtre entière : Échap ailleurs, pour fermer un menu ou
  // abandonner une saisie, refusait l'outil sans que rien ne l'annonce. Un
  // bandeau qui n'a pas le focus ne doit pas capter les touches de toute
  // l'application.

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
  // Un risque critique exige une confirmation saisie, et un blocage dur montre
  // pourquoi : dans ces deux cas rien ne se replie.
  const expanded = open || isCritical || hardBlocked;

  return (
    <div
      ref={dialogRef}
      className={`locaryn-approval locaryn-approval-bandeau locaryn-approval-compact locaryn-approval-${approval.risk}${isRemote ? " locaryn-approval-remote" : ""}`}
      // `region` et non `dialog` : ce bandeau ne capture ni le focus ni les
      // clics. Il vit au-dessus du composeur, dans le flux, et l'on peut
      // continuer à naviguer pendant qu'il attend.
      role="region"
      aria-labelledby="locaryn-approval-title"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !isCritical) {
          e.preventDefault();
          handleDeny();
        }
      }}
    >
      <div className="locaryn-approval-line">
        <span className="locaryn-approval-pulse" aria-hidden="true" />
        <div className="locaryn-approval-summary">
          <div className="locaryn-approval-headline">
            <h2 id="locaryn-approval-title" className="locaryn-approval-banner-title">
              {RISK_LABEL[approval.risk]}
            </h2>
            <code className="locaryn-approval-tool">{approval.tool}</code>
          </div>
          <p className={`locaryn-approval-reason${expanded ? " is-open" : ""}`}>
            {approval.reason}
          </p>
        </div>
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
              hardBlocked
                ? "Cette action est bloquée par le niveau de permission de la conversation"
                : isCritical && !understand
                  ? "Cochez d'abord la case de confirmation"
                  : isCritical && targetNeedsTyping && confirmText.trim() !== confirmTargetLabel
                    ? `Tapez « ${confirmTargetLabel} » pour confirmer`
                    : ""
            }
          >
            {scope === "once" ? "Autoriser" : `Autoriser (${SCOPE_LABEL[scope]})`}
          </button>
        </div>
      </div>

      {!isCritical && !hardBlocked && (
        <button
          type="button"
          className="locaryn-approval-toggle"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? "Masquer les détails" : "Détails et portée"}
        </button>
      )}

      {expanded && (
        <div className="locaryn-approval-body">
          {approval.diff && (
            <div className="locaryn-approval-row locaryn-approval-row-diff">
              <span className="locaryn-approval-label">Modifications</span>
              <pre className="locaryn-approval-diff">{approval.diff}</pre>
            </div>
          )}

          {isCritical && (
            <div className="locaryn-approval-confirm">
              <label className="locaryn-approval-checkbox">
                <input
                  type="checkbox"
                  checked={understand}
                  onChange={(e) => setUnderstand(e.target.checked)}
                />
                <span>
                  Je comprends que cette action est irréversible et peut modifier le système
                </span>
              </label>

              {targetNeedsTyping && (
                <div className="locaryn-approval-confirm-input">
                  <label
                    htmlFor="locaryn-approval-confirm-input"
                    className="locaryn-approval-confirm-input-label"
                  >
                    Tapez <code>{confirmTargetLabel}</code> pour confirmer :
                  </label>
                  <input
                    id="locaryn-approval-confirm-input"
                    ref={confirmInputRef}
                    type="text"
                    value={confirmText}
                    onChange={(e) => setConfirmText(e.target.value)}
                    placeholder={confirmTargetLabel}
                    autoComplete="off"
                    spellCheck="false"
                  />
                </div>
              )}
            </div>
          )}

          <div className="locaryn-approval-row">
            <span className="locaryn-approval-label">Portée</span>
            <div className="locaryn-approval-scopes">
              {scopeOptions.map((sc) => {
                const isSelected = scope === sc;
                const isDefault = sc === minimumAllowedScope(approval.risk);
                return (
                  <button
                    key={sc}
                    type="button"
                    className={`locaryn-approval-chip${isSelected ? " is-selected" : ""}${
                      isDefault ? " is-default" : ""
                    }`}
                    onClick={() => setScope(sc)}
                    title={SCOPE_TOOLTIP[sc]}
                  >
                    <span>{SCOPE_LABEL[sc]}</span>
                    {isDefault && <span className="locaryn-approval-chip-hint">défaut</span>}
                  </button>
                );
              })}
            </div>
          </div>

          {isRemote && (
            <div className="locaryn-approval-row">
              <label htmlFor="locaryn-approval-audit-note" className="locaryn-approval-label">
                Note d'audit (optionnel)
              </label>
              <input
                id="locaryn-approval-audit-note"
                type="text"
                className="locaryn-input"
                value={auditNote}
                onChange={(e) => setAuditNote(e.target.value)}
                placeholder="ex. Ticket #1234, maintenance planifiée…"
              />
            </div>
          )}

          <span className="locaryn-approval-call-id" title={approval.call_id}>
            appel {approval.call_id.slice(0, 8)}
          </span>
        </div>
      )}
    </div>
  );
}
