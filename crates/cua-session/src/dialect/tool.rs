//! The tool-call dialects: Qwen's `computer_use` function and Holo 3.1's flat verbs. They share
//! the way calls are read and the retry text; they differ in prompt files and schema.

use std::borrow::Cow;

use cua_action::{ActionProfile, CallFormat, ModelSpace, ResolutionWording, ToolDialect};
use cua_parse::{
    ParamKinds, ParseError, ParseLimits, Parsed, ToolContext, parse_tool_calls_with, read_calls,
};
use model_provider::{JsonText, SchemaText, ToolCall, ToolName, ToolSpec};
use vision_prep::FrameMap;

use super::ActionDialect;
use crate::prompt::{body, points, resolution, shown_name};
use crate::{CuaTaskText, TurnTranscript, answer, calls, shown};

const QWEN_SYSTEM: &str = include_str!("../../prompts/qwen_computer_use.txt");
const QWEN_TOOL: &str = include_str!("../../prompts/qwen_computer_use.tool.txt");
const QWEN_SCHEMA: &str = include_str!("../../prompts/computer_use.schema.json");
const HOLO_SYSTEM: &str = include_str!("../../prompts/holo_31.txt");
const HOLO_TOOL: &str = include_str!("../../prompts/holo_31.tool.txt");
const HOLO_SCHEMA: &str = include_str!("../../prompts/holo_31.schema.json");

/// The function name both tool dialects declare.
const FUNCTION: &str = "computer_use";

pub(super) struct Qwen;

impl ActionDialect for Qwen {
    fn is_tool(&self) -> bool {
        true
    }

    fn system(
        &self,
        actions: ActionProfile,
        map: &FrameMap,
        _task: &CuaTaskText,
    ) -> Option<String> {
        Some(
            body(QWEN_SYSTEM)
                .replace("{points}", &points(map))
                .replace("{terminate}", shown_name(actions, "terminate")),
        )
    }

    fn schema(&self, actions: ActionProfile) -> Option<Cow<'static, str>> {
        Some(shown::schema(QWEN_SCHEMA, actions))
    }

    fn tools(&self, actions: ActionProfile, map: &FrameMap) -> Vec<ToolSpec> {
        function(
            QWEN_TOOL,
            &shown::schema(QWEN_SCHEMA, actions),
            map,
            actions.resolution,
        )
    }

    fn retry(&self, actions: ActionProfile) -> String {
        retry(actions)
    }

    fn parse(
        &self,
        space: ModelSpace,
        context: ToolContext,
        reply: &TurnTranscript,
        limits: ParseLimits,
    ) -> Result<Parsed, ParseError> {
        parse(
            self,
            ToolDialect::QwenComputerUse,
            space,
            context,
            reply,
            limits,
        )
    }
}

pub(super) struct Holo;

impl ActionDialect for Holo {
    fn is_tool(&self) -> bool {
        true
    }

    fn system(
        &self,
        _actions: ActionProfile,
        map: &FrameMap,
        _task: &CuaTaskText,
    ) -> Option<String> {
        Some(body(HOLO_SYSTEM).replace("{points}", &points(map)))
    }

    fn schema(&self, _actions: ActionProfile) -> Option<Cow<'static, str>> {
        Some(Cow::Borrowed(HOLO_SCHEMA))
    }

    fn tools(&self, actions: ActionProfile, map: &FrameMap) -> Vec<ToolSpec> {
        function(HOLO_TOOL, HOLO_SCHEMA, map, actions.resolution)
    }

    fn retry(&self, actions: ActionProfile) -> String {
        retry(actions)
    }

    fn parse(
        &self,
        space: ModelSpace,
        context: ToolContext,
        reply: &TurnTranscript,
        limits: ParseLimits,
    ) -> Result<Parsed, ParseError> {
        parse(self, ToolDialect::Holo31, space, context, reply, limits)
    }
}

fn function(
    description: &str,
    schema: &str,
    map: &FrameMap,
    wording: ResolutionWording,
) -> Vec<ToolSpec> {
    let parameters = JsonText::new(schema.trim()).map(SchemaText);
    let name = ToolName::new(FUNCTION);
    match (name, parameters) {
        (Ok(name), Ok(parameters)) => vec![ToolSpec::Function {
            name,
            description: body(description)
                .replace("{resolution}", &resolution(map, wording))
                .replace("{scroll}", cua_parse::SCROLL_PIXELS_RULE),
            parameters,
        }],
        _ => unreachable!("the shipped prompt files are valid"),
    }
}

fn retry(actions: ActionProfile) -> String {
    // A model whose calls we read from its text is told how a call is written.
    let said = "Answer again with one call to the computer_use function.";
    [
        Some(said),
        calls::reminder(actions.calls),
        answer::reminder(actions.answer),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<&str>>()
    .join(" ")
}

fn parse(
    dialect: &dyn ActionDialect,
    tool: ToolDialect,
    space: ModelSpace,
    context: ToolContext,
    reply: &TurnTranscript,
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    let calls = calls_of(dialect, &context, reply, limits)?;
    parse_tool_calls_with(tool, context, space, &calls, limits)
}

/// The calls of a reply: the engine's, or, for a model whose profile says we read them, those
/// written in its text (calls the engine did parse anyway are used as they are).
fn calls_of<'a>(
    dialect: &dyn ActionDialect,
    context: &ToolContext,
    reply: &'a TurnTranscript,
    limits: ParseLimits,
) -> Result<Cow<'a, [ToolCall]>, ParseError> {
    match context.profile.calls {
        CallFormat::Engine => Ok(Cow::Borrowed(&reply.calls)),
        _ if !reply.calls.is_empty() => Ok(Cow::Borrowed(&reply.calls)),
        format => {
            let schema = dialect.schema(context.profile).unwrap_or_default();
            let kinds = ParamKinds::from_schema(&schema);
            read_calls(format, &reply.text, &reply.thought, &kinds, limits).map(Cow::Owned)
        }
    }
}
