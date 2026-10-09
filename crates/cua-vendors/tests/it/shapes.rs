use cua_action::WireDialect;
use cua_vendors::{StepResult, WireError, codec};
use model_provider::ToolCallId;

const DIALECTS: [WireDialect; 4] = [
    WireDialect::AnthropicToolset20260801,
    WireDialect::AnthropicComputer20251124,
    WireDialect::OpenAiComputer,
    WireDialect::GeminiComputerUse,
];

#[test]
fn dialects_round_trip_through_their_codec() {
    for dialect in DIALECTS {
        assert_eq!(codec(dialect).dialect(), dialect);
    }
}

#[test]
fn step_results_and_errors_round_trip() {
    let results = [
        StepResult::Done(ToolCallId("a".into())),
        StepResult::Refused {
            id: ToolCallId("b".into()),
            why: "outside the lease".into(),
        },
        StepResult::NotRun(ToolCallId("c".into())),
    ];
    for result in results {
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(serde_json::from_str::<StepResult>(&json).unwrap(), result);
    }
    let json = serde_json::to_string(&WireError::UnknownTool).unwrap();
    assert_eq!(json, r#"{"kind":"unknown_tool"}"#);
}
