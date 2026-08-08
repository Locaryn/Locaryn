export function PreviewFrame({ url, sandbox = "allow-scripts" }: { url: string; sandbox?: string }) {
  return (
    <iframe
      title="lochor-preview"
      sandbox={sandbox}
      src={url}
      style={{ width: "100%", height: "100%", border: "none", background: "#fff" }}
    />
  );
}

export function PreviewTabs({
  kinds,
  active,
  onSelect,
}: {
  kinds: string[];
  active: string;
  onSelect: (k: string) => void;
}) {
  return (
    <div style={{ display: "flex", gap: 4, padding: "6px 8px", borderBottom: "1px solid #2a313c" }}>
      {kinds.map((k) => (
        <button
          type="button"
          key={k}
          onClick={() => onSelect(k)}
          style={{
            color: k === active ? "#e6edf3" : "#9aa6b2",
            background: "transparent",
            border: "none",
            cursor: "pointer",
            borderBottom: k === active ? "2px solid #388bfd" : "none",
          }}
        >
          {k}
        </button>
      ))}
    </div>
  );
}
