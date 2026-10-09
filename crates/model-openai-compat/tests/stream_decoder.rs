//! The stream decoder: tool-call assembly (rig's §3.3), the quirk cases (§3.2) and the streaming
//! conformance list (§3.7), over frames written by hand.

use model_openai_compat::{CodecError, Flavor, StreamDecoder};
use model_provider::{
    CallIndex, JsonText, ModelName, ProviderError, StopReason, Tokens, ToolCall, ToolCallId,
    ToolName, TurnEnd, TurnEvent, TurnUsage,
};
use model_wire::ChatDecoder;
use proptest::prelude::*;
use serde_json::{Value, json};

fn decoder(flavor: Flavor) -> StreamDecoder {
    StreamDecoder::new(flavor, ModelName("holo".into()))
}

/// Everything a stream produced: events, then either the end or the error.
#[derive(Debug, PartialEq)]
struct Run {
    events: Vec<TurnEvent>,
    outcome: Result<TurnEnd, CodecError>,
}

/// Feeds frames until one fails, then asks for the end.
fn run_with(flavor: Flavor, frames: &[String]) -> Run {
    let mut d = decoder(flavor);
    let mut events = Vec::new();
    for frame in frames {
        match d.feed(frame) {
            Ok(more) => events.extend(more),
            Err(error) => {
                return Run {
                    events,
                    outcome: Err(error),
                };
            }
        }
    }
    Run {
        events,
        outcome: d.finish(),
    }
}

fn run(frames: &[String]) -> Run {
    run_with(Flavor::Vllm, frames)
}

