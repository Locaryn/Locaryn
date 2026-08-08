import { useEffect, useRef, useState } from "react";
import { core } from "../lib/core";

type TermLine = { stream: "stdout" | "stderr" | "cmd" | "meta"; text: string };

type Props = { cwd: string | null; sessionId?: string | null };

export function BottomPanel({ cwd, sessionId }: Props) {
  const [resolvedCwd, setResolvedCwd] = useState<string | null>(cwd ?? null);
  const [tab, setTab] = useState<"terminal" | "logs">("terminal");
  const [lines, setLines] = useState<TermLine[]>([
    { stream: "meta", text: "Lochor terminal — line-based exec (full PTY in V1)" },
  ]);
  const [cmd, setCmd] = useState("");
  const [running, setRunning] = useState(false);
  const [history, setHistory] = useState<string[]>([]);
  const [histIdx, setHistIdx] = useState(-1);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines]);

  // Resolve the real workspace for the active session. For free chats this
  // returns the per-session temp folder created by the backend.
  useEffect(() => {
    if (!sessionId) {
      setResolvedCwd(cwd ?? null);
      return;
    }
    core.sessionWorkspace(sessionId)
      .then((p) => setResolvedCwd(p))
      .catch(() => setResolvedCwd(cwd ?? null));
  }, [sessionId, cwd]);

  async function run() {
    const command = cmd.trim();
    if (!command || running) return;
    setCmd("");
    setHistory((h) => [...h, command]);
    setHistIdx(-1);
    setRunning(true);
    setLines((prev) => [...prev, { stream: "cmd", text: `$ ${command}` }]);
    try {
      await core.runTerminal(command, resolvedCwd, (ev) => {
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
    <footer className="lochor-bottom">
      <div className="lochor-bottom-tabs">
        <button
          type="button"
          className={`lochor-tab-btn${tab === "terminal" ? " lochor-active" : ""}`}
          onClick={() => setTab("terminal")}
        >
          Terminal
        </button>
        <button
          type="button"
          className={`lochor-tab-btn${tab === "logs" ? " lochor-active" : ""}`}
          onClick={() => setTab("logs")}
        >
          Logs
        </button>
        {tab === "terminal" && lines.length > 1 && (
          <button
            type="button"
            className="lochor-tab-action"
            onClick={() =>
              setLines([{ stream: "meta", text: "cleared" }])
            }
          >
            clear
          </button>
        )}
        <span className="lochor-term-cwd" title={resolvedCwd ?? ""}>
          {resolvedCwd ?? "no workspace"}
        </span>
      </div>
      <div className="lochor-bottom-content">
        {tab === "terminal" ? (
          <div className="lochor-terminal">
            <div className="lochor-term-scroll" ref={scrollRef}>
              {lines.map((l, i) => (
                <div key={i} className={`lochor-term-line lochor-term-${l.stream}`}>
                  {l.text}
                </div>
              ))}
            </div>
            <div className="lochor-term-input-row">
              <span className="lochor-term-prompt">{running ? "…" : "❯"}</span>
              <input
                className="lochor-term-input"
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
          <div className="lochor-logs-empty">
            <code>Daemon &amp; supervisor logs land here in V1.</code>
          </div>
        )}
      </div>
    </footer>
  );
}
