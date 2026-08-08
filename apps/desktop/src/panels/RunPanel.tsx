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
      <aside className="lochor-right">
        <div className="lochor-run-empty">
          <div className="lochor-run-empty-title">Rien à afficher</div>
          <div className="lochor-run-empty-sub">
            Exécutez un bloc de code depuis une réponse : la sortie du terminal ou
            la page rendue s'affichera ici.
          </div>
        </div>
      </aside>
    );
  }

  return (
    <aside className="lochor-right">
      <div className="lochor-run-head">
        <span className="lochor-run-title">
          {run.kind === "terminal" ? `Terminal · ${run.lang}` : run.title}
        </span>
        {run.kind === "terminal" && run.running && (
          <span className="lochor-run-badge">en cours…</span>
        )}
        {run.kind === "terminal" && !run.running && (
          <span
            className={`lochor-run-badge${run.exitCode ? " lochor-run-badge-bad" : ""}`}
          >
            {run.exitCode ? `code ${run.exitCode}` : "terminé"}
          </span>
        )}
        <button
          type="button"
          className="lochor-icon-btn"
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
    <div className="lochor-run-term" ref={ref}>
      <div className="lochor-run-cwd">{run.cwd}</div>
      <div className="lochor-run-cmd">$ {run.command}</div>
      {run.lines.map((l, i) => (
        <div className="lochor-run-line" key={i}>
          {l}
        </div>
      ))}
      {run.running && <div className="lochor-run-cursor" />}
      {!run.running && run.lines.length === 0 && (
        <div className="lochor-run-line lochor-run-dim">(aucune sortie)</div>
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
      className="lochor-run-web"
      title="Aperçu"
      sandbox="allow-scripts"
      srcDoc={html}
    />
  );
}
