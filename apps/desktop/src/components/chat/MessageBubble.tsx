import { Icon } from "@locaryn/ui-core";
import { useEffect, useRef, useState } from "react";
import { core } from "../../lib/core";
import { pickSaveFile } from "../../lib/dialog";
import { renderMarkdown } from "../../lib/markdown";
import { splitReasoning } from "../../lib/reasoning";
import { ReasoningBlock } from "./ReasoningBlock";

type Props = {
  role: "user" | "assistant";
  text: string;
  /** data-URL or asset URLs rendered in the message. */
  images?: string[];
  /** Native source paths for generated images, when available. */
  imagePaths?: Array<string | undefined>;
  /** Show the Edit action (last user message, not streaming). */
  canEdit?: boolean;
  onEdit?: () => void;
  /** Run a code block from the answer (the panel asks for confirmation). */
  onRunCode?: (code: string, lang: string) => void;
};

/** Languages we can actually execute on the user's machine. */
const RUNNABLE: Record<string, string> = {
  python: "Python",
  py: "Python",
  bash: "Shell",
  sh: "Shell",
  shell: "Shell",
  powershell: "PowerShell",
  ps1: "PowerShell",
  javascript: "Node",
  js: "Node",
  node: "Node",
  // Not executed on the machine: rendered live in the preview panel.
  html: "Aperçu",
  htm: "Aperçu",
  svg: "Aperçu",
};

function sourcePathFromAssetUrl(src: string): string | null {
  try {
    const url = new URL(src);
    if (url.protocol !== "asset:") return null;
    const decoded = decodeURIComponent(url.pathname.replace(/^\/+/, ""));
    // convertFileSrc encodes the complete Windows path as one URL segment;
    // POSIX paths need their leading slash restored after URL parsing.
    return /^[A-Za-z]:[\\/]/.test(decoded) || decoded.startsWith("/") ? decoded : `/${decoded}`;
  } catch {
    return null;
  }
}

