//! Total parsing: no input panics, and what comes out is always within bounds.

mod common;

use cua_action::{GridMax, ModelSpace, TextDialect, ToolDialect};
use cua_parse::{InSpace, ParseLimits, Parsed, parse_text, parse_tool_calls};
use model_provider::{JsonText, ToolCall, ToolCallId, ToolName};
use proptest::prelude::*;

const SPACES: [ModelSpace; 2] = [ModelSpace::Image, ModelSpace::Grid(GridMax(1000))];

fn within_bounds(parsed: &Parsed, limits: ParseLimits) -> bool {
    let count = match &parsed.actions {
        InSpace::Image(a) => a.len(),
        InSpace::Grid(_, a) => a.len(),
    };
    count <= usize::from(limits.max_actions.0)
        && parsed
            .dropped
            .iter()
            .all(|d| d.verb.as_str().chars().count() <= 64)
}

/// Text that looks like the dialect: calls, quotes, brackets, numbers, markers.
fn grammar_soup() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("Thought: ".to_owned()),
        Just("Action: ".to_owned()),
        Just("click(".to_owned()),
        Just("type(content='".to_owned()),
        Just("start_box='(".to_owned()),
        Just("<|box_start|>".to_owned()),
        Just("hotkey(key='".to_owned()),
        Just("'".to_owned()),
        Just("\"".to_owned()),
        Just(")".to_owned()),
        Just("(".to_owned()),
        Just(",".to_owned()),
        Just("\\".to_owned()),
        Just("\n\n".to_owned()),
        Just("=".to_owned()),
        "[0-9]{1,12}",
        "[a-z_]{1,8}",
        "\\PC{0,6}",
    ];
    prop::collection::vec(piece, 0..40).prop_map(|p| p.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn never_panics(text in prop_oneof![any::<String>(), grammar_soup()]) {
        let limits = ParseLimits::default();
        for space in SPACES {
            if let Ok(parsed) = parse_text(TextDialect::UiTars15, space, &text, limits) {
                prop_assert!(within_bounds(&parsed, limits));
            }
        }
    }

    #[test]
    fn never_panics_on_tool_calls(
        name in prop_oneof![Just("computer_use".to_owned()), Just("click".to_owned()), "[a-z_.-]{1,12}"],
        body in prop_oneof![any::<String>(), grammar_soup(), json_soup()],
        copies in 0usize..12,
    ) {
        let (Ok(name), Ok(input)) = (ToolName::new(name), JsonText::new(body)) else { return Ok(()) };
        let call = ToolCall { id: ToolCallId("c".into()), name, input };
        let calls = vec![call; copies];
        let limits = ParseLimits::default();
        for dialect in [ToolDialect::QwenComputerUse, ToolDialect::Holo31] {
            for space in SPACES {
                if let Ok(parsed) = parse_tool_calls(dialect, space, &calls, limits) {
                    prop_assert!(within_bounds(&parsed, limits));
                }
            }
        }
    }
}

/// Valid JSON objects with the argument names the dialects read and hostile values.
fn json_soup() -> impl Strategy<Value = String> {
    let value = prop_oneof![
        Just("1".to_owned()),
        Just("-1".to_owned()),
        Just("1.5".to_owned()),
        Just("4294967296".to_owned()),
        Just("1e999".to_owned()),
        Just("[1,2]".to_owned()),
        Just("[1]".to_owned()),
        Just("null".to_owned()),
        Just("\"\"".to_owned()),
        Just("\"ctrl+a\"".to_owned()),
        Just("\"success\"".to_owned()),
        Just("{}".to_owned()),
        "\"[ -~]{0,20}\"",
    ];
    let key = prop_oneof![
        Just("action"),
        Just("coordinate"),
        Just("x"),
        Just("y"),
        Just("keys"),
        Just("text"),
        Just("pixels"),
        Just("time"),
        Just("status"),
        Just("direction"),
        Just("question"),
        Just("choices"),
        Just("start_coordinate"),
    ];
    let action = prop_oneof![
        Just("\"left_click\""),
        Just("\"scroll\""),
        Just("\"key\""),
        Just("\"wait\""),
        Just("\"terminate\""),
        Just("\"type\""),
        Just("\"left_click_drag\""),
    ];
    (action, prop::collection::vec((key, value), 0..6)).prop_map(|(action, pairs)| {
        let fields: Vec<String> = pairs.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!(
            "{{\"action\":{action}{}{}}}",
            if fields.is_empty() { "" } else { "," },
            fields.join(",")
        )
    })
}