fn chunk(delta: Value, finish: Option<&str>) -> String {
    json!({"id":"c","object":"chat.completion.chunk","model":"holo",
           "choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    .to_string()
}

fn usage_chunk(usage: Value) -> String {
    json!({"id":"c","choices":[],"usage":usage}).to_string()
}

fn call_delta(index: u32, id: Option<&str>, name: Option<&str>, args: Option<&str>) -> Value {
    let mut call = json!({"index": index, "type": "function", "function": {}});
    if let Some(id) = id {
        call["id"] = json!(id);
    }
    if let Some(name) = name {
        call["function"]["name"] = json!(name);
    }
    if let Some(args) = args {
        call["function"]["arguments"] = json!(args);
    }
    json!({"tool_calls": [call]})
}

fn done() -> String {
    "[DONE]".to_owned()
}

fn text(s: &str) -> TurnEvent {
    TurnEvent::TextDelta(s.into())
}

fn started(index: u16, id: &str, name: &str) -> TurnEvent {
    TurnEvent::ToolCallStarted {
        index: CallIndex(index),
        id: ToolCallId(id.into()),
        name: ToolName::new(name).unwrap(),
    }
}

fn delta(index: u16, fragment: &str) -> TurnEvent {
    TurnEvent::ToolCallDelta {
        index: CallIndex(index),
        fragment: fragment.into(),
    }
}

fn finished(id: &str, name: &str, input: &str) -> TurnEvent {
    TurnEvent::ToolCallDone(ToolCall {
        id: ToolCallId(id.into()),
        name: ToolName::new(name).unwrap(),
        input: JsonText::new(input).unwrap(),
    })
}

fn end(stop: StopReason, usage: TurnUsage) -> Result<TurnEnd, CodecError> {
    Ok(TurnEnd {
        stop,
        usage,
        served: ModelName("holo".into()),
        first_token: None,
    })
}

fn plain(stop: StopReason) -> Result<TurnEnd, CodecError> {
    end(stop, TurnUsage::default())
}

#[test]
fn text_streams_and_ends_on_a_finish_reason() {
    let got = run(&[
        chunk(json!({"role":"assistant","content":""}), None),
        chunk(json!({"content":"Hel"}), None),
        chunk(json!({"content":"lo"}), None),
        chunk(json!({}), Some("stop")),
        done(),
    ]);
    assert_eq!(
        got,
        Run {
            events: vec![text("Hel"), text("lo")],
            outcome: plain(StopReason::EndTurn)
        }
    );
}

#[test]
fn finish_reasons_map_to_stop_reasons() {
    const CASES: &[(&str, StopReason)] = &[
        ("stop", StopReason::EndTurn),
        ("length", StopReason::MaxTokens),
        ("content_filter", StopReason::ContentFilter),
        ("tool_calls", StopReason::ToolUse),
        ("function_call", StopReason::ToolUse),
        ("something_new", StopReason::EndTurn),
    ];
    for (reason, stop) in CASES {
        let got = run(&[chunk(json!({"content":"x"}), Some(reason))]);
        assert_eq!(got.outcome, plain(*stop), "{reason}");
    }
}

#[test]
fn a_finish_reason_alone_closes_the_stream_and_done_adds_nothing() {
    let with_done = run(&[chunk(json!({"content":"x"}), Some("stop")), done()]);
    let without = run(&[chunk(json!({"content":"x"}), Some("stop"))]);
    assert_eq!(with_done, without);
}

#[test]
fn done_without_a_finish_reason_closes_a_stream_that_had_chunks() {
    let got = run(&[chunk(json!({"content":"x"}), None), done()]);
    assert_eq!(got.outcome, plain(StopReason::EndTurn));
}

#[test]
fn the_usage_only_trailing_chunk_with_no_choices() {
    let usage = json!({"prompt_tokens": 40, "completion_tokens": 7, "total_tokens": 47});
    let got = run(&[
        chunk(json!({"content":"x"}), Some("stop")),
        usage_chunk(usage),
        done(),
    ]);
    let want = TurnUsage {
        input: Tokens(40),
        output: Tokens(7),
        ..TurnUsage::default()
    };
    assert_eq!(got.events, vec![text("x"), TurnEvent::Usage(want)]);
    assert_eq!(got.outcome, end(StopReason::EndTurn, want));
}

#[test]
fn usage_variants_are_reported_or_absent() {
    let cases: Vec<(&str, Flavor, Value, TurnUsage)> = vec![
        ("absent", Flavor::Vllm, Value::Null, TurnUsage::default()),
        ("null", Flavor::Vllm, json!({}), TurnUsage::default()),
        (
            "vllm cached",
            Flavor::Vllm,
            json!({"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":8}}),
            TurnUsage {
                input: Tokens(10),
                output: Tokens(2),
                cached: Tokens(8),
                ..Default::default()
            },
        ),
        (
            "cached clamped to input",
            Flavor::Vllm,
            json!({"prompt_tokens":10,"completion_tokens":2,"prompt_tokens_details":{"cached_tokens":99}}),
            TurnUsage {
                input: Tokens(10),
                output: Tokens(2),
                cached: Tokens(10),
                ..Default::default()
            },
        ),
        (
            "null details",
            Flavor::Vllm,
            json!({"prompt_tokens":3,"completion_tokens":1,"prompt_tokens_details":null}),
            TurnUsage {
                input: Tokens(3),
                output: Tokens(1),
                ..Default::default()
            },
        ),
        (
            "giant count saturates",
            Flavor::Vllm,
            json!({"prompt_tokens":1u64 << 40,"completion_tokens":1}),
            TurnUsage {
                input: Tokens(u32::MAX),
                output: Tokens(1),
                ..Default::default()
            },
        ),
    ];
    for (label, flavor, usage, want) in cases {
        let mut frames = vec![chunk(json!({"content":"x"}), Some("stop"))];
        if !usage.is_null() {
            frames.push(usage_chunk(usage));
        }
        let got = run_with(flavor, &frames);
        assert_eq!(got.outcome, end(StopReason::EndTurn, want), "{label}");
    }
}

#[test]
fn llama_server_reports_cached_tokens_in_timings() {
    let frame = json!({"choices":[],"usage":{"prompt_tokens":30,"completion_tokens":4},"timings":{"cache_n":24,"prompt_n":6}}).to_string();
    let frames = [chunk(json!({"content":"x"}), Some("stop")), frame];
    let want = TurnUsage {
        input: Tokens(30),
        output: Tokens(4),
        cached: Tokens(24),
        ..Default::default()
    };
    assert_eq!(
        run_with(Flavor::LlamaServer, &frames).outcome,
        end(StopReason::EndTurn, want)
    );
    // The same fields on another flavor are read too, when its own spelling is absent.
    assert_eq!(
        run_with(Flavor::Vllm, &frames).outcome,
        end(StopReason::EndTurn, want)
    );
}

#[test]
fn reasoning_arrives_as_either_key_and_the_first_wins_when_both_are_present() {
    let got = run(&[
        chunk(json!({"reasoning_content":"a"}), None),
        chunk(json!({"reasoning":"b"}), None),
        chunk(json!({"reasoning_content":"c","reasoning":"IGNORED"}), None),
        chunk(json!({"reasoning_content":"","content":"x"}), Some("stop")),
    ]);
    let thought = |s: &str| TurnEvent::ThoughtDelta(s.into());
    assert_eq!(
        got.events,
        vec![thought("a"), thought("b"), thought("c"), text("x")]
    );
}

#[test]
fn content_may_be_an_array_of_parts_and_a_refusal_is_text() {
    let got = run(&[
        chunk(
            json!({"content":[{"type":"text","text":"he"},{"type":"text","text":"llo"}]}),
            None,
        ),
        chunk(json!({"content":null,"refusal":"I cannot"}), Some("stop")),
    ]);
    assert_eq!(got.events, vec![text("hello"), text("I cannot")]);
    assert_eq!(got.outcome, plain(StopReason::EndTurn));
}

#[test]
fn a_delta_less_choice_prelude_is_a_noop() {
    let frames = [
        json!({"choices":[{"index":0}]}).to_string(),
        chunk(json!({}), None),
        chunk(json!({"role":"assistant"}), None),
        json!({"choices":[{"index":0,"delta":null,"finish_reason":null}]}).to_string(),
        chunk(json!({"content":"x"}), Some("stop")),
    ];
    assert_eq!(
        run(&frames),
        Run {
            events: vec![text("x")],
            outcome: plain(StopReason::EndTurn)
        }
    );
}

#[test]
fn a_second_choice_is_ignored() {
    let frame = json!({"choices":[{"index":1,"delta":{"content":"other"},"finish_reason":"stop"},{"index":0,"delta":{"content":"mine"},"finish_reason":"stop"}]}).to_string();
    assert_eq!(run(&[frame]).events, vec![text("mine")]);
}

// ---- tool calls ----

#[test]
fn a_call_streams_by_fragments_and_closes_at_tool_calls() {
    let got = run(&[
        chunk(
            call_delta(0, Some("call_1"), Some("mail.search"), Some("")),
            None,
        ),
        chunk(call_delta(0, None, None, Some("{\"q\":")), None),
        chunk(call_delta(0, None, None, Some("\"bill\"}")), None),
        chunk(json!({}), Some("tool_calls")),
        done(),
    ]);
    assert_eq!(
        got.events,
        vec![
            started(0, "call_1", "mail.search"),
            delta(0, "{\"q\":"),
            delta(0, "\"bill\"}"),
            finished("call_1", "mail.search", "{\"q\":\"bill\"}")
        ]
    );
    assert_eq!(got.outcome, plain(StopReason::ToolUse));
}

#[test]
fn parallel_tool_calls_are_joined_by_index_and_delivered_in_order() {
    let got = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("{\"x\"")), None),
        chunk(call_delta(1, Some("b"), Some("g"), Some("{\"y\"")), None),
        chunk(call_delta(0, None, None, Some(":1}")), None),
        chunk(call_delta(1, None, None, Some(":2}")), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    assert_eq!(
        got.events,
        vec![
            started(0, "a", "f"),
            delta(0, "{\"x\""),
            started(1, "b", "g"),
            delta(1, "{\"y\""),
            delta(0, ":1}"),
            delta(1, ":2}"),
            finished("a", "f", "{\"x\":1}"),
            finished("b", "g", "{\"y\":2}")
        ]
    );
}

