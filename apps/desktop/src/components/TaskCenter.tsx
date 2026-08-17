import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { type AppTask, TASK_META, taskCenter, useTasks } from "../lib/taskCenter";

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
  const panelRef = useRef<HTMLDialogElement | null>(null);
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
        className={`locaryn-footer${running.length > 0 ? " locaryn-footer-active" : ""}`}
        onClick={() => setOpen((v) => !v)}
        title="Centre de notifications (téléchargements, générations, workflows)"
      >
        <span className="locaryn-footer-dot" data-on={running.length > 0} />
        <span className="locaryn-footer-summary">{summary}</span>
        {running.length > 0 && (
          <span className="locaryn-footer-mini">
            {running.slice(0, 3).map((t) => (
              <span key={t.id} title={t.label} style={{ color: TASK_META[t.type].color }}>
                {TASK_META[t.type].icon}
              </span>
            ))}
          </span>
        )}
        <span className="locaryn-footer-chevron">{open ? "▾" : "▴"}</span>
      </button>

      {/* Expandable notification panel (bottom-right) */}
      {open && (
        // `dialog open` : l'élément natif porte la sémantique sans réclamer le
        // fond modal — ce panneau est ancré en bas à droite et laisse le reste
        // de l'application utilisable.
        <dialog
          open
          ref={panelRef}
          className="locaryn-notif-panel"
          aria-label="Centre de notifications"
        >
          <div className="locaryn-notif-head">
            <strong>Centre de notifications</strong>
            <div style={{ display: "flex", gap: 8 }}>
              {tasks.some((t) => t.status !== "running") && (
                <button
                  type="button"
                  className="locaryn-notif-clear"
                  onClick={() => taskCenter.clearFinished()}
                >
                  Effacer terminées
                </button>
              )}
              <button
                type="button"
                className="locaryn-icon-btn"
                onClick={() => setOpen(false)}
                aria-label="Fermer"
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          </div>

          <div className="locaryn-notif-list">
            {tasks.length === 0 ? (
              <div className="locaryn-notif-empty">Rien pour l'instant.</div>
            ) : (
              tasks.map((t) => {
                const m = TASK_META[t.type];
                // Toutes les notifications ne mènent nulle part : un
                // téléchargement n'a rien à rouvrir. Les attributs interactifs
                // ne sont donc posés que sur celles qui agissent vraiment —
                // annoncer un bouton qui ne fait rien est pire que se taire.
                const actionable =
                  t.type === "generation" || t.type === "edit" || Boolean(t.resultImageUrl);
                const activate = () => {
                  if (t.type === "generation" || t.type === "edit") {
                    onReopenImageGen?.();
                    setOpen(false);
                  } else if (t.resultImageUrl) {
                    onOpenResult?.(t);
                  }
                };
                return (
                  <div
                    key={t.id}
                    className="locaryn-notif-item"
                    style={{
                      borderLeft: `3px solid ${m.color}`,
                      cursor: actionable ? "pointer" : "default",
                    }}
                    {...(actionable
                      ? {
                          role: "button",
                          tabIndex: 0,
                          onClick: activate,
                          onKeyDown: (e: React.KeyboardEvent) => {
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              activate();
                            }
                          },
                        }
                      : {})}
                    title={
                      t.type === "generation" || t.type === "edit"
                        ? "Rouvrir la génération d'images"
                        : undefined
                    }
                  >
                    <div className="locaryn-notif-item-head">
                      <span style={{ color: m.color }}>{m.icon}</span>
                      <span className="locaryn-notif-item-label" title={t.label}>
                        {t.label}
                      </span>
                      {t.type === "workflow" && (t.attempt ?? 1) > 1 && (
                        <span
                          className="locaryn-notif-attempt"
                          title="Relancé après échec de la vérification"
                        >
                          essai {t.attempt}
                        </span>
                      )}
                      <span className={`locaryn-notif-status locaryn-notif-${t.status}`}>
                        {t.status === "running"
                          ? t.type === "workflow" && t.steps && t.steps.length
                            ? `${Math.min(t.stepIndex ?? 0, t.steps.length)}/${t.steps.length}`
                            : (t.detail ?? "en cours")
                          : t.status === "done"
                            ? "terminé"
                            : "échec"}
                      </span>
                    </div>

                    {/* Workflow: dynamic step list (plan generated by the LLM) */}
                    {t.type === "workflow" && t.steps && t.steps.length > 0 ? (
                      <>
                        <div className="locaryn-notif-bar">
                          <div
                            className="locaryn-notif-bar-fill"
                            style={{
                              width: `${Math.round(((t.stepIndex ?? 0) / t.steps.length) * 100)}%`,
                              background: m.color,
                            }}
                          />
                        </div>
                        <ol className="locaryn-wf-steps">
                          {t.steps.map((s, i) => {
                            const done = i < (t.stepIndex ?? 0);
                            const current = i === (t.stepIndex ?? 0) && t.status === "running";
                            return (
                              <li
                                // biome-ignore lint/suspicious/noArrayIndexKey: le plan est figé à la création de la tâche — jamais réordonné ni filtré — et deux étapes peuvent porter le même texte, donc le contenu ne fournit pas de clé.
                                key={i}
                                className={`locaryn-wf-step${done ? " done" : ""}${current ? " current" : ""}`}
                              >
                                <span className="locaryn-wf-mark">
                                  {done ? <Icon name="check" size={13} /> : current ? "▸" : "·"}
                                </span>
                                <span className="locaryn-wf-text">{s}</span>
                              </li>
                            );
                          })}
                        </ol>
                      </>
                    ) : (
                      t.status === "running" &&
                      typeof t.progress === "number" && (
                        <div className="locaryn-notif-bar">
                          <div
                            className="locaryn-notif-bar-fill"
                            style={{ width: `${t.progress}%`, background: m.color }}
                          />
                        </div>
                      )
                    )}

                    {t.resultImageUrl && (
                      <img
                        className="locaryn-notif-thumb"
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
                      // biome-ignore lint/a11y/useMediaCaption: audio produit par le modèle sur cette machine ; aucune piste de sous-titres n'existe et en inventer une vide n'aiderait personne.
                      <audio
                        className="locaryn-notif-audio"
                        src={t.resultAudioUrl}
                        controls
                        style={{ width: "100%", marginTop: 8 }}
                      />
                    )}
                    {t.error && <div className="locaryn-notif-error">{t.error}</div>}
                  </div>
                );
              })
            )}
          </div>
        </dialog>
      )}
    </>
  );
}
