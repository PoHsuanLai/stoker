//! The stream decoder: SSE data frames of `chat.completion.chunk` into turn events.

use std::collections::{BTreeMap, BTreeSet};

use model_provider::{
    CallIndex, ModelName, ProviderError, StopReason, Tokens, TurnEnd, TurnEvent, TurnUsage,
};
use model_wire::{ChatDecoder, CodecError};
use serde_json::Value;

use crate::Flavor;
use crate::assemble::{Closed, Fragment, IfMalformed, Pending};
use crate::envelope::envelope_error;

/// How far the stream has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No chunk of this wire has been read yet: a bare `[DONE]` here fabricates nothing.
    Nothing,
    Streaming,
    /// A finish reason arrived; only a usage chunk and `[DONE]` may follow.
    Finished(StopReason),
}

/// Turns SSE data frames into turn events. Tool-call argument fragments are joined per index and
/// checked as JSON; `[DONE]`, or a finish reason followed by the usage chunk, ends the stream; a
/// malformed frame is `Unreadable`, never a panic.
///
/// Servers differ in small ways that the decoder accepts all at once: reasoning as
/// `reasoning_content` or `reasoning` (the first wins when both are present), `content` as a string
/// or an array of text parts, arguments as a string or an object, calls with no id (one is
/// minted), a whole call in one chunk, an index reused for a second call, a finish reason
/// followed by a usage-only chunk with no choices, and an error envelope inside a 200.
#[derive(Debug, Clone)]
pub struct StreamDecoder {
    flavor: Flavor,
    served: ModelName,
    phase: Phase,
    done: Done,
    open: BTreeMap<u32, Pending>,
    ids: BTreeSet<String>,
    delivered: u16,
    opened: u16,
    usage: Option<TurnUsage>,
    fault: Option<ProviderError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Done {
    No,
    Marker,
}

impl StreamDecoder {
    pub fn new(flavor: Flavor, served: ModelName) -> Self {
        Self {
            flavor,
            served,
            phase: Phase::Nothing,
            done: Done::No,
            open: BTreeMap::new(),
            ids: BTreeSet::new(),
            delivered: 0,
            opened: 0,
            usage: None,
            fault: None,
        }
    }

    fn chunk(&mut self, value: &Value) -> Result<Vec<TurnEvent>, CodecError> {
        if let Some(error) = envelope_error(value) {
            self.fault = Some(error);
            return Err(CodecError::Unreadable);
        }
        let choices = value.get("choices").and_then(Value::as_array);
        let usage = value.get("usage").filter(|u| !u.is_null());
        if choices.is_none() && usage.is_none() {
            // Not a chunk of this wire (a comment, a ping, a vendor event): skipped, and not
            // counted as a valid chunk.
            return Ok(Vec::new());
        }
        if self.phase == Phase::Nothing {
            self.phase = Phase::Streaming;
        }
        let mut events = Vec::new();
        let first = choices
            .into_iter()
            .flatten()
            .find(|c| c.get("index").and_then(Value::as_u64).unwrap_or(0) == 0);
        if let Some(choice) = first {
            if let Some(delta) = choice.get("delta").filter(|d| !d.is_null()) {
                events.extend(self.delta(delta)?);
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                events.extend(self.finish_reason(reason)?);
            }
        }
        if usage.is_some() {
            let usage = read_usage(self.flavor, value);
            self.usage = Some(usage);
            events.push(TurnEvent::Usage(usage));
        }
        Ok(events)
    }

    fn delta(&mut self, delta: &Value) -> Result<Vec<TurnEvent>, CodecError> {
        let object = delta.as_object().ok_or(CodecError::Unreadable)?;
        let mut events = Vec::new();
        let thought = ["reasoning_content", "reasoning"]
            .iter()
            .filter_map(|key| object.get(*key))
            .find_map(|v| v.as_str().filter(|s| !s.is_empty()));
        events.extend(thought.map(|t| TurnEvent::ThoughtDelta(t.to_owned())));
        let text = content_text(object.get("content"))?;
        events.extend((!text.is_empty()).then_some(TurnEvent::TextDelta(text)));
        let refusal = object
            .get("refusal")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());
        events.extend(refusal.map(|t| TurnEvent::TextDelta(t.to_owned())));
        if let Some(calls) = object.get("tool_calls").filter(|c| !c.is_null()) {
            let calls = calls.as_array().ok_or(CodecError::Unreadable)?;
            for (position, call) in calls.iter().enumerate() {
                events.extend(self.fragment(position, call)?);
            }
        }
        Ok(events)
    }

