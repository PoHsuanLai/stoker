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
    Audio,
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
