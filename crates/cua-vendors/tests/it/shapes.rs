use cua_vendors::{StepResult, WireError};
use model_provider::ToolCallId;

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
