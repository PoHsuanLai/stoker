//! Reading a reply by dialect.

use std::borrow::Cow;

use cua_action::{
    CallFormat, CuaAction, CuaDialect, FinishOutcome, ModelSpace, Summary, ToolDialect,
};
use cua_parse::{
    ByteOffset, InSpace, ParamKinds, ParseError, ParseLimits, Parsed, ToolContext, parse_text,
    parse_tool_calls_with, read_calls,
};
use cua_vendors::{WireCodec, codec};
use model_provider::ToolCall;

use crate::{TurnTranscript, prompt};

pub(crate) fn parse(
    dialect: CuaDialect,
    space: ModelSpace,
    context: ToolContext,
    reply: &TurnTranscript,
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    match dialect {
        CuaDialect::Text(text) => parse_text(text, space, &reply.text, limits),
        CuaDialect::Tool(tool) => {
            let calls = calls_of(tool, &context, reply, limits)?;
            parse_tool_calls_with(tool, context, space, &calls, limits)
        }
        CuaDialect::Wire(_) if reply.calls.is_empty() => finished_by_talking(&reply.text),
        CuaDialect::Wire(wire) => codec(wire)
            .decode(&reply.calls, &reply.safety)
            .map_err(|_| ParseError::Malformed { at: ByteOffset(0) }),
    }
}

/// The calls of a reply: the engine's, or, for a model whose profile says we read them, those
/// written in its text (calls the engine did parse anyway are used as they are).
fn calls_of<'a>(
    dialect: ToolDialect,
    context: &ToolContext,
    reply: &'a TurnTranscript,
    limits: ParseLimits,
) -> Result<Cow<'a, [ToolCall]>, ParseError> {
    match context.profile.calls {
        CallFormat::Engine => Ok(Cow::Borrowed(&reply.calls)),
        _ if !reply.calls.is_empty() => Ok(Cow::Borrowed(&reply.calls)),
        format => {
            let kinds = ParamKinds::from_schema(&prompt::schema_text(dialect, context.profile));
            read_calls(format, &reply.text, &reply.thought, &kinds, limits).map(Cow::Owned)
        }
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
