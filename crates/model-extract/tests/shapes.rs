use model_extract::{ExtractMode, ExtractSession, FINAL_RESULT_TOOL, RepairBudget, RepairsLeft};
use model_provider::{ChoiceText, Extract, JsonText, OutputShape, Shape, ShapeFault, ToolName};

/// A hand-written output type, as the five or six real ones will be.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Verdict(String);

impl Extract for Verdict {
    fn shape() -> Shape {
        Shape::Choice(vec![ChoiceText("allow".into()), ChoiceText("deny".into())])
    }

    fn read(json: &JsonText) -> Result<Self, ShapeFault> {
        serde_json::from_str(json.as_str())
            .map(Verdict)
            .map_err(|_| ShapeFault::NotJson)
    }
}

#[test]
fn a_session_starts_with_its_whole_budget() {
    let session = ExtractSession::<Verdict>::new(
        ExtractMode::Native(OutputShape::Choice(vec!["allow".into(), "deny".into()])),
        RepairBudget(1),
    );
    assert_eq!(session.left(), RepairsLeft(1));
    assert_eq!(
        session.mode(),
        &ExtractMode::Native(OutputShape::Choice(vec!["allow".into(), "deny".into()]))
    );
}

#[test]
fn the_synthetic_tool_name_is_a_valid_tool_name() {
    assert!(ToolName::new(FINAL_RESULT_TOOL).is_ok());
}

#[test]
fn a_verdict_reads_its_json() {
    assert_eq!(
        Verdict::read(&JsonText::new("\"allow\"").unwrap()),
        Ok(Verdict("allow".into()))
    );
    assert_eq!(Verdict::shape(), Verdict::shape());
}
