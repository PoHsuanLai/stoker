use cua_action::{CuaDialect, GridMax, ModelSpace, ToolDialect, WireDialect};
use model_provider::{
    Batching, CallIndex, Caps, Constraint, CuaSupport, Effort, Flow, ImageBytes, ImageCount,
    ImageDetail, ImageInput, ImageLimits, InputKind, JsonText, Limits, Message, Milli, ModelInfo,
    ModelName, NativeTool, OutputShape, Part, Provider, ProviderError, Reasoning, RetrySeconds,
    Role, SafetySignal, SchemaText, Script, ScriptedProvider, StopReason, Support, Tokens,
    ToolCall, ToolCallId, ToolChoice, ToolName, ToolResult, ToolSpec, ToolStatus, ToolSupport,
    TurnEnd, TurnEvent, TurnRequest, TurnSink, TurnUsage, Zoom,
};
use model_provider::{EngineExtras, Knob, Sampling, ThoughtSeal, ToolParallelism};
use vision_prep::{MediaType, PatchFactor, PixelCount, ResizeRule};

fn call() -> ToolCall {
    ToolCall {
        id: ToolCallId("call_1".into()),
        name: ToolName::new("computer_use").unwrap(),
        input: JsonText::new(r#"{"action":"left_click","coordinate":[10,20]}"#).unwrap(),
    }
}

fn request() -> TurnRequest {
    let image = ImageInput {
        media: MediaType::Png,
        bytes: ImageBytes(vec![1, 2, 3]),
        detail: ImageDetail::Original,
    };
    TurnRequest {
        model: ModelName("holo".into()),
        messages: vec![
            Message {
                role: Role::System,
                parts: vec![Part::Text("act".into())],
            },
            Message {
                role: Role::User,
                parts: vec![
                    Part::Text("go".into()),
                    Part::Image(image),
                    Part::Thought {
                        text: "hm".into(),
                        seal: ThoughtSeal::None,
                    },
                ],
            },
            Message {
                role: Role::Assistant,
                parts: vec![Part::ToolCall(call())],
            },
            Message {
                role: Role::Tool,
                parts: vec![Part::ToolResult(ToolResult {
                    id: ToolCallId("call_1".into()),
                    status: ToolStatus::Error,
                    parts: vec![Part::Text("refused".into())],
                })],
            },
        ],
        tools: vec![
            ToolSpec::Function {
                name: ToolName::new("mail.search").unwrap(),
                description: "search".into(),
                parameters: SchemaText(JsonText::new(r#"{"type":"object"}"#).unwrap()),
            },
            ToolSpec::Native(NativeTool {
                dialect: WireDialect::AnthropicToolset20260801,
                config: JsonText::new(r#"{"display_width_px":1366}"#).unwrap(),
            }),
        ],
        tool_choice: ToolChoice::Named(ToolName::new("computer_use").unwrap()),
        tool_calls: ToolParallelism::One,
        output: OutputShape::JsonSchema(SchemaText(JsonText::new("{}").unwrap())),
        limits: Limits {
            max_output: Tokens(512),
            stop: vec!["\n\n".into()],
        },
        sampling: Sampling {
            temperature: Milli(0),
            top_p: Knob::Off,
            top_k: Knob::Off,
            min_p: Knob::Off,
            repeat_penalty: Knob::Off,
            seed: Knob::Off,
        },
        reasoning: Reasoning::On(Effort::Low),
        engine: EngineExtras::None,
        choice_scores: model_provider::ChoiceScores::Off,
    }
}

#[test]
fn requests_and_events_round_trip() {
    let req = request();
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(serde_json::from_str::<TurnRequest>(&json).unwrap(), req);
    let events = vec![
        TurnEvent::TextDelta("a".into()),
        TurnEvent::ThoughtDelta("b".into()),
        TurnEvent::ToolCallStarted {
            index: CallIndex(0),
            id: ToolCallId("c".into()),
            name: ToolName::new("t").unwrap(),
        },
        TurnEvent::ToolCallDelta {
            index: CallIndex(0),
            fragment: "{".into(),
        },
        TurnEvent::ToolCallDone(call()),
        TurnEvent::Safety(SafetySignal::RequireConfirmation("buy".into())),
        TurnEvent::Safety(SafetySignal::Blocked("no".into())),
        TurnEvent::Usage(TurnUsage {
            input: Tokens(10),
            output: Tokens(2),
            cached: Tokens(8),
            images: ImageCount(1),
        }),
    ];
    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serde_json::from_str::<TurnEvent>(&json).unwrap(),
            event,
            "{json}"
        );
    }
}

#[test]
fn image_bytes_serialise_as_base64_and_debug_hides_them() {
    let bytes = ImageBytes(vec![0xff, 0xd8, 0xff]);
    assert_eq!(serde_json::to_string(&bytes).unwrap(), r#""/9j/""#);
    assert_eq!(format!("{bytes:?}"), "ImageBytes(<3 bytes>)");
    assert!(serde_json::from_str::<ImageBytes>(r#""!!""#).is_err());
}

#[test]
fn json_text_and_tool_names_refuse_bad_input() {
    assert!(JsonText::new("{not json").is_err());
    assert!(serde_json::from_str::<JsonText>(r#""{not json""#).is_err());
    assert!(ToolName::new("").is_err());
    assert!(ToolName::new("has space").is_err());
    assert!(ToolName::new("mail.thread.archive").is_ok());
}

#[test]
fn provider_error_json_is_pinned() {
    let cases: &[(ProviderError, &str)] = &[
        (ProviderError::Unreachable, r#"{"kind":"unreachable"}"#),
        (
            ProviderError::RateLimited(RetrySeconds(3)),
            r#"{"kind":"rate_limited","v":3}"#,
        ),
        (
            ProviderError::ContextOverflow {
                limit: Tokens(32768),
            },
            r#"{"kind":"context_overflow","v":{"limit":32768}}"#,
        ),
    ];
    for (error, json) in cases {
        assert_eq!(&serde_json::to_string(error).unwrap(), json);
        assert_eq!(&serde_json::from_str::<ProviderError>(json).unwrap(), error);
    }
}

#[test]
fn caps_round_trip() {
    let caps = Caps {
        inputs: [InputKind::Text, InputKind::Image].into(),
        tools: ToolSupport::ServerParsed,
        output: [Constraint::JsonSchema].into(),
        reasoning: Support::Present,
        streaming: Support::Present,
        images: ImageLimits {
            per_prompt: ImageCount(3),
            rule: ResizeRule::SmartResize {
                factor: PatchFactor(32),
                min_pixels: PixelCount(65_536),
                max_pixels: PixelCount(16_777_216),
            },
            space: ModelSpace::Grid(GridMax(1000)),
        },
        context: Tokens(32768),
        max_output: Tokens(4096),
        computer_use: CuaSupport::Dialect {
            dialect: CuaDialect::Tool(ToolDialect::Holo31),
            batching: Batching::One,
            zoom: Zoom::Absent,
        },
    };
    let json = serde_json::to_string(&caps).unwrap();
    assert_eq!(serde_json::from_str::<Caps>(&json).unwrap(), caps);
}

struct Collect(Vec<TurnEvent>, usize);

impl TurnSink for Collect {
    fn event(&mut self, event: TurnEvent) -> Flow {
        self.0.push(event);
        if self.0.len() == self.1 {
            Flow::Stop
        } else {
            Flow::Continue
        }
    }
}

fn end() -> TurnEnd {
    TurnEnd {
        stop: StopReason::ToolUse,
        usage: TurnUsage::default(),
        served: ModelName("holo".into()),
        first_token: None,
    }
}

fn script() -> Script {
    Script {
        events: vec![
            TurnEvent::TextDelta("one".into()),
            TurnEvent::TextDelta("two".into()),
            TurnEvent::TextDelta("three".into()),
        ],
        end: Ok(end()),
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the scripted provider never waits"),
    }
}

#[test]
fn scripted_provider_replays_and_records() {
    let provider = ScriptedProvider::new(
        vec![ModelInfo {
            name: ModelName("holo".into()),
            loaded_context: Tokens(32768),
            trained_context: Tokens(262144),
        }],
        vec![script()],
    );
    let mut sink = Collect(Vec::new(), usize::MAX);
    let got = block_on(provider.turn(&request(), &mut sink)).unwrap();
    assert_eq!(got, end());
    assert_eq!(sink.0, script().events);
    assert_eq!(provider.requests(), vec![request()]);
    assert_eq!(block_on(provider.describe()).unwrap().len(), 1);
    // The script is spent: the next turn has nothing to play.
    assert_eq!(
        block_on(provider.turn(&request(), &mut sink)),
        Err(ProviderError::NotReady)
    );
}

#[test]
fn stop_flow_ends_turn() {
    let provider = ScriptedProvider::new(Vec::new(), vec![script()]);
    let mut sink = Collect(Vec::new(), 2);
    let got = block_on(provider.turn(&request(), &mut sink)).unwrap();
    assert_eq!(got.stop, StopReason::EndTurn);
    assert_eq!(sink.0.len(), 2, "no event after the sink said stop");
}
