//! Prompt text: the system prompt and tool declaration of a dialect (files under `prompts/`), the
//! words of one observation, and the repair message. Pure string building.

use std::collections::VecDeque;

use cua_action::{CuaDialect, ModelSpace, TextDialect, ToolDialect};
use cua_parse::{DropReason, ParseError};
use cua_vendors::{StepResult, WireCodec, codec};
use model_provider::{JsonText, SchemaText, ToolName, ToolSpec};
use vision_prep::FrameMap;

use crate::history::HistoryTurn;
use crate::{CuaTaskText, MaskedRegions, ObservationIn, StepLines};

const UI_TARS: &str = include_str!("../prompts/ui_tars_15.txt");
const QWEN_SYSTEM: &str = include_str!("../prompts/qwen_computer_use.txt");
const QWEN_TOOL: &str = include_str!("../prompts/qwen_computer_use.tool.txt");
const QWEN_SCHEMA: &str = include_str!("../prompts/computer_use.schema.json");
const HOLO_SYSTEM: &str = include_str!("../prompts/holo_31.txt");
const HOLO_TOOL: &str = include_str!("../prompts/holo_31.tool.txt");
const HOLO_SCHEMA: &str = include_str!("../prompts/holo_31.schema.json");

/// The function name both tool dialects declare.
const FUNCTION: &str = "computer_use";
/// The language of `Thought:` for the text dialect.
const THOUGHT_LANGUAGE: &str = "English";
/// How much of a refusal's reason is shown to the model.
const WHY_CHARS: usize = 120;

/// A prompt file without its header: the leading lines that start with `#`.
fn body(file: &str) -> &str {
    let mut rest = file;
    while rest.starts_with('#') {
        rest = rest.split_once('\n').map_or("", |(_, tail)| tail);
    }
    rest.trim_end()
}

/// `1000x1000` for a grid, the image's size for pixels.
fn resolution(map: &FrameMap) -> String {
    match map.space {
        ModelSpace::Grid(max) => format!("{0}x{0}", max.0),
        ModelSpace::Image => format!("{}x{}", map.image.w.0, map.image.h.0),
    }
}

fn points(map: &FrameMap) -> String {
    match map.space {
        ModelSpace::Grid(max) => format!(
            "A point is [x, y] on a grid of 0 to {} over the screenshot, x across and y down.",
            max.0
        ),
        ModelSpace::Image => format!(
            "A point is [x, y] in pixels of the screenshot, which is {} pixels.",
            resolution(map)
        ),
    }
}

fn instruction(task: &CuaTaskText) -> String {
    let hints = task.hints.iter().map(|hint| format!("\nHint: {hint}"));
    std::iter::once(task.goal.clone()).chain(hints).collect()
}

/// The system message, or `None` for a vendor wire (its tool declaration carries the prompt).
pub(crate) fn system_text(
    dialect: CuaDialect,
    map: &FrameMap,
    task: &CuaTaskText,
) -> Option<String> {
    match dialect {
        CuaDialect::Text(TextDialect::UiTars15) => Some(
            body(UI_TARS)
                .replace("{language}", THOUGHT_LANGUAGE)
                .replace("{instruction}", &instruction(task)),
        ),
        CuaDialect::Tool(ToolDialect::QwenComputerUse) => {
            Some(body(QWEN_SYSTEM).replace("{points}", &points(map)))
        }
        CuaDialect::Tool(ToolDialect::Holo31) => {
            Some(body(HOLO_SYSTEM).replace("{points}", &points(map)))
        }
        CuaDialect::Wire(_) => None,
    }
}

/// Whether the goal is already in the system message (the text dialect's prompt ends with it).
pub(crate) fn goal_in_system(dialect: CuaDialect) -> bool {
    matches!(dialect, CuaDialect::Text(_))
}

fn function(description: &str, schema: &str, map: &FrameMap) -> Vec<ToolSpec> {
    let parameters = JsonText::new(schema.trim()).map(SchemaText);
    let name = ToolName::new(FUNCTION);
    match (name, parameters) {
        (Ok(name), Ok(parameters)) => vec![ToolSpec::Function {
            name,
            description: body(description).replace("{resolution}", &resolution(map)),
            parameters,
        }],
        _ => unreachable!("the shipped prompt files are valid"),
    }
}

/// The tools a request declares.
pub(crate) fn tools(dialect: CuaDialect, map: &FrameMap) -> Vec<ToolSpec> {
    match dialect {
        CuaDialect::Text(_) => Vec::new(),
        CuaDialect::Tool(ToolDialect::QwenComputerUse) => function(QWEN_TOOL, QWEN_SCHEMA, map),
        CuaDialect::Tool(ToolDialect::Holo31) => function(HOLO_TOOL, HOLO_SCHEMA, map),
        CuaDialect::Wire(wire) => codec(wire).tools(map.image),
    }
}

