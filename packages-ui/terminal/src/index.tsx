// @locaryn/ui-terminal — xterm.js wrapper. V1 wires the real xterm + PTY
// via a Tauri command; this skeleton exports a typed placeholder.

export function Terminal({ lines }: { lines: string[] }) {
  return (
    <pre
      style={{
        background: "#0e1116",
        color: "#2ea043",
        fontFamily: "ui-monospace, Menlo, Consolas, monospace",
        fontSize: 13,
        padding: 12,
        margin: 0,
        overflow: "auto",
        height: "100%",
      }}
    >
      {lines.join("\n")}
    </pre>
  );
}
