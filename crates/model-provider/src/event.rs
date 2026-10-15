//! The streaming side of a turn: events pushed into a sink.

use serde::{Deserialize, Serialize};

use crate::{
    CallIndex, FirstTokenLogprobs, ImageCount, ModelName, ThoughtSeal, Tokens, ToolCall,
    ToolCallId, ToolName,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TurnEvent {
    TextDelta(String),
    ThoughtDelta(String),
    /// The seal of the thought just streamed (a signature arrives after its text).
    ThoughtSealed(ThoughtSeal),
    ToolCallStarted {
        index: CallIndex,
        id: ToolCallId,
        name: ToolName,
    },
    ToolCallDelta {
        index: CallIndex,
        fragment: String,
    },
    /// The joined arguments, checked as JSON.
    ToolCallDone(ToolCall),
    /// A vendor's confirmation hint; it may only add asks, never remove one.
    Safety(SafetySignal),
    Usage(TurnUsage),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum SafetySignal {
    RequireConfirmation(String),
    Blocked(String),
}

/// What one turn used, in tokens and images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TurnUsage {
    pub input: Tokens,
    pub output: Tokens,
    /// Input tokens the server served from its prompt cache (llama.cpp `cache_n`); a subset of
    /// `input`.
    pub cached: Tokens,
    pub images: ImageCount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnEnd {
    pub stop: StopReason,
    pub usage: TurnUsage,
    pub served: ModelName,
    /// The first answer token's top-k, when the request asked and the engine answered usably.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_token: Option<FirstTokenLogprobs>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    ContentFilter,
    /// The server ended the turn with a reason this crate does not know (`error`, `abort`, a
    /// gateway's own word): the reply may be cut, so a caller must not treat it as a clean stop.
    Unrecognized,
}

/// A sink's answer to an event: keep going, or end the turn early (with `StopReason::EndTurn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flow {
    Continue,
    Stop,
}

/// Where a provider pushes a turn's events.
pub trait TurnSink: Send {
    fn event(&mut self, event: TurnEvent) -> Flow;

    /// Called once per attempt when the provider is done with the sink, however the attempt ended:
    /// complete, failed, stopped early by this sink, or dropped mid-stream (cancellation). A
    /// caller that bills a turn charges from this, not from `TurnEnd`, which an error or a drop
    /// never delivers. A retried turn reports once per attempt; the sum is the cost. The default
    /// ignores it.
    fn ended(&mut self, report: UsageReport) {
        let _ = report;
    }
}

/// What an attempt is known to have cost when it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
#[non_exhaustive]
pub enum UsageReport {
    /// The server's final count of a complete turn.
    Known(TurnUsage),
    /// The model may have run and no final count arrived: the server sent none, the stream broke,
    /// this sink stopped it, or the caller dropped the turn. `seen` is the most the server had
    /// said (often zero); the provider may still bill for more, so a caller should charge a
    /// pessimistic estimate, never `seen` alone.
    Unknown { seen: TurnUsage },
    /// The request was refused or never reached a model (could not connect, a non-success
    /// answer): nothing ran, nothing to charge.
    Nothing,
}

impl From<StopReason> for genai_names::Finish {
    fn from(stop: StopReason) -> Self {
        match stop {
            StopReason::EndTurn | StopReason::StopSequence => genai_names::Finish::Stop,
            StopReason::ToolUse => genai_names::Finish::ToolCalls,
            StopReason::MaxTokens => genai_names::Finish::Length,
            StopReason::ContentFilter => genai_names::Finish::ContentFilter,
            StopReason::Unrecognized => genai_names::Finish::Error,
        }
    }
}
