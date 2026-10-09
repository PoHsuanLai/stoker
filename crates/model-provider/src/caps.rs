//! What one concrete model can do behind one backend.

use std::collections::BTreeSet;

use cua_action::{CuaDialect, ModelSpace};
use serde::{Deserialize, Serialize};
use vision_prep::ResizeRule;

use crate::{ImageCount, Tokens};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    pub inputs: BTreeSet<InputKind>,
    pub tools: ToolSupport,
    pub output: BTreeSet<Constraint>,
    pub reasoning: Support,
    pub streaming: Support,
    pub images: ImageLimits,
    pub context: Tokens,
    pub max_output: Tokens,
    pub computer_use: CuaSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Text,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSupport {
    Absent,
    /// The provider has its own tool protocol (`ToolSpec::Native`).
    Native,
    /// The engine parses tool calls out of the model's text.
    ServerParsed,
}

/// Output constraints an engine can enforce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Constraint {
    JsonSchema,
    Regex,
    Lark,
    /// llama.cpp's grammar format.
    Gbnf,
    /// The reply is exactly one of a list of strings.
    Choice,
    /// The engine guarantees only that the reply is one JSON object (`response_format` of type
    /// `json_object`), not which one: the schema travels in the prompt and the caller validates
    /// the reply. For models that take no `json_schema`.
    JsonObject,
}

/// Whether a server still calls tools when a response format is set on the same request. Some
/// servers suppress tool calls then (`response_format` plus `tools`); the extraction mode
/// picks its route by it. A property of the engine flavor, not of the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeWithTools {
    /// Both at once.
    Together,
    /// The format is sent only after a tool result exists.
    AfterResult,
    /// Never both.
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Absent,
    Present,
}

/// How a model wants images: how many per prompt, how they are resized, where its points live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageLimits {
    pub per_prompt: ImageCount,
    pub rule: ResizeRule,
    pub space: ModelSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum CuaSupport {
    Absent,
    Dialect {
        dialect: CuaDialect,
        batching: Batching,
        zoom: Zoom,
    },
}

/// Whether a model answers one action or several per step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Batching {
    One,
    Many,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zoom {
    Absent,
    Native,
}
