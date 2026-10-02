use model_provider::{
    ImageDetail, Limits, Milli, ModelName, OutputShape, ProviderError, Reasoning, Role, StopReason,
    Tokens, ToolChoice, TurnEnd, TurnEvent, TurnUsage,
};
use model_replay::{
    BackendLabel, ByteCount, Cassette, CassetteError, CassetteHeader, CassetteVersion, ImageDigest,
    ImagePrint, Interaction, MessagePrint, PartPrint, RecordedAt, RequestPrint,
};
use vision_prep::MediaType;

fn print() -> RequestPrint {
    RequestPrint {
        model: ModelName("holo".into()),
        messages: vec![MessagePrint {
            role: Role::User,
            parts: vec![
                PartPrint::Text("go".into()),
                PartPrint::Image(ImagePrint {
                    media: MediaType::Png,
                    digest: ImageDigest("ab".repeat(32)),
                    len: ByteCount(4096),
                    detail: ImageDetail::Auto,
                }),
            ],
        }],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        output: OutputShape::Free,
        limits: Limits {
            max_output: Tokens(256),
            temperature: Milli(0),
            stop: vec![],
        },
        reasoning: Reasoning::Off,
    }
}

fn cassette() -> Cassette {
    let end = TurnEnd {
        stop: StopReason::EndTurn,
        usage: TurnUsage::default(),
        served: ModelName("holo".into()),
    };
    Cassette {
        header: CassetteHeader {
            vocab: CassetteVersion::CURRENT,
            backend: BackendLabel("vllm".into()),
            model: ModelName("holo".into()),
            recorded: RecordedAt(1_790_000_000),
        },
        interactions: vec![
            Interaction {
                request: print(),
                events: vec![TurnEvent::TextDelta("hi".into())],
                end: Ok(end),
            },
            Interaction {
                request: print(),
                events: vec![],
                end: Err(ProviderError::Timeout),
            },
        ],
    }
}

#[test]
fn cassette_round_trips_as_jsonl() {
    let text = cassette().to_jsonl();
    assert_eq!(
        text.lines().count(),
        3,
        "a header line and one line per interaction"
    );
    assert_eq!(Cassette::from_jsonl(&text).unwrap(), cassette());
}

#[test]
fn header_line_is_pinned() {
    let text = cassette().to_jsonl();
    assert_eq!(
        text.lines().next().unwrap(),
        r#"{"vocab":1,"backend":"vllm","model":"holo","recorded":1790000000}"#
    );
}

#[test]
fn images_are_digests_in_the_file() {
    let text = cassette().to_jsonl();
    assert!(text.contains(&"ab".repeat(32)));
    assert!(
        !text.contains("bytes"),
        "no image bytes field in a cassette"
    );
}

#[test]
fn bad_files_name_the_line() {
    assert_eq!(Cassette::from_jsonl(""), Err(CassetteError::Empty));
    let mut text = cassette().to_jsonl();
    text.push_str("{not json}\n");
    assert_eq!(
        Cassette::from_jsonl(&text),
        Err(CassetteError::BadLine { line: 4 })
    );
    let old = text.replacen(r#""vocab":1"#, r#""vocab":9"#, 1);
    assert_eq!(
        Cassette::from_jsonl(&old),
        Err(CassetteError::Version {
            found: CassetteVersion(9),
            want: CassetteVersion::CURRENT
        })
    );
}
