//! The codec behind the driver, over wire cassettes: the conformance list of research-rig 3.7
//! through the head, the framer and the decoder, with every way of cutting the bytes.
//!
//! The scenario names follow rig's streaming conformance suite (rig-core `test_utils`, commit
//! acdcf34, MIT, Copyright (c) 2024 Playgrounds Analytics Inc.; see THIRD-PARTY-NOTICES). The
//! bodies are written here against our types.

use crate::support;

use std::sync::Mutex;

use model_http::{BodyKind, HttpError, HttpStatus, ResponseHead, WaitSeconds};
use model_openai_compat::{Flavor, OpenAiCodec};
use model_provider::{
    CallIndex, Dims, EmbedRole, EmbedTurn, Embedder, Flow, JsonText, Knob, ModelName, Provider,
    ProviderError, RetrySeconds, StopReason, Tokens, ToolCall, ToolCallId, ToolName, TurnEnd,
    TurnEvent, TurnSink, TurnUsage,
};
use model_replay::{
    ByteStep, ChunkPlan, HeadPrint, RecordingTransport, ReplayMode, ReplayTransport, WireBody,
    WireCassette, WireEnd, WireExchange, WireFrame, WireReply, WireRequest, WireSink,
};
use model_wire::Driver;
use serde_json::{Value, json};
use support::*;

#[derive(Default)]
struct Keep(Vec<TurnEvent>);

impl TurnSink for Keep {
    fn event(&mut self, event: TurnEvent) -> Flow {
        self.0.push(event);
        Flow::Continue
    }
}

fn chunk(delta: Value, finish: Option<&str>) -> String {
    json!({"id":"c","choices":[{"index":0,"delta":delta,"finish_reason":finish}]}).to_string()
}

fn frames_of(items: &[Value]) -> Vec<String> {
    items.iter().map(Value::to_string).collect()
}

/// A turn that speaks, thinks, calls a tool in fragments and reports usage.
fn scenario() -> Vec<String> {
    let mut frames = vec![
        chunk(json!({"role": "assistant", "content": ""}), None),
        chunk(json!({"reasoning_content": "hm"}), None),
        chunk(json!({"content": "Opening "}), None),
        chunk(json!({"content": "it."}), None),
        chunk(
            json!({"tool_calls": [{"index": 0, "id": "call_1", "type": "function",
                "function": {"name": "click", "arguments": ""}}]}),
            None,
        ),
        chunk(
            json!({"tool_calls": [{"index": 0, "function": {"arguments": "{\"x\":"}}]}),
            None,
        ),
        chunk(
            json!({"tool_calls": [{"index": 0, "function": {"arguments": "12}"}}]}),
            None,
        ),
        chunk(json!({}), Some("tool_calls")),
    ];
    frames.push(
        json!({"id":"c","choices":[],"usage":{"prompt_tokens":40,"completion_tokens":9}})
            .to_string(),
    );
    frames.push("[DONE]".into());
    frames
}

fn call_done() -> TurnEvent {
    TurnEvent::ToolCallDone(ToolCall {
        id: ToolCallId("call_1".into()),
        name: ToolName::new("click").unwrap(),
        input: JsonText::new("{\"x\":12}").unwrap(),
    })
}

fn expected_events() -> Vec<TurnEvent> {
    vec![
        TurnEvent::ThoughtDelta("hm".into()),
        TurnEvent::TextDelta("Opening ".into()),
        TurnEvent::TextDelta("it.".into()),
        TurnEvent::ToolCallStarted {
            index: CallIndex(0),
            id: ToolCallId("call_1".into()),
            name: ToolName::new("click").unwrap(),
        },
        TurnEvent::ToolCallDelta {
            index: CallIndex(0),
            fragment: "{\"x\":".into(),
        },
        TurnEvent::ToolCallDelta {
            index: CallIndex(0),
            fragment: "12}".into(),
        },
        call_done(),
        TurnEvent::Usage(usage()),
    ]
}

fn usage() -> TurnUsage {
    TurnUsage {
        input: Tokens(40),
        output: Tokens(9),
        ..TurnUsage::default()
    }
}

fn want_end() -> TurnEnd {
    TurnEnd {
        stop: StopReason::ToolUse,
        usage: usage(),
        served: ModelName("holo".into()),
        first_token: None,
    }
}

fn replay(cassette: WireCassette, plan: ChunkPlan) -> Driver<OpenAiCodec, ReplayTransport> {
    Driver::new(
        OpenAiCodec::new(Flavor::Vllm),
        ReplayTransport::new(cassette, ReplayMode::InOrder, plan),
    )
}

