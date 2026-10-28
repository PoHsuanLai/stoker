//! The vendor protocols: the tool declaration carries the prompt and the reply is decoded by
//! `cua-vendors`.

use cua_action::{ActionProfile, CuaAction, FinishOutcome, ModelSpace, Summary, WireDialect};
use cua_parse::{ByteOffset, InSpace, ParseError, ParseLimits, Parsed, ToolContext};
use cua_vendors::WireCodec;
use model_provider::ToolSpec;
use vision_prep::FrameMap;

use super::ActionDialect;
use crate::TurnTranscript;

const ANTHROPIC_TOOLSET: &Wire = &Wire(WireDialect::AnthropicToolset20260801);
const ANTHROPIC_COMPUTER: &Wire = &Wire(WireDialect::AnthropicComputer20251124);
const OPENAI_COMPUTER: &Wire = &Wire(WireDialect::OpenAiComputer);
const GEMINI_COMPUTER_USE: &Wire = &Wire(WireDialect::GeminiComputerUse);

pub(super) fn of(wire: WireDialect) -> &'static dyn ActionDialect {
    match wire {
        WireDialect::AnthropicToolset20260801 => ANTHROPIC_TOOLSET,
        WireDialect::AnthropicComputer20251124 => ANTHROPIC_COMPUTER,
        WireDialect::OpenAiComputer => OPENAI_COMPUTER,
        WireDialect::GeminiComputerUse => GEMINI_COMPUTER_USE,
    }
}

struct Wire(WireDialect);

impl ActionDialect for Wire {
    fn wire(&self) -> Option<WireDialect> {
        Some(self.0)
    }

    fn tools(&self, _actions: ActionProfile, map: &FrameMap) -> Vec<ToolSpec> {
        self.0.tools(map.image)
    }

    fn parse(
        &self,
        _space: ModelSpace,
        _context: ToolContext,
        reply: &TurnTranscript,
        _limits: ParseLimits,
    ) -> Result<Parsed, ParseError> {
        if reply.calls.is_empty() {
            return finished_by_talking(&reply.text);
        }
        self.0
            .decode(&reply.calls, &reply.safety)
            .map_err(|_| ParseError::Malformed { at: ByteOffset(0) })
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
