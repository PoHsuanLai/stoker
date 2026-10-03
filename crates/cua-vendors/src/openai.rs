//! OpenAI's computer tool (`{"type": "computer"}`): a `computer_call` item holds an ordered
//! `actions` array, each action an object with a `type`.
//!
//! Written from OpenAI's computer-use guide (2026-10-03). A call is a [`ToolCall`] named
//! `computer` (or `computer_call`) whose id is the `call_id` and whose input is
//! `{"actions": [...]}` (the older single `{"action": {...}}` reads too). Coordinates are pixels
//! of the screenshot (`ImageSpace`). The result of the whole call is one `computer_call_output`.
//!
//! A `wait` waits [`WAIT_MS`] (the action names no duration). A `scroll` with both axes acts
//! along the longer one. A `drag` goes from the first point of its path to the last.
//! `click` buttons `back` and `forward` are dropped; `wheel` is the middle button.

use cua_action::{
    Button, ClickCount, CuaAction, ImageSpace, Length, ScrollBy, ScrollDir, Target, TypedText,
    WaitMs,
};
use cua_parse::{DropReason, bounded, chord_from_words};
use serde_json::Value;

use crate::args::{Args, Verdict, field, point, signed, string, xy};

/// How long a `wait` action waits.
pub(crate) const WAIT_MS: u32 = 2_000;

/// The actions of one call, each with the verb it is reported under.
pub(crate) fn actions_of(args: &Args) -> Vec<(String, Verdict<CuaAction<ImageSpace>>)> {
    let list: Vec<&Value> = match (args.get("actions"), args.get("action")) {
        (Some(Value::Array(items)), _) => items.iter().collect(),
        (None, Some(single)) => vec![single],
        _ => Vec::new(),
    };
    list.into_iter()
        .map(|item| match item.as_object() {
            Some(map) => {
                let verb = map.get("type").and_then(Value::as_str).unwrap_or("");
                (verb.to_owned(), action(verb, map))
            }
            None => (String::new(), Err(DropReason::BadArgument)),
        })
        .collect()
}

fn at(args: &Args) -> Verdict<Target<ImageSpace>> {
    Ok(Target::Point(point(xy(args, "x", "y")?, None)?))
}

fn button(args: &Args) -> Verdict<Button> {
    match args.get("button").and_then(Value::as_str).unwrap_or("left") {
        "left" => Ok(Button::Left),
        "right" => Ok(Button::Right),
        "wheel" | "middle" => Ok(Button::Middle),
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

fn scroll(args: &Args) -> Verdict<CuaAction<ImageSpace>> {
    let across = args.get("scroll_x").map(signed).transpose()?.unwrap_or(0);
    let down = args.get("scroll_y").map(signed).transpose()?.unwrap_or(0);
    let (dir, pixels) = if down.abs() >= across.abs() {
        (
            if down >= 0 {
                ScrollDir::Down
            } else {
                ScrollDir::Up
            },
            down.unsigned_abs(),
        )
    } else {
        (
            if across >= 0 {
                ScrollDir::Right
            } else {
                ScrollDir::Left
            },
            across.unsigned_abs(),
        )
    };
    let pixels = u32::try_from(pixels)
        .ok()
        .filter(|p| *p > 0)
        .ok_or(DropReason::BadNumber)?;
    Ok(CuaAction::Scroll {
        at: at(args)?,
        dir,
        by: ScrollBy::Distance(Length::new(cua_action::Coord(pixels))),
    })
}

fn drag(args: &Args) -> Verdict<CuaAction<ImageSpace>> {
    let path = field(args, "path")?
        .as_array()
        .ok_or(DropReason::BadArgument)?;
    let end = |p: &Value| -> Verdict<Target<ImageSpace>> {
        let map = p.as_object().ok_or(DropReason::BadArgument)?;
        at(map)
    };
    match (path.first(), path.last()) {
        (Some(from), Some(to)) if path.len() >= 2 => Ok(CuaAction::Drag {
            from: end(from)?,
            to: end(to)?,
            button: Button::Left,
        }),
        _ => Err(DropReason::MissingArgument),
    }
}

fn action(verb: &str, args: &Args) -> Verdict<CuaAction<ImageSpace>> {
    let click = |count| -> Verdict<CuaAction<ImageSpace>> {
        Ok(CuaAction::Click {
            at: at(args)?,
            button: if matches!(count, ClickCount::One) {
                button(args)?
            } else {
                Button::Left
            },
            count,
            mods: Default::default(),
        })
    };
    match verb {
        "click" => click(ClickCount::One),
        "double_click" => click(ClickCount::Two),
        "move" => Ok(CuaAction::MoveTo { at: at(args)? }),
        "scroll" => scroll(args),
        "drag" => drag(args),
        "keypress" => Ok(CuaAction::Key {
            chord: keys(args)?,
            repeat: cua_action::Repeat::ONCE,
        }),
        "type" => Ok(CuaAction::Type {
            text: bounded(TypedText::new(string(args, "text")?))?,
        }),
        "wait" => Ok(CuaAction::Wait {
            for_ms: WaitMs::new(WAIT_MS).map_err(|_| DropReason::BadNumber)?,
        }),
        "screenshot" => Ok(CuaAction::Observe),
        _ => Err(DropReason::UnsupportedVerb),
    }
}