#[test]
fn an_index_reused_for_a_second_call_closes_the_first() {
    let got = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("{\"x\":1}")), None),
        chunk(call_delta(0, Some("b"), Some("g"), Some("{\"y\":2}")), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    assert_eq!(
        got.events,
        vec![
            started(0, "a", "f"),
            delta(0, "{\"x\":1}"),
            finished("a", "f", "{\"x\":1}"),
            started(1, "b", "g"),
            delta(1, "{\"y\":2}"),
            finished("b", "g", "{\"y\":2}")
        ]
    );
}

// fuzz-decode: rig closed a displaced call with `{}` whatever its arguments were, delivering a call
// the model never wrote. Arguments that never began mean `{}`; half-written ones are a fault.
#[test]
fn a_displaced_call_with_broken_arguments_is_a_fault_and_one_with_none_is_an_empty_object() {
    let broken = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("{\"x\":")), None),
        chunk(call_delta(0, Some("b"), Some("g"), Some("{}")), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    assert_eq!(broken.outcome, Err(CodecError::BadToolArguments));
    assert!(
        !broken
            .events
            .iter()
            .any(|e| matches!(e, TurnEvent::ToolCallDone(_)))
    );
    let bare = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), None), None),
        chunk(call_delta(0, Some("b"), Some("g"), Some("{}")), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    assert!(
        bare.events.contains(&finished("a", "f", "{}")),
        "{:?}",
        bare.events
    );
    assert!(bare.events.contains(&finished("b", "g", "{}")));
}

