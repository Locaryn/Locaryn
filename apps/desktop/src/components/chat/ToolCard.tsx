import { Icon, type IconName } from "@locaryn/ui-core";
import { useState } from "react";

type Props = {
  tool: string;
  args: unknown;
  status: "running" | "ok" | "error";
  output: string;
};

const TOOL_ICONS: Record<string, IconName> = {
  read_file: "models",
  write_file: "edit",
  run_command: "cpu",
  search: "search",
  list_dir: "project",
};

/** Compact one-line summary of the tool arguments. */
function argsSummary(args: unknown): string {
  if (args && typeof args === "object") {
    const entries = Object.entries(args as Record<string, unknown>);
    if (entries.length > 0) {
      return entries
        .map(([k, v]) => `${k}: ${typeof v === "string" ? v : JSON.stringify(v)}`)
        .join(" · ");
    }
  }
  return JSON.stringify(args);
}

export function ToolCard({ tool, args, status, output }: Props) {
  const [open, setOpen] = useState(false);
  const icon = TOOL_ICONS[tool] ?? "extensions";

  return (
    <div className={`locaryn-tool-card locaryn-tool-${status}`}>
      <button
        type="button"
        className="locaryn-tool-head"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span className="locaryn-tool-icon" aria-hidden="true">
          <Icon name={icon} size={14} />
        </span>
        <span className="locaryn-tool-name">{tool}</span>
        <span className="locaryn-tool-summary">{argsSummary(args)}</span>
        <span className={`locaryn-tool-status locaryn-tool-status-${status}`}>
          {status === "running" ? (
            <span className="locaryn-tool-spinner" aria-label="running" />
          ) : status === "ok" ? (
            <Icon name="check" size={13} />
          ) : (
            <Icon name="close" size={13} />
          )}
        </span>
        <span className="locaryn-tool-chevron" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
      </button>
      {open && (
        <div className="locaryn-tool-body">
          <div className="locaryn-tool-section">args</div>
          <pre className="locaryn-tool-pre">{JSON.stringify(args, null, 2)}</pre>
          {output && (
            <>
              <div className="locaryn-tool-section">output</div>
              <pre className="locaryn-tool-pre">
                {output.length > 4000 ? `${output.slice(0, 4000)}…` : output}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}