fn turn(
    driver: &Driver<OpenAiCodec, ReplayTransport>,
) -> (Result<TurnEnd, ProviderError>, Vec<TurnEvent>) {
    let mut sink = Keep::default();
    let mut request = base();
    request.messages = vec![user("open it")];
    let result = block_on(driver.turn(&request, &mut sink));
    (result, sink.0)
}

fn plans() -> Vec<ChunkPlan> {
    let mut plans = vec![ChunkPlan::Whole, ChunkPlan::Lines];
    plans.extend([1, 2, 3, 5, 7, 16, 100].map(|n| ChunkPlan::Every(ByteStep(n))));
    plans.extend((0..25).map(|s| ChunkPlan::Seeded(model_provider::Seed(s))));
    plans
}

#[test]
fn a_recorded_turn_replays_to_the_same_events_under_every_chunking() {
    let recorded = sse_exchange(chat_request(None), &scenario(), WireEnd::Complete);
    for plan in plans() {
        let (result, events) = turn(&replay(cassette(vec![recorded.clone()]), plan));
        assert_eq!(result, Ok(want_end()), "{plan:?}");
        assert_eq!(events, expected_events(), "{plan:?}");
    }
}

#[test]
fn an_exchange_recorded_through_the_driver_replays_through_the_driver() {
    let mut raw = String::new();
    for frame in scenario() {
        raw.push_str(&format!("data: {frame}\n\n"));
    }
    let server = FakeServer {
        head: ResponseHead {
            status: HttpStatus(200),
            body: BodyKind::EventStream,
            retry_after: None,
            request_id: None,
        },
        chunks: raw.as_bytes().chunks(13).map(<[u8]>::to_vec).collect(),
        result: Ok(HttpStatus(200)),
    };
    let written = Written::default();
    let recording = Driver::new(
        OpenAiCodec::new(Flavor::Vllm),
        RecordingTransport::new(server, &written),
    );
    let mut sink = Keep::default();
    let mut request = base();
    request.messages = vec![user("open it")];
    assert_eq!(
        block_on(recording.turn(&request, &mut sink)),
        Ok(want_end())
    );
    assert_eq!(sink.0, expected_events());

    // The cassette holds the request the codec built and the body as frames, scrubbed of nothing
    // it should keep.
    let exchanges = written.0.lock().unwrap().clone();
    assert_eq!(exchanges.len(), 1);
    let sent: Value =
        serde_json::from_str(exchanges[0].request.body.as_ref().unwrap().as_str()).unwrap();
    assert_eq!(sent["messages"][0]["content"], json!("open it"));
    let WireBody::Frames(frames) = &exchanges[0].reply.body else {
        panic!("an event stream is recorded as frames");
    };
    assert_eq!(
        frames.iter().map(|f| f.data.clone()).collect::<Vec<_>>(),
        scenario()
    );
    assert_eq!(exchanges[0].reply.end, WireEnd::Complete);

    // Through the file text and back, the cassette still replays the turn, in strict mode: the
    // codec must build the same request again.
    let file = cassette(exchanges).to_jsonl();
    let reread = WireCassette::from_jsonl(&file).unwrap();
    let replaying = Driver::new(
        OpenAiCodec::new(Flavor::Vllm),
        ReplayTransport::new(reread, ReplayMode::Strict, ChunkPlan::Every(ByteStep(3))),
    );
    let (result, events) = turn(&replaying);
    assert_eq!(result, Ok(want_end()));
    assert_eq!(events, expected_events());
    assert!(replaying.transport().misses().is_empty());
}

#[derive(Default)]
struct Written(Mutex<Vec<WireExchange>>);

impl WireSink for &Written {
    fn write(&self, exchange: &WireExchange) -> Result<(), model_replay::SinkError> {
        self.0.lock().unwrap().push(exchange.clone());
        Ok(())
    }
}

