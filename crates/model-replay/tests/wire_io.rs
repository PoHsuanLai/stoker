//! The wire cassette transports: replay in every mode and plan, recording over a fake transport.

use std::sync::Mutex;

use model_http::{
    BodyKind, BodySink, ChunkFlow, EventName, Exchange, Framing, HttpError, HttpStatus, JsonBody,
    ResponseHead, RouteRoot, Transport, UrlPath, Verb, WaitSeconds,
};
use model_provider::{ModelName, Seed, Tokens};
use model_replay::{
    BuildLabel, ByteStep, CassetteHeader, CassetteVersion, ChunkPlan, ContextStamp, EngineLabel,
    EngineStamp, HeadPrint, InteractionId, RecordedAt, RecordingTransport, ReplayMode,
    ReplayTransport, SinkError, WireBody, WireCassette, WireEnd, WireExchange, WireFrame, WireMiss,
    WireReply, WireRequest, WireSink,
};

fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("these transports never wait"),
    }
}

#[derive(Default)]
struct Seen {
    head: Option<ResponseHead>,
    chunks: Vec<Vec<u8>>,
    stop_after: Option<usize>,
}

impl Seen {
    fn bytes(&self) -> String {
        String::from_utf8(self.chunks.concat()).unwrap()
    }
}

impl BodySink for Seen {
    fn head(&mut self, head: &ResponseHead) -> ChunkFlow {
        self.head = Some(head.clone());
        ChunkFlow::Continue
    }
    fn chunk(&mut self, bytes: &[u8]) -> ChunkFlow {
        self.chunks.push(bytes.to_vec());
        match self.stop_after {
            Some(n) if self.chunks.len() >= n => ChunkFlow::Stop,
            _ => ChunkFlow::Continue,
        }
    }
}

fn ex(path: &str, body: Option<&str>, framing: Framing) -> Exchange {
    Exchange {
        verb: if body.is_some() {
            Verb::PostJson
        } else {
            Verb::Get
        },
        root: RouteRoot::Base,
        path: UrlPath(path.into()),
        body: body.map(|b| JsonBody(b.into())),
        framing,
    }
}

fn req(path: &str, body: Option<&str>) -> WireRequest {
    WireRequest {
        verb: if body.is_some() {
            Verb::PostJson
        } else {
            Verb::Get
        },
        root: RouteRoot::Base,
        path: UrlPath(path.into()),
        body: body.map(|b| model_provider::JsonText::new(b).unwrap()),
    }
}

fn head(status: u16, body: BodyKind) -> HeadPrint {
    HeadPrint {
        status: HttpStatus(status),
        body,
        retry_after: None,
    }
}

