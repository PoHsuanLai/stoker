//! How a model was trained to write its answer, as data: the text format of its tool calls, the
//! shape of the whole reply, how its history is replayed and how the screen's size is worded.
//!
//! None of these is a model name. A model that was fine-tuned on one answer shape does best when
//! it is shown that shape, and several models share one; the catalogue entry names the shape
//! (in its [`ActionProfile`](crate::ActionProfile)) and the session and the parser follow it. Each
//! enum's default is what every entry did before the option existed, so an entry that names
//! nothing is read and prompted exactly as it was.

use serde::{Deserialize, Serialize};

/// Who reads the model's tool calls out of its text, and in which format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallFormat {
    /// The engine's own parser reads the calls and hands them over as tool calls (the entry's
    /// `tools` is `server_parsed`); the request declares the tools.
    #[default]
    Engine,
    /// We read the calls from the reply text, written the way the Qwen3 chat templates write
    /// them: `<tool_call>\n<function=NAME>\n<parameter=KEY>\nVALUE\n</parameter>\n</function>\n</tool_call>`.
    Qwen3CoderXml,
    /// We read the calls from the reply text, written as `<tool_call>{"name": .., "arguments": {..}}</tool_call>`
    /// (the Hermes and Qwen2.5 chat templates).
    HermesJson,
}

impl CallFormat {
    /// Whether the session reads the calls itself: the request then declares no tools to the
    /// server, which would otherwise run its parser and strip the text this format is read from.
    pub const fn is_read_by_us(self) -> bool {
        !matches!(self, CallFormat::Engine)
    }
}