#[test]
fn a_request_that_differs_from_the_recorded_one_is_a_miss_in_strict_mode() {
    let recorded = sse_exchange(
        chat_request(Some(r#"{"model":"other"}"#)),
        &scenario(),
        WireEnd::Complete,
    );
    let driver = Driver::new(
        OpenAiCodec::new(Flavor::Vllm),
        ReplayTransport::new(
            cassette(vec![recorded]),
            ReplayMode::Strict,
            ChunkPlan::Whole,
        ),
    );
    let (result, events) = turn(&driver);
    assert_eq!(
        result,
        Err(ProviderError::BadRequest("no recorded exchange".into()))
    );
    assert!(events.is_empty());
    assert_eq!(driver.transport().misses().len(), 1);
}

#[test]
fn error_in_200_is_the_envelopes_error_with_the_heads_retry_after() {
    let frames = vec![
        chunk(json!({"content": "par"}), None),
        json!({"error": {"type": "rate_limit_error", "message": "SECRET"}}).to_string(),
        chunk(json!({"content": "never"}), None),
    ];
    let mut exchange = sse_exchange(chat_request(None), &frames, WireEnd::Complete);
    exchange.reply.head.retry_after = Some(WaitSeconds(9));
    for plan in plans() {
        let (result, events) = turn(&replay(cassette(vec![exchange.clone()]), plan));
        assert_eq!(
            result,
            Err(ProviderError::RateLimited(RetrySeconds(9))),
            "{plan:?}"
        );
        assert_eq!(events, vec![TurnEvent::TextDelta("par".into())], "{plan:?}");
    }
}

#[test]
fn error_in_200_of_another_kind_keeps_its_kind() {
    let frames = vec![json!({"error": {"type": "authentication_error"}}).to_string()];
    let exchange = sse_exchange(chat_request(None), &frames, WireEnd::Complete);
    let (result, _) = turn(&replay(cassette(vec![exchange]), ChunkPlan::Whole));
    assert_eq!(result, Err(ProviderError::Unauthorized));
}

#[test]
fn html_200_page() {
    let exchange = WireExchange {
        request: chat_request(None),
        reply: WireReply {
            head: HeadPrint {
                status: HttpStatus(200),
                body: BodyKind::Html,
                retry_after: None,
            },
            body: WireBody::Whole("<html><body>Please sign in</body></html>".into()),
            end: WireEnd::Complete,
        },
    };
    let (result, events) = turn(&replay(
        cassette(vec![exchange]),
        ChunkPlan::Every(ByteStep(5)),
    ));
    assert_eq!(
        result,
        Err(ProviderError::Unreadable(
            "an HTML page was served instead of a reply".into()
        ))
    );
    assert!(events.is_empty());
}

#[test]
fn a_failing_status_is_read_from_its_json_body() {
    let exchange = WireExchange {
        request: chat_request(None),
        reply: WireReply {
            head: HeadPrint {
                status: HttpStatus(400),
                body: BodyKind::Json,
                retry_after: None,
            },
            body: WireBody::Whole(
                r#"{"error":{"message":"This model's maximum context length is 8192 tokens.","type":"BadRequestError","code":400}}"#.into(),
            ),
            end: WireEnd::Complete,
        },
    };
    let (result, _) = turn(&replay(
        cassette(vec![exchange]),
        ChunkPlan::Every(ByteStep(4)),
    ));
    assert_eq!(
        result,
        Err(ProviderError::ContextOverflow {
            limit: Tokens(8192)
        })
    );
}

#[test]
fn transport_error_after_a_tool_call_started_yields_the_events_then_the_error() {
    let frames = frames_of(&[
        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"click","arguments":"{\"x\""}}]}}]}),
    ]);
    let exchange = sse_exchange(chat_request(None), &frames, WireEnd::Reset);
    let (result, events) = turn(&replay(cassette(vec![exchange]), ChunkPlan::Lines));
    assert_eq!(result, Err(ProviderError::Unreachable));
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, TurnEvent::ToolCallDone(_))),
        "a half-formed call is never delivered: {events:?}"
    );
    assert!(matches!(events[0], TurnEvent::ToolCallStarted { .. }));
}

#[test]
fn truncation_preserves_content_without_a_terminal() {
    let frames = vec![chunk(json!({"content": "partial"}), None)];
    for end in [WireEnd::Cut, WireEnd::Complete] {
        let exchange = sse_exchange(chat_request(None), &frames, end);
        let (result, events) = turn(&replay(cassette(vec![exchange]), ChunkPlan::Whole));
        assert_eq!(
            result,
            Err(ProviderError::Unreadable(
                "the stream ended before a finish reason".into()
            )),
            "{end:?}"
        );
        assert_eq!(events, vec![TurnEvent::TextDelta("partial".into())]);
    }
}

