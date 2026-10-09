//! The bounds: input size, batch size, and the report of what was dropped.

use crate::common;

use common::*;
use cua_action::{ModelSpace, TextDialect, ToolDialect};
use cua_parse::{
    ActionCount, ByteLen, DropReason, ParseError, ParseLimits, parse_text, parse_tool_calls,
};

#[test]
fn limits_hold_for_text() {
    let nine = format!("Action: {}", "wait()\n\n".repeat(9));
    let p = ui_tars(&nine).unwrap();
    assert_eq!(image_actions(&p).len(), 8);
    assert_eq!(drops(&p), [("wait", DropReason::OverBatchLimit)]);

    let big = format!("Action: wait() {}", " ".repeat(33 * 1024));
    assert_eq!(ui_tars(&big), Err(ParseError::TooLarge));
    // Exactly at the limit is accepted.
    let at_limit = format!(
        "Action: wait(){}",
        " ".repeat(32 * 1024 - "Action: wait()".len())
    );
    assert_eq!(at_limit.len(), 32 * 1024);
    assert!(ui_tars(&at_limit).is_ok());
}

#[test]
fn limits_hold_for_tool_calls() {
    let one = call("computer_use", r#"{"action":"wait","time":1}"#);
    let nine = vec![one.clone(); 9];
    let p = tools(ToolDialect::QwenComputerUse, &nine).unwrap();
    assert_eq!(image_actions(&p).len(), 8);
    assert_eq!(drops(&p), [("wait", DropReason::OverBatchLimit)]);

    let pad = " ".repeat(33 * 1024);
    let big = call(
        "computer_use",
        &format!(r#"{{"action":"wait","time":1}}{pad}"#),
    );
    assert_eq!(
        tools(ToolDialect::QwenComputerUse, &[big]),
        Err(ParseError::TooLarge)
    );
}

#[test]
fn custom_limits_apply() {
    let limits = ParseLimits {
        max_input: ByteLen(20),
        max_actions: ActionCount(1),
    };
    let text = "Action: wait()\n\nwait()";
    assert_eq!(
        parse_text(TextDialect::UiTars15, ModelSpace::Image, text, limits),
        Err(ParseError::TooLarge)
    );
    let limits = ParseLimits {
        max_input: ByteLen(64),
        max_actions: ActionCount(1),
    };
    let p = parse_text(TextDialect::UiTars15, ModelSpace::Image, text, limits).unwrap();
    assert_eq!(image_actions(&p).len(), 1);
    let none = ParseLimits {
        max_input: ByteLen(64),
        max_actions: ActionCount(0),
    };
    let p = parse_text(TextDialect::UiTars15, ModelSpace::Image, text, none).unwrap();
    assert!(image_actions(&p).is_empty());
    assert_eq!(p.dropped.len(), 2);
    assert_eq!(
        parse_tool_calls(ToolDialect::Holo31, ModelSpace::Image, &[], limits),
        Err(ParseError::NoAction)
    );
}

#[test]
fn the_dropped_report_is_bounded_and_clean() {
    let many = format!("Action: {}", "bogus()\n".repeat(500));
    let p = ui_tars(&many).unwrap();
    assert_eq!(p.dropped.len(), 64);
    let long = format!("Action: {}()", "v".repeat(300));
    let p = ui_tars(&long).unwrap();
    assert_eq!(p.dropped[0].verb.as_str().chars().count(), 64);
}
