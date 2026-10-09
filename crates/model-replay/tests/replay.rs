//! Chat cassettes: prints, hashes, sequences, replay in all three modes, recording.

use std::sync::Mutex;

use model_provider::{
    CallIndex, EngineExtras, ImageBytes, ImageDetail, ImageInput, JsonText, Knob, Limits, Message,
    Milli, ModelName, OutputShape, Part, Provider, ProviderError, Reasoning, Role, Sampling,
    Script, ScriptedProvider, StopReason, Tokens, ToolCall, ToolCallId, ToolChoice, ToolName,
    ToolParallelism, ToolResult, ToolStatus, TurnEnd, TurnEvent, TurnRequest, TurnSink, TurnUsage,
};
use model_replay::{
    BuildLabel, Cassette, CassetteError, CassetteHeader, CassetteSink, CassetteVersion,
    ContextStamp, EngineLabel, EngineStamp, Interaction, InteractionId, PartPrint, RecordedAt,
    RecordingProvider, ReplayError, ReplayMode, ReplayProvider, RequestPrint, SinkError,
    StreamFault, check_sequence,
};
use vision_prep::MediaType;

fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("replay never waits"),
    }
}

struct Collect(Vec<TurnEvent>, usize);

impl TurnSink for Collect {
    fn event(&mut self, event: TurnEvent) -> model_provider::Flow {
        self.0.push(event);
        if self.0.len() >= self.1 {
            model_provider::Flow::Stop
        } else {
            model_provider::Flow::Continue
        }
    }
}

fn request(text: &str, image: &[u8]) -> TurnRequest {
    TurnRequest {
        model: ModelName("holo".into()),
        messages: vec![Message {
            role: Role::User,
            parts: vec![
                Part::Text(text.into()),
                Part::Image(ImageInput {
                    media: MediaType::Png,
                    bytes: ImageBytes(image.to_vec()),
                    detail: ImageDetail::Auto,
                }),
            ],
        }],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        tool_calls: ToolParallelism::One,
        output: OutputShape::Free,
        limits: Limits {
            max_output: Tokens(64),
            stop: vec![],
        },
        sampling: Sampling {
            temperature: Milli(0),
            top_p: Knob::Off,
            top_k: Knob::Off,
            min_p: Knob::Off,
            repeat_penalty: Knob::Off,
            seed: Knob::Off,
        },
        reasoning: Reasoning::Off,
        engine: EngineExtras::None,
        choice_scores: model_provider::ChoiceScores::Off,
    }
}

fn end(stop: StopReason) -> TurnEnd {
    TurnEnd {
        stop,
        usage: TurnUsage::default(),
        served: ModelName("holo".into()),
        first_token: None,
    }
}

fn interaction(id: u32, text: &str, reply: &str) -> Interaction {
    let print = RequestPrint::of(&request(text, b"png"));
    Interaction {
        id: InteractionId(id),
        print: print.hash(),
        request: print,
        events: vec![TurnEvent::TextDelta(reply.into())],
        end: Ok(end(StopReason::EndTurn)),
    }
}

fn header() -> CassetteHeader {
    CassetteHeader {
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
    }
}

fn cassette() -> Cassette {
    Cassette {
        header: header(),
        interactions: vec![interaction(0, "a", "first"), interaction(1, "b", "second")],
    }
}

