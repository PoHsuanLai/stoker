//! Anthropic's computer-use tools: the 2026-08-01 toolset (`computer_toolset_20260801`, members
//! `left_click`, `type`, `screenshot`, ... each a tool of its own) and the 2025-11-24 tool
//! (`computer_20251124`, one tool named `computer` with the member in `input.action`).
//!
//! Written from Anthropic's computer-use documentation (2026-10-03). Coordinates are pixels of
//! the screenshot the model was shown (`ImageSpace`). A call is a [`ToolCall`] whose name is the
//! member (the toolset) or `computer` (the 2025 tool) and whose input is the call's JSON; the
//! cloud backend that builds the call from a `tool_use` block keeps its `id`.
//!
//! Members with no action of ours are dropped as unsupported (`left_mouse_down`, `left_mouse_up`,
//! `cursor_position`, `hold_key`); a click, scroll or move with no coordinate is dropped for it,
//! because "at the cursor" is not a place the executor can name.

use std::collections::BTreeSet;

use cua_action::{
    Button, ClickCount, CuaAction, ImageSpace, Modifier, Notches, Rect, Repeat, ScrollBy,
    ScrollDir, Size, Target, TypedText, WaitMs,
};
use cua_parse::{DropReason, bounded, chord_from_words};
use serde_json::Value;

use crate::args::{Args, Verdict, field, number, pair, point, string};

/// The member a call names: the tool's own name for the toolset, `input.action` for the 2025 tool.
pub(crate) fn member(name: &str, args: &Args, legacy: bool) -> Option<String> {
    match (legacy, name) {
        (true, "computer") => args
            .get("action")
            .and_then(Value::as_str)
            .map(str::to_owned),
        (true, _) => None,
        (false, name) => Some(name.to_owned()),
    }
}

/// Whether a member exists in either Anthropic tool (it may still be one we do not act on).
pub(crate) fn is_member(verb: &str) -> bool {
    matches!(
        verb,
        "screenshot"
            | "zoom"
            | "left_click"
            | "right_click"
            | "middle_click"
            | "double_click"
            | "triple_click"
            | "left_click_drag"
            | "mouse_move"
            | "left_mouse_down"
            | "left_mouse_up"
            | "cursor_position"
            | "scroll"
            | "type"
            | "key"
            | "hold_key"
            | "wait"
    )
}

fn at(args: &Args, key: &str) -> Verdict<Target<ImageSpace>> {
    Ok(Target::Point(point(pair(args, key)?, None)?))
}

/// `text` as modifier keys held through the action: `shift`, `ctrl`, `alt`, `super`, `ctrl+shift`.
fn modifiers(args: &Args) -> Verdict<BTreeSet<Modifier>> {
    let Some(text) = args.get("text") else {
        return Ok(BTreeSet::new());
    };
    let text = text.as_str().ok_or(DropReason::BadArgument)?;
    text.split('+')
        .map(|word| match word.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Ok(Modifier::Ctrl),
            "alt" => Ok(Modifier::Alt),
            "shift" => Ok(Modifier::Shift),
            "super" | "meta" | "cmd" => Ok(Modifier::Super),
            _ => Err(DropReason::BadArgument),
        })
        .collect()
}

fn click(args: &Args, button: Button, count: ClickCount) -> Verdict<CuaAction<ImageSpace>> {
    Ok(CuaAction::Click {
        at: at(args, "coordinate")?,
        button,
        count,
        mods: modifiers(args)?,
    })
}

/// Xdotool-style key names: `ctrl+s`, `alt+Tab`, `Page_Down`.
fn chord(text: &str) -> Verdict<cua_action::Chord> {
    let words: Vec<String> = text
        .split('+')
        .map(|w| {
            if w.chars().count() > 1 {
                w.replace('_', "")
            } else {
                w.to_owned()
            }
        })
        .collect();
    chord_from_words(&words)
}

fn direction(text: &str) -> Verdict<ScrollDir> {
    match text {
        "up" => Ok(ScrollDir::Up),
        "down" => Ok(ScrollDir::Down),
        "left" => Ok(ScrollDir::Left),
        "right" => Ok(ScrollDir::Right),
        _ => Err(DropReason::BadArgument),
    }
}

/// Seconds as a wait, at most what a `WaitMs` holds.
fn seconds(args: &Args) -> Verdict<WaitMs> {
    let secs = number(args, "duration")?;
    secs.checked_mul(1000)
        .and_then(|ms| WaitMs::new(ms).ok())
        .ok_or(DropReason::BadNumber)
}

pub(crate) fn action(verb: &str, args: &Args) -> Verdict<CuaAction<ImageSpace>> {
    match verb {
        "screenshot" => Ok(CuaAction::Observe),
        "zoom" => zoom(args),
        "left_click" => click(args, Button::Left, ClickCount::One),
        "right_click" => click(args, Button::Right, ClickCount::One),
        "middle_click" => click(args, Button::Middle, ClickCount::One),
        "double_click" => click(args, Button::Left, ClickCount::Two),
        "triple_click" => click(args, Button::Left, ClickCount::Three),
        "left_click_drag" => {
            if args.contains_key("text") {
                return Err(DropReason::BadArgument);
            }
            Ok(CuaAction::Drag {
                from: at(args, "start_coordinate")?,
                to: at(args, "coordinate")?,
                button: Button::Left,
            })
        }
        "mouse_move" => Ok(CuaAction::MoveTo {
            at: at(args, "coordinate")?,
        }),
        "scroll" => {
            if args.contains_key("text") {
                return Err(DropReason::BadArgument);
            }
            let amount = u16::try_from(number(args, "scroll_amount")?)
                .ok()
                .filter(|n| *n > 0)
                .ok_or(DropReason::BadNumber)?;
            Ok(CuaAction::Scroll {
                at: at(args, "coordinate")?,
                dir: direction(string(args, "scroll_direction")?)?,
                by: ScrollBy::Notches(Notches(amount)),
            })
        }
        "type" => Ok(CuaAction::Type {
            text: bounded(TypedText::new(string(args, "text")?))?,
        }),
        "key" => {
            let times = match args.get("repeat") {
                None => 1,
                Some(v) => crate::args::whole(v)?,
            };
            Ok(CuaAction::Key {
                chord: chord(string(args, "text")?)?,
                repeat: u8::try_from(times)
                    .ok()
                    .and_then(|t| Repeat::new(t).ok())
                    .ok_or(DropReason::BadNumber)?,
            })
        }
        "wait" => Ok(CuaAction::Wait {
            for_ms: seconds(args)?,
        }),
        _ => Err(DropReason::UnsupportedVerb),
    }
}

/// `region: [x0, y0, x1, y1]` as a rectangle.
fn zoom(args: &Args) -> Verdict<CuaAction<ImageSpace>> {
    let corners: Vec<u32> = field(args, "region")?
        .as_array()
        .ok_or(DropReason::BadArgument)?
        .iter()
        .map(crate::args::whole)
        .collect::<Verdict<_>>()?;
    let [x0, y0, x1, y1] = corners[..] else {
        return Err(DropReason::BadArgument);
    };
    let (w, h) = (
        x1.checked_sub(x0).filter(|w| *w > 0),
        y1.checked_sub(y0).filter(|h| *h > 0),
    );
    let (Some(w), Some(h)) = (w, h) else {
        return Err(DropReason::BadNumber);
    };
    Ok(CuaAction::Zoom {
        region: Rect {
            origin: point((x0, y0), None)?,
            size: Size::new(cua_action::Coord(w), cua_action::Coord(h)),
        },
    })
}
