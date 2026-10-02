//! The two entry points.

use cua_action::{ModelSpace, TextDialect, ToolDialect};
use model_provider::ToolCall;

use crate::{ParseError, ParseLimits, Parsed};

/// Parses a text dialect (`Thought: .. Action: click(start_box='(x,y)')`) whose points live in
/// `space`. Never panics; input over `limits.max_input` is `TooLarge`.
pub fn parse_text(
    dialect: TextDialect,
    space: ModelSpace,
    text: &str,
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    let _ = (dialect, space, text, limits);
    todo!("parse_text: UiTars15 grammar")
}

/// Parses the tool calls an engine extracted from a reply. Never panics.
pub fn parse_tool_calls(
    dialect: ToolDialect,
    space: ModelSpace,
    calls: &[ToolCall],
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    let _ = (dialect, space, calls, limits);
    todo!("parse_tool_calls: QwenComputerUse and Holo31 schemas")
}