export function MessageBubble({
  role,
  text,
  images,
  imagePaths,
  canEdit,
  onEdit,
  onRunCode,
}: Props) {
  const [copied, setCopied] = useState(false);
  const [lightbox, setLightbox] = useState<{ src: string; path?: string } | null>(null);
  const [imageFeedback, setImageFeedback] = useState<string | null>(null);
  const mdRef = useRef<HTMLDivElement | null>(null);

  // Reasoning models stream their scratchpad inline with the reply. Split it
  // out so the answer is what the reader sees; the deliberation stays folded.
  const { reasoning, answer, reasoningInProgress } =
    role === "assistant"
      ? splitReasoning(text)
      : { reasoning: "", answer: text, reasoningInProgress: false };

  useEffect(() => {
    if (!lightbox) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setLightbox(null);
    };
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [lightbox]);

  async function saveImage() {
    if (!lightbox) return;
    const destination = await pickSaveFile("image.png", ["png", "jpg", "jpeg", "webp"], "Image");
    if (!destination) return;
    try {
      const sourcePath = lightbox.path ?? sourcePathFromAssetUrl(lightbox.src);
      if (sourcePath) {
        await core.saveImageAs(sourcePath, destination);
      } else {
        // Browser/demo fallback for a data URL or a remote image.
        const link = document.createElement("a");
        link.href = lightbox.src;
        link.download = destination.split(/[\\\\/]/).pop() || "image.png";
        document.body.appendChild(link);
        link.click();
        link.remove();
      }
      setImageFeedback("Image enregistrée");
    } catch (error) {
      setImageFeedback(`Enregistrement impossible : ${String(error)}`);
    }
  }

  async function copyImage() {
    if (!lightbox) return;
    try {
      const response = await fetch(lightbox.src);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const blob = await response.blob();
      if (!navigator.clipboard?.write || typeof ClipboardItem === "undefined") {
        throw new Error("presse-papier image indisponible");
      }
      await navigator.clipboard.write([new ClipboardItem({ [blob.type || "image/png"]: blob })]);
      setImageFeedback("Image copiée");
    } catch (error) {
      setImageFeedback(`Copie impossible : ${String(error)}`);
    }
  }

  function imageViewer() {
    if (!lightbox) return null;
    return (
      <div
        className="locaryn-image-lightbox"
        role="presentation"
        onMouseDown={(event) => {
          if (event.target === event.currentTarget) setLightbox(null);
        }}
      >
        <div
          className="locaryn-image-lightbox-dialog"
          role="dialog"
          aria-modal="true"
          aria-label="Image agrandie"
        >
          <div className="locaryn-image-lightbox-toolbar">
            <button type="button" className="locaryn-image-lightbox-action" onClick={saveImage}>
              <Icon name="download" size={14} /> Enregistrer sous
            </button>
            <button type="button" className="locaryn-image-lightbox-action" onClick={copyImage}>
              Copier l'image
            </button>
            <button
              type="button"
              className="locaryn-image-lightbox-close"
              onClick={() => setLightbox(null)}
              aria-label="Fermer l'image agrandie"
              title="Fermer"
            >
              <Icon name="close" size={18} />
            </button>
          </div>
          <img className="locaryn-image-lightbox-image" src={lightbox.src} alt="Image agrandie" />
          {imageFeedback && (
            <div className="locaryn-image-lightbox-feedback" role="status">
              {imageFeedback}
            </div>
          )}
        </div>
      </div>
    );
  }

  // The markdown renderer emits raw <pre class="md-code">; enhance each block
  // with a Copy (and, for runnable languages, an Exec) toolbar. Done on the DOM
  // because the HTML is injected, not composed from React children.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `answer` n'est pas lu ici mais commande le HTML réinjecté ; sans cette dépendance les blocs de code apparus pendant le streaming n'auraient jamais leur barre d'outils.
  useEffect(() => {
    const root = mdRef.current;
    if (!root) return;
    for (const pre of root.querySelectorAll<HTMLPreElement>("pre.md-code")) {
      if (pre.dataset.enhanced === "1") continue;
      pre.dataset.enhanced = "1";
      const code = pre.querySelector("code")?.textContent ?? "";
      const lang = (pre.dataset.lang || "").toLowerCase();

      const bar = document.createElement("div");
      bar.className = "md-code-bar";
      if (lang) {
        const tag = document.createElement("span");
        tag.className = "md-code-lang";
        tag.textContent = lang;
        bar.appendChild(tag);
      }

      const copyBtn = document.createElement("button");
      copyBtn.type = "button";
      copyBtn.className = "md-code-btn";
      copyBtn.textContent = "Copier";
      copyBtn.onclick = async () => {
        // navigator.clipboard needs a focused document; fall back to a hidden
        // textarea + execCommand so copying works in every embedding context.
        let ok = false;
        try {
          await navigator.clipboard.writeText(code);
          ok = true;
        } catch {
          try {
            const ta = document.createElement("textarea");
            ta.value = code;
            ta.style.position = "fixed";
            ta.style.opacity = "0";
            document.body.appendChild(ta);
            ta.select();
            ok = document.execCommand("copy");
            document.body.removeChild(ta);
          } catch {
            ok = false;
          }
        }
        copyBtn.textContent = ok ? "Copié" : "Échec";
        window.setTimeout(() => {
          copyBtn.textContent = "Copier";
        }, 1500);
      };
      bar.appendChild(copyBtn);

      if (onRunCode && RUNNABLE[lang]) {
        const runBtn = document.createElement("button");
        runBtn.type = "button";
        runBtn.className = "md-code-btn md-code-run";
        runBtn.textContent = `▶ Exécuter (${RUNNABLE[lang]})`;
        runBtn.onclick = () => onRunCode(code, lang);
        bar.appendChild(runBtn);
      }

      pre.prepend(bar);
    }
  }, [answer, onRunCode]);

  async function copy() {
    try {
      // The answer, not the scratchpad: nobody wants to paste the model's
      // deliberation into a document.
      await navigator.clipboard.writeText(role === "assistant" ? answer : text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard unavailable (permissions) — silently ignore.
    }
  }

  if (role === "user") {
    return (
      <div className="locaryn-msg-row locaryn-msg-row-user">
        <div className="locaryn-msg locaryn-msg-user">
          {images && images.length > 0 && (
            <div className="locaryn-msg-images">
              {images.map((src, i) => (
                <img
                  key={src}
                  src={src}
                  alt="attachment"
                  className="locaryn-msg-image locaryn-msg-image-clickable"
                  onClick={() => setLightbox({ src, path: imagePaths?.[i] })}
                />
              ))}
            </div>
          )}
          {text && <div className="locaryn-msg-text">{text}</div>}
          <div className="locaryn-msg-actions">
            <button type="button" className="locaryn-msg-action" onClick={copy}>
              {copied ? "Copié" : "Copier"}
            </button>
            {canEdit && onEdit && (
              <button type="button" className="locaryn-msg-action" onClick={onEdit}>
                Edit
              </button>
            )}
          </div>
        </div>
        {imageViewer()}
      </div>
    );
  }

  return (
    <div className="locaryn-msg-row locaryn-msg-row-assistant">
      <div className="locaryn-msg-meta">
        <span className="locaryn-msg-avatar" aria-hidden="true" />
        <span className="locaryn-msg-author">Locaryn</span>
        <button type="button" className="locaryn-msg-action" onClick={copy}>
          {copied ? "Copié" : "Copier"}
        </button>
      </div>
      <ReasoningBlock reasoning={reasoning} inProgress={reasoningInProgress} />
      <div
        ref={mdRef}
        className="locaryn-msg-md"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: renderMarkdown échappe tout le HTML source avant d'injecter ses propres balises (modèle de sûreté en tête de lib/markdown.ts). Rien de ce que produit le modèle n'atteint le DOM sous forme de balise.
        dangerouslySetInnerHTML={{ __html: renderMarkdown(answer) }}
      />
      {images && images.length > 0 && (
        <div className="locaryn-msg-images">
          {images.map((src, i) => (
            <img
              key={src}
              src={src}
              alt="génération"
              className="locaryn-msg-image locaryn-msg-image-clickable"
              onClick={() => setLightbox({ src, path: imagePaths?.[i] })}
            />
          ))}
        </div>
      )}
      {imageViewer()}
    </div>
  );
}