#[test]
fn an_argument_less_opening_fragment_after_arguments_starts_a_new_call() {
    let got = run(&[
        chunk(call_delta(0, None, Some("f"), Some("{\"x\":1}")), None),
        chunk(call_delta(0, None, Some("g"), None), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    let dones: Vec<_> = got
        .events
        .iter()
        .filter(|e| matches!(e, TurnEvent::ToolCallDone(_)))
        .collect();
    assert_eq!(dones.len(), 2, "{:?}", got.events);
}

#[test]
fn a_whole_call_in_one_chunk() {
    let got = run(&[
        chunk(
            call_delta(0, Some("a"), Some("f"), Some("{\"x\":1}")),
            Some("tool_calls"),
        ),
        done(),
    ]);
    assert_eq!(
        got.events,
        vec![
            started(0, "a", "f"),
            delta(0, "{\"x\":1}"),
            finished("a", "f", "{\"x\":1}")
        ]
    );
    assert_eq!(got.outcome, plain(StopReason::ToolUse));
}

#[test]
fn empty_arguments_mean_an_empty_object() {
    for args in [Some(""), None, Some("   ")] {
        let got = run(&[
            chunk(call_delta(0, Some("a"), Some("f"), args), None),
            chunk(json!({}), Some("tool_calls")),
        ]);
        assert_eq!(
            got.events.last(),
            Some(&finished("a", "f", "{}")),
            "{args:?}"
        );
    }
}

#[test]
fn a_null_first_is_superseded_by_the_first_real_fragment() {
    let got = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("null")), None),
        chunk(call_delta(0, None, None, Some("{\"x\":1}")), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    assert_eq!(
        got.events,
        vec![
            started(0, "a", "f"),
            delta(0, "{\"x\":1}"),
            finished("a", "f", "{\"x\":1}")
        ]
    );
    let alone = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("null")), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    assert_eq!(alone.events.last(), Some(&finished("a", "f", "{}")));
    // A JSON null in the arguments field is no fragment at all.
    let mut call = call_delta(0, Some("a"), Some("f"), None);
    call["tool_calls"][0]["function"]["arguments"] = Value::Null;
    let got = run(&[chunk(call, None), chunk(json!({}), Some("tool_calls"))]);
    assert_eq!(got.events.last(), Some(&finished("a", "f", "{}")));
}

#[test]
fn arguments_sent_as_an_object_are_accepted() {
    let mut call = call_delta(0, Some("a"), Some("f"), None);
    call["tool_calls"][0]["function"]["arguments"] = json!({"x": 1});
    let got = run(&[chunk(call, Some("tool_calls"))]);
    assert_eq!(got.events.last(), Some(&finished("a", "f", "{\"x\":1}")));
}

#[test]
fn a_call_with_no_id_gets_one_and_a_call_without_an_index_uses_its_position() {
    let frame = chunk(
        json!({"tool_calls":[{"function":{"name":"f","arguments":"{}"}},{"function":{"name":"g","arguments":"{}"}}]}),
        Some("tool_calls"),
    );
    let got = run(&[frame]);
    assert_eq!(got.events[0], started(0, "call_0", "f"));
    assert!(
        got.events.contains(&finished("call_1", "g", "{}")),
        "{:?}",
        got.events
    );
}

#[test]
fn arguments_before_the_name_are_kept() {
    let got = run(&[
        chunk(call_delta(0, Some("a"), None, Some("{\"x\"")), None),
        chunk(call_delta(0, None, Some("f"), Some(":1}")), None),
        chunk(json!({}), Some("tool_calls")),
    ]);
    assert_eq!(
        got.events,
        vec![
            started(0, "a", "f"),
            delta(0, "{\"x\""),
            delta(0, ":1}"),
            finished("a", "f", "{\"x\":1}")
        ]
    );
}

#[test]
fn the_four_outcomes_of_a_malformed_call() {
    let broken = |finish: &str| {
        run(&[chunk(
            call_delta(0, Some("a"), Some("f"), Some("{\"x\":")),
            Some(finish),
        )])
    };
    // Fail: the provider said the call was complete.
    assert_eq!(
        broken("tool_calls").outcome,
        Err(CodecError::BadToolArguments)
    );
    assert_eq!(broken("stop").outcome, Err(CodecError::BadToolArguments));
    // Drop: the budget cut it; no malformed call is delivered.
    let cut = broken("length");
    assert!(
        !cut.events
            .iter()
            .any(|e| matches!(e, TurnEvent::ToolCallDone(_)))
    );
    assert_eq!(cut.outcome, plain(StopReason::MaxTokens));
    // Superseded: covered by the displaced call above. KeepOpen: a call waiting for a finish
    // stays open and is not delivered early.
    let open = run(&[chunk(
        call_delta(0, Some("a"), Some("f"), Some("{\"x\":1}")),
        None,
    )]);
    assert!(
        !open
            .events
            .iter()
            .any(|e| matches!(e, TurnEvent::ToolCallDone(_)))
    );
}

#[test]
fn a_tool_turn_cut_by_max_tokens_keeps_calls_that_are_whole() {
    let got = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("{\"x\":1}")), None),
        chunk(call_delta(1, Some("b"), Some("g"), Some("{\"y\"")), None),
        chunk(json!({}), Some("length")),
    ]);
    let dones: Vec<_> = got
        .events
        .iter()
        .filter(|e| matches!(e, TurnEvent::ToolCallDone(_)))
        .collect();
    assert_eq!(dones, vec![&finished("a", "f", "{\"x\":1}")]);
    assert_eq!(got.outcome, plain(StopReason::MaxTokens));
}

