//! One trait for the models behind every engine and account, local or cloud.
//!
//! A [`Provider`] is one endpoint (a running engine or a cloud account). A turn pushes events
//! into a [`TurnSink`]; cancellation is dropping the future.

mod caps;
mod event;
mod ids;
mod provider;
mod request;
#[cfg(feature = "testing")]
mod scripted;
mod units;

pub use caps::{
    Batching, Caps, Constraint, CuaSupport, ImageLimits, InputKind, Support, ToolSupport, Zoom,
};
pub use event::{Flow, SafetySignal, StopReason, TurnEnd, TurnEvent, TurnSink, TurnUsage};
pub use ids::{JsonError, JsonText, ModelName, SchemaText, ToolCallId, ToolName, ToolNameError};
pub use provider::{ModelInfo, Provider, ProviderError};
pub use request::{
    Effort, ImageBytes, ImageDetail, ImageInput, Limits, Message, NativeTool, OutputShape, Part,
    Reasoning, Role, ToolCall, ToolChoice, ToolResult, ToolSpec, ToolStatus, TurnRequest,
};
#[cfg(feature = "testing")]
pub use scripted::{Script, ScriptedProvider};
pub use units::{CallIndex, ImageCount, Milli, RetrySeconds, Tokens};
