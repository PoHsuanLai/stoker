//! UI-TARS 1.5: the model answers in `Thought:` and `Action:` text.

use cua_action::{ActionProfile, ModelSpace, TextDialect};
use cua_parse::{ParseError, ParseLimits, Parsed, ToolContext, parse_text};
use vision_prep::FrameMap;

use super::ActionDialect;
use crate::prompt::{body, instruction};
use crate::{CuaTaskText, TurnTranscript};

const SYSTEM: &str = include_str!("../../prompts/ui_tars_15.txt");
/// The language of `Thought:`.
const THOUGHT_LANGUAGE: &str = "English";

pub(super) fn of(dialect: TextDialect) -> &'static dyn ActionDialect {
    match dialect {
        TextDialect::UiTars15 => &UiTars,
    }
}

struct UiTars;

impl ActionDialect for UiTars {
    fn is_text(&self) -> bool {
        true
    }

    fn system(
        &self,
        _actions: ActionProfile,
        _map: &FrameMap,
        task: &CuaTaskText,
    ) -> Option<String> {
        Some(
            body(SYSTEM)
                .replace("{language}", THOUGHT_LANGUAGE)
                .replace("{instruction}", &instruction(task)),
        )
    }

    fn retry(&self, _actions: ActionProfile) -> String {
        "Reply again as `Thought: ...` and then `Action: ...`, with one action from the action space."
            .to_owned()
    }

    fn parse(
        &self,
        space: ModelSpace,
        _context: ToolContext,
        reply: &TurnTranscript,
        limits: ParseLimits,
    ) -> Result<Parsed, ParseError> {
        parse_text(TextDialect::UiTars15, space, &reply.text, limits)
    }
}
