export function PreviewPanel() {
  return (
    <aside className="lochor-right">
      <div className="lochor-preview-tabs">
        <span className="lochor-active">HTML</span>
        <span>Markdown</span>
        <span>Image</span>
      </div>
      <div className="lochor-preview-frame">
        {/*
          S6: load the artifact from the in-process core and render it in a
          sandboxed iframe with strict CSP (see the lochor-preview crate).
          sandbox="allow-scripts" only — NEVER allow-same-origin.
        */}
        <div className="lochor-preview-placeholder">
          <div className="lochor-preview-placeholder-mark" aria-hidden="true" />
          <div className="lochor-preview-placeholder-title">No artifact yet</div>
          <div className="lochor-preview-placeholder-sub">
            When the agent generates HTML, Markdown or an image, it renders
            here in a sandboxed frame.
          </div>
        </div>
      </div>
      <div className="lochor-preview-actions">
        <button type="button" className="lochor-ghost-btn" disabled>
          Open in browser
        </button>
      </div>
    </aside>
  );
}