#[test]
fn a_malformed_frame_ends_the_reply_at_that_frame() {
    let mut frames = vec![chunk(json!({"content": "a"}), None), "{not json".into()];
    frames.push(chunk(json!({"content": "b"}), Some("stop")));
    let exchange = sse_exchange(chat_request(None), &frames, WireEnd::Complete);
    let (result, events) = turn(&replay(cassette(vec![exchange]), ChunkPlan::Whole));
    assert_eq!(
        result,
        Err(ProviderError::Unreadable("a frame is not a chunk".into()))
    );
    assert_eq!(events, vec![TurnEvent::TextDelta("a".into())]);
}

#[test]
fn bare_terminal_after_only_unparseable_frames_fabricates_nothing() {
    let frames = vec![
        json!({"id": "x", "object": "ping"}).to_string(),
        "[DONE]".into(),
    ];
    let exchange = sse_exchange(chat_request(None), &frames, WireEnd::Complete);
    let (result, events) = turn(&replay(cassette(vec![exchange]), ChunkPlan::Whole));
    assert!(result.is_err());
    assert!(events.is_empty());
}

#[test]
fn interleaved_reasoning_and_text_keep_their_order() {
    let frames = vec![
        chunk(json!({"reasoning_content": "a"}), None),
        chunk(json!({"content": "b"}), None),
        chunk(json!({"reasoning": "c"}), None),
        chunk(json!({"content": "d"}), Some("stop")),
        "[DONE]".into(),
    ];
    let exchange = sse_exchange(chat_request(None), &frames, WireEnd::Complete);
    let (result, events) = turn(&replay(cassette(vec![exchange]), ChunkPlan::Lines));
    assert_eq!(
        events,
        vec![
            TurnEvent::ThoughtDelta("a".into()),
            TurnEvent::TextDelta("b".into()),
            TurnEvent::ThoughtDelta("c".into()),
            TurnEvent::TextDelta("d".into()),
        ]
    );
    assert_eq!(result.unwrap().stop, StopReason::EndTurn);
}

#[test]
fn an_unencodable_request_asks_the_transport_nothing() {
    let llama = Driver::new(
        OpenAiCodec::new(Flavor::LlamaServer),
        ReplayTransport::new(cassette(vec![]), ReplayMode::InOrder, ChunkPlan::Whole),
    );
    let mut request = base();
    request.tools = vec![function("click", "{}")];
    request.tool_choice = model_provider::ToolChoice::Named(ToolName::new("click").unwrap());
    let result = block_on(llama.turn(&request, &mut Keep::default()));
    assert_eq!(
        result,
        Err(ProviderError::BadRequest(
            "unsupported shape for this wire".into()
        ))
    );
    assert!(llama.transport().misses().is_empty());
}

fn get(path: &str, root: model_http::RouteRoot) -> WireRequest {
    WireRequest {
        verb: model_http::Verb::Get,
        root,
        path: model_http::UrlPath(path.into()),
        body: None,
    }
}

fn json_exchange(request: WireRequest, status: u16, body: &str) -> WireExchange {
    WireExchange {
        request,
        reply: WireReply {
            head: HeadPrint {
                status: HttpStatus(status),
                body: BodyKind::Json,
                retry_after: None,
            },
            body: WireBody::Whole(body.into()),
            end: WireEnd::Complete,
        },
    }
}

#[test]
fn describe_goes_to_props_for_llama_server_and_models_for_vllm() {
    let props = json_exchange(
        get("/props", model_http::RouteRoot::Server),
        200,
        r#"{"default_generation_settings":{"n_ctx":8192},"model_alias":"holo"}"#,
    );
    let llama = Driver::new(
        OpenAiCodec::new(Flavor::LlamaServer),
        ReplayTransport::new(
            cassette(vec![props]),
            ReplayMode::Strict,
            ChunkPlan::Every(ByteStep(9)),
        ),
    );
    let models = block_on(llama.describe()).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(
        (models[0].name.0.as_str(), models[0].loaded_context),
        ("holo", Tokens(8192))
    );

    let list = json_exchange(
        get("/models", model_http::RouteRoot::Base),
        200,
        r#"{"data":[{"id":"a","max_model_len":4096}]}"#,
    );
    let vllm = replay(cassette(vec![list]), ChunkPlan::Lines);
    let models = block_on(vllm.describe()).unwrap();
    assert_eq!(models[0].trained_context, Tokens(4096));

    let wrong_root = json_exchange(get("/props", model_http::RouteRoot::Base), 200, "{}");
    let strict = Driver::new(
        OpenAiCodec::new(Flavor::LlamaServer),
        ReplayTransport::new(
            cassette(vec![wrong_root]),
            ReplayMode::Strict,
            ChunkPlan::Whole,
        ),
    );
    assert!(
        block_on(strict.describe()).is_err(),
        "the server root is not the base"
    );
}