#[test]
fn a_stop_reason_of_stop_after_calls_is_tool_use() {
    let got = run(&[chunk(
        call_delta(0, Some("a"), Some("f"), Some("{}")),
        Some("stop"),
    )]);
    assert_eq!(got.outcome, plain(StopReason::ToolUse));
}

#[test]
fn done_closes_calls_when_no_finish_reason_came() {
    let got = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("{}")), None),
        done(),
    ]);
    assert_eq!(got.events.last(), Some(&finished("a", "f", "{}")));
    assert_eq!(got.outcome, plain(StopReason::ToolUse));
}

#[test]
fn duplicate_ids_and_bad_names_are_unreadable() {
    let dup = run(&[
        chunk(call_delta(0, Some("a"), Some("f"), Some("{}")), None),
        chunk(call_delta(1, Some("a"), Some("g"), Some("{}")), None),
    ]);
    assert_eq!(dup.outcome, Err(CodecError::Unreadable));
    let bad = run(&[chunk(
        call_delta(0, Some("a"), Some("has space"), None),
        None,
    )]);
    assert_eq!(bad.outcome, Err(CodecError::Unreadable));
}

#[test]
fn oversize_arguments_do_not_grow_without_end() {
    let big = "x".repeat(1 << 20);
    let mut frames = vec![chunk(
        call_delta(0, Some("a"), Some("f"), Some("{\"k\":\"")),
        None,
    )];
    frames.extend((0..40).map(|_| chunk(call_delta(0, None, None, Some(&big)), None)));
    frames.push(chunk(json!({}), Some("tool_calls")));
    let got = run(&frames);
    assert_eq!(got.outcome, Err(CodecError::BadToolArguments));
}

// ---- conformance ----

#[test]
fn truncation_preserves_content_without_terminal() {
    let got = run(&[
        chunk(json!({"content":"par"}), None),
        chunk(json!({"content":"tial"}), None),
    ]);
    assert_eq!(got.events, vec![text("par"), text("tial")]);
    assert_eq!(got.outcome, Err(CodecError::Truncated));
}

#[test]
fn a_truncated_call_is_never_delivered() {
    let got = run(&[chunk(
        call_delta(0, Some("a"), Some("f"), Some("{\"x\":")),
        None,
    )]);
    assert!(
        !got.events
            .iter()
            .any(|e| matches!(e, TurnEvent::ToolCallDone(_)))
    );
    assert_eq!(got.outcome, Err(CodecError::Truncated));
}

