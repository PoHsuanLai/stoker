//! The UI-TARS-1.5 text dialect:
//!
//! ```text
//! Thought: the Save button is at the bottom right.
//! Action: click(start_box='(812,640)')
//! ```
//!
//! Verbs (the model card's computer-use prompt): `click`, `left_double`, `right_single`,
//! `drag`, `hotkey`, `type`, `scroll`, `wait`, `finished`, `call_user`. A box is `(x,y)` or
//! `(x1,y1,x2,y2)` (its centre), with or without the `<|box_start|>` and `<point>` tags.
//! Points are in the pixels of the resized image the model was shown.

use cua_action::{
    Button, ClickCount, CoordSpace, CuaAction, FinishOutcome, Notches, ScrollBy, ScrollDir,
    Summary, Target, TypedText, WaitMs,
};

use crate::common::{Batch, Collected, Ctx, Dialect, bounded, click};
use crate::scan::{self, Call};
use crate::{DropReason, ParseError, ParseLimits, chord};

const ACTION: &str = "Action:";
const THOUGHT: &str = "Thought:";
/// The model card: `wait()` waits five seconds.
const WAIT_MS: u32 = 5_000;
/// The model card gives `scroll` no amount; this many notches is a screenful on most apps.
const SCROLL_NOTCHES: u16 = 5;

pub(crate) struct UiTars<'a>(pub &'a str);

impl Dialect for UiTars<'_> {
    fn collect<S: CoordSpace>(
        &self,
        ctx: &Ctx,
        limits: ParseLimits,
    ) -> Result<Collected<S>, ParseError> {
        let text = self.0;
        let at = action_marker(text).ok_or(ParseError::NoAction)?;
        let thought = text[..at]
            .trim()
            .strip_prefix(THOUGHT)
            .unwrap_or(text[..at].trim())
            .trim();
        let thought = (!thought.is_empty()).then(|| thought.to_owned());
        let body = at + ACTION.len();
        let mut batch = Batch::new(limits);
        for call in scan::calls(&text[body..], body)? {
            batch.push(call.verb, action(ctx, &call));
        }
        batch.finish(thought)
    }
}

/// The last `Action:` that starts its line: a thought may mention the word mid-line.
fn action_marker(text: &str) -> Option<usize> {
    text.match_indices(ACTION)
        .map(|(at, _)| at)
        .filter(|&at| {
            let line_start = text[..at].rfind('\n').map_or(0, |n| n + 1);
            text[line_start..at].trim().is_empty()
        })
        .last()
}

fn action<S: CoordSpace>(ctx: &Ctx, call: &Call<'_>) -> Result<CuaAction<S>, DropReason> {
    let point = |name: &[&str]| -> Result<_, DropReason> {
        let value = name
            .iter()
            .find_map(|n| call.arg(n))
            .ok_or(DropReason::MissingArgument)?;
        let (x, y) = centre(value)?;
        ctx.point::<S>(x, y)
    };
    let start = || point(&["start_box", "point", "start_point"]);
    match call.verb {
        "click" | "left_single" => Ok(click(start()?, Button::Left, ClickCount::One)),
        "left_double" | "double_click" => Ok(click(start()?, Button::Left, ClickCount::Two)),
        "right_single" | "right_click" => Ok(click(start()?, Button::Right, ClickCount::One)),
        "drag" => Ok(CuaAction::Drag {
            from: Target::Point(start()?),
            to: Target::Point(point(&["end_box", "end_point"])?),
            button: Button::Left,
        }),
        "hotkey" => {
            let keys = call
                .arg("key")
                .or_else(|| call.arg("keys"))
                .ok_or(DropReason::MissingArgument)?;
            Ok(CuaAction::Key {
                chord: chord::from_text(keys)?,
                repeat: cua_action::Repeat::ONCE,
            })
        }
        "type" => {
            let text = call.arg("content").ok_or(DropReason::MissingArgument)?;
            Ok(CuaAction::Type {
                text: bounded(TypedText::new(text))?,
            })
        }
        "scroll" => Ok(CuaAction::Scroll {
            at: Target::Point(start()?),
            dir: direction(call.arg("direction"))?,
            by: ScrollBy::Notches(Notches(SCROLL_NOTCHES)),
        }),
        "wait" => Ok(CuaAction::Wait {
            for_ms: WaitMs::new(WAIT_MS).map_err(|_| DropReason::BadNumber)?,
        }),
        "finished" => Ok(CuaAction::Finish {
            outcome: FinishOutcome::Done,
            summary: bounded(Summary::new(call.arg("content").unwrap_or_default()))?,
            extracted: Vec::new(),
        }),
        "call_user" => Ok(CuaAction::Ask {
            question: bounded(Summary::new("The model asks for the person to take over."))?,
            choices: Vec::new(),
        }),
        _ => Err(DropReason::UnsupportedVerb),
    }
}

fn direction(value: Option<&str>) -> Result<ScrollDir, DropReason> {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("up") => Ok(ScrollDir::Up),
        Some("down") => Ok(ScrollDir::Down),
        Some("left") => Ok(ScrollDir::Left),
        Some("right") => Ok(ScrollDir::Right),
        _ => Err(DropReason::MissingArgument),
    }
}

/// The point a box names: `(x,y)`, or the centre of `(x1,y1,x2,y2)`. Tags such as
/// `<|box_start|>` and `<point>` are skipped; a number that is not a whole `u32` is `BadNumber`.
fn centre(value: &str) -> Result<(u32, u32), DropReason> {
    let numbers = numbers(value)?;
    let mid = |a: u32, b: u32| u32::try_from((u64::from(a) + u64::from(b)) / 2).unwrap_or(u32::MAX);
    match numbers.as_slice() {
        [x, y] => Ok((*x, *y)),
        [x1, y1, x2, y2] => Ok((mid(*x1, *x2), mid(*y1, *y2))),
        [] => Err(DropReason::MissingArgument),
        _ => Err(DropReason::BadNumber),
    }
}

fn numbers(value: &str) -> Result<Vec<u32>, DropReason> {
    let mut untagged = String::with_capacity(value.len());
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => untagged.push(c),
            _ => {}
        }
    }
    untagged
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '[' | ']'))
        .filter(|word| !word.is_empty())
        .map(|word| word.parse::<u32>().map_err(|_| DropReason::BadNumber))
        .collect()
}
