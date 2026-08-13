import { useEffect, useRef, useState } from "react";
import { core } from "../lib/core";

type TermLine = { stream: "stdout" | "stderr" | "cmd" | "meta"; text: string };

type Props = { cwd: string | null; sessionId?: string | null };

export function BottomPanel({ cwd, sessionId }: Props) {
  const [resolvedCwd, setResolvedCwd] = useState<string | null>(cwd ?? null);
  const [tab, setTab] = useState<"terminal" | "logs">("terminal");
  const [lines, setLines] = useState<TermLine[]>([
    { stream: "meta", text: "Locaryn terminal — line-based exec (full PTY in V1)" },
  ]);
  const [cmd, setCmd] = useState("");
  const [running, setRunning] = useState(false);
  const [history, setHistory] = useState<string[]>([]);
  const [histIdx, setHistIdx] = useState(-1);
  const scrollRef = useRef<HTMLDivElement>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `lines` n'est pas lu ici, il déclenche : c'est son changement qui signale qu'il y a du nouveau à suivre. Le retirer figerait le terminal sur sa première ligne.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  async function run() {
    const command = cmd.trim();
    if (!command || running) return;
    // Resolve the workspace lazily, at the moment a command actually runs.
    // For a free chat this creates the per-session temp folder on first use —
    // a terminal that is only opened never touches the disk.
    let cwd = resolvedCwd;
    if (!cwd && sessionId) {
      try {
        const ws = await core.sessionWorkspace(sessionId, true);
        if (ws.exists) {
          cwd = ws.path;
          setResolvedCwd(cwd);
        }
      } catch {
        // keep null → run wherever the shell starts
      }
    }
    setCmd("");
    setHistory((h) => [...h, command]);
    setHistIdx(-1);
    setRunning(true);
    setLines((prev) => [...prev, { stream: "cmd", text: `$ ${command}` }]);
    try {
      await core.runTerminal(command, cwd, (ev) => {
        setLines((prev) => [
          ...prev,
          ev.type === "line"
            ? { stream: ev.stream, text: ev.text }
            : { stream: "meta", text: `— exit ${ev.code ?? "?"}` },
        ]);
      });
    } catch (e) {
      setLines((prev) => [...prev, { stream: "stderr", text: String(e) }]);
    } finally {
      setRunning(false);
    }
  }

  // Up/Down recall command history.
  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      run();
    } else if (e.key === "ArrowUp" && history.length > 0) {
      e.preventDefault();
      const idx = histIdx < 0 ? history.length - 1 : Math.max(0, histIdx - 1);
      setHistIdx(idx);
      setCmd(history[idx]);
    } else if (e.key === "ArrowDown" && histIdx >= 0) {
      e.preventDefault();
      const idx = histIdx + 1;
      if (idx >= history.length) {
        setHistIdx(-1);
        setCmd("");
      } else {
        setHistIdx(idx);
        setCmd(history[idx]);
      }
    }
  }

  return (
    <footer className="locaryn-bottom">
      <div className="locaryn-bottom-tabs">
        <button
          type="button"
          className={`locaryn-tab-btn${tab === "terminal" ? " locaryn-active" : ""}`}
          onClick={() => setTab("terminal")}
        >
          Terminal
        </button>
        <button
          type="button"
          className={`locaryn-tab-btn${tab === "logs" ? " locaryn-active" : ""}`}
          onClick={() => setTab("logs")}
        >
          Logs
        </button>
        {tab === "terminal" && lines.length > 1 && (
          <button
            type="button"
            className="locaryn-tab-action"
            onClick={() => setLines([{ stream: "meta", text: "cleared" }])}
          >
            clear
          </button>
        )}
        <span className="locaryn-term-cwd" title={resolvedCwd ?? ""}>
          {resolvedCwd ?? "no workspace"}
        </span>
      </div>
      <div className="locaryn-bottom-content">
        {tab === "terminal" ? (
          <div className="locaryn-terminal">
            <div className="locaryn-term-scroll" ref={scrollRef}>
              {lines.map((l, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: flux du terminal : les lignes ne sont qu'ajoutées en fin, jamais insérées ni supprimées, et deux lignes identiques sont courantes.
                <div key={i} className={`locaryn-term-line locaryn-term-${l.stream}`}>
                  {l.text}
                </div>
              ))}
            </div>
            <div className="locaryn-term-input-row">
              <span className="locaryn-term-prompt">{running ? "…" : "❯"}</span>
              <input
                className="locaryn-term-input"
                value={cmd}
                disabled={running}
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                placeholder="run a command…"
                onChange={(e) => setCmd(e.target.value)}
                onKeyDown={onKeyDown}
              />
            </div>
          </div>
        ) : (
          <div className="locaryn-logs-empty">
            <code>Daemon &amp; supervisor logs land here in V1.</code>
          </div>
        )}
      </div>
    </footer>
  );
}
