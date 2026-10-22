//! The wrappers of a reply shaped `<think>...</think>`, `<action>...</action>`, then the calls
//! (`cua_action::AnswerShape::ThinkActionCall`). The calls are found without them
//! (`read_calls`), so a reply that leaves a wrapper out still acts; this reads the wrappers for
//! the words of the step. Total and bounded: ASCII case does not matter, nothing is evaluated.

/// The most characters kept of either wrapper's text.
const KEPT_CHARS: usize = 4096;

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";
const ACTION_OPEN: &str = "<action>";
const ACTION_CLOSE: &str = "</action>";

/// What the wrappers of a reply held. A wrapper that is missing, never closes or is empty is
/// `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnswerParts {
    /// The reasoning, up to the last `</think>` (a template may have opened the block already, so
    /// the opening tag is optional).
    pub think: Option<String>,
    /// The short imperative of the first `<action>` after the reasoning.
    pub action: Option<String>,
}

/// The wrappers of `text`.
pub fn answer_parts(text: &str) -> AnswerParts {
    // ASCII lowercasing keeps every byte offset, so offsets found in `lower` slice `text`.
    let lower = text.to_ascii_lowercase();
    let think_end = lower.rfind(THINK_CLOSE);
    let think = think_end.and_then(|end| {
        let start = lower
            .find(THINK_OPEN)
            .filter(|open| *open < end)
            .map_or(0, |open| open + THINK_OPEN.len());
        kept(&text[start..end])
    });
    let after = think_end.map_or(0, |end| end + THINK_CLOSE.len());
    let action = lower[after..].find(ACTION_OPEN).and_then(|open| {
        let from = after + open + ACTION_OPEN.len();
        let close = lower[from..].find(ACTION_CLOSE)?;
        kept(&text[from..from + close])
    });
    AnswerParts { think, action }
}

fn kept(text: &str) -> Option<String> {
    let text: String = text.trim().chars().take(KEPT_CHARS).collect();
    (!text.is_empty()).then_some(text)
}
