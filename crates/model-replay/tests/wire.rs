use model_http::{BodyKind, EventName, HttpStatus, RouteRoot, UrlPath, Verb, WaitSeconds};
use model_provider::{CallIndex, JsonText, ModelName, Seed, ToolCallId, ToolName, TurnEvent};
use model_replay::{
    BackendLabel, BuildLabel, ByteStep, Cassette, CassetteError, CassetteHeader, CassetteVersion,
    ChunkPlan, EngineLabel, EngineStamp, HeadPrint, RecordedAt, StreamFault, WireBody,
    WireCassette, WireEnd, WireExchange, WireFrame, WireReply, WireRequest,
};

fn header() -> CassetteHeader {
    CassetteHeader {
        vocab: CassetteVersion::CURRENT,
        engine: EngineStamp {
            kind: EngineLabel("llama_server".into()),
            build: BuildLabel("b10964-b29c606e2".into()),
        },
        model: ModelName("holo".into()),
        recorded: RecordedAt(1_790_000_000),
    }
}

fn streaming() -> WireExchange {
    WireExchange {
        request: WireRequest {
            verb: Verb::PostJson,
            root: RouteRoot::Base,
            path: UrlPath("/chat/completions".into()),
            body: Some(JsonText::new(r#"{"model":"holo","stream":true}"#).unwrap()),
        },
        reply: WireReply {
            head: HeadPrint {
                status: HttpStatus(200),
                body: BodyKind::EventStream,
                retry_after: None,
            },
            body: WireBody::Frames(vec![
                WireFrame {
                    event: None,
                    data: r#"{"choices":[{"delta":{"content":"hi"}}]}"#.into(),
                },
                WireFrame {
                    event: Some(EventName("ping".into())),
                    data: String::new(),
                },
                WireFrame {
                    event: None,
                    data: "[DONE]".into(),
                },
            ]),
            end: WireEnd::Complete,
        },
    }
}

fn overloaded() -> WireExchange {
    WireExchange {
        request: WireRequest {
            verb: Verb::Get,
            root: RouteRoot::Server,
            path: UrlPath("/props".into()),
            body: None,
        },
        reply: WireReply {
            head: HeadPrint {
                status: HttpStatus(503),
                body: BodyKind::Json,
                retry_after: Some(WaitSeconds(2)),
            },
            body: WireBody::Whole(r#"{"error":{"message":"loading"}}"#.into()),
            end: WireEnd::Cut,
        },
    }
}

fn cassette() -> WireCassette {
    WireCassette {
        header: header(),
        exchanges: vec![streaming(), overloaded()],
    }
}

#[test]
fn a_wire_cassette_round_trips_as_jsonl() {
    let text = cassette().to_jsonl();
    assert_eq!(
        text.lines().count(),
        3,
        "a header and one line per exchange"
    );
    assert_eq!(WireCassette::from_jsonl(&text).unwrap(), cassette());
}

#[test]
fn the_header_and_an_exchange_are_pinned() {
    let text = cassette().to_jsonl();
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap(),
        r#"{"vocab":1,"engine":{"kind":"llama_server","build":"b10964-b29c606e2"},"model":"holo","recorded":1790000000}"#
    );
    assert_eq!(
        lines.nth(1).unwrap(),
        r#"{"request":{"verb":"get","root":"server","path":"/props","body":null},"reply":{"head":{"status":503,"body":"json","retry_after":2},"body":{"kind":"whole","v":"{\"error\":{\"message\":\"loading\"}}"},"end":"cut"}}"#
    );
}

#[test]
fn a_head_print_keeps_exactly_three_fields() {
    let json = serde_json::to_string(&overloaded().reply.head).unwrap();
    assert_eq!(json, r#"{"status":503,"body":"json","retry_after":2}"#);
}

#[test]
fn bad_wire_files_name_the_line() {
    assert_eq!(WireCassette::from_jsonl(""), Err(CassetteError::Empty));
    let mut text = cassette().to_jsonl();
    text.push_str("{nope}\n");
    assert_eq!(
        WireCassette::from_jsonl(&text),
        Err(CassetteError::BadLine { line: 4 })
    );
}

#[test]
fn wire_ends_and_chunk_plans_have_stable_json() {
    for (end, json) in [
        (WireEnd::Complete, r#""complete""#),
        (WireEnd::Cut, r#""cut""#),
        (WireEnd::Reset, r#""reset""#),
    ] {
        assert_eq!(serde_json::to_string(&end).unwrap(), json);
        assert_eq!(serde_json::from_str::<WireEnd>(json).unwrap(), end);
    }
    let plans = [
        (ChunkPlan::Whole, r#"{"kind":"whole"}"#),
        (ChunkPlan::Lines, r#"{"kind":"lines"}"#),
        (ChunkPlan::Every(ByteStep(1)), r#"{"kind":"every","v":1}"#),
        (ChunkPlan::Seeded(Seed(42)), r#"{"kind":"seeded","v":42}"#),
    ];
    for (plan, json) in plans {
        assert_eq!(serde_json::to_string(&plan).unwrap(), json);
        assert_eq!(serde_json::from_str::<ChunkPlan>(json).unwrap(), plan);
    }
}

#[test]
fn the_old_backend_label_is_the_engine_label() {
    let label: BackendLabel = EngineLabel("vllm".into());
    assert_eq!(label.0, "vllm");
}

#[test]
fn stream_faults_are_closed() {
    let all = [
        StreamFault::UnknownCall,
        StreamFault::EndedTwice,
        StreamFault::Unclosed,
        StreamFault::AfterDone,
    ];
    for (i, a) in all.iter().enumerate() {
        assert!(!a.to_string().is_empty());
        for (j, b) in all.iter().enumerate() {
            assert_eq!(i == j, a == b);
        }
    }
    // The events the sequence check will read are the ordinary ones.
    let _ = TurnEvent::ToolCallStarted {
        index: CallIndex(0),
        id: ToolCallId("c".into()),
        name: ToolName::new("t").unwrap(),
    };
    let _ = Cassette::from_jsonl("").unwrap_err();
}
