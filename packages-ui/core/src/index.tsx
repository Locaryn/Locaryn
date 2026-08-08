// @locaryn/ui-core — shared design tokens and primitives.

export const tokens = {
  /** Sen (Google Fonts) — UI font for all Locaryn interfaces. Bundled
   *  locally in the desktop app; falls back to system sans-serif. */
  fontFamily:
    '"Sen", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
  /** Monospace stack for terminal / code — Sen is not monospace. */
  fontFamilyMono: '"SFMono-Regular", ui-monospace, Menlo, Consolas, monospace',
  bg: "#0e1116",
  panel: "#161b22",
  border: "#2a313c",
  text: "#e6edf3",
  textDim: "#9aa6b2",
  green: "#2ea043",
  blue: "#388bfd",
  amber: "#d29922",
  red: "#f85149",
} as const;

export type Variant = "primary" | "secondary" | "danger";

export interface ButtonProps {
  variant?: Variant;
  label: string;
  onClick?: () => void;
}

export function Button({ variant = "primary", label, onClick }: ButtonProps) {
  const bg =
    variant === "primary" ? tokens.green : variant === "danger" ? tokens.red : tokens.panel;
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        background: bg,
        color: "#fff",
        border: `1px solid ${tokens.border}`,
        borderRadius: 6,
        padding: "6px 12px",
        cursor: "pointer",
        fontWeight: 600,
      }}
    >
      {label}
    </button>
  );
}

export function Panel({ title, children }: { title?: string; children: React.ReactNode }) {
  return (
    <div
      style={{
        background: tokens.panel,
        border: `1px solid ${tokens.border}`,
        borderRadius: 8,
        padding: 12,
      }}
    >
      {title && (
        <div
          style={{
            color: tokens.textDim,
            fontSize: 11,
            textTransform: "uppercase",
            letterSpacing: 0.6,
            marginBottom: 8,
          }}
        >
          {title}
        </div>
      )}
      {children}
    </div>
  );
}
