//! Tool-call dialects: the engine has already pulled the calls out of the reply, and each call
//! arrives as a name and a JSON object of arguments.
//!
//! `QwenComputerUse` is Qwen's `computer_use` function: one function, the verb in its `action`
//! argument (`left_click`, `type`, `key`, `scroll`, `terminate`, ...), a point as
//! `coordinate: [x, y]`.
//!
//! A model's `ActionProfile` (from its catalogue entry) is read first, through `profile`: verbs
//! it writes under other names, a drag from the cursor, a scroll whose axis is an argument. A
//! verb the profile does not claim is read as above, so an entry without a profile parses as it
//! always did.
//!
//! `Holo31` reads the same schema and adds a few verbs the session's prompt declares: the verb
//! may also be the function's own name, a point may be `x` and `y`, and `click`, `drag`, `move`,
//! `finish`, `ask` and `observe` exist. The chat template of Holo 3.1 names no computer-use
//! function, so this schema is ours, provisional until a recorded step (`dev/record-engine.sh`)
//! shows what the model writes; the fixtures under `fixtures/` pin what is read today.

use cua_action::{
    Button, Choice, ClickCount, CoordSpace, CuaAction, FinishOutcome, Notches, ScrollBy, ScrollDir,
    Summary, Target, TypedText, WaitMs,
};
use model_provider::ToolCall;
use serde_json::{Map, Value};

use crate::common::{Batch, Collected, Ctx, Dialect, bounded, click, scroll_length};
use crate::profile::{self, ToolContext};
use crate::verbs::Verbs;
use crate::{DropReason, ParseError, ParseLimits, chord};

/// Qwen's function name.
const QWEN_FUNCTION: &str = "computer_use";
/// What a model is told about the sign of a scroll's `pixels`: the words the tool description
/// carries, so that what it is told and what [`scroll`] reads cannot drift apart (a test pins both).
pub const SCROLL_PIXELS_RULE: &str = "A scroll's `pixels` is positive to scroll up and negative to scroll down; for a horizontal scroll, positive is right and negative is left.";
/// Holo's `scroll` with a direction and no pixel amount.
const SCROLL_NOTCHES: u16 = 5;
const MS_PER_SECOND: f64 = 1000.0;

pub(crate) struct Tools<'a> {
    pub verbs: &'static Verbs,
    pub context: ToolContext,
    pub calls: &'a [ToolCall],
}

pub(crate) type Args = Map<String, Value>;
pub(crate) type Verdict<T> = Result<T, DropReason>;

impl Dialect for Tools<'_> {
    fn collect<S: CoordSpace>(
        &self,
        ctx: &Ctx,
        limits: ParseLimits,
    ) -> Result<Collected<S>, ParseError> {
        let mut batch = Batch::new(limits);
        let mut context = self.context;
        for call in self.calls {
            let (verb, result) = self.one(&context, ctx, call);
            if let Ok(action) = &result {
                context.cursor = profile::cursor_after(context.cursor, action);
            }
            batch.push(&verb, result);
        }
        batch.finish(None)
    }
}

impl Tools<'_> {
    /// The verb of one call, and its action or the reason it is not one. `context` carries the
    /// cursor as the reply's earlier calls left it.
    fn one<S: CoordSpace>(
        &self,
        context: &ToolContext,
        ctx: &Ctx,
        call: &ToolCall,
    ) -> (String, Verdict<CuaAction<S>>) {
        let name = call.name.as_str();
        let Ok(Value::Object(args)) = serde_json::from_str::<Value>(call.input.as_str()) else {
            return (name.to_owned(), Err(DropReason::MissingArgument));
        };
        let wrapped = name == QWEN_FUNCTION;
        if !wrapped && !self.verbs.function_is_verb {
            return (name.to_owned(), Err(DropReason::UnsupportedVerb));
        }
        let verb = if wrapped {
            match args.get("action").and_then(Value::as_str) {
                Some(action) => action.to_owned(),
                None => return (name.to_owned(), Err(DropReason::MissingArgument)),
            }
        } else {
            name.to_owned()
        };
        let result = profile::action(context, self.verbs, ctx, &verb, &args)
            .unwrap_or_else(|| verb_action(self.verbs, ctx, &verb, &args));
        (verb, result)
    }
}

