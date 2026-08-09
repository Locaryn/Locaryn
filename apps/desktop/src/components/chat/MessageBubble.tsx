import { useEffect, useRef, useState } from "react";
import { renderMarkdown } from "../../lib/markdown";
import { splitReasoning } from "../../lib/reasoning";
import { ReasoningBlock } from "./ReasoningBlock";

type Props = {
  role: "user" | "assistant";
  text: string;
  /** data-URL images attached to a user message. */
  images?: string[];
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

export function MessageBubble({ role, text, images, canEdit, onEdit, onRunCode }: Props) {
  const [copied, setCopied] = useState(false);
  const mdRef = useRef<HTMLDivElement | null>(null);

  // Reasoning models stream their scratchpad inline with the reply. Split it
  // out so the answer is what the reader sees; the deliberation stays folded.
  const { reasoning, answer, reasoningInProgress } =
    role === "assistant"
      ? splitReasoning(text)
      : { reasoning: "", answer: text, reasoningInProgress: false };

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
        copyBtn.textContent = ok ? "Copié ✓" : "Échec";
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
                // biome-ignore lint/suspicious/noArrayIndexKey: les pièces jointes sont figées au moment de l'envoi, jamais réordonnées ni insérées ; la data-URL ne peut pas servir de clé car deux fois la même image donnerait deux clés identiques.
                <img key={i} src={src} alt="attachment" className="locaryn-msg-image" />
              ))}
            </div>
          )}
          {text && <div className="locaryn-msg-text">{text}</div>}
          <div className="locaryn-msg-actions">
            <button type="button" className="locaryn-msg-action" onClick={copy}>
              {copied ? "Copied ✓" : "Copy"}
            </button>
            {canEdit && onEdit && (
              <button type="button" className="locaryn-msg-action" onClick={onEdit}>
                Edit
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="locaryn-msg-row locaryn-msg-row-assistant">
      <div className="locaryn-msg-meta">
        <span className="locaryn-msg-avatar" aria-hidden="true" />
        <span className="locaryn-msg-author">Locaryn</span>
        <button type="button" className="locaryn-msg-action" onClick={copy}>
          {copied ? "Copied ✓" : "Copy"}
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
            // biome-ignore lint/suspicious/noArrayIndexKey: images d'un message déjà émis, liste immuable ; la data-URL ne peut pas servir de clé car deux générations identiques la partageraient.
            <img key={i} src={src} alt="génération" className="locaryn-msg-image" />
          ))}
        </div>
      )}
    </div>
  );
}