    fn fragment(&mut self, position: usize, call: &Value) -> Result<Vec<TurnEvent>, CodecError> {
        let wire = match call.get("index") {
            None | Some(Value::Null) => {
                u32::try_from(position).map_err(|_| CodecError::Unreadable)?
            }
            Some(index) => index
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(CodecError::Unreadable)?,
        };
        let function = call.get("function").filter(|f| !f.is_null());
        let text = |v: Option<&Value>, key: &str| {
            v.and_then(|v| v.get(key))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let fragment = Fragment {
            id: text(Some(call), "id"),
            name: text(function, "name"),
            arguments: arguments_text(function.and_then(|f| f.get("arguments")))?,
        };
        let mut events = Vec::new();
        let displaced = match self.open.get(&wire) {
            Some(existing) if existing.evicted_by(&fragment) => self.open.remove(&wire),
            _ => None,
        };
        if let Some(displaced) = displaced {
            events.extend(self.close_one(displaced, IfMalformed::EmptyObject)?);
        }
        let ordinal = CallIndex(self.opened);
        if !self.open.contains_key(&wire) {
            self.opened = self.opened.saturating_add(1);
        }
        let pending = self
            .open
            .entry(wire)
            .or_insert_with(|| Pending::new(ordinal));
        events.extend(pending.absorb(fragment, &mut self.ids)?);
        Ok(events)
    }

    fn close_one(
        &mut self,
        pending: Pending,
        how: IfMalformed,
    ) -> Result<Vec<TurnEvent>, CodecError> {
        Ok(match pending.close(how)? {
            Closed::Done(call) => {
                self.delivered = self.delivered.saturating_add(1);
                vec![TurnEvent::ToolCallDone(call)]
            }
            Closed::Dropped | Closed::Open => Vec::new(),
        })
    }

    /// Closes every open call: when the provider said the turn was complete, bad arguments are a
    /// fault; when the budget cut it, a half-formed call is dropped.
    fn close_all(&mut self, how: IfMalformed) -> Result<Vec<TurnEvent>, CodecError> {
        let open = std::mem::take(&mut self.open);
        let mut ordered: Vec<Pending> = open.into_values().collect();
        ordered.sort_by_key(Pending::ordinal);
        let mut events = Vec::new();
        for pending in ordered {
            events.extend(self.close_one(pending, how)?);
        }
        Ok(events)
    }

    fn finish_reason(&mut self, reason: &str) -> Result<Vec<TurnEvent>, CodecError> {
        let how = match reason {
            "length" => IfMalformed::Drop,
            _ => IfMalformed::Fail,
        };
        let events = self.close_all(how)?;
        let stop = match reason {
            "length" => StopReason::MaxTokens,
            "content_filter" => StopReason::ContentFilter,
            "tool_calls" | "function_call" => StopReason::ToolUse,
            // Some servers say `stop` for a turn that made calls.
            _ if self.delivered > 0 => StopReason::ToolUse,
            _ => StopReason::EndTurn,
        };
        self.phase = Phase::Finished(stop);
        Ok(events)
    }

    fn marker(&mut self) -> Result<Vec<TurnEvent>, CodecError> {
        self.done = Done::Marker;
        if self.phase != Phase::Streaming {
            return Ok(Vec::new());
        }
        let events = self.close_all(IfMalformed::Fail)?;
        let stop = if self.delivered > 0 {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        };
        self.phase = Phase::Finished(stop);
        Ok(events)
    }
}

impl ChatDecoder for StreamDecoder {
    fn feed(&mut self, frame: &str) -> Result<Vec<TurnEvent>, CodecError> {
        let frame = frame.trim();
        if frame.is_empty() || self.done == Done::Marker {
            return Ok(Vec::new());
        }
        if frame == "[DONE]" {
            return self.marker();
        }
        let value: Value = serde_json::from_str(frame).map_err(|_| CodecError::Unreadable)?;
        if !value.is_object() {
            return Err(CodecError::Unreadable);
        }
        self.chunk(&value)
    }

    fn fault(&self) -> Option<ProviderError> {
        self.fault.clone()
    }

    fn finish(self) -> Result<TurnEnd, CodecError> {
        match self.phase {
            Phase::Finished(stop) => Ok(TurnEnd {
                stop,
                usage: self.usage.unwrap_or_default(),
                served: self.served,
            }),
            Phase::Nothing => Err(CodecError::Unreadable),
            Phase::Streaming => Err(CodecError::Truncated),
        }
    }
}

/// `content` as text: a string, or an array of parts whose `text` is joined.
fn content_text(content: Option<&Value>) -> Result<String, CodecError> {
    match content {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(Value::Array(parts)) => Ok(parts
            .iter()
            .filter_map(|p| p.as_str().or_else(|| p.get("text").and_then(Value::as_str)))
            .collect()),
        Some(_) => Err(CodecError::Unreadable),
    }
}

/// Arguments as text: a string, or a JSON object a gateway sent unstringified.
fn arguments_text(arguments: Option<&Value>) -> Result<Option<String>, CodecError> {
    match arguments {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok((!text.is_empty()).then(|| text.clone())),
        Some(object @ Value::Object(_)) => Ok(Some(object.to_string())),
        Some(_) => Err(CodecError::Unreadable),
    }
}

fn tokens(value: Option<&Value>) -> Tokens {
    Tokens(
        value
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX)),
    )
}

/// The usage of a chunk. Cached input comes from llama-server's `timings.cache_n` or the
/// `prompt_tokens_details.cached_tokens` of the others (the flavor's own first); it never exceeds
/// the input.
fn read_usage(flavor: Flavor, chunk: &Value) -> TurnUsage {
    let usage = chunk.get("usage");
    let input = tokens(usage.and_then(|u| u.get("prompt_tokens")));
    let output = tokens(usage.and_then(|u| u.get("completion_tokens")));
    let details = usage
        .and_then(|u| u.get("prompt_tokens_details"))
        .and_then(|d| d.get("cached_tokens"));
    let timings = chunk.get("timings").and_then(|t| t.get("cache_n"));
    let (first, second) = match flavor {
        Flavor::LlamaServer => (timings, details),
        Flavor::Vllm | Flavor::LiteLlm | Flavor::OpenRouter => (details, timings),
    };
    let cached = first.or(second).map_or(Tokens(0), |v| tokens(Some(v)));
    TurnUsage {
        input,
        output,
        cached: cached.min(input),
        ..TurnUsage::default()
    }
}