#[test]
fn describe_reads_a_failing_status() {
    let down = json_exchange(
        get("/models", model_http::RouteRoot::Base),
        503,
        r#"{"error":{"code":503,"message":"Loading model","type":"unavailable_error"}}"#,
    );
    let driver = replay(cassette(vec![down]), ChunkPlan::Whole);
    assert_eq!(block_on(driver.describe()), Err(ProviderError::NotReady));
}

fn embed_exchange(body: &str) -> WireExchange {
    let mut request = chat_request(None);
    request.path = model_http::UrlPath("/embeddings".into());
    json_exchange(request, 200, body)
}

fn turn_of(n: usize, dims: Knob<Dims>) -> EmbedTurn {
    EmbedTurn {
        model: ModelName("nomic".into()),
        inputs: (0..n).map(|i| format!("text {i}")).collect(),
        role: EmbedRole::Document,
        dims,
    }
}

#[test]
fn embed_through_the_driver_checks_count_and_width() {
    let two = r#"{"data":[{"index":1,"embedding":[3,4]},{"index":0,"embedding":[1,2]}],"usage":{"prompt_tokens":4}}"#;
    let ok = replay(
        cassette(vec![embed_exchange(two)]),
        ChunkPlan::Every(ByteStep(6)),
    );
    let end = block_on(ok.embed(&turn_of(2, Knob::Set(Dims(2))))).unwrap();
    assert_eq!(end.vectors[0].0, vec![1.0, 2.0]);
    assert_eq!(end.usage.input, Tokens(4));

    let short = replay(cassette(vec![embed_exchange(two)]), ChunkPlan::Whole);
    assert_eq!(
        block_on(short.embed(&turn_of(3, Knob::Off))),
        Err(ProviderError::Unreadable(
            "asked for 3 vectors, got 2".into()
        ))
    );
    let wide = replay(cassette(vec![embed_exchange(two)]), ChunkPlan::Whole);
    assert_eq!(
        block_on(wide.embed(&turn_of(2, Knob::Set(Dims(768))))),
        Err(ProviderError::Unreadable(
            "expected vectors of width 768, got 2".into()
        ))
    );
}

#[test]
fn llama_server_is_never_sent_dimensions_even_when_asked() {
    let driver = Driver::new(
        OpenAiCodec::new(Flavor::LlamaServer),
        ReplayTransport::new(
            cassette(vec![{
                let mut e = embed_exchange(r#"{"data":[{"index":0,"embedding":[1,2]}]}"#);
                e.request.body = Some(
                    JsonText::new(
                        r#"{"model":"nomic","input":["text 0"],"encoding_format":"float"}"#,
                    )
                    .unwrap(),
                );
                e
            }]),
            ReplayMode::Strict,
            ChunkPlan::Whole,
        ),
    );
    let end = block_on(driver.embed(&turn_of(1, Knob::Set(Dims(2))))).unwrap();
    assert_eq!(
        end.vectors.len(),
        1,
        "strict replay matched a body with no `dimensions`"
    );
}

#[test]
fn a_transport_that_cannot_connect_is_unreachable() {
    let driver = Driver::new(
        OpenAiCodec::new(Flavor::Vllm),
        FakeServer {
            head: ResponseHead {
                status: HttpStatus(200),
                body: BodyKind::Json,
                retry_after: None,
                request_id: None,
            },
            chunks: vec![],
            result: Err(HttpError::Connect),
        },
    );
    assert_eq!(block_on(driver.describe()), Err(ProviderError::Unreachable));
}

#[test]
fn the_wire_frame_type_keeps_event_names_out_of_the_data() {
    let frame = WireFrame {
        event: Some(model_http::EventName("ping".into())),
        data: "{}".into(),
    };
    let exchange = WireExchange {
        request: chat_request(None),
        reply: WireReply {
            head: HeadPrint {
                status: HttpStatus(200),
                body: BodyKind::EventStream,
                retry_after: None,
            },
            body: WireBody::Frames(vec![
                frame,
                WireFrame {
                    event: None,
                    data: chunk(json!({"content": "ok"}), Some("stop")),
                },
            ]),
            end: WireEnd::Complete,
        },
    };
    let (result, events) = turn(&replay(cassette(vec![exchange]), ChunkPlan::Whole));
    assert_eq!(events, vec![TurnEvent::TextDelta("ok".into())]);
    assert!(result.is_ok(), "{result:?}");
}
