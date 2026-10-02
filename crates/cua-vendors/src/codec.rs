//! The codec trait and the closed set of codecs.

use cua_action::{ImageSpace, Size, WireDialect};
use cua_parse::Parsed;
use model_provider::{ImageInput, Part, SafetySignal, ToolCall, ToolSpec};
use serde::{Deserialize, Serialize};

use crate::StepResult;

/// One vendor's computer-use encoding.
pub trait WireCodec {
    /// The tool declaration: a `NativeTool` or `Function` per dialect, for an image of this size.
    fn tools(&self, image: Size<ImageSpace>) -> Vec<ToolSpec>;
    /// Decodes the model's calls. Vendor safety hints may only add asks.
    fn decode(&self, calls: &[ToolCall], safety: &[SafetySignal]) -> Result<Parsed, WireError>;
    /// Encodes the results of the previous step and the next observation as the vendor wants
    /// them (`tool_result`, `computer_call_output`, `function_response`).
    fn results(&self, done: &[StepResult], next: &ImageInput) -> Vec<Part>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum WireError {
    #[error("the call names a tool this dialect does not have")]
    UnknownTool,
    #[error("the call's arguments do not fit the dialect's schema")]
    BadArguments,
}

/// The codecs, one per [`WireDialect`]; a closed set, so an enum rather than `dyn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireCodecs {
    AnthropicToolset20260801,
    AnthropicComputer20251124,
    OpenAiComputer,
    GeminiComputerUse,
}

impl WireCodecs {
    pub fn dialect(&self) -> WireDialect {
        match self {
            WireCodecs::AnthropicToolset20260801 => WireDialect::AnthropicToolset20260801,
            WireCodecs::AnthropicComputer20251124 => WireDialect::AnthropicComputer20251124,
            WireCodecs::OpenAiComputer => WireDialect::OpenAiComputer,
            WireCodecs::GeminiComputerUse => WireDialect::GeminiComputerUse,
        }
    }
}

/// The codec for a dialect.
pub fn codec(dialect: WireDialect) -> WireCodecs {
    match dialect {
        WireDialect::AnthropicToolset20260801 => WireCodecs::AnthropicToolset20260801,
        WireDialect::AnthropicComputer20251124 => WireCodecs::AnthropicComputer20251124,
        WireDialect::OpenAiComputer => WireCodecs::OpenAiComputer,
        WireDialect::GeminiComputerUse => WireCodecs::GeminiComputerUse,
    }
}

impl WireCodec for WireCodecs {
    fn tools(&self, image: Size<ImageSpace>) -> Vec<ToolSpec> {
        let _ = image;
        todo!("WireCodecs::tools: one declaration per vendor")
    }

    fn decode(&self, calls: &[ToolCall], safety: &[SafetySignal]) -> Result<Parsed, WireError> {
        let _ = (calls, safety);
        todo!("WireCodecs::decode: one decoder per vendor")
    }

    fn results(&self, done: &[StepResult], next: &ImageInput) -> Vec<Part> {
        let _ = (done, next);
        todo!("WireCodecs::results: stop-at-first-failure text per vendor")
    }
}
