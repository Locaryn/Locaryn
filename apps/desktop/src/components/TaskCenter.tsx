import { Icon, LoProgress, LoSpinner } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { type AppTask, TASK_META, taskCenter, useTasks } from "../lib/taskCenter";

type Props = {
  onOpenResult?: (t: AppTask) => void;
};

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Tauri/WebView permissions can reject navigator.clipboard. Keep the same
    // fallback as the chat copy action so an error is never trapped in the UI.
    try {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      const copied = document.execCommand("copy");
      textarea.remove();
      return copied;
    } catch {
      return false;
    }
  }
}

/**
 * Ce qui tourne, au centre de la barre d'état : une roue et un libellé en
 * mono, rien d'autre. Muet quand il n'y a rien — une barre qui annonce
 * « aucune tâche » occupe la place sans rien dire.
 */
export function RunningTask() {
  const tasks = useTasks();
  const running = tasks.filter((t) => t.status === "running");
  if (running.length === 0) return null;
  const first = running[0];
  const rest = running.length - 1;
  return (
    <span className="locaryn-status-task">
      <LoSpinner size="sm" />
      <span className="locaryn-status-task-label" title={first.label}>
        {first.detail ? `${first.label} — ${first.detail}` : first.label}
      </span>
      {rest > 0 && <span className="locaryn-status-task-more">+{rest}</span>}
    </span>
  );
}

/**
 * Le centre de notifications, à droite de la barre d'état : une cloche qui
 * sonne tant qu'il reste quelque chose à lire, et un panneau translucide qui
 * s'ouvre au-dessus de la barre. Chaque entrée s'ignore seule ; « Tout
 * effacer » vide la pile.
 */
export function TaskCenter({ onOpenResult }: Props) {
  const tasks = useTasks();
  const [open, setOpen] = useState(false);
  const [copyFeedback, setCopyFeedback] = useState<{ id: string; text: string } | null>(null);
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
  const unread = tasks.filter((t) => !t.read).length;
  const summary =
    running.length > 0
      ? `${running.length} tâche${running.length > 1 ? "s" : ""} en cours`
      : tasks.length > 0
        ? `${tasks.length} récente${tasks.length > 1 ? "s" : ""}`
        : "Aucune tâche";

  return (
    <>
      {/* Cloche du centre de notifications */}
      <button
        ref={btnRef}
        type="button"
        className={`locaryn-bell${unread > 0 ? " locaryn-bell-unread" : ""}`}
        onClick={() => {
          setOpen((v) => {
            if (!v) taskCenter.markAllRead();
            return !v;
          });
        }}
        aria-expanded={open}
        title={`Centre de notifications — ${summary}`}
      >
        <Icon name={unread > 0 ? "bell-ringing" : "bell"} size={16} />
        {unread > 0 && <span className="locaryn-bell-count">{unread}</span>}
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
                  onClick={() => taskCenter.clearGallery()}
                >
                  Tout effacer
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
                // Une erreur est une action de copie, pas une route vers un
                // panneau de génération qui n'a plus de sens. Les autres
                // notifications restent interactives uniquement si elles ont
                // réellement une destination.
                const isError = t.status === "error" && Boolean(t.error);
                const actionable = isError || (t.status !== "error" && Boolean(t.resultImageUrl));
                const activate = async () => {
                  if (isError && t.error) {
                    const copied = await copyText(t.error);
                    setCopyFeedback({
                      id: t.id,
                      text: copied ? "Erreur copiée" : "Copie impossible",
                    });
                    window.setTimeout(() => {
                      setCopyFeedback((current) => (current?.id === t.id ? null : current));
                    }, 1800);
                    return;
                  }
                  if (t.resultImageUrl) {
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
                      isError
                        ? "Copier le détail de l'erreur"
                        : t.resultImageUrl
                          ? "Ouvrir le résultat"
                          : undefined
                    }
                  >
                    <div className="locaryn-notif-item-head">
                      <span style={{ color: m.color }} title={m.label}>
                        <Icon name={m.icon} size={13} />
                      </span>
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
                      <button
                        type="button"
                        className="locaryn-notif-dismiss"
                        aria-label="Ignorer cette notification"
                        title="Ignorer"
                        onClick={(e) => {
                          e.stopPropagation();
                          taskCenter.remove(t.id);
                        }}
                      >
                        <Icon name="close" size={12} />
                      </button>
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
                        <LoProgress
                          value={(t.stepIndex ?? 0) / t.steps.length}
                          on="surface-2"
                          label={t.label}
                        />
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
                        <LoProgress value={t.progress / 100} on="surface-2" label={t.label} />
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
                      <audio
                        className="locaryn-notif-audio"
                        src={t.resultAudioUrl}
                        controls
                        style={{ width: "100%", marginTop: 8 }}
                      />
                    )}
                    {t.error && <div className="locaryn-notif-error">{t.error}</div>}
                    {copyFeedback?.id === t.id && (
                      <div className="locaryn-notif-copy-feedback" role="status">
                        {copyFeedback.text}
                      </div>
                    )}
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
