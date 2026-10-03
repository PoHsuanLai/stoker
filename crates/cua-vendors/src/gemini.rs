//! Gemini's computer-use tool (`{"type": "computer_use", "environment": "desktop"}`): function
//! calls named for the action, with coordinates normalised to 0..=999 over the screenshot
//! (`GridSpace` with a maximum of [`GRID_MAX`]).
//!
//! Written from Google's computer-use documentation (2026-10-03). A call is a [`ToolCall`] named
//! for the action, whose input is the call's `args`; the backend that builds it mints the id (a
//! function call has none) and turns an `args.safety_decision` into a `SafetySignal`.
//!
//! Browser and phone functions (`navigate`, `go_back`, `go_forward`, `open_app`, `long_press`)
//! have no action of ours and are dropped. `scroll` gives pixels where our action counts wheel
//! notches: [`NOTCH_PIXELS`] to a notch, at least one. `type` with `press_enter` types the text
//! and a newline.

use cua_action::{
    Button, ClickCount, CuaAction, GridMax, GridSpace, Notches, ScrollBy, ScrollDir, Target,
    TypedText,
};
use cua_parse::{DropReason, bounded, chord_from_words};
use serde_json::Value;

use crate::args::{Args, Verdict, field, number, point, string, xy};

/// The top of Gemini's grid: its points run 0 to 999.
pub(crate) const GRID_MAX: GridMax = GridMax(1000);
/// Pixels of a scroll that make one wheel notch.
pub(crate) const NOTCH_PIXELS: u32 = 100;

fn at(args: &Args, x: &str, y: &str) -> Verdict<Target<GridSpace>> {
    Ok(Target::Point(point(
        xy(args, x, y)?,
        Some(u32::from(GRID_MAX.0)),
    )?))
}

fn click(args: &Args, button: Button, count: ClickCount) -> Verdict<CuaAction<GridSpace>> {
    Ok(CuaAction::Click {
        at: at(args, "x", "y")?,
        button,
        count,
        mods: Default::default(),
    })
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

fn keys(args: &Args) -> Verdict<cua_action::Chord> {
    let words: Vec<&str> = field(args, "keys")?
        .as_array()
        .ok_or(DropReason::BadArgument)?
        .iter()
        .map(|k| k.as_str().ok_or(DropReason::BadArgument))
        .collect::<Verdict<_>>()?;
    chord_from_words(&words)
}

/// Every function the tool declares, whether we act on it or not.
pub(crate) fn is_function(name: &str) -> bool {
    matches!(
        name,
        "click"
            | "double_click"
            | "right_click"
            | "type"
            | "navigate"
            | "go_back"
            | "go_forward"
            | "scroll"
            | "drag_and_drop"
            | "press_key"
            | "hotkey"
            | "open_app"
            | "long_press"
    )
}

pub(crate) fn action(verb: &str, args: &Args) -> Verdict<CuaAction<GridSpace>> {
    match verb {
        "click" => click(args, Button::Left, ClickCount::One),
        "double_click" => click(args, Button::Left, ClickCount::Two),
        "right_click" => click(args, Button::Right, ClickCount::One),
        "type" => {
            let enter = args.get("press_enter") == Some(&Value::Bool(true));
            let text = format!("{}{}", string(args, "text")?, if enter { "\n" } else { "" });
            Ok(CuaAction::Type {
                text: bounded(TypedText::new(text))?,
            })
        }
        "scroll" => {
            let pixels = number(args, "magnitude_in_pixels")?;
            let notches = u16::try_from(pixels.div_ceil(NOTCH_PIXELS).max(1)).unwrap_or(u16::MAX);
            Ok(CuaAction::Scroll {
                at: at(args, "x", "y")?,
                dir: direction(string(args, "direction")?)?,
                by: ScrollBy::Notches(Notches(notches)),
            })
        }
        "drag_and_drop" => Ok(CuaAction::Drag {
            from: at(args, "start_x", "start_y")?,
            to: at(args, "end_x", "end_y")?,
            button: Button::Left,
        }),
        "press_key" => Ok(CuaAction::Key {
            chord: chord_from_words(&[string(args, "key")?])?,
            repeat: cua_action::Repeat::ONCE,
        }),
        "hotkey" => Ok(CuaAction::Key {
            chord: keys(args)?,
            repeat: cua_action::Repeat::ONCE,
        }),
        _ => Err(DropReason::UnsupportedVerb),
    }
}
