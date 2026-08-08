//! Separating a model's private reasoning from its answer.
//!
//! Reasoning models wrap their scratchpad in `<think>` tags and stream it inline
//! with the reply. Every surface has to deal with it — the desktop folds it into
//! a collapsible block, a terminal shows a spinner then the answer — so the
//! split belongs next to the model interaction rather than being reimplemented
//! per client.
//!
//! The parser works on partial text: tokens arrive one at a time, so a block is
//! routinely open with no closing tag yet. An unterminated block means "still
//! deliberating", not "broken markup".

/// Tag names models use for their scratchpad.
const TAGS: &[&str] = &["think", "thinking", "reasoning", "reflection", "scratchpad"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Split {
    /// Everything the model said to itself, blocks separated by a blank line.
    pub reasoning: String,
    /// What the reader should actually see.
    pub answer: String,
    /// True while a block is open — the model has not finished thinking.
    pub in_progress: bool,
}

/// Case-insensitive search for `needle` starting at `from`.
fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    if from > haystack.len() {
        return None;
    }
    let hay = haystack[from..].to_ascii_lowercase();
    hay.find(&needle.to_ascii_lowercase()).map(|i| i + from)
}

/// Earliest opening tag at or after `from`, as (byte offset, tag name).
fn next_open(text: &str, from: usize) -> Option<(usize, &'static str)> {
    TAGS.iter()
        .filter_map(|t| find_ci(text, &format!("<{t}>"), from).map(|i| (i, *t)))
        .min_by_key(|(i, _)| *i)
}

/// Split streamed text into reasoning and answer.
pub fn split_reasoning(text: &str) -> Split {
    if text.is_empty() {
        return Split::default();
    }
    let mut blocks: Vec<&str> = Vec::new();
    let mut answer = String::new();
    let mut cursor = 0usize;
    let mut in_progress = false;

    while let Some((open_at, tag)) = next_open(text, cursor) {
        answer.push_str(&text[cursor..open_at]);
        let body_start = open_at + tag.len() + 2; // <tag>
        let close = format!("</{tag}>");
        match find_ci(text, &close, body_start) {
            Some(close_at) => {
                blocks.push(&text[body_start..close_at]);
                cursor = close_at + close.len();
            }
            None => {
                // Still streaming: the rest is reasoning so far.
                blocks.push(&text[body_start..]);
                in_progress = true;
                cursor = text.len();
                break;
            }
        }
    }
    answer.push_str(&text[cursor..]);

    Split {
        reasoning: blocks
            .iter()
            .map(|b| b.trim())
            .filter(|b| !b.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        answer: answer.trim_start().to_string(),
        in_progress,
    }
}

/// Last non-empty line of the reasoning, trimmed for a one-line status row.
pub fn peek(reasoning: &str, max: usize) -> String {
    let last = reasoning
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .next_back()
        .unwrap_or("");
    if last.chars().count() > max {
        let cut: String = last.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        last.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_block_is_separated_from_the_answer() {
        let s = split_reasoning("<think>Je calcule 2+2.\nDonc 4.</think>La réponse est 4.");
        assert_eq!(s.reasoning, "Je calcule 2+2.\nDonc 4.");
        assert_eq!(s.answer, "La réponse est 4.");
        assert!(!s.in_progress);
    }

    #[test]
    fn an_unclosed_block_means_the_model_is_still_thinking() {
        // The common case mid-stream: tokens arrive one at a time.
        let s = split_reasoning("<think>Je réfléchis encore");
        assert_eq!(s.reasoning, "Je réfléchis encore");
        assert_eq!(s.answer, "");
        assert!(
            s.in_progress,
            "an open block must not be treated as answer text"
        );
    }

    #[test]
    fn plain_text_passes_straight_through() {
        let s = split_reasoning("Bonjour, voici la réponse.");
        assert_eq!(s.answer, "Bonjour, voici la réponse.");
        assert!(s.reasoning.is_empty());
        assert!(!s.in_progress);
    }

    #[test]
    fn every_tag_spelling_is_recognised_whatever_the_case() {
        for tag in [
            "think",
            "THINK",
            "Thinking",
            "reasoning",
            "reflection",
            "scratchpad",
        ] {
            let raw = format!("<{tag}>x</{tag}>ok");
            let s = split_reasoning(&raw);
            assert_eq!(s.reasoning, "x", "tag {tag}");
            assert_eq!(s.answer, "ok", "tag {tag}");
        }
    }

    #[test]
    fn several_blocks_are_joined_and_the_text_between_them_is_kept() {
        let s = split_reasoning("Avant <think>a</think> milieu <think>b</think> après");
        assert_eq!(s.reasoning, "a\n\nb");
        assert!(s.answer.contains("milieu"));
        assert!(s.answer.contains("après"));
    }

    #[test]
    fn accented_text_is_not_split_mid_character() {
        // Byte offsets on multibyte text would panic if handled carelessly.
        let s = split_reasoning("<think>éàü réflexion</think>Réponse accentuée é");
        assert_eq!(s.reasoning, "éàü réflexion");
        assert_eq!(s.answer, "Réponse accentuée é");
    }

    #[test]
    fn empty_input_is_safe() {
        assert_eq!(split_reasoning(""), Split::default());
    }

    #[test]
    fn peek_returns_the_last_line_and_bounds_its_length() {
        assert_eq!(peek("un\ndeux\ntrois", 40), "trois");
        assert_eq!(peek("", 40), "");
        let long = "a".repeat(100);
        let p = peek(&long, 20);
        assert_eq!(p.chars().count(), 20);
        assert!(p.ends_with('…'));
    }
}
