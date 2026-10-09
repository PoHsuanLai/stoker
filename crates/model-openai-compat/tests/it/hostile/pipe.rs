//! The path the driver takes: bytes in chunks, through the SSE framer, into the stream decoder.

use std::collections::BTreeSet;

use model_http::{SseDecoder, SseError};
use model_openai_compat::{CodecError, Flavor, LeakMarker, StreamDecoder};
use model_provider::{ModelName, StopReason, TurnEnd, TurnEvent};
use model_wire::ChatDecoder;
use serde_json::{Value, json};

/// Why a reply was refused: the framing or the decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Framing(SseError),
    Decoding(CodecError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub events: Vec<TurnEvent>,
    pub leak: Option<LeakMarker>,
    pub end: Result<TurnEnd, Refused>,
}

pub fn decoder() -> StreamDecoder {
    StreamDecoder::new(Flavor::Vllm, ModelName("holo".into()))
}

/// Feeds `chunks` in order, then the end of the stream.
pub fn run(chunks: &[&[u8]]) -> Reply {
    let (mut framer, mut decoder) = (SseDecoder::new(), decoder());
    let mut events = Vec::new();
    let failed = |events, decoder: &StreamDecoder, why| Reply {
        events,
        leak: decoder.leaked_call(),
        end: Err(why),
    };
    for chunk in chunks {
        let frames = match framer.feed(chunk) {
            Ok(frames) => frames,
            Err(e) => return failed(events, &decoder, Refused::Framing(e)),
        };
        for frame in frames {
            match decoder.feed(&frame.data) {
                Ok(more) => events.extend(more),
                Err(e) => return failed(events, &decoder, Refused::Decoding(e)),
            }
        }
    }
    for frame in framer.finish() {
        match decoder.feed(&frame.data) {
            Ok(more) => events.extend(more),
            Err(e) => return failed(events, &decoder, Refused::Decoding(e)),
        }
    }
    let leak = decoder.leaked_call();
    let end = decoder.finish().map_err(Refused::Decoding);
    Reply { events, leak, end }
}

/// `bytes` cut at `cuts` (each taken modulo the length).
pub fn run_cut(bytes: &[u8], cuts: &[usize]) -> Reply {
    let mut points: Vec<usize> = cuts.iter().map(|c| c % (bytes.len() + 1)).collect();
    points.extend([0, bytes.len()]);
    points.sort_unstable();
    let chunks: Vec<&[u8]> = points.windows(2).map(|w| &bytes[w[0]..w[1]]).collect();
    run(&chunks)
}

pub fn done(reply: &Reply) -> Vec<&model_provider::ToolCall> {
    reply
        .events
        .iter()
        .filter_map(|e| match e {
            TurnEvent::ToolCallDone(call) => Some(call),
            _ => None,
        })
        .collect()
}

/// What must hold of every reply whatever the bytes: a delivered call was started first, has a
/// unique id and arguments that are a whole JSON object; the usage never says cached above input;
/// a turn that ends cleanly after delivering a call says it used a tool or ran out of tokens.
pub fn assert_sound(reply: &Reply) {
    let mut started = BTreeSet::new();
    let mut finished = BTreeSet::new();
    for event in &reply.events {
        match event {
            TurnEvent::ToolCallStarted { id, .. } => {
                assert!(started.insert(id.0.clone()), "id started twice: {id:?}");
            }
            TurnEvent::ToolCallDone(call) => {
                assert!(started.contains(&call.id.0), "done without start");
                assert!(finished.insert(call.id.0.clone()), "done twice");
                let parsed: Value =
                    serde_json::from_str(call.input.as_str()).expect("delivered arguments parse");
                assert!(parsed.is_object(), "arguments are an object");
            }
            TurnEvent::Usage(u) => assert!(u.cached <= u.input),
            _ => {}
        }
    }
    if let Ok(end) = &reply.end {
        assert!(end.usage.cached <= end.usage.input);
        // A call that arrived whole before a `length` cut stands (its arguments are a complete
        // object); the turn then says `MaxTokens`.
        if !finished.is_empty() {
            assert!(matches!(
                end.stop,
                StopReason::ToolUse | StopReason::MaxTokens
            ));
        }
        // A clean end leaves no call half-delivered, except one the token budget cut off (a
        // started call is then dropped, never completed).
        if end.stop != StopReason::MaxTokens {
            assert_eq!(
                started.len(),
                finished.len(),
                "a started call was never done"
            );
        }
    }
}

pub fn sse(frames: &[String]) -> Vec<u8> {
    let mut body: String = frames.iter().map(|f| format!("data: {f}\n\n")).collect();
    body.push_str("data: [DONE]\n\n");
    body.into_bytes()
}

pub fn chunk(delta: Value, finish: Option<&str>) -> String {
    json!({"id":"c","object":"chat.completion.chunk","model":"holo",
           "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    .to_string()
}

pub fn call(index: u32, id: Option<&str>, name: Option<&str>, args: Option<&str>) -> Value {
    let mut c = json!({"index": index, "type": "function", "function": {}});
    if let Some(id) = id {
        c["id"] = json!(id);
    }
    if let Some(name) = name {
        c["function"]["name"] = json!(name);
    }
    if let Some(args) = args {
        c["function"]["arguments"] = json!(args);
    }
    c
}

pub fn calls(list: Vec<Value>) -> Value {
    json!({ "tool_calls": list })
}

/// A valid reply with two calls, text with multi-byte characters, a thought, and usage.
pub fn scenario() -> Vec<String> {
    vec![
        chunk(
            json!({"role":"assistant","reasoning_content":"思考 "}),
            None,
        ),
        chunk(json!({"content":"héllo 世界 "}), None),
        chunk(
            calls(vec![call(0, Some("a"), Some("f"), Some("{\"x\""))]),
            None,
        ),
        chunk(calls(vec![call(1, Some("b"), Some("g"), Some("{}"))]), None),
        chunk(calls(vec![call(0, None, None, Some(":\"ü\"}"))]), None),
        chunk(json!({}), Some("tool_calls")),
        json!({"id":"c","choices":[],"usage":{"prompt_tokens":9,"completion_tokens":4}})
            .to_string(),
    ]
}
