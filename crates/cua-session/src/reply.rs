//! Reading a reply by dialect.

use cua_action::{CuaDialect, ModelSpace};
use cua_parse::{ByteOffset, ParseError, ParseLimits, Parsed, parse_text, parse_tool_calls};
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
        CuaDialect::Wire(wire) => codec(wire)
            .decode(&reply.calls, &[])
            .map_err(|_| ParseError::Malformed { at: ByteOffset(0) }),
    }
}
