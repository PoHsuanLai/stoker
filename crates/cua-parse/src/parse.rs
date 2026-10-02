//! The two entry points.

use cua_action::{ModelSpace, TextDialect, ToolDialect};
use model_provider::ToolCall;

use crate::common::in_space;
use crate::tools::Tools;
use crate::ui_tars::UiTars;
use crate::{ParseError, ParseLimits, Parsed};

/// Parses a text dialect (`Thought: .. Action: click(start_box='(x,y)')`) whose points live in
/// `space`. Never panics; input over `limits.max_input` is `TooLarge`.
///
/// A reply without an `Action:` line is `NoAction`. A call that never closes is `Unterminated`,
/// and reading beyond the batch limit drops the extra actions as `OverBatchLimit`.
pub fn parse_text(
    dialect: TextDialect,
    space: ModelSpace,
    text: &str,
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    if text.len() > limits.max_input.0 as usize {
        return Err(ParseError::TooLarge);
    }
    match dialect {
        TextDialect::UiTars15 => in_space(&UiTars(text), space, limits),
    }
}

/// Parses the tool calls an engine extracted from a reply. Never panics.
///
/// No calls is `NoAction`; the calls' names and arguments together over `limits.max_input` is
/// `TooLarge`.
pub fn parse_tool_calls(
    dialect: ToolDialect,
    space: ModelSpace,
    calls: &[ToolCall],
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    let size: usize = calls
        .iter()
        .map(|c| c.name.as_str().len() + c.input.as_str().len())
        .sum();
    if size > limits.max_input.0 as usize {
        return Err(ParseError::TooLarge);
    }
    in_space(&Tools { dialect, calls }, space, limits)
}