pub(crate) fn verb_action<S: CoordSpace>(
    verbs: &Verbs,
    ctx: &Ctx,
    verb: &str,
    args: &Args,
) -> Verdict<CuaAction<S>> {
    let at = |prefixes: &[&str]| point(ctx, args, prefixes);
    match verbs.canonical(verb) {
        "left_click" => Ok(click(at(&[""])?, Button::Left, ClickCount::One)),
        "right_click" => Ok(click(at(&[""])?, Button::Right, ClickCount::One)),
        "middle_click" => Ok(click(at(&[""])?, Button::Middle, ClickCount::One)),
        "double_click" => Ok(click(at(&[""])?, Button::Left, ClickCount::Two)),
        "triple_click" => Ok(click(at(&[""])?, Button::Left, ClickCount::Three)),
        "mouse_move" => Ok(CuaAction::MoveTo {
            at: Target::Point(at(&[""])?),
        }),
        // Qwen: `start_coordinate` is the start and `coordinate` the end.
        "left_click_drag" => Ok(CuaAction::Drag {
            from: Target::Point(at(&["start_"])?),
            to: Target::Point(at(&["end_", ""])?),
            button: Button::Left,
        }),
        "type" => Ok(CuaAction::Type {
            text: bounded(TypedText::new(string(args, "text")?))?,
        }),
        "key" => Ok(CuaAction::Key {
            chord: keys(args)?,
            repeat: cua_action::Repeat::ONCE,
        }),
        "scroll" => scroll(verbs, ctx, args, Axis::Vertical),
        "hscroll" => scroll(verbs, ctx, args, Axis::Horizontal),
        "wait" => Ok(CuaAction::Wait {
            for_ms: seconds(args.get("time").ok_or(DropReason::MissingArgument)?)?,
        }),
        "terminate" => Ok(CuaAction::Finish {
            outcome: outcome(string(args, "status")?)?,
            summary: bounded(Summary::new(final_words(args)))?,
            extracted: Vec::new(),
        }),
        "answer" => Ok(CuaAction::Finish {
            outcome: FinishOutcome::Done,
            summary: bounded(Summary::new(string(args, "text")?))?,
            extracted: Vec::new(),
        }),
        "ask" if verbs.has("ask") => Ok(CuaAction::Ask {
            question: bounded(Summary::new(string(args, "question")?))?,
            choices: choices(args)?,
        }),
        "observe" if verbs.has("observe") => Ok(CuaAction::Observe),
        _ => Err(DropReason::UnsupportedVerb),
    }
}

/// A whole number: a JSON integer, or a float with nothing after the point; never negative.
fn whole(value: &Value) -> Verdict<u32> {
    let n = value
        .as_u64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|f| f.fract() == 0.0 && *f >= 0.0)
                .map(|f| f as u64)
        })
        .ok_or(DropReason::BadNumber)?;
    u32::try_from(n).map_err(|_| DropReason::BadNumber)
}

/// A point by one prefix after another: `{prefix}coordinate: [x, y]`, else `{prefix}x` and
/// `{prefix}y`. The first prefix that names anything decides.
pub(crate) fn point<S: CoordSpace>(
    ctx: &Ctx,
    args: &Args,
    prefixes: &[&str],
) -> Verdict<cua_action::Point<S>> {
    for prefix in prefixes {
        if let Some(pair) = args.get(&format!("{prefix}coordinate")) {
            let [x, y] = pair
                .as_array()
                .map(Vec::as_slice)
                .ok_or(DropReason::BadNumber)?
            else {
                return Err(DropReason::BadNumber);
            };
            return ctx.point(whole(x)?, whole(y)?);
        }
        match (
            args.get(&format!("{prefix}x")),
            args.get(&format!("{prefix}y")),
        ) {
            (Some(x), Some(y)) => return ctx.point(whole(x)?, whole(y)?),
            (None, None) => {}
            _ => return Err(DropReason::MissingArgument),
        }
    }
    Err(DropReason::MissingArgument)
}

/// Whether the call says anything about a point under `prefix` (`coordinate`, `x` or `y`).
fn names_a_point(args: &Args, prefix: &str) -> bool {
    ["coordinate", "x", "y"]
        .iter()
        .any(|key| args.contains_key(&format!("{prefix}{key}")))
}

pub(crate) fn string<'a>(args: &'a Args, name: &str) -> Verdict<&'a str> {
    match args.get(name) {
        None => Err(DropReason::MissingArgument),
        Some(value) => value.as_str().ok_or(DropReason::BadArgument),
    }
}

fn optional_string<'a>(args: &'a Args, name: &str) -> &'a str {
    args.get(name).and_then(Value::as_str).unwrap_or_default()
}

/// The words a finishing call says to the person: `summary`, or `text` when there is no
/// `summary` argument at all (Holo 3.1 writes its answer as `text`). A present `summary` wins.
fn final_words(args: &Args) -> &str {
    if args.contains_key("summary") {
        optional_string(args, "summary")
    } else {
        optional_string(args, "text")
    }
}

