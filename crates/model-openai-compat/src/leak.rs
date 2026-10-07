//! Tool calls a server left in the message content.
//!
//! When a server's tool-call parser fails (a model wrote Qwen3-coder XML or a `<tool_call>` block
//! the server did not recognise), the call arrives as ordinary text. The decoder never turns
//! content into a call; it only watches the content for the markers of the common dialects, so
//! the caller can tell "the model answered in prose" from "the model tried to call a tool and the
//! server could not read it".

use serde::{Deserialize, Serialize};

/// The marker that gave a call in the content away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakMarker {
    /// `<tool_call>`: Hermes and Qwen JSON calls, and Qwen3-coder's XML wrapper.
    ToolCallTag,
    /// `<function=`: the Qwen3-coder XML call itself.
    QwenFunctionTag,
    /// `<function_call>`.
    FunctionCallTag,
    /// `[TOOL_CALLS]`: Mistral.
    MistralToolCalls,
    /// `<|python_tag|>`: Llama.
    PythonTag,
}

const MARKERS: [(LeakMarker, &str); 5] = [
    (LeakMarker::ToolCallTag, "<tool_call>"),
    (LeakMarker::QwenFunctionTag, "<function="),
    (LeakMarker::FunctionCallTag, "<function_call>"),
    (LeakMarker::MistralToolCalls, "[TOOL_CALLS]"),
    (LeakMarker::PythonTag, "<|python_tag|>"),
];

/// The longest marker, in bytes: the tail kept between chunks is one byte shorter.
const LONGEST: usize = 15;

/// Watches content text, chunk by chunk, for the first marker; a marker split across chunks is
/// still found.
#[derive(Debug, Clone, Default)]
pub(crate) struct LeakWatch {
    tail: String,
    found: Option<LeakMarker>,
}

impl LeakWatch {
    pub(crate) fn see(&mut self, text: &str) {
        if self.found.is_some() || text.is_empty() {
            return;
        }
        let window = format!("{}{}", self.tail, text);
        self.found = MARKERS
            .iter()
            .filter_map(|(marker, needle)| window.find(needle).map(|at| (at, *marker)))
            .min_by_key(|(at, _)| *at)
            .map(|(_, marker)| marker);
        let mut keep = window.len().saturating_sub(LONGEST - 1);
        while !window.is_char_boundary(keep) {
            keep += 1;
        }
        self.tail = window[keep..].to_owned();
    }

    pub(crate) fn found(&self) -> Option<LeakMarker> {
        self.found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(chunks: &[&str]) -> Option<LeakMarker> {
        let mut w = LeakWatch::default();
        chunks.iter().for_each(|c| w.see(c));
        w.found()
    }

    #[test]
    fn each_marker_is_found_whole_and_split_at_every_byte() {
        for (marker, needle) in MARKERS {
            let text = format!("sure, here you go: {needle}{{}}");
            assert_eq!(watch(&[&text]), Some(marker), "{needle}");
            for cut in (0..text.len()).filter(|c| text.is_char_boundary(*c)) {
                let (a, b) = text.split_at(cut);
                assert_eq!(watch(&[a, b]), Some(marker), "{needle} at {cut}");
            }
        }
    }

    #[test]
    fn prose_and_near_misses_are_not_a_leak() {
        for text in [
            "",
            "I will call the tool now",
            "<tool_cal",
            "<function>",
            "[TOOL",
        ] {
            assert_eq!(watch(&[text]), None, "{text:?}");
        }
    }

    #[test]
    fn multibyte_text_between_chunks_never_splits_a_character() {
        let chunks = ["日本語日本語日本語", "日本語<tool_", "call>"];
        assert_eq!(watch(&chunks), Some(LeakMarker::ToolCallTag));
    }
}
