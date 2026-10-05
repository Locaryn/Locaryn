import { useState } from "react";
import { TerminalConsole } from "../components/workspace/TerminalConsole";
import { useSessionWorkspace } from "../hooks/useSessionWorkspace";

type Props = { cwd: string | null; sessionId?: string | null };

export function BottomPanel({ cwd, sessionId }: Props) {
  // Le vrai dossier de la conversation : pour une conversation libre, le
  // dossier temporaire que le service lui a créé.
  const resolvedCwd = useSessionWorkspace(sessionId, cwd);
  const [tab, setTab] = useState<"terminal" | "logs">("terminal");

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
        <span className="locaryn-term-cwd" title={resolvedCwd ?? ""}>
          {resolvedCwd ?? "no workspace"}
        </span>
      </div>
      <div className="locaryn-bottom-content">
        {/* Gardé monté quand on passe aux journaux : la sortie reste. */}
        <div hidden={tab !== "terminal"} className="locaryn-bottom-term">
          <TerminalConsole cwd={resolvedCwd} />
        </div>
        {tab === "logs" && (
          <div className="locaryn-logs-empty">
            <code>Daemon &amp; supervisor logs land here in V1.</code>
          </div>
        )}
      </div>
    </footer>
  );
}
