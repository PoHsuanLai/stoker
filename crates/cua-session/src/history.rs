//! What the session remembers of earlier steps: one turn per reply, the last few with their frame.

use std::collections::VecDeque;

use cua_action::{CoordSpace, CuaAction, Target};
use cua_parse::InSpace;
use model_provider::{ImageInput, Part, ToolCall, ToolResult};

use crate::{FrameBudget, StepLines};

/// The longest reply text kept for the text dialect's native alternation.
const KEPT_REPLY_CHARS: usize = 2048;

/// One past step the prompt may replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryTurn {
    /// The number of the step within this session, from 0.
    pub(crate) number: u32,
    /// What the model did, one phrase per action, in the model's own coordinates.
    pub(crate) actions: Vec<String>,
    /// The reply text, cut (the text dialect replays it as the assistant's turn).
    pub(crate) reply: String,
    /// The frame the model was shown, while it is within the frame budget.
    pub(crate) frame: Option<ImageInput>,
    /// For a vendor wire, whose history is real messages: the user message this step was asked
    /// in (the results of the step before, the words, the frame), and the calls the model made.
    pub(crate) user: Vec<Part>,
    pub(crate) calls: Vec<ToolCall>,
}

/// `parts` without any image, in a tool result too.
pub(crate) fn without_images(parts: &[Part]) -> Vec<Part> {
    parts
        .iter()
        .filter_map(|part| match part {
            Part::Image(_) => None,
            Part::ToolResult(result) => Some(Part::ToolResult(ToolResult {
                parts: without_images(&result.parts),
                ..result.clone()
            })),
            other => Some(other.clone()),
        })
        .collect()
}

/// The turns the prompt keeps: at most `lines` of them, the newest last, and only the last
/// `frames` of those with their frame.
pub(crate) fn push(
    history: &mut VecDeque<HistoryTurn>,
    turn: HistoryTurn,
    frames: FrameBudget,
    lines: StepLines,
) {
    history.push_back(turn);
    while history.len() > usize::from(lines.0.max(frames.0)) {
        history.pop_front();
    }
    let keep_from = history.len().saturating_sub(usize::from(frames.0));
    history.iter_mut().take(keep_from).for_each(|turn| {
        turn.frame = None;
        turn.user = without_images(&turn.user);
    });
}

pub(crate) fn cut(reply: &str) -> String {
    reply.chars().take(KEPT_REPLY_CHARS).collect()
}

/// A phrase per action. Typed text and key names are not repeated: what the person typed is
/// personal, and the model does not need it to know it typed.
pub(crate) fn phrases(actions: &InSpace) -> Vec<String> {
    match actions {
        InSpace::Image(list) => list.iter().map(phrase).collect(),
        InSpace::Grid(_, list) => list.iter().map(phrase).collect(),
    }
}

fn phrase<S: CoordSpace>(action: &CuaAction<S>) -> String {
    let at = |target: &Target<S>| match target {
        Target::Point(p) => format!("({}, {})", p.x.0, p.y.0),
        Target::Node(_) => "an element".to_owned(),
        Target::Centre => "the centre".to_owned(),
    };
    match action {
        CuaAction::Click {
            at: target,
            button,
            count,
            ..
        } => format!(
            "click {} x{} at {}",
            format!("{button:?}").to_lowercase(),
            count_of(*count),
            at(target)
        ),
        CuaAction::MoveTo { at: target } => format!("move the pointer to {}", at(target)),
        CuaAction::Drag { from, to, .. } => format!("drag from {} to {}", at(from), at(to)),
        CuaAction::Type { text } => format!("typed {} characters", text.as_str().chars().count()),
        CuaAction::Key { .. } => "pressed a key combination".to_owned(),
        CuaAction::Scroll {
            at: target, dir, ..
        } => {
            format!(
                "scroll {} at {}",
                format!("{dir:?}").to_lowercase(),
                at(target)
            )
        }
        CuaAction::Wait { for_ms } => format!("wait {} ms", for_ms.ms()),
        CuaAction::Zoom { .. } => "zoom".to_owned(),
        CuaAction::Observe => "look at the screen again".to_owned(),
        CuaAction::Finish { .. } => "finish".to_owned(),
        CuaAction::Ask { .. } => "ask the person".to_owned(),
        _ => "act".to_owned(),
    }
}

fn count_of(count: cua_action::ClickCount) -> u8 {
    match count {
        cua_action::ClickCount::One => 1,
        cua_action::ClickCount::Two => 2,
        cua_action::ClickCount::Three => 3,
    }
}
