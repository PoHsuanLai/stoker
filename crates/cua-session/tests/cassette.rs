//! A whole run over wire cassettes: the session builds each request, `Driver<OpenAiCodec, _>`
//! sends it through a `RecordingTransport` over a canned vLLM stream, and the cassette is then
//! replayed in strict mode under every chunking, so the request the session builds must be the
//! same bytes every time and the outcomes must not depend on how the stream is cut.
//!
//! The streams are hand-written from the OpenAI chat-completions chunk format vLLM answers in
//! (they are not recordings of a model); a recorded run replaces them when
//! `dev/record-engine.sh` is run against Holo-3.1-4B.

mod support;

use std::sync::Mutex;

use cua_action::{CuaAction, CuaDialect, ModelSpace, TextDialect, ToolDialect};
use cua_session::{CuaSession, StepOutcome, TranscriptSink};
use model_http::{
    BodyKind, BodySink, ChunkFlow, Exchange, HttpError, HttpStatus, ResponseHead, Transport,
};
use model_openai_compat::{Flavor, OpenAiCodec};
use model_provider::{ModelName, Provider, ProviderError, Seed, Tokens, TurnRequest};
use model_replay::{
    BuildLabel, ByteStep, CassetteHeader, CassetteVersion, ChunkPlan, ContextStamp, EngineLabel,
    EngineStamp, RecordedAt, RecordingTransport, ReplayMode, ReplayTransport, SinkError,
    WireCassette, WireExchange, WireSink,
};
use model_wire::Driver;
use serde_json::{Value, json};
use support::*;

/// Answers every request with the next canned body, as a server would.
struct Canned {
    bodies: Mutex<Vec<String>>,
}