#[test]
fn a_malformed_frame_ends_the_reply() {
    let got = run(&[
        chunk(json!({"content":"ok"}), None),
        "{not json".into(),
        chunk(json!({"content":"never"}), None),
    ]);
    assert_eq!(got.events, vec![text("ok")]);
    assert_eq!(got.outcome, Err(CodecError::Unreadable));
    assert_eq!(run(&["[1,2]".into()]).outcome, Err(CodecError::Unreadable));
    assert_eq!(
        run(&["\"str\"".into()]).outcome,
        Err(CodecError::Unreadable)
    );
}

#[test]
fn an_unknown_frame_is_skipped_and_a_known_one_with_a_wrong_type_is_defective() {
    let got = run(&[
        json!({"type":"ping"}).to_string(),
        String::new(),
        chunk(json!({"content":"x"}), Some("stop")),
    ]);
    assert_eq!(
        got,
        Run {
            events: vec![text("x")],
            outcome: plain(StopReason::EndTurn)
        }
    );
    for defective in [
        chunk(json!({"content": 5}), None),
        chunk(json!({"tool_calls": "no"}), None),
        chunk(json!({"tool_calls": [{"index": "zero"}]}), None),
        chunk(
            json!({"tool_calls": [{"function": {"arguments": 7}}]}),
            None,
        ),
    ] {
        assert_eq!(
            run(std::slice::from_ref(&defective)).outcome,
            Err(CodecError::Unreadable),
            "{defective}"
        );
    }
}

#[test]
fn a_bare_done_after_only_skipped_frames_fabricates_nothing() {
    assert_eq!(run(&[done()]).outcome, Err(CodecError::Unreadable));
    assert_eq!(
        run(&[json!({"type":"ping"}).to_string(), done()]).outcome,
        Err(CodecError::Unreadable)
    );
    assert_eq!(run(&[]).outcome, Err(CodecError::Unreadable));
}

#[test]
fn frames_after_done_are_ignored() {
    let got = run(&[
        chunk(json!({"content":"a"}), Some("stop")),
        done(),
        chunk(json!({"content":"late"}), None),
    ]);
    assert_eq!(got.events, vec![text("a")]);
}

