//! The names of the OpenTelemetry GenAI semantic conventions, and nothing else.
//!
//! `tracing` field names must be literals, so a daemon spells them in its own `macro_rules!` span
//! constructors and a test there asserts the macro's field set equals [`attr::ALL`]. These
//! constants are the one list. The crate has no dependencies, so every repo may depend on it, and
//! no content attribute exists here: `gen_ai.input.messages`, `gen_ai.output.messages`,
//! `gen_ai.system_instructions` and the tool-argument and tool-result attributes are
//! deliberately absent (prompts, completions and arguments are personal and never go on a span).
//!
//! The conventions are still marked development-status upstream and have been renamed before
//! (`gen_ai.system` became `gen_ai.provider.name`); every spelling below is checked against the
//! current registry when the span macros are written (`FINDINGS.md`).
#![forbid(unsafe_code)]

/// Span and metric attribute keys.
pub mod attr {
    pub const OPERATION_NAME: &str = "gen_ai.operation.name";
    pub const PROVIDER_NAME: &str = "gen_ai.provider.name";
    pub const REQUEST_MODEL: &str = "gen_ai.request.model";
    pub const REQUEST_MAX_TOKENS: &str = "gen_ai.request.max_tokens";
    /// Recorded as a decimal string made from a thousandths value, never a stored float.
    pub const REQUEST_TEMPERATURE: &str = "gen_ai.request.temperature";
    pub const REQUEST_TOP_P: &str = "gen_ai.request.top_p";
    pub const REQUEST_STOP: &str = "gen_ai.request.stop_sequences";
    pub const RESPONSE_ID: &str = "gen_ai.response.id";
    pub const RESPONSE_MODEL: &str = "gen_ai.response.model";
    pub const RESPONSE_FINISH: &str = "gen_ai.response.finish_reasons";
    pub const USAGE_INPUT: &str = "gen_ai.usage.input_tokens";
    pub const USAGE_OUTPUT: &str = "gen_ai.usage.output_tokens";
    pub const CONVERSATION_ID: &str = "gen_ai.conversation.id";
    pub const AGENT_NAME: &str = "gen_ai.agent.name";
    pub const TOOL_NAME: &str = "gen_ai.tool.name";
    pub const TOOL_CALL_ID: &str = "gen_ai.tool.call.id";
    pub const TOOL_TYPE: &str = "gen_ai.tool.type";
    pub const ERROR_TYPE: &str = "error.type";
    pub const SERVER_ADDRESS: &str = "server.address";

    /// Every key above, in declaration order: what a span macro's declared fields are tested
    /// against.
    pub const ALL: &[&str] = &[
        OPERATION_NAME,
        PROVIDER_NAME,
        REQUEST_MODEL,
        REQUEST_MAX_TOKENS,
        REQUEST_TEMPERATURE,
        REQUEST_TOP_P,
        REQUEST_STOP,
        RESPONSE_ID,
        RESPONSE_MODEL,
        RESPONSE_FINISH,
        USAGE_INPUT,
        USAGE_OUTPUT,
        CONVERSATION_ID,
        AGENT_NAME,
        TOOL_NAME,
        TOOL_CALL_ID,
        TOOL_TYPE,
        ERROR_TYPE,
        SERVER_ADDRESS,
    ];
}

/// Metric names.
pub mod metric {
    pub const TOKEN_USAGE: &str = "gen_ai.client.token.usage";
    pub const OPERATION_DURATION: &str = "gen_ai.client.operation.duration";
    pub const TIME_TO_FIRST_TOKEN: &str = "gen_ai.server.time_to_first_token";
    pub const TIME_PER_OUTPUT_TOKEN: &str = "gen_ai.server.time_per_output_token";

    pub const ALL: &[&str] = &[
        TOKEN_USAGE,
        OPERATION_DURATION,
        TIME_TO_FIRST_TOKEN,
        TIME_PER_OUTPUT_TOKEN,
    ];
}

/// The `gen_ai.operation.name` values. Streaming is not an operation: it is a span event
/// (`first_token`) and a metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    Chat,
    Embeddings,
    ExecuteTool,
    InvokeAgent,
    TextCompletion,
    GenerateContent,
}

impl Operation {
    pub const ALL: [Operation; 6] = [
        Operation::Chat,
        Operation::Embeddings,
        Operation::ExecuteTool,
        Operation::InvokeAgent,
        Operation::TextCompletion,
        Operation::GenerateContent,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Operation::Chat => "chat",
            Operation::Embeddings => "embeddings",
            Operation::ExecuteTool => "execute_tool",
            Operation::InvokeAgent => "invoke_agent",
            Operation::TextCompletion => "text_completion",
            Operation::GenerateContent => "generate_content",
        }
    }
}

/// The `gen_ai.response.finish_reasons` values. `model-provider` converts its `StopReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finish {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
}

impl Finish {
    pub const ALL: [Finish; 5] = [
        Finish::Stop,
        Finish::Length,
        Finish::ToolCalls,
        Finish::ContentFilter,
        Finish::Error,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Finish::Stop => "stop",
            Finish::Length => "length",
            Finish::ToolCalls => "tool_calls",
            Finish::ContentFilter => "content_filter",
            Finish::Error => "error",
        }
    }
}
