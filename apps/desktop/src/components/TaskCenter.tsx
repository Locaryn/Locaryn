import { useEffect, useRef, useState } from "react";
import { useTasks, taskCenter, TASK_META, type AppTask } from "../lib/taskCenter";

type Props = {
  onOpenResult?: (t: AppTask) => void;
  /** Reopen the image-generation popup (clicking a generation/edit task). */
  onReopenImageGen?: () => void;
};

/**
 * Footer status bar + expandable notification center (bottom-right). Shows every
 * background task — downloads, image generations, model edits, workflows —
 * colour-coded by type so they are trivial to tell apart. Clicking the footer
 * expands the panel; clicking a generation/edit task reopens the image popup;
 * clicking anywhere else closes the panel.
 */
export function TaskCenter({ onOpenResult, onReopenImageGen }: Props) {
  const tasks = useTasks();
  const [open, setOpen] = useState(false);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const btnRef = useRef<HTMLButtonElement | null>(null);

  // Close the panel when clicking anywhere outside it (and outside the toggle).
  useEffect(() => {
    if (!open) return;
    function onDocDown(e: MouseEvent) {
      const t = e.target as Node;
      if (panelRef.current?.contains(t) || btnRef.current?.contains(t)) return;
      setOpen(false);
    }
    document.addEventListener("mousedown", onDocDown);
    return () => document.removeEventListener("mousedown", onDocDown);
  }, [open]);

  const running = tasks.filter((t) => t.status === "running");
  const summary =
    running.length > 0
      ? `${running.length} tâche${running.length > 1 ? "s" : ""} en cours`
      : tasks.length > 0
        ? `${tasks.length} récente${tasks.length > 1 ? "s" : ""}`
        : "Aucune tâche";

  return (
    <>
      {/* Footer status bar */}
      <button
        ref={btnRef}
        type="button"
        className={`lochor-footer${running.length > 0 ? " lochor-footer-active" : ""}`}
        onClick={() => setOpen((v) => !v)}
        title="Centre de notifications (téléchargements, générations, workflows)"
      >
        <span className="lochor-footer-dot" data-on={running.length > 0} />
        <span className="lochor-footer-summary">{summary}</span>
        {running.length > 0 && (
          <span className="lochor-footer-mini">
            {running.slice(0, 3).map((t) => (
              <span key={t.id} title={t.label} style={{ color: TASK_META[t.type].color }}>
                {TASK_META[t.type].icon}
              </span>
            ))}
          </span>
        )}
        <span className="lochor-footer-chevron">{open ? "▾" : "▴"}</span>
      </button>

      {/* Expandable notification panel (bottom-right) */}
      {open && (
        <div ref={panelRef} className="lochor-notif-panel" role="dialog" aria-label="Centre de notifications">
          <div className="lochor-notif-head">
            <strong>Centre de notifications</strong>
            <div style={{ display: "flex", gap: 8 }}>
              {tasks.some((t) => t.status !== "running") && (
                <button type="button" className="lochor-notif-clear" onClick={() => taskCenter.clearFinished()}>
                  Effacer terminées
                </button>
              )}
              <button type="button" className="lochor-icon-btn" onClick={() => setOpen(false)} aria-label="Fermer">✕</button>
            </div>
          </div>

          <div className="lochor-notif-list">
            {tasks.length === 0 ? (
              <div className="lochor-notif-empty">Rien pour l'instant.</div>
            ) : (
              tasks.map((t) => {
                const m = TASK_META[t.type];
                return (
                  <div
                    key={t.id}
                    className="lochor-notif-item"
                    style={{
                      borderLeft: `3px solid ${m.color}`,
                      cursor: t.type === "generation" || t.type === "edit" || t.resultImageUrl ? "pointer" : "default",
                    }}
                    onClick={() => {
                      if (t.type === "generation" || t.type === "edit") {
                        onReopenImageGen?.();
                        setOpen(false);
                      } else if (t.resultImageUrl) {
                        onOpenResult?.(t);
                      }
                    }}
                    title={t.type === "generation" || t.type === "edit" ? "Rouvrir la génération d'images" : undefined}
                  >
                    <div className="lochor-notif-item-head">
                      <span style={{ color: m.color }}>{m.icon}</span>
                      <span className="lochor-notif-item-label" title={t.label}>{t.label}</span>
                      {t.type === "workflow" && (t.attempt ?? 1) > 1 && (
                        <span className="lochor-notif-attempt" title="Relancé après échec de la vérification">
                          essai {t.attempt}
                        </span>
                      )}
                      <span className={`lochor-notif-status lochor-notif-${t.status}`}>
                        {t.status === "running"
                          ? t.type === "workflow" && t.steps && t.steps.length
                            ? `${Math.min(t.stepIndex ?? 0, t.steps.length)}/${t.steps.length}`
                            : (t.detail ?? "en cours")
                          : t.status === "done" ? "terminé" : "échec"}
                      </span>
                    </div>

                    {/* Workflow: dynamic step list (plan generated by the LLM) */}
                    {t.type === "workflow" && t.steps && t.steps.length > 0 ? (
                      <>
                        <div className="lochor-notif-bar">
                          <div
                            className="lochor-notif-bar-fill"
                            style={{ width: `${Math.round(((t.stepIndex ?? 0) / t.steps.length) * 100)}%`, background: m.color }}
                          />
                        </div>
                        <ol className="lochor-wf-steps">
                          {t.steps.map((s, i) => {
                            const done = i < (t.stepIndex ?? 0);
                            const current = i === (t.stepIndex ?? 0) && t.status === "running";
                            return (
                              <li key={i} className={`lochor-wf-step${done ? " done" : ""}${current ? " current" : ""}`}>
                                <span className="lochor-wf-mark">{done ? "✓" : current ? "▸" : "·"}</span>
                                <span className="lochor-wf-text">{s}</span>
                              </li>
                            );
                          })}
                        </ol>
                      </>
                    ) : (
                      t.status === "running" && typeof t.progress === "number" && (
                        <div className="lochor-notif-bar">
                          <div className="lochor-notif-bar-fill" style={{ width: `${t.progress}%`, background: m.color }} />
                        </div>
                      )
                    )}

                    {t.resultImageUrl && (
                      <img
                        className="lochor-notif-thumb"
                        src={t.resultImageUrl}
                        alt="résultat"
                        onError={(e) => {
                          // If the asset URL cannot be loaded (CSP / path issue),
                          // hide the broken image placeholder instead of showing
                          // the alt text and a broken-icon frame.
                          // eslint-disable-next-line no-console
                          console.error("[TaskCenter] thumbnail load failed:", t.resultImageUrl);
                          (e.currentTarget as HTMLElement).style.display = "none";
                        }}
                      />
                    )}
                    {t.resultAudioUrl && (
                      <audio
                        className="lochor-notif-audio"
                        src={t.resultAudioUrl}
                        controls
                        style={{ width: "100%", marginTop: 8 }}
                      />
                    )}
                    {t.error && <div className="lochor-notif-error">{t.error}</div>}
                  </div>
                );
              })
            )}
          </div>
        </div>
      )}
    </>
  );
}
