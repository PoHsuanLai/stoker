//! What differs between the servers that share the chat-completions wire, as data.
//!
//! A table of closed enums, one row per flavor, never a `bool`. The rows come from rig's dialect
//! constants and our own survey; entries marked "to verify" are pinned by the first recorded
//! fixture of that engine (`FINDINGS.md`).

use model_http::RouteRoot;
use model_provider::ShapeWithTools;
use serde::{Deserialize, Serialize};

use crate::Flavor;
use crate::logprobs::LogprobsAsk;

/// Whether the request asks for a usage chunk (`stream_options.include_usage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageAsk {
    Request,
    /// The server always sends usage and rejects the option (OpenRouter).
    Never,
}

/// Which `tool_choice` values the server honours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolNaming {
    /// A named function too.
    Any,
    /// `auto`, `none` and `required` only: a named function is silently treated as `auto`
    /// (llama-server), so a request for one is refused at encode.
    AutoOnly,
}

/// Where an image in a tool result goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolImages {
    /// Inside the `role: "tool"` message.
    InToolMessage,
    /// Moved to a user message that follows it.
    NextUserMessage,
}

/// Whether the `dimensions` field of an embeddings request is honoured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionsField {
    Send,
    /// The server ignores it (llama.cpp): never send one, or the reported width lies.
    Ignored,
}

/// One flavor's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Quirks {
    pub usage: UsageAsk,
    pub tool_naming: ToolNaming,
    pub tool_images: ToolImages,
    pub shape_with_tools: ShapeWithTools,
    /// Where `describe` reads: llama-server keeps `/props` at the server root.
    pub describe_root: RouteRoot,
    pub dimensions: DimensionsField,
    /// Whether `logprobs`/`top_logprobs` are sent for a first-token ask.
    pub logprobs: LogprobsAsk,
}

impl Flavor {
    /// The flavor's row.
    pub const fn quirks(self) -> Quirks {
        match self {
            Flavor::LlamaServer => Quirks {
                usage: UsageAsk::Request,
                tool_naming: ToolNaming::AutoOnly,
                tool_images: ToolImages::InToolMessage,
                // To verify: whether `response_format` suppresses tool calls on llama-server.
                shape_with_tools: ShapeWithTools::AfterResult,
                describe_root: RouteRoot::Server,
                dimensions: DimensionsField::Ignored,
                logprobs: LogprobsAsk::Request,
            },
            Flavor::Vllm => Quirks {
                usage: UsageAsk::Request,
                tool_naming: ToolNaming::Any,
                tool_images: ToolImages::NextUserMessage,
                // To verify against vLLM's structured-output backends.
                shape_with_tools: ShapeWithTools::Together,
                describe_root: RouteRoot::Base,
                dimensions: DimensionsField::Send,
                logprobs: LogprobsAsk::Request,
            },
            Flavor::LiteLlm => Quirks {
                usage: UsageAsk::Request,
                tool_naming: ToolNaming::Any,
                tool_images: ToolImages::NextUserMessage,
                shape_with_tools: ShapeWithTools::AfterResult,
                describe_root: RouteRoot::Base,
                dimensions: DimensionsField::Send,
                logprobs: LogprobsAsk::Unsupported,
            },
            Flavor::OpenRouter => Quirks {
                usage: UsageAsk::Never,
                tool_naming: ToolNaming::Any,
                tool_images: ToolImages::NextUserMessage,
                shape_with_tools: ShapeWithTools::AfterResult,
                describe_root: RouteRoot::Base,
                dimensions: DimensionsField::Send,
                logprobs: LogprobsAsk::Unsupported,
            },
        }
    }
}