fn streaming() -> WireExchange {
    WireExchange {
        request: req(
            "/chat/completions",
            Some(r#"{"model":"holo","stream":true}"#),
        ),
        reply: WireReply {
            head: head(200, BodyKind::EventStream),
            body: WireBody::Frames(vec![
                WireFrame {
                    event: None,
                    data: r#"{"a":1}"#.into(),
                },
                WireFrame {
                    event: Some(EventName("ping".into())),
                    data: "x".into(),
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

fn models() -> WireExchange {
    WireExchange {
        request: req("/models", None),
        reply: WireReply {
            head: head(200, BodyKind::Json),
            body: WireBody::Whole(r#"{"data":[]}"#.into()),
            end: WireEnd::Complete,
        },
    }
}

fn failing(status: u16, end: WireEnd) -> WireExchange {
    WireExchange {
        request: req("/models", None),
        reply: WireReply {
            head: HeadPrint {
                retry_after: Some(WaitSeconds(2)),
                ..head(status, BodyKind::Json)
            },
            body: WireBody::Whole(r#"{"error":"busy"}"#.into()),
            end,
        },
    }
}

fn cassette(exchanges: Vec<WireExchange>) -> WireCassette {
    WireCassette {
        header: CassetteHeader {
            vocab: CassetteVersion::CURRENT,
            engine: EngineStamp {
                kind: EngineLabel("vllm".into()),
                build: BuildLabel("v0".into()),
            },
            model: ModelName("holo".into()),
            recorded: RecordedAt(1),
            context: ContextStamp {
                loaded: Tokens(8192),
                trained: Tokens(32768),
            },
            speech: None,
        },
        exchanges,
    }
}

const SSE: &str = "data: {\"a\":1}\n\nevent: ping\ndata: x\n\ndata: [DONE]\n\n";

fn replay(exchanges: Vec<WireExchange>, mode: ReplayMode, plan: ChunkPlan) -> ReplayTransport {
    ReplayTransport::new(cassette(exchanges), mode, plan)
}

#[test]
fn replay_delivers_head_then_the_body_as_wire_bytes() {
    let t = replay(vec![streaming()], ReplayMode::InOrder, ChunkPlan::Whole);
    let mut seen = Seen::default();
    let status = block_on(t.exchange(
        &ex(
            "/chat/completions",
            Some(r#"{"model":"holo","stream":true}"#),
            Framing::Sse,
        ),
        &mut seen,
    ));
    assert_eq!(status, Ok(HttpStatus(200)));
    assert_eq!(seen.head.as_ref().unwrap().body, BodyKind::EventStream);
    assert_eq!(seen.chunks.len(), 1);
    assert_eq!(seen.bytes(), SSE);
}

#[test]
fn every_plan_delivers_the_same_bytes() {
    let plans = [
        (ChunkPlan::Whole, 1),
        (ChunkPlan::Lines, 3),
        (ChunkPlan::Every(ByteStep(1)), SSE.len()),
        (ChunkPlan::Seeded(Seed(7)), 0),
    ];
    for (plan, chunks) in plans {
        let t = replay(vec![streaming()], ReplayMode::InOrder, plan);
        let mut seen = Seen::default();
        block_on(t.exchange(&ex("/x", None, Framing::Sse), &mut seen)).unwrap();
        assert_eq!(seen.bytes(), SSE, "{plan:?}");
        if chunks > 0 {
            assert_eq!(seen.chunks.len(), chunks, "{plan:?}");
        } else {
            assert!(seen.chunks.len() > 1, "{plan:?}");
        }
    }
}

#[test]
fn ends_map_to_the_transport_contract() {
    let table = [
        (
            "complete",
            failing(200, WireEnd::Complete),
            Ok(HttpStatus(200)),
        ),
        (
            "cut still returns",
            failing(200, WireEnd::Cut),
            Ok(HttpStatus(200)),
        ),
        (
            "reset is broken",
            failing(200, WireEnd::Reset),
            Err(HttpError::Broken),
        ),
        (
            "a failing head is rejected",
            failing(503, WireEnd::Complete),
            Err(HttpError::Rejected),
        ),
    ];
    for (name, exchange, want) in table {
        let t = replay(vec![exchange], ReplayMode::InOrder, ChunkPlan::Whole);
        let mut seen = Seen::default();
        assert_eq!(
            block_on(t.exchange(&ex("/models", None, Framing::Whole), &mut seen)),
            want,
            "{name}"
        );
        assert_eq!(
            seen.bytes(),
            r#"{"error":"busy"}"#,
            "{name}: head and body still arrive"
        );
    }
    let t = replay(
        vec![failing(503, WireEnd::Complete)],
        ReplayMode::InOrder,
        ChunkPlan::Whole,
    );
    let mut seen = Seen::default();
    block_on(t.exchange(&ex("/models", None, Framing::Whole), &mut seen)).unwrap_err();
    assert_eq!(seen.head.unwrap().retry_after, Some(WaitSeconds(2)));
}

#[test]
fn a_sink_that_stops_ends_the_replay() {
    let t = replay(vec![streaming()], ReplayMode::InOrder, ChunkPlan::Lines);
    let mut seen = Seen {
        stop_after: Some(1),
        ..Seen::default()
    };
    assert_eq!(
        block_on(t.exchange(&ex("/x", None, Framing::Sse), &mut seen)),
        Ok(HttpStatus(200))
    );
    assert_eq!(seen.chunks.len(), 1);
}

#[test]
fn modes_pick_exchanges() {
    let both = || vec![streaming(), models()];
    let a = ex(
        "/chat/completions",
        Some(r#"{"stream":true,"model":"holo"}"#),
        Framing::Sse,
    );
    let b = ex("/models", None, Framing::Whole);
    let run = |t: &ReplayTransport, e: &Exchange| {
        let mut seen = Seen::default();
        let r = block_on(t.exchange(e, &mut seen));
        (r, seen.bytes())
    };

    let by = replay(both(), ReplayMode::ByRequest, ChunkPlan::Whole);
    assert_eq!(
        run(&by, &b),
        (Ok(HttpStatus(200)), r#"{"data":[]}"#.into()),
        "by request, out of order"
    );
    assert_eq!(
        run(&by, &a).0,
        Ok(HttpStatus(200)),
        "key order does not matter"
    );
    assert_eq!(
        run(&by, &a).0,
        Err(HttpError::ReplayMiss),
        "each is used once"
    );
    assert_eq!(by.misses(), vec![WireMiss::Exhausted]);

    let order = replay(both(), ReplayMode::InOrder, ChunkPlan::Whole);
    let sse_b = ex("/models", None, Framing::Sse);
    assert_eq!(run(&order, &sse_b).1, SSE, "in order ignores the request");

    let strict = replay(both(), ReplayMode::Strict, ChunkPlan::Whole);
    assert_eq!(run(&strict, &b).0, Err(HttpError::ReplayMiss));
    let [WireMiss::Mismatch { index, want, got }] = &strict.misses()[..] else {
        panic!("one mismatch");
    };
    assert_eq!(*index, InteractionId(0));
    assert_eq!(want.path, UrlPath("/chat/completions".into()));
    assert_eq!(got.path, UrlPath("/models".into()));
}

#[test]
fn the_model_path_in_a_request_matches_its_scrubbed_recording() {
    let t = replay(vec![streaming()], ReplayMode::Strict, ChunkPlan::Whole);
    let mut seen = Seen::default();
    let scrubbed_from = ex(
        "/chat/completions",
        Some(r#"{"model":"holo","stream":true}"#),
        Framing::Sse,
    );
    assert!(block_on(t.exchange(&scrubbed_from, &mut seen)).is_ok());
}

/// Answers each call with the next canned reply, in the given chunks.
struct Fake(Mutex<Vec<Canned>>);

struct Canned {
    head: Option<ResponseHead>,
    chunks: Vec<&'static str>,
    result: Result<HttpStatus, HttpError>,
}

impl Transport for Fake {
    fn exchange<K: BodySink>(
        &self,
        _ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let canned = self.0.lock().unwrap().remove(0);
        if let Some(h) = canned
            .head
            .as_ref()
            .filter(|h| sink.head(h) == ChunkFlow::Continue)
        {
            let _ = h;
            for c in &canned.chunks {
                if sink.chunk(c.as_bytes()) == ChunkFlow::Stop {
                    break;
                }
            }
        }
        std::future::ready(canned.result)
    }
}

#[derive(Default)]
struct Written(Mutex<Vec<WireExchange>>);

impl WireSink for &Written {
    fn write(&self, exchange: &WireExchange) -> Result<(), SinkError> {
        self.0.lock().unwrap().push(exchange.clone());
        Ok(())
    }
}

fn response(status: u16, kind: BodyKind) -> ResponseHead {
    ResponseHead {
        status: HttpStatus(status),
        body: kind,
        retry_after: Some(WaitSeconds(3)),
        request_id: None,
    }
}

#[test]
fn recording_scrubs_and_keeps_head_and_body() {
    let written = Written::default();
    let fake = Fake(Mutex::new(vec![Canned {
        head: Some(response(200, BodyKind::Json)),
        chunks: vec![r#"{"model":"/home/me/m.gguf","#, r#""n":1}"#],
        result: Ok(HttpStatus(200)),
    }]));
    let rec = RecordingTransport::new(fake, &written);
    let mut seen = Seen::default();
    let body = r#"{"stream":false,"model":"/home/me/m.gguf","image":"data:image/png;base64,QUJD"}"#;
    block_on(rec.exchange(&ex("/props", Some(body), Framing::Whole), &mut seen)).unwrap();
    assert_eq!(
        seen.chunks.len(),
        2,
        "the caller's sink saw the live chunks"
    );
    assert!(
        seen.bytes().contains("/home/me"),
        "the caller is not scrubbed, the file is"
    );

    let got = written.0.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    let text = serde_json::to_string(&got[0]).unwrap();
    assert!(
        !text.contains("/home/me") && !text.contains("QUJD"),
        "{text}"
    );
    assert!(text.contains("/REDACTED_PATH") && text.contains("print=blake3:"));
    assert_eq!(
        got[0].reply.head,
        HeadPrint {
            retry_after: Some(WaitSeconds(3)),
            ..head(200, BodyKind::Json)
        }
    );
    assert_eq!(
        got[0].reply.body,
        WireBody::Whole(r#"{"model":"/REDACTED_PATH","n":1}"#.into())
    );
    assert_eq!(got[0].reply.end, WireEnd::Complete);
}

#[test]
fn recording_marks_rejected_reset_and_cut_and_skips_headless_failures() {
    let written = Written::default();
    let fake = Fake(Mutex::new(vec![
        Canned {
            head: Some(response(503, BodyKind::Json)),
            chunks: vec!["{}"],
            result: Err(HttpError::Rejected),
        },
        Canned {
            head: Some(response(200, BodyKind::Json)),
            chunks: vec!["{"],
            result: Err(HttpError::Broken),
        },
        Canned {
            head: Some(response(200, BodyKind::Json)),
            chunks: vec!["a", "b"],
            result: Ok(HttpStatus(200)),
        },
        Canned {
            head: None,
            chunks: vec![],
            result: Err(HttpError::Connect),
        },
    ]));
    let rec = RecordingTransport::new(fake, &written);
    let one = || ex("/models", None, Framing::Whole);
    assert_eq!(
        block_on(rec.exchange(&one(), &mut Seen::default())),
        Err(HttpError::Rejected)
    );
    assert_eq!(
        block_on(rec.exchange(&one(), &mut Seen::default())),
        Err(HttpError::Broken)
    );
    let mut stopper = Seen {
        stop_after: Some(1),
        ..Seen::default()
    };
    assert_eq!(
        block_on(rec.exchange(&one(), &mut stopper)),
        Ok(HttpStatus(200))
    );
    assert_eq!(
        block_on(rec.exchange(&one(), &mut Seen::default())),
        Err(HttpError::Connect)
    );
    let ends: Vec<WireEnd> = written
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|e| e.reply.end)
        .collect();
    assert_eq!(
        ends,
        [WireEnd::Complete, WireEnd::Reset, WireEnd::Cut],
        "the headless failure writes nothing"
    );
}

struct Refuse;

impl WireSink for Refuse {
    fn write(&self, _: &WireExchange) -> Result<(), SinkError> {
        Err(SinkError)
    }
}

#[test]
fn a_refusing_sink_fails_the_exchange() {
    let fake = Fake(Mutex::new(vec![Canned {
        head: Some(response(200, BodyKind::Json)),
        chunks: vec!["{}"],
        result: Ok(HttpStatus(200)),
    }]));
    let rec = RecordingTransport::new(fake, Refuse);
    assert_eq!(
        block_on(rec.exchange(&ex("/m", None, Framing::Whole), &mut Seen::default())),
        Err(HttpError::Broken)
    );
}

#[test]
fn what_is_recorded_replays() {
    let written = Written::default();
    let fake = Fake(Mutex::new(vec![Canned {
        head: Some(response(200, BodyKind::Json)),
        chunks: vec!["{\"ok\":", "true}"],
        result: Ok(HttpStatus(200)),
    }]));
    let rec = RecordingTransport::new(fake, &written);
    let call = ex("/props", Some(r#"{"b":1,"a":2}"#), Framing::Whole);
    block_on(rec.exchange(&call, &mut Seen::default())).unwrap();
    let file = WireCassette {
        exchanges: written.0.lock().unwrap().clone(),
        ..cassette(vec![])
    };
    let loaded = WireCassette::from_jsonl(&file.to_jsonl()).unwrap();
    let t = ReplayTransport::new(loaded, ReplayMode::Strict, ChunkPlan::Every(ByteStep(1)));
    let mut seen = Seen::default();
    assert_eq!(block_on(t.exchange(&call, &mut seen)), Ok(HttpStatus(200)));
    assert_eq!(seen.bytes(), r#"{"ok":true}"#);
}
