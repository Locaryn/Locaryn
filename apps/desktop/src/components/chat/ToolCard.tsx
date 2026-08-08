import { useState } from "react";

type Props = {
  tool: string;
  args: unknown;
  status: "running" | "ok" | "error";
  output: string;
};

const TOOL_ICONS: Record<string, string> = {
  read_file: "📄",
  write_file: "✏️",
  run_command: "❯_",
  search: "🔍",
  list_dir: "📁",
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
  const icon = TOOL_ICONS[tool] ?? "🔧";

  return (
    <div className={`lochor-tool-card lochor-tool-${status}`}>
      <button
        type="button"
        className="lochor-tool-head"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span className="lochor-tool-icon" aria-hidden="true">
          {icon}
        </span>
        <span className="lochor-tool-name">{tool}</span>
        <span className="lochor-tool-summary">{argsSummary(args)}</span>
        <span className={`lochor-tool-status lochor-tool-status-${status}`}>
          {status === "running" ? (
            <span className="lochor-tool-spinner" aria-label="running" />
          ) : status === "ok" ? (
            "✓"
          ) : (
            "✗"
          )}
        </span>
        <span className="lochor-tool-chevron" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
      </button>
      {open && (
        <div className="lochor-tool-body">
          <div className="lochor-tool-section">args</div>
          <pre className="lochor-tool-pre">{JSON.stringify(args, null, 2)}</pre>
          {output && (
            <>
              <div className="lochor-tool-section">output</div>
              <pre className="lochor-tool-pre">
                {output.length > 4000 ? `${output.slice(0, 4000)}…` : output}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}
