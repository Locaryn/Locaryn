import { useEffect, useRef, useState } from "react";
import { clearRun, getRun, subscribeRun, type RunView } from "../lib/runPanel";

/**
 * The right-hand pane: what running a code block produced.
 *
 * Two shapes, because "run this" means two different things. A script produces
 * a stream of lines and an exit code, so it gets a terminal. A page produces
 * something to look at, so it gets rendered — sandboxed, since it comes from a
 * language model and may contain anything.
 */
export function RunPanel() {
  const [run, setRun] = useState<RunView | null>(getRun());
  useEffect(() => subscribeRun(() => setRun(getRun())), []);

  if (!run) {
    return (
      <aside className="locaryn-right">
        <div className="locaryn-run-empty">
          <div className="locaryn-run-empty-title">Rien à afficher</div>
          <div className="locaryn-run-empty-sub">
            Exécutez un bloc de code depuis une réponse : la sortie du terminal ou
            la page rendue s'affichera ici.
          </div>
        </div>
      </aside>
    );
  }

  return (
    <aside className="locaryn-right">
      <div className="locaryn-run-head">
        <span className="locaryn-run-title">
          {run.kind === "terminal" ? `Terminal · ${run.lang}` : run.title}
        </span>
        {run.kind === "terminal" && run.running && (
          <span className="locaryn-run-badge">en cours…</span>
        )}
        {run.kind === "terminal" && !run.running && (
          <span
            className={`locaryn-run-badge${run.exitCode ? " locaryn-run-badge-bad" : ""}`}
          >
            {run.exitCode ? `code ${run.exitCode}` : "terminé"}
          </span>
        )}
        <button
          type="button"
          className="locaryn-icon-btn"
          onClick={clearRun}
          aria-label="Fermer"
          title="Fermer"
        >
          ✕
        </button>
      </div>
      {run.kind === "terminal" ? <TerminalView run={run} /> : <WebView html={run.html} />}
    </aside>
  );
}

function TerminalView({ run }: { run: Extract<RunView, { kind: "terminal" }> }) {
  const ref = useRef<HTMLDivElement | null>(null);

  // Follow the tail, but only while the user is already at the bottom —
  // otherwise scrolling back to read an error fights the incoming output.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < 60) {
      el.scrollTop = el.scrollHeight;
    }
  }, [run.lines]);

  return (
    <div className="locaryn-run-term" ref={ref}>
      <div className="locaryn-run-cwd">{run.cwd}</div>
      <div className="locaryn-run-cmd">$ {run.command}</div>
      {run.lines.map((l, i) => (
        <div className="locaryn-run-line" key={i}>
          {l}
        </div>
      ))}
      {run.running && <div className="locaryn-run-cursor" />}
      {!run.running && run.lines.length === 0 && (
        <div className="locaryn-run-line locaryn-run-dim">(aucune sortie)</div>
      )}
    </div>
  );
}

function WebView({ html }: { html: string }) {
  // `sandbox` without allow-same-origin: the document is model-generated, so it
  // must not reach this app's origin, storage or cookies. Scripts are allowed
  // so a demo page actually behaves like one, but they run walled off.
  return (
    <iframe
      className="locaryn-run-web"
      title="Aperçu"
      sandbox="allow-scripts"
      srcDoc={html}
    />
  );
}
