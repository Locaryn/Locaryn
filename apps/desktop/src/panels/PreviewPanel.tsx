export function PreviewPanel() {
  return (
    <aside className="locaryn-right">
      <div className="locaryn-preview-tabs">
        <span className="locaryn-active">HTML</span>
        <span>Markdown</span>
        <span>Image</span>
      </div>
      <div className="locaryn-preview-frame">
        {/*
          S6: load the artifact from the in-process core and render it in a
          sandboxed iframe with strict CSP (see the locaryn-preview crate).
          sandbox="allow-scripts" only — NEVER allow-same-origin.
        */}
        <div className="locaryn-preview-placeholder">
          <div className="locaryn-preview-placeholder-mark" aria-hidden="true" />
          <div className="locaryn-preview-placeholder-title">No artifact yet</div>
          <div className="locaryn-preview-placeholder-sub">
            When the agent generates HTML, Markdown or an image, it renders
            here in a sandboxed frame.
          </div>
        </div>
      </div>
      <div className="locaryn-preview-actions">
        <button type="button" className="locaryn-ghost-btn" disabled>
          Open in browser
        </button>
      </div>
    </aside>
  );
}
