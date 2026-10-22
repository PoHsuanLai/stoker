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

/// The shape of a whole reply the model was trained to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerShape {
    /// Whatever the model writes, then its calls (the common convention).
    #[default]
    Free,
    /// A `<think>...</think>` block, then one `<action>...</action>` (a short imperative saying
    /// what to do in the interface), then the `<tool_call>` blocks, and nothing else. The session
    /// states this in the system message, and takes the action sentence as the step's words.
    ThinkActionCall,
}

/// How the size of a normalised grid is worded when the model is told the screen's resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionWording {
    /// The grid's top: a grid of 0 to 1000 is "1000x1000" (the common convention).
    #[default]
    GridTop,
    /// The grid's cell count: a grid of 0 to 999 has 1000 cells a side, "1000x1000". For a model
    /// trained to be told so while its points run 0-999.
    GridCells,
}

/// How the earlier steps are shown to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryShape {
    /// A list of one line per earlier step, and the last few frames labelled by step (the common
    /// convention).
    #[default]
    ActionList,
    /// The model's own earlier replies, replayed as its turns: each earlier step is the frame it
    /// was shown (a placeholder where the frame is no longer kept) and the reply it gave. For a
    /// model trained on its own transcript, whose replies carry the call blocks as text, so it
    /// goes with calls the session reads itself (`CallFormat`).
    OwnReplies,
}
