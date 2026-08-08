/**
 * Separate a model's private reasoning from the answer meant for the reader.
 *
 * Reasoning models wrap their scratchpad in `<think>` tags and stream it inline
 * with the reply. Rendered as-is it buries the actual answer under paragraphs
 * of deliberation, which is why every serious chat UI collapses it.
 *
 * The parser has to work on partial text: tokens arrive one at a time, so a
 * block is routinely open with no closing tag yet. An unterminated block is
 * treated as reasoning still in progress rather than as broken markup.
 */

/** Tag names models use for their scratchpad, lowercase. */
const TAGS = ["think", "thinking", "reasoning", "reflection", "scratchpad"];

export interface SplitMessage {
  /** Everything the model said to itself, blocks joined by a blank line. */
  reasoning: string;
  /** What the reader should actually see. */
  answer: string;
  /** True while a block is open — the model is still deliberating. */
  reasoningInProgress: boolean;
}

const OPEN = new RegExp(`<(${TAGS.join("|")})\\s*>`, "i");
const CLOSE_FOR = (tag: string) => new RegExp(`</${tag}\\s*>`, "i");

/**
 * Split streamed text into reasoning and answer.
 *
 * Pure and cheap: it runs on every token, so it walks the string once rather
 * than building a parse tree.
 */
export function splitReasoning(text: string): SplitMessage {
  if (!text) {
    return { reasoning: "", answer: "", reasoningInProgress: false };
  }
  // Fast path: no scratchpad at all, which is the common case.
  if (!OPEN.test(text)) {
    return { reasoning: "", answer: text, reasoningInProgress: false };
  }

  const blocks: string[] = [];
  let answer = "";
  let rest = text;
  let inProgress = false;

  for (;;) {
    const open = rest.match(OPEN);
    if (!open || open.index === undefined) {
      answer += rest;
      break;
    }
    answer += rest.slice(0, open.index);
    const tag = open[1].toLowerCase();
    const afterOpen = rest.slice(open.index + open[0].length);

    const close = afterOpen.match(CLOSE_FOR(tag));
    if (!close || close.index === undefined) {
      // Still streaming: everything after the opening tag is reasoning so far.
      blocks.push(afterOpen);
      inProgress = true;
      break;
    }
    blocks.push(afterOpen.slice(0, close.index));
    rest = afterOpen.slice(close.index + close[0].length);
  }

  return {
    reasoning: blocks.map((b) => b.trim()).filter(Boolean).join("\n\n"),
    answer: answer.replace(/^\s+/, ""),
    reasoningInProgress: inProgress,
  };
}

/**
 * Last non-empty line of the reasoning, for the one-line peek shown while the
 * model is still thinking. Long lines are cut so the row cannot grow.
 */
export function reasoningPeek(reasoning: string, max = 90): string {
  const lines = reasoning.split("\n").map((l) => l.trim()).filter(Boolean);
  const last = lines[lines.length - 1] ?? "";
  return last.length > max ? `${last.slice(0, max - 1)}…` : last;
}

/** Rough reading time signal for the collapsed header. */
export function reasoningSummary(reasoning: string): string {
  const words = reasoning.split(/\s+/).filter(Boolean).length;
  if (words === 0) return "";
  if (words < 60) return "quelques secondes";
  if (words < 300) return "réflexion courte";
  return "longue réflexion";
}