fn result_text(result: &StepResult) -> String {
    match result {
        StepResult::Done(_) => "done".to_owned(),
        StepResult::Refused { why, .. } => {
            let why: String = why
                .chars()
                .filter(|c| !c.is_control())
                .take(WHY_CHARS)
                .collect();
            format!("refused ({why})")
        }
        StepResult::NotRun(_) => "not run".to_owned(),
    }
}

/// Where the cursor is, in the model's own coordinates.
fn cursor_text(map: &FrameMap, at: cua_action::Point<cua_action::WindowSpace>) -> String {
    let (x, y) = match map.space {
        ModelSpace::Image => {
            let p = map.window_to_image(at);
            (p.x.0, p.y.0)
        }
        ModelSpace::Grid(max) => {
            let axis = |v: u32, window: u32| {
                let scaled = u64::from(v) * u64::from(max.0) / u64::from(window.max(1));
                u32::try_from(scaled)
                    .unwrap_or(u32::MAX)
                    .min(u32::from(max.0))
            };
            (axis(at.x.0, map.window.w.0), axis(at.y.0, map.window.h.0))
        }
    };
    format!("The cursor is at [{x}, {y}].")
}

fn earlier_line(turn: &HistoryTurn) -> String {
    let said = if turn.actions.is_empty() {
        "no valid action".to_owned()
    } else {
        turn.actions.join("; ")
    };
    format!("- step {}: {said}", turn.number)
}

/// The lines before the screenshot: goal and hints (unless the system message holds them), the
/// earlier steps that have no frame of their own, what happened to the last actions, the masked
/// regions and the cursor.
pub(crate) fn observation_lines(
    dialect: CuaDialect,
    task: &CuaTaskText,
    history: &VecDeque<HistoryTurn>,
    lines: StepLines,
    obs: &ObservationIn,
    map: &FrameMap,
) -> Vec<String> {
    let mut out = Vec::new();
    if !goal_in_system(dialect) {
        out.push(format!("Goal: {}", task.goal));
        out.extend(task.hints.iter().map(|hint| format!("Hint: {hint}")));
    }
    // The text dialect replays steps that have a frame as turns; the others list them all.
    let shown: Vec<&HistoryTurn> = history
        .iter()
        .skip(history.len().saturating_sub(usize::from(lines.0)))
        .filter(|turn| !(goal_in_system(dialect) && turn.frame.is_some()))
        .collect();
    if !shown.is_empty() {
        out.push("Earlier actions:".to_owned());
        out.extend(shown.into_iter().map(earlier_line));
    }
    if !obs.prev.is_empty() {
        let results: Vec<String> = obs.prev.iter().map(result_text).collect();
        out.push(format!(
            "What happened to your last actions: {}.",
            results.join(", ")
        ));
    }
    if let MaskedRegions(n @ 1..) = obs.masked {
        out.push(format!("{n} region(s) of the window are hidden from you."));
    }
    if let Some(at) = obs.cursor {
        out.push(cursor_text(map, at));
    }
    out
}

/// Why a reply is sent back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Fault {
    Unparsed(ParseError),
    /// Every action in the reply was refused, for these reasons.
    Refused(Vec<DropReason>),
}

fn reason_words(reason: DropReason) -> &'static str {
    match reason {
        DropReason::UnsupportedVerb => "an action that does not exist",
        DropReason::MissingArgument => "a missing argument",
        DropReason::BadArgument => "an argument that cannot be used",
        DropReason::BadNumber => "a number that is not a whole number",
        DropReason::TooLong => "text that is too long",
        DropReason::OverBatchLimit => "more actions than are allowed at once",
        DropReason::OutOfFrame => "a point outside the screenshot",
    }
}

/// The user message of a repair. It names what was wrong and never repeats the reply.
pub(crate) fn repair_text(dialect: CuaDialect, fault: &Fault) -> String {
    let what = match fault {
        Fault::Unparsed(ParseError::TooLarge) => "Your last reply was too long.".to_owned(),
        Fault::Unparsed(_) => "Your last reply did not hold an action I can run.".to_owned(),
        Fault::Refused(reasons) => {
            let mut words: Vec<&str> = reasons.iter().map(|r| reason_words(*r)).collect();
            words.dedup();
            format!(
                "Every action in your last reply was refused: it had {}.",
                words.join(", ")
            )
        }
    };
    let how = match dialect {
        CuaDialect::Text(_) => {
            "Reply again as `Thought: ...` and then `Action: ...`, with one action from the action space."
        }
        CuaDialect::Tool(_) => "Answer again with one call to the computer_use function.",
        CuaDialect::Wire(_) => "Answer again with one call to the computer tool.",
    };
    format!("{what} {how}")
}
