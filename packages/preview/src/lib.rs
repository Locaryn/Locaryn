//! Locaryn preview — artifact model + sandboxed HTML generation.
//!
//! Preview artifacts are served from an isolated origin
//! (`locaryn-preview://` or `tauri://localhost/preview`) and rendered in an
//! iframe with `sandbox="allow-scripts"` (never `allow-same-origin`) and a
//! strict CSP that blocks network unless the user explicitly grants it.

use locaryn_shared_types::{Artifact, ArtifactKind};
use serde::{Deserialize, Serialize};

/// Strict CSP for preview iframes. No network, no external resources.
pub const PREVIEW_CSP_STRICT: &str = "default-src 'none'; \
    script-src 'self' 'unsafe-inline'; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data:; \
    font-src 'self' data:; \
    connect-src 'none'; \
    frame-ancestors 'none'";

/// CSP variant when the user has granted `network` for an artifact.
pub const PREVIEW_CSP_NETWORK: &str = "default-src 'none'; \
    script-src 'self' 'unsafe-inline'; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data: https:; \
    font-src 'self' data: https:; \
    connect-src https:";

/// Sandbox attribute for the preview iframe. `allow-scripts` only — never
/// `allow-same-origin` (that would let the artifact escape the sandbox).
pub const PREVIEW_IFRAME_SANDBOX: &str = "allow-scripts";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewOrigin {
    /// Tauri desktop: a dedicated custom protocol origin.
    Tauri,
    /// Daemon-served preview (CLI / external browser).
    Daemon,
}

impl PreviewOrigin {
    pub fn base_url(&self) -> &'static str {
        match self {
            Self::Tauri => "tauri://localhost/preview",
            Self::Daemon => "http://127.0.0.1:7474/preview",
        }
    }

    pub fn url_for(&self, artifact_id: &str) -> String {
        format!("{}/{}", self.base_url(), artifact_id)
    }
}

/// Wrap a raw HTML artifact in a sandboxed envelope that enforces the CSP
/// via a `<meta>` tag (defense in depth — the iframe sandbox + CSP header
/// from the serving origin are the real enforcement).
pub fn wrap_html(artifact_id: &str, raw_html: &str, allow_network: bool) -> String {
    let csp = if allow_network {
        PREVIEW_CSP_NETWORK
    } else {
        PREVIEW_CSP_STRICT
    };
    format!(
        "<!doctype html>\n<html>\n<head>\n\
         <meta charset=\"utf-8\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"{csp}\">\n\
         <meta name=\"locaryn-artifact-id\" content=\"{artifact_id}\">\n\
         </head>\n<body>\n{raw_html}\n</body>\n</html>"
    )
}

/// Decide whether an artifact kind requires network access by default.
pub fn needs_network(kind: ArtifactKind) -> bool {
    matches!(kind, ArtifactKind::PlotlyHtml)
}

/// Render a markdown artifact to safe HTML for inline (non-iframe) display.
/// V1 wires `marked` + `sanitize`. Skeleton returns escaped HTML.
pub fn render_markdown(md: &str) -> String {
    // Skeleton: escape + wrap in <pre>. V1 uses marked + DOMPurify-equivalent.
    let escaped = md
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<pre class=\"locaryn-md\">{escaped}</pre>")
}

/// A request to render an artifact in the preview panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRequest {
    pub artifact_id: String,
    pub kind: ArtifactKind,
    pub allow_network: bool,
}

/// The resolved render payload the UI needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRender {
    pub artifact_id: String,
    pub url: String,
    pub iframe_sandbox: String,
    pub csp: String,
    pub is_inline: bool,
    pub inline_html: Option<String>,
}

pub fn resolve_render(req: PreviewRequest, origin: PreviewOrigin) -> PreviewRender {
    let csp = if req.allow_network {
        PREVIEW_CSP_NETWORK
    } else {
        PREVIEW_CSP_STRICT
    };
    match req.kind {
        // HTML & Plotly go into a sandboxed iframe.
        // Le son se sert comme une image : par son adresse, le client décide
        // comment le rendre.
        ArtifactKind::Html
        | ArtifactKind::PlotlyHtml
        | ArtifactKind::ImagePng
        | ArtifactKind::AudioWav => PreviewRender {
            artifact_id: req.artifact_id.clone(),
            url: origin.url_for(&req.artifact_id),
            iframe_sandbox: PREVIEW_IFRAME_SANDBOX.to_string(),
            csp: csp.to_string(),
            is_inline: false,
            inline_html: None,
        },
        // Markdown & Python text are rendered inline (no iframe).
        ArtifactKind::Markdown | ArtifactKind::PythonText => PreviewRender {
            artifact_id: req.artifact_id.clone(),
            url: origin.url_for(&req.artifact_id),
            iframe_sandbox: PREVIEW_IFRAME_SANDBOX.to_string(),
            csp: csp.to_string(),
            is_inline: true,
            inline_html: Some(render_markdown("# artifact\n\n(inline render skeleton)")),
        },
    }
}

/// Convert an `Artifact` (from storage) to a `PreviewRequest`.
pub fn artifact_to_request(art: &Artifact, allow_network: bool) -> PreviewRequest {
    PreviewRequest {
        artifact_id: art.id.to_string(),
        kind: art.kind,
        allow_network: allow_network || needs_network(art.kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_html_has_csp() {
        let out = wrap_html("a1", "<p>hi</p>", false);
        assert!(out.contains("Content-Security-Policy"));
        assert!(out.contains("connect-src 'none'"));
        assert!(out.contains("<p>hi</p>"));
    }

    #[test]
    fn network_csp_allows_https() {
        let out = wrap_html("a1", "<p>x</p>", true);
        assert!(out.contains("connect-src https:"));
    }

    #[test]
    fn markdown_inline_render() {
        let req = PreviewRequest {
            artifact_id: "x".into(),
            kind: ArtifactKind::Markdown,
            allow_network: false,
        };
        let r = resolve_render(req, PreviewOrigin::Tauri);
        assert!(r.is_inline);
        assert!(r.inline_html.is_some());
    }
}