#[test]
fn an_error_envelope_in_a_200_is_unreadable_and_names_its_fault() {
    let cases: Vec<(Value, ProviderError)> = vec![
        (
            json!({"error":{"message":"secret prompt text","type":"invalid_request_error","code":"context_length_exceeded"}}),
            ProviderError::ContextOverflow { limit: Tokens(0) },
        ),
        (
            json!({"error":{"message":"x","type":"rate_limit_error"}}),
            ProviderError::RateLimited(model_provider::RetrySeconds(0)),
        ),
        (
            json!({"error":{"message":"x","type":"authentication_error"}}),
            ProviderError::Unauthorized,
        ),
        (
            json!({"error":{"message":"x","type":"server_error"}}),
            ProviderError::Server(model_provider::ServerStatus(500)),
        ),
        (
            json!({"error":{"message":"x","code":503}}),
            ProviderError::Server(model_provider::ServerStatus(503)),
        ),
        (
            json!({"error":{"message":"secret","type":"Weird Type!"}}),
            ProviderError::BadRequest("weirdtype".into()),
        ),
        (
            json!({"error":"a plain string"}),
            ProviderError::BadRequest("error".into()),
        ),
    ];
    for (frame, want) in cases {
        let mut d = decoder(Flavor::Vllm);
        assert!(
            d.feed(r#"{"choices":[{"index":0,"delta":{"content":"hi"}}]}"#)
                .is_ok()
        );
        assert_eq!(
            d.feed(&frame.to_string()),
            Err(CodecError::Unreadable),
            "{frame}"
        );
        let fault = d.fault();
        assert_eq!(fault.as_ref(), Some(&want), "{frame}");
        assert!(!format!("{fault:?}").contains("secret"));
    }
    // A null error is not an error.
    assert!(
        decoder(Flavor::Vllm)
            .feed(r#"{"error":null,"choices":[{"index":0,"delta":{"content":"x"}}]}"#)
            .is_ok()
    );
}

#[test]
fn flavors_read_the_same_chunks() {
    let frames = [
        chunk(json!({"reasoning_content":"t"}), None),
        chunk(
            call_delta(0, Some("a"), Some("f"), Some("{}")),
            Some("tool_calls"),
        ),
    ];
    let want = run_with(Flavor::Vllm, &frames);
    for flavor in [Flavor::LlamaServer, Flavor::LiteLlm, Flavor::OpenRouter] {
        assert_eq!(run_with(flavor, &frames), want, "{flavor:?}");
    }
}

// ---- every split offset, end to end through the SSE framer ----

fn sse_body(frames: &[String]) -> Vec<u8> {
    frames
        .iter()
        .map(|f| format!("data: {f}\n\n"))
        .collect::<String>()
        .into_bytes()
}

/// SSE bytes in chunks through the framer into the decoder.
fn through_framer(body: &[&[u8]]) -> Run {
    let mut framer = model_http::SseDecoder::new();
    let mut d = decoder(Flavor::Vllm);
    let mut events = Vec::new();
    let mut frames = Vec::new();
    for bytes in body {
        frames.extend(framer.feed(bytes).unwrap());
    }
    frames.extend(framer.finish());
    for frame in frames {
        match d.feed(&frame.data) {
            Ok(more) => events.extend(more),
            Err(error) => {
                return Run {
                    events,
                    outcome: Err(error),
                };
            }
        }
    }
    Run {
        events,
        outcome: d.finish(),
    }
}

fn scenario() -> Vec<String> {
    vec![
        chunk(json!({"role":"assistant","content":""}), None),
        chunk(json!({"reasoning_content":"h\u{e9}"}), None),
        chunk(json!({"content":"Hi \u{1f600}"}), None),
        chunk(call_delta(0, Some("a"), Some("f"), Some("{\"x\":")), None),
        chunk(call_delta(1, Some("b"), Some("g"), Some("{}")), None),
        chunk(call_delta(0, None, None, Some("1}")), None),
        chunk(json!({}), Some("tool_calls")),
        usage_chunk(json!({"prompt_tokens": 5, "completion_tokens": 2})),
        done(),
    ]
}

#[test]
fn every_chunk_split_offset_gives_the_same_reply() {
    let body = sse_body(&scenario());
    let whole = through_framer(&[&body]);
    assert_eq!(
        whole.outcome,
        end(
            StopReason::ToolUse,
            TurnUsage {
                input: Tokens(5),
                output: Tokens(2),
                ..Default::default()
            }
        )
    );
    assert_eq!(
        whole
            .events
            .iter()
            .filter(|e| matches!(e, TurnEvent::ToolCallDone(_)))
            .count(),
        2
    );
    for cut in 0..=body.len() {
        assert_eq!(
            through_framer(&[&body[..cut], &body[cut..]]),
            whole,
            "cut {cut}"
        );
    }
    let bytes: Vec<&[u8]> = body.chunks(1).collect();
    assert_eq!(through_framer(&bytes), whole);
}

proptest! {
    #[test]
    fn any_chunking_of_the_scenario_gives_the_same_reply(cuts in proptest::collection::vec(any::<usize>(), 0..12)) {
        let body = sse_body(&scenario());
        let mut points: Vec<usize> = cuts.iter().map(|c| c % (body.len() + 1)).collect();
        points.extend([0, body.len()]);
        points.sort_unstable();
        let chunks: Vec<&[u8]> = points.windows(2).map(|w| &body[w[0]..w[1]]).collect();
        prop_assert_eq!(through_framer(&chunks), through_framer(&[&body]));
    }

    #[test]
    fn arbitrary_frames_never_panic(frames in proptest::collection::vec(".{0,80}", 0..8)) {
        let _ = run(&frames);
    }

    #[test]
    fn arbitrary_json_frames_never_panic(frames in proptest::collection::vec(arbitrary_chunk(), 0..8)) {
        let frames: Vec<String> = frames.into_iter().map(|v| v.to_string()).collect();
        let _ = run(&frames);
    }
}

fn arbitrary_chunk() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i32>().prop_map(|n| json!(n)),
        "[a-z{}\":0-9]{0,6}".prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..3).prop_map(Value::Array),
            proptest::collection::btree_map("(choices|index|delta|content|tool_calls|function|name|arguments|id|finish_reason|usage|prompt_tokens|error|reasoning)", inner, 0..4)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}
