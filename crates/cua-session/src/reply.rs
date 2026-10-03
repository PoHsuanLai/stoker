//! Reading a reply by dialect.

use cua_action::{CuaAction, CuaDialect, FinishOutcome, ModelSpace, Summary};
use cua_parse::{
    ByteOffset, InSpace, ParseError, ParseLimits, Parsed, parse_text, parse_tool_calls,
};
use cua_vendors::{WireCodec, codec};

use crate::TurnTranscript;

pub(crate) fn parse(
    dialect: CuaDialect,
    space: ModelSpace,
    reply: &TurnTranscript,
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    match dialect {
        CuaDialect::Text(text) => parse_text(text, space, &reply.text, limits),
        CuaDialect::Tool(tool) => parse_tool_calls(tool, space, &reply.calls, limits),
        CuaDialect::Wire(_) if reply.calls.is_empty() => finished_by_talking(&reply.text),
        CuaDialect::Wire(wire) => codec(wire)
            .decode(&reply.calls, &[])
            .map_err(|_| ParseError::Malformed { at: ByteOffset(0) }),
    }
}

/// A vendor model that answers with text and no tool call has stopped: that is its way of saying
/// the goal is done (or that it has nothing more to do), so the text is the summary of a `Finish`.
fn finished_by_talking(text: &str) -> Result<Parsed, ParseError> {
    let text: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(Summary::MAX_CHARS as usize)
        .collect();
    let summary = Summary::new(text).map_err(|_| ParseError::NoAction)?;
    if summary.as_str().is_empty() {
        return Err(ParseError::NoAction);
    }
    Ok(Parsed {
        thought: None,
        actions: InSpace::Image(vec![CuaAction::Finish {
            outcome: FinishOutcome::Done,
            summary,
            extracted: Vec::new(),
        }]),
        dropped: Vec::new(),
    })
}
