//! One trait for the models behind every engine and account, local or cloud.
//!
//! A [`Provider`] is one endpoint (a running engine or a cloud account). A turn pushes events
//! into a [`TurnSink`]; cancellation is dropping the future.

mod caps;
mod control;
mod embed;
mod event;
mod ids;
mod provider;
mod request;
mod retry;
#[cfg(feature = "testing")]
mod scripted;
mod sequence;
mod shape;
mod units;

pub use caps::{
    Batching, Caps, Constraint, CuaSupport, ImageLimits, InputKind, ShapeWithTools, Support,
    ToolSupport, Zoom,
};
pub use control::{
    EngineExtras, GuidedBackend, KeepAlive, LlamaExtras, OllamaExtras, PromptCache, Sampling,
    ThoughtSeal, ToolParallelism, VllmExtras,
};
pub use embed::{
    EmbedCaps, EmbedEnd, EmbedFault, EmbedPrompts, EmbedRole, EmbedTurn, EmbedVector, Embedder,
    PrefixText, plan_batches,
};
pub use event::{Flow, SafetySignal, StopReason, TurnEnd, TurnEvent, TurnSink, TurnUsage};
pub use ids::{
    JsonError, JsonText, ModelName, OpaqueText, SchemaText, SignatureText, ToolCallId, ToolName,
    ToolNameError,
};
pub use provider::{ModelInfo, Provider, ProviderError};
pub use request::{
    Effort, ImageBytes, ImageDetail, ImageInput, Limits, Message, NativeTool, OutputShape, Part,
    Reasoning, Role, ToolCall, ToolChoice, ToolResult, ToolSpec, ToolStatus, TurnRequest,
};
pub use retry::{RetryClass, RetryPolicy, Retrying, Sleeper, next_wait};
#[cfg(feature = "testing")]
pub use scripted::{Script, ScriptedProvider};
pub use sequence::{SequenceFault, check as check_sequence};
pub use shape::{
    ChoiceText, Extract, Field, FieldName, IdentError, SchemaDialect, Shape, ShapeFault, ShapeKind,
    Variant, VariantName, sanitize_schema,
};
pub use units::{
    Attempt, BatchMax, CallIndex, CharCount, Count, Dims, ImageCount, Knob, Milli, Permille,
    RetrySeconds, Seconds, Seed, ServerStatus, SlotId, Tokens, WaitMs,
};