fn texts(sink: &Collect) -> Vec<String> {
    sink.0
        .iter()
        .filter_map(|e| match e {
            TurnEvent::TextDelta(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn images_are_digests() {
    let print = RequestPrint::of(&request("a", b"png-bytes"));
    let PartPrint::Image(image) = &print.messages[0].parts[1] else {
        panic!("the second part is the image");
    };
    assert_eq!(image.len.0, 9);
    assert_eq!(
        image.digest.0,
        blake3::hash(b"png-bytes").to_hex().to_string()
    );
    let json = serde_json::to_string(&print).unwrap();
    assert!(
        !json.contains("png-bytes") && !json.contains("cG5n"),
        "no bytes in the print"
    );
}

#[test]
fn tool_results_keep_their_nesting() {
    let mut req = request("a", b"x");
    req.messages.push(Message {
        role: Role::Tool,
        parts: vec![Part::ToolResult(ToolResult {
            id: ToolCallId("c1".into()),
            status: ToolStatus::Ok,
            parts: vec![Part::Image(ImageInput {
                media: MediaType::Png,
                bytes: ImageBytes(vec![1, 2, 3]),
                detail: ImageDetail::Original,
            })],
        })],
    });
    let print = RequestPrint::of(&req);
    let PartPrint::ToolResult(result) = &print.messages[1].parts[0] else {
        panic!("a tool result");
    };
    assert!(matches!(result.parts[0], PartPrint::Image(_)));
}

#[test]
fn the_hash_follows_the_request_not_the_field_order() {
    let a = RequestPrint::of(&request("a", b"1"));
    assert_eq!(a.hash(), RequestPrint::of(&request("a", b"1")).hash());
    assert_eq!(a.hash().0.len(), 64);
    let table = [request("b", b"1"), request("a", b"2"), {
        let mut r = request("a", b"1");
        r.sampling.temperature = Milli(700);
        r
    }];
    for other in table {
        assert_ne!(a.hash(), RequestPrint::of(&other).hash());
    }
    // Reordering the keys of the stored print does not change its hash.
    let json = serde_json::to_value(&a).unwrap();
    let reparsed: RequestPrint = serde_json::from_str(&json.to_string()).unwrap();
    assert_eq!(reparsed.hash(), a.hash());
}

fn started(i: u16, id: &str) -> TurnEvent {
    TurnEvent::ToolCallStarted {
        index: CallIndex(i),
        id: ToolCallId(id.into()),
        name: ToolName::new("click").unwrap(),
    }
}

fn delta(i: u16) -> TurnEvent {
    TurnEvent::ToolCallDelta {
        index: CallIndex(i),
        fragment: "{}".into(),
    }
}

fn done(id: &str) -> TurnEvent {
    TurnEvent::ToolCallDone(ToolCall {
        id: ToolCallId(id.into()),
        name: ToolName::new("click").unwrap(),
        input: JsonText::new("{}").unwrap(),
    })
}

#[test]
fn check_sequence_table() {
    use StreamFault::*;
    let text = || TurnEvent::TextDelta("t".into());
    let table: Vec<(&str, Vec<TurnEvent>, Result<(), StreamFault>)> = vec![
        ("empty", vec![], Ok(())),
        ("text only", vec![text()], Ok(())),
        (
            "one call",
            vec![started(0, "a"), delta(0), delta(0), done("a")],
            Ok(()),
        ),
        ("no deltas", vec![started(0, "a"), done("a")], Ok(())),
        (
            "two interleaved",
            vec![
                started(0, "a"),
                started(1, "b"),
                delta(1),
                delta(0),
                done("b"),
                done("a"),
            ],
            Ok(()),
        ),
        (
            "an index reused after done",
            vec![
                started(0, "a"),
                done("a"),
                started(0, "b"),
                delta(0),
                done("b"),
            ],
            Ok(()),
        ),
        ("delta before start", vec![delta(0)], Err(UnknownCall)),
        ("done before start", vec![done("a")], Err(UnknownCall)),
        (
            "done for another id",
            vec![started(0, "a"), done("z")],
            Err(UnknownCall),
        ),
        (
            "started twice",
            vec![started(0, "a"), started(0, "b")],
            Err(EndedTwice),
        ),
        (
            "done twice",
            vec![started(0, "a"), done("a"), done("a")],
            Err(EndedTwice),
        ),
        (
            "delta after done",
            vec![started(0, "a"), done("a"), delta(0)],
            Err(AfterDone),
        ),
        ("never done", vec![started(0, "a"), delta(0)], Err(Unclosed)),
        (
            "one of two never done",
            vec![started(0, "a"), started(1, "b"), done("a")],
            Err(Unclosed),
        ),
    ];
    for (name, events, want) in table {
        assert_eq!(check_sequence(&events), want, "{name}");
    }
}

#[test]
fn a_bad_sequence_names_its_line() {
    let mut c = cassette();
    c.interactions[1].events = vec![started(0, "a")];
    assert_eq!(
        c.check_sequences(),
        Err(CassetteError::BadSequence { line: 3 })
    );
    assert_eq!(
        Cassette::from_jsonl(&c.to_jsonl()),
        Err(CassetteError::BadSequence { line: 3 })
    );
    assert_eq!(Cassette::from_jsonl(&cassette().to_jsonl()), Ok(cassette()));
}

#[test]
fn in_order_ignores_the_request_and_runs_out() {
    let p = ReplayProvider::new(cassette(), ReplayMode::InOrder);
    let mut sink = Collect(vec![], usize::MAX);
    block_on(p.turn(&request("zzz", b"q"), &mut sink)).unwrap();
    block_on(p.turn(&request("a", b"png"), &mut sink)).unwrap();
    assert_eq!(texts(&sink), ["first", "second"]);
    assert_eq!(
        block_on(p.turn(&request("a", b"png"), &mut sink)),
        Err(ProviderError::NotReady)
    );
    assert_eq!(p.misses(), vec![ReplayError::Exhausted]);
}

#[test]
fn by_request_matches_in_any_order_and_uses_each_once() {
    let p = ReplayProvider::new(cassette(), ReplayMode::ByRequest);
    let mut sink = Collect(vec![], usize::MAX);
    block_on(p.turn(&request("b", b"png"), &mut sink)).unwrap();
    block_on(p.turn(&request("a", b"png"), &mut sink)).unwrap();
    assert_eq!(texts(&sink), ["second", "first"]);
    assert_eq!(
        block_on(p.turn(&request("a", b"png"), &mut sink)),
        Err(ProviderError::NotReady)
    );
}

#[test]
fn mismatch_names_index() {
    let p = ReplayProvider::new(cassette(), ReplayMode::ByRequest);
    let mut sink = Collect(vec![], usize::MAX);
    let got = block_on(p.turn(&request("nope", b"png"), &mut sink));
    assert!(matches!(got, Err(ProviderError::BadRequest(_))));
    let [ReplayError::Mismatch { index, want, got }] = &p.misses()[..] else {
        panic!("one mismatch: {:?}", p.misses());
    };
    assert_eq!(*index, InteractionId(0));
    assert_eq!(**want, cassette().interactions[0].request);
    assert_eq!(**got, RequestPrint::of(&request("nope", b"png")));
}

#[test]
fn strict_needs_order_and_hash() {
    let p = ReplayProvider::new(cassette(), ReplayMode::Strict);
    let mut sink = Collect(vec![], usize::MAX);
    block_on(p.turn(&request("a", b"png"), &mut sink)).unwrap();
    // The second turn asks for "a" again, but the second interaction is "b".
    let got = block_on(p.turn(&request("a", b"png"), &mut sink));
    assert!(matches!(got, Err(ProviderError::BadRequest(m)) if m.contains("InteractionId(1)")));
    assert_eq!(texts(&sink), ["first"]);
}

#[test]
fn replay_ends_early_when_the_sink_stops() {
    let mut c = cassette();
    c.interactions[0].events = vec![
        TurnEvent::TextDelta("1".into()),
        TurnEvent::TextDelta("2".into()),
    ];
    let p = ReplayProvider::new(c, ReplayMode::InOrder);
    let mut sink = Collect(vec![], 1);
    let end = block_on(p.turn(&request("a", b"png"), &mut sink)).unwrap();
    assert_eq!(end.stop, StopReason::EndTurn);
    assert_eq!(sink.0.len(), 1);
}

#[test]
fn replay_returns_a_recorded_error() {
    let mut c = cassette();
    c.interactions[0].end = Err(ProviderError::Timeout);
    let p = ReplayProvider::new(c, ReplayMode::InOrder);
    let mut sink = Collect(vec![], usize::MAX);
    assert_eq!(
        block_on(p.turn(&request("a", b"png"), &mut sink)),
        Err(ProviderError::Timeout)
    );
}

#[test]
fn describe_names_the_cassette_model() {
    let p = ReplayProvider::new(cassette(), ReplayMode::InOrder);
    let models = block_on(p.describe()).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].name, ModelName("holo".into()));
}

#[test]
fn describe_reports_the_context_the_header_recorded() {
    let p = ReplayProvider::new(cassette(), ReplayMode::InOrder);
    let models = block_on(p.describe()).unwrap();
    assert_eq!(models[0].loaded_context, Tokens(8192));
    assert_eq!(models[0].trained_context, Tokens(32768));
}

#[derive(Default)]
struct Lines(Mutex<Vec<Interaction>>);

impl CassetteSink for &Lines {
    fn write(&self, line: &Interaction) -> Result<(), SinkError> {
        self.0.lock().unwrap().push(line.clone());
        Ok(())
    }
}

struct Refuse;

impl CassetteSink for Refuse {
    fn write(&self, _: &Interaction) -> Result<(), SinkError> {
        Err(SinkError)
    }
}

fn script(text: &str) -> Script {
    Script {
        events: vec![TurnEvent::TextDelta(text.into())],
        end: Ok(end(StopReason::EndTurn)),
    }
}

#[test]
fn recording_tees_events_and_replays_identically() {
    let lines = Lines::default();
    let inner = ScriptedProvider::new(vec![], vec![script("first"), script("second")]);
    let recorder = RecordingProvider::new(inner, &lines);
    let mut sink = Collect(vec![], usize::MAX);
    block_on(recorder.turn(&request("a", b"png"), &mut sink)).unwrap();
    block_on(recorder.turn(&request("b", b"png"), &mut sink)).unwrap();
    assert_eq!(
        texts(&sink),
        ["first", "second"],
        "the caller still sees every event"
    );
    let recorded = lines.0.lock().unwrap().clone();
    assert_eq!(recorded, cassette().interactions);
    // Written and read back through the file format, then replayed strictly.
    let file = Cassette {
        header: header(),
        interactions: recorded,
    };
    let replay = ReplayProvider::new(
        Cassette::from_jsonl(&file.to_jsonl()).unwrap(),
        ReplayMode::Strict,
    );
    let mut again = Collect(vec![], usize::MAX);
    block_on(replay.turn(&request("a", b"png"), &mut again)).unwrap();
    block_on(replay.turn(&request("b", b"png"), &mut again)).unwrap();
    assert_eq!(again.0, sink.0);
}

#[test]
fn recording_keeps_errors_and_a_refusing_sink_fails_the_turn() {
    let lines = Lines::default();
    let failing = Script {
        events: vec![],
        end: Err(ProviderError::Timeout),
    };
    let recorder = RecordingProvider::new(ScriptedProvider::new(vec![], vec![failing]), &lines);
    let mut sink = Collect(vec![], usize::MAX);
    assert_eq!(
        block_on(recorder.turn(&request("a", b"png"), &mut sink)),
        Err(ProviderError::Timeout)
    );
    assert_eq!(lines.0.lock().unwrap()[0].end, Err(ProviderError::Timeout));

    let refusing = RecordingProvider::new(ScriptedProvider::new(vec![], vec![script("x")]), Refuse);
    assert!(matches!(
        block_on(refusing.turn(&request("a", b"png"), &mut sink)),
        Err(ProviderError::Unreadable(_))
    ));
}
