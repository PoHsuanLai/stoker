//! Dialect names, so that capability records and the catalog can refer to a model's action
//! language without depending on the code that speaks it.

use serde::{Deserialize, Serialize};

use crate::GridMax;

/// A vendor's own computer-use tool protocol (encoded by `cua-vendors`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireDialect {
    AnthropicToolset20260801,
    AnthropicComputer20251124,
    OpenAiComputer,
    GeminiComputerUse,
}

/// A local model that answers in text: `Thought: .. Action: click(start_box='(x,y)')`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDialect {
    UiTars15,
}

/// A local model whose engine parses tool calls server-side; `cua-parse` reads the calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolDialect {
    /// Qwen's `computer_use` function with an `action` argument.
    QwenComputerUse,
    /// Holo 3.1: the engine parses the Qwen3-coder XML call format into tool calls, and the
    /// action schema is the one the session's prompt declares (its chat template names no
    /// computer-use function).
    Holo31,
}

/// How a model's actions are written and parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum CuaDialect {
    Wire(WireDialect),
    Text(TextDialect),
    Tool(ToolDialect),
}

/// Where a dialect's points live: the pixels of the image shown, or a normalised grid over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ModelSpace {
    Image,
    Grid(GridMax),
}