impl Transport for Canned {
    fn exchange<K: BodySink>(
        &self,
        _ex: &Exchange,
        sink: &mut K,
    ) -> impl Future<Output = Result<HttpStatus, HttpError>> + Send {
        let body = self.bodies.lock().unwrap().remove(0);
        let mut flow = sink.head(&ResponseHead {
            status: HttpStatus(200),
            body: BodyKind::EventStream,
            retry_after: None,
            request_id: None,
        });
        for chunk in body.as_bytes().chunks(11) {
            if flow == ChunkFlow::Stop {
                break;
            }
            flow = sink.chunk(chunk);
        }
        std::future::ready(Ok(HttpStatus(200)))
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

fn chunk(delta: Value, finish: Option<&str>) -> String {
    format!(
        "data: {}\n\n",
        json!({"id":"c","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    )
}

fn usage_and_done() -> String {
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"id":"c","choices":[],"usage":{"prompt_tokens":900,"completion_tokens":20}})
    )
}

/// A stream that answers with one tool call, its arguments in two fragments.
fn tool_stream(thought: &str, arguments: &str) -> String {
    let (head, tail) = arguments.split_at(arguments.len() / 2);
    [
        chunk(json!({"role":"assistant","content":""}), None),
        chunk(json!({"reasoning_content": thought}), None),
        chunk(
            json!({"tool_calls":[{"index":0,"id":"call_1","type":"function",
                "function":{"name":"computer_use","arguments":head}}]}),
            None,
        ),
        chunk(
            json!({"tool_calls":[{"index":0,"function":{"arguments":tail}}]}),
            None,
        ),
        chunk(json!({}), Some("tool_calls")),
        usage_and_done(),
    ]
    .concat()
}

/// A stream that answers with text, in pieces.
fn text_stream(text: &str) -> String {
    let mid = text
        .char_indices()
        .nth(text.chars().count() / 2)
        .map_or(0, |(i, _)| i);
    let (head, tail) = text.split_at(mid);
    [
        chunk(json!({"role":"assistant","content":head}), None),
        chunk(json!({"content":tail}), None),
        chunk(json!({}), Some("stop")),
        usage_and_done(),
    ]
    .concat()
}

fn header() -> CassetteHeader {
    CassetteHeader {
        vocab: CassetteVersion::CURRENT,
        engine: EngineStamp {
            kind: EngineLabel("vllm".into()),
            build: BuildLabel("v0.12.0".into()),
        },
        model: ModelName("holo".into()),
        recorded: RecordedAt(1_790_000_000),
        context: ContextStamp {
            loaded: Tokens(32768),
            trained: Tokens(32768),
        },
        speech: None,
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("these transports never wait"),
    }
}

/// What one step came to, for comparing runs.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Acted(Vec<CuaAction<cua_action::WindowSpace>>),
    Repaired,
    Gave(String),
}

/// Runs steps against `driver` until `frames` run out, repairing as the session asks.
fn run<T: Transport>(
    driver: &Driver<OpenAiCodec, T>,
    mut session: CuaSession,
    map: &vision_prep::FrameMap,
    frames: &[&str],
) -> Result<Vec<Step>, ProviderError> {
    let mut steps = Vec::new();
    for (n, tag) in frames.iter().enumerate() {
        let mut request: TurnRequest = session.request(&obs(n as u32), map, frame(tag));
        loop {
            let mut sink = TranscriptSink::new();
            let end = block_on(driver.turn(&request, &mut sink))?;
            let outcome = session.absorb_for(&request, sink.finish(end), map);
            match outcome {
                StepOutcome::Actions { actions, .. } => {
                    steps.push(Step::Acted(actions));
                    break;
                }
                StepOutcome::Repair(repair) => {
                    steps.push(Step::Repaired);
                    request = repair;
                }
                StepOutcome::Unparseable(error) => {
                    steps.push(Step::Gave(error.to_string()));
                    break;
                }
            }
        }
    }
    Ok(steps)
}

fn plans() -> Vec<ChunkPlan> {
    let mut plans = vec![ChunkPlan::Whole, ChunkPlan::Lines];
    plans.extend([1, 2, 3, 7, 64].map(|n| ChunkPlan::Every(ByteStep(n))));
    plans.extend((0..12).map(|s| ChunkPlan::Seeded(Seed(s))));
    plans
}

/// Records `bodies` through the session, then replays the cassette strictly under every plan.
fn record_and_replay(
    dialect: CuaDialect,
    space: ModelSpace,
    bodies: Vec<String>,
    frames: &[&str],
) -> (Vec<Step>, WireCassette) {
    let map = map(space);
    let written = Written::default();
    let recording = Driver::new(
        OpenAiCodec::new(Flavor::Vllm),
        RecordingTransport::new(
            Canned {
                bodies: Mutex::new(bodies),
            },
            &written,
        ),
    );
    let recorded = run(&recording, session(dialect, space, 2, 1), &map, frames).unwrap();
    let cassette = WireCassette {
        header: header(),
        exchanges: written.0.lock().unwrap().clone(),
    };
    let reread = WireCassette::from_jsonl(&cassette.to_jsonl()).unwrap();
    for plan in plans() {
        let replaying = Driver::new(
            OpenAiCodec::new(Flavor::Vllm),
            ReplayTransport::new(reread.clone(), ReplayMode::Strict, plan),
        );
        let steps = run(&replaying, session(dialect, space, 2, 1), &map, frames).unwrap();
        assert_eq!(steps, recorded, "{plan:?}");
        assert!(replaying.transport().misses().is_empty(), "{plan:?}");
    }
    (recorded, cassette)
}

fn body_of(exchange: &WireExchange) -> Value {
    serde_json::from_str(exchange.request.body.as_ref().unwrap().as_str()).unwrap()
}

#[test]
fn holo_runs_three_steps_through_the_wire_and_replays_identically() {
    let dialect = CuaDialect::Tool(ToolDialect::Holo31);
    let bodies = vec![
        tool_stream(
            "the Save button",
            r#"{"action":"left_click","coordinate":[500,250]}"#,
        ),
        tool_stream("a dialog", r#"{"action":"type","text":"notes.txt"}"#),
        tool_stream(
            "done",
            r#"{"action":"terminate","status":"success","summary":"saved"}"#,
        ),
    ];
    let (steps, cassette) = record_and_replay(dialect, grid(), bodies, &["f0", "f1", "f2"]);
    assert_eq!(steps.len(), 3);
    let Step::Acted(first) = &steps[0] else {
        panic!("acted")
    };
    assert!(matches!(&first[..], [CuaAction::Click { .. }]));
    assert!(matches!(&steps[1], Step::Acted(a) if matches!(&a[..], [CuaAction::Type { .. }])));
    assert!(matches!(&steps[2], Step::Acted(a) if matches!(&a[..], [CuaAction::Finish { .. }])));

    // What went on the wire: the system prompt, the function, no reasoning switch left to the
    // engine but off, and the frames of the last two steps ahead of the current one.
    let first = body_of(&cassette.exchanges[0]);
    assert_eq!(first["messages"][0]["role"], "system");
    assert_eq!(first["tools"][0]["function"]["name"], "computer_use");
    assert_eq!(first["chat_template_kwargs"]["enable_thinking"], false);
    assert_eq!(first["temperature"], 0.0);
    assert_eq!(first["stream"], true);
    let user = &first["messages"][1]["content"];
    assert!(
        user[0]["text"]
            .as_str()
            .unwrap()
            .contains("Goal: save the file")
    );
    let third = body_of(&cassette.exchanges[2]);
    let content = third["messages"][1]["content"].as_array().unwrap();
    let image_parts = content.iter().filter(|p| p["type"] == "image_url").count();
    assert_eq!(image_parts, 3, "two earlier frames and this one");
    let lead = content[0]["text"].as_str().unwrap();
    assert!(
        lead.contains("- step 0: click left x1 at (500, 250)"),
        "{lead}"
    );
    assert!(lead.contains("- step 1: typed 9 characters"), "{lead}");
    assert!(!lead.contains("notes.txt"));
}

#[test]
fn qwen_in_pixel_space_repairs_once_over_the_wire() {
    let dialect = CuaDialect::Tool(ToolDialect::QwenComputerUse);
    let m = map(ModelSpace::Image);
    let (x, y) = (m.image.w.0 / 2, m.image.h.0 / 2);
    let bodies = vec![
        text_stream("I will click the button now."),
        tool_stream(
            "",
            &format!(r#"{{"action":"left_click","coordinate":[{x},{y}]}}"#),
        ),
    ];
    let (steps, cassette) = record_and_replay(dialect, ModelSpace::Image, bodies, &["f0"]);
    assert_eq!(steps[0], Step::Repaired);
    let Step::Acted(actions) = &steps[1] else {
        panic!("acted after the repair")
    };
    let [
        CuaAction::Click {
            at: cua_action::Target::Point(p),
            ..
        },
    ] = &actions[..]
    else {
        panic!("a click")
    };
    assert_eq!((p.x.0, p.y.0), (WINDOW_W / 2, WINDOW_H / 2));
    // The repair request is the first one plus a message, with the same frame.
    let (first, repair) = (
        body_of(&cassette.exchanges[0]),
        body_of(&cassette.exchanges[1]),
    );
    let (a, b) = (
        first["messages"].as_array().unwrap(),
        repair["messages"].as_array().unwrap(),
    );
    assert_eq!(&b[..a.len()], &a[..]);
    assert_eq!(b.len(), a.len() + 1);
    assert!(
        b[a.len()]["content"]
            .as_str()
            .unwrap()
            .contains("computer_use")
    );
    assert!(!repair.to_string().contains("click the button now"));
}

#[test]
fn ui_tars_alternates_frames_and_replies_over_the_wire() {
    let dialect = CuaDialect::Text(TextDialect::UiTars15);
    let m = map(ModelSpace::Image);
    let (x, y) = (m.image.w.0 / 4, m.image.h.0 / 4);
    let step = |what: &str| {
        text_stream(&format!(
            "Thought: {what}\nAction: click(start_box='<|box_start|>({x},{y})<|box_end|>')"
        ))
    };
    let bodies = vec![step("first"), step("second"), step("third")];
    let (steps, cassette) =
        record_and_replay(dialect, ModelSpace::Image, bodies, &["f0", "f1", "f2"]);
    assert!(
        steps
            .iter()
            .all(|s| matches!(s, Step::Acted(a) if a.len() == 1))
    );
    let last = body_of(&cassette.exchanges[2]);
    let roles: Vec<&str> = last["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        ["system", "user", "assistant", "user", "assistant", "user"]
    );
    assert!(last.get("tools").is_none());
    let replayed = &last["messages"][2]["content"];
    assert!(replayed.as_str().unwrap().starts_with("Thought: first"));
}
