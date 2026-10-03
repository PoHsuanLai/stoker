//! The request side of a turn.

use core::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vision_prep::MediaType;

use crate::{
    EngineExtras, JsonText, ModelName, Sampling, SchemaText, ThoughtSeal, Tokens, ToolCallId,
    ToolName, ToolParallelism,
};
use cua_action::WireDialect;

/// One request to one concrete model: sampling, native tools, prepared images.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRequest {
    pub model: ModelName,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub tool_choice: ToolChoice,
    pub tool_calls: ToolParallelism,
    pub output: OutputShape,
    pub limits: Limits,
    pub sampling: Sampling,
    pub reasoning: Reasoning,
    /// Knobs of one engine flavor; `EngineExtras::None` for none.
    pub engine: EngineExtras,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Part {
    Text(String),
    Image(ImageInput),
    /// Reasoning content handed back where the wire wants it, with its seal when the provider
    /// gave one (signed or redacted thinking must return unchanged when tools are in play).
    Thought {
        text: String,
        seal: ThoughtSeal,
    },
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

/// An encoded image. Serialises its bytes as base64 text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInput {
    pub media: MediaType,
    pub bytes: ImageBytes,
    pub detail: ImageDetail,
}

/// The bytes of an encoded image (a screenshot, so Debug shows the length only).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ImageBytes(pub Vec<u8>);

impl fmt::Debug for ImageBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ImageBytes(<{} bytes>)", self.0.len())
    }
}

impl Serialize for ImageBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for ImageBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD
            .decode(text)
            .map(ImageBytes)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageDetail {
    Auto,
    Original,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: ToolCallId,
    pub name: ToolName,
    pub input: JsonText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    pub id: ToolCallId,
    pub status: ToolStatus,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToolSpec {
    Function {
        name: ToolName,
        description: String,
        parameters: SchemaText,
    },
    /// A provider-built tool such as `computer_toolset_20260801`.
    Native(NativeTool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTool {
    pub dialect: WireDialect,
    pub config: JsonText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    Never,
    Required,
    Named(ToolName),
}

/// What shape the reply must take. `Regex`, `Lark`, `Gbnf` and `Choice` are for local engines
/// only: llama-server takes GBNF (not Lark), vLLM's guided backends take regex, choice and
/// Lark. `Choice` is a reply that is exactly one of the listed strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum OutputShape {
    Free,
    JsonSchema(SchemaText),
    Regex(String),
    Lark(String),
    Gbnf(String),
    Choice(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    pub max_output: Tokens,
    pub stop: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Reasoning {
    /// Whatever the engine and the model's template do when nothing is said: a codec sends no
    /// reasoning field for it. (A caller that wants a short turn asks for `Off`.)
    EngineDefault,
    Off,
    On(Effort),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
}
