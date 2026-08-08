// Minimal, dependency-free markdown renderer for chat messages.
//
// Safety model: the source text is HTML-escaped FIRST, then our own tags are
// injected around already-escaped content, so no user/model-provided HTML can
// reach the DOM. Links are restricted to http(s) and open externally.
//
// Supported: fenced code blocks, inline code, bold, italic, links,
// headings (###), blockquotes, ordered/unordered lists, paragraphs.

function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** Inline transforms on an already-escaped line. */
function renderInline(escaped: string): string {
  return (
    escaped
      // `code`
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      // **bold**
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      // *italic* (single asterisks, not part of **)
      .replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, "$1<em>$2</em>")
      // [label](https://url)
      .replace(
        /\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
        '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>',
      )
  );
}

/** Render markdown to sanitized HTML (see safety model above). */
export function renderMarkdown(src: string): string {
  const lines = src.replaceAll("\r\n", "\n").split("\n");
  const out: string[] = [];
  let i = 0;

  // Paragraph accumulator.
  let para: string[] = [];
  const flushPara = () => {
    if (para.length > 0) {
      out.push(`<p>${para.map((l) => renderInline(escapeHtml(l))).join("<br/>")}</p>`);
      para = [];
    }
  };

  while (i < lines.length) {
    const line = lines[i];

    // Fenced code block.
    const fence = line.match(/^```(\w*)\s*$/);
    if (fence) {
      flushPara();
      const lang = fence[1];
      const code: string[] = [];
      i += 1;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        code.push(lines[i]);
        i += 1;
      }
      i += 1; // skip closing fence (or EOF)
      const langAttr = lang ? ` data-lang="${escapeHtml(lang)}"` : "";
      out.push(
        `<pre class="md-code"${langAttr}><code>${escapeHtml(code.join("\n"))}</code></pre>`,
      );
      continue;
    }

    // Heading (#, ##, ### … all rendered as a bold heading line).
    const heading = line.match(/^(#{1,4})\s+(.*)$/);
    if (heading) {
      flushPara();
      out.push(`<p class="md-heading">${renderInline(escapeHtml(heading[2]))}</p>`);
      i += 1;
      continue;
    }

    // Blockquote (consecutive `>` lines).
    if (/^>\s?/.test(line)) {
      flushPara();
      const quote: string[] = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        quote.push(lines[i].replace(/^>\s?/, ""));
        i += 1;
      }
      out.push(
        `<blockquote>${quote.map((l) => renderInline(escapeHtml(l))).join("<br/>")}</blockquote>`,
      );
      continue;
    }

    // Lists (consecutive `- ` / `* ` / `1. ` lines).
    const ulMatch = /^\s*[-*]\s+/.test(line);
    const olMatch = /^\s*\d+\.\s+/.test(line);
    if (ulMatch || olMatch) {
      flushPara();
      const tag = ulMatch ? "ul" : "ol";
      const items: string[] = [];
      const re = ulMatch ? /^\s*[-*]\s+/ : /^\s*\d+\.\s+/;
      while (i < lines.length && re.test(lines[i])) {
        items.push(`<li>${renderInline(escapeHtml(lines[i].replace(re, "")))}</li>`);
        i += 1;
      }
      out.push(`<${tag}>${items.join("")}</${tag}>`);
      continue;
    }

    // Blank line: paragraph boundary.
    if (line.trim() === "") {
      flushPara();
      i += 1;
      continue;
    }

    para.push(line);
    i += 1;
  }
  flushPara();

  return out.join("");
}