/// `keys: ["ctrl", "c"]` (Qwen), or a string such as `"ctrl+c"` under `keys` or `key`.
fn keys(args: &Args) -> Verdict<cua_action::Chord> {
    match args.get("keys").or_else(|| args.get("key")) {
        Some(Value::String(text)) => chord::from_text(text),
        Some(Value::Array(words)) => {
            let words: Option<Vec<&str>> = words.iter().map(Value::as_str).collect();
            chord::from_words(&words.ok_or(DropReason::BadArgument)?)
        }
        Some(_) => Err(DropReason::BadArgument),
        None => Err(DropReason::MissingArgument),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Axis {
    Vertical,
    Horizontal,
}

/// Qwen scrolls by `pixels`: positive up (vertical) or right (horizontal), negative the other
/// way. Holo may instead give a `direction` and scroll by a fixed number of notches. A scroll
/// that names no point acts at the centre of the frame (Qwen's schema makes the coordinate
/// optional); a half-given point is still refused.
pub(crate) fn scroll<S: CoordSpace>(
    verbs: &Verbs,
    ctx: &Ctx,
    args: &Args,
    axis: Axis,
) -> Verdict<CuaAction<S>> {
    let at = if names_a_point(args, "") {
        Target::Point(point(ctx, args, &[""])?)
    } else {
        Target::Centre
    };
    match (args.get("pixels"), args.get("direction")) {
        (Some(pixels), _) => {
            let signed = pixels
                .as_i64()
                .or_else(|| {
                    pixels
                        .as_f64()
                        .filter(|f| f.fract() == 0.0)
                        .map(|f| f as i64)
                })
                .ok_or(DropReason::BadNumber)?;
            let magnitude =
                u32::try_from(signed.unsigned_abs()).map_err(|_| DropReason::BadNumber)?;
            if magnitude == 0 {
                return Err(DropReason::BadNumber);
            }
            let dir = match (axis, signed > 0) {
                (Axis::Vertical, true) => ScrollDir::Up,
                (Axis::Vertical, false) => ScrollDir::Down,
                (Axis::Horizontal, true) => ScrollDir::Right,
                (Axis::Horizontal, false) => ScrollDir::Left,
            };
            Ok(CuaAction::Scroll {
                at,
                dir,
                by: ScrollBy::Distance(scroll_length(magnitude)),
            })
        }
        (None, Some(dir)) if verbs.scroll_by_direction => Ok(CuaAction::Scroll {
            at,
            dir: direction(dir.as_str().ok_or(DropReason::BadArgument)?)?,
            by: ScrollBy::Notches(Notches(SCROLL_NOTCHES)),
        }),
        _ => Err(DropReason::MissingArgument),
    }
}

fn direction(word: &str) -> Verdict<ScrollDir> {
    match word.trim().to_ascii_lowercase().as_str() {
        "up" => Ok(ScrollDir::Up),
        "down" => Ok(ScrollDir::Down),
        "left" => Ok(ScrollDir::Left),
        "right" => Ok(ScrollDir::Right),
        _ => Err(DropReason::BadArgument),
    }
}

/// Seconds, whole or fractional, as a bounded wait.
fn seconds(value: &Value) -> Verdict<WaitMs> {
    let secs = value.as_f64().filter(|s| s.is_finite() && *s >= 0.0);
    let ms = secs.map(|s| (s * MS_PER_SECOND).round());
    ms.filter(|ms| *ms <= f64::from(WaitMs::MAX))
        .and_then(|ms| WaitMs::new(ms as u32).ok())
        .ok_or(DropReason::BadNumber)
}

fn outcome(status: &str) -> Verdict<FinishOutcome> {
    match status.trim().to_ascii_lowercase().as_str() {
        "success" | "done" => Ok(FinishOutcome::Done),
        "failure" | "failed" => Ok(FinishOutcome::Failed),
        "infeasible" => Ok(FinishOutcome::Infeasible),
        _ => Err(DropReason::BadArgument),
    }
}

fn choices(args: &Args) -> Verdict<Vec<Choice>> {
    let Some(value) = args.get("choices") else {
        return Ok(Vec::new());
    };
    let items = value.as_array().ok_or(DropReason::BadArgument)?;
    if items.len() > Choice::MAX_PER_ASK {
        return Err(DropReason::TooLong);
    }
    items
        .iter()
        .map(|item| bounded(Choice::new(item.as_str().ok_or(DropReason::BadArgument)?)))
        .collect()
}
