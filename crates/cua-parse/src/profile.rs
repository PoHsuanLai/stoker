//! A model's action profile at read time: what a catalogue entry says about how its calls differ
//! from the common convention (see `cua_action::ActionProfile`), and what the parser needs to
//! know about the step to apply it.
//!
//! A profile claims a verb only when it states a difference for it. A verb it does not claim goes
//! on to the dialect's own table, so the default profile changes nothing.

use cua_action::{
    ActionProfile, Button, CoordSpace, CuaAction, DragStyle, ScrollAxis, Summary, Target, VerbAlias,
};
use serde_json::Value;

use crate::DropReason;
use crate::common::{Ctx, bounded};
use crate::tools::{Args, Axis, Verdict, point, scroll, string, verb_action};
use crate::verbs::Verbs;

/// Where the cursor is, in the model's own space (the grid or the image's pixels). A drag that
/// starts at the cursor needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorAt {
    pub x: u32,
    pub y: u32,
}

/// What a tool-call parse knows about the model and the step beyond the calls themselves. The
/// default is the common convention and no known cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolContext {
    pub profile: ActionProfile,
    pub cursor: Option<CursorAt>,
}

/// The action a profile makes of a verb, or `None` when the profile says nothing about it.
pub(crate) fn action<S: CoordSpace>(
    context: &ToolContext,
    verbs: &Verbs,
    ctx: &Ctx,
    verb: &str,
    args: &Args,
) -> Option<Verdict<CuaAction<S>>> {
    let profile = context.profile;
    let alias = profile.aliases.resolve(verb);
    let canonical: &str = alias.map_or(verb, |a| a.canonical());
    match (alias, canonical, profile.drag, profile.scroll) {
        (Some(VerbAlias::CallUser), ..) => Some(call_user(args)),
        (_, "left_click_drag", DragStyle::FromCursor, _) => {
            Some(drag_from_cursor(context, ctx, args))
        }
        (_, "scroll", _, ScrollAxis::ByDirection) => Some(scroll_by_direction(verbs, ctx, args)),
        (Some(_), ..) => Some(verb_action(verbs, ctx, canonical, args)),
        (None, ..) => None,
    }
}

/// Where the cursor is once `action` has run: a click, move, scroll or drag leaves it at its
/// point; anything else leaves it where it was. A reply that moves and then drags from the cursor
/// drags from where it moved, as the model meant.
pub(crate) fn cursor_after<S: CoordSpace>(
    cursor: Option<CursorAt>,
    action: &CuaAction<S>,
) -> Option<CursorAt> {
    let target = match action {
        CuaAction::Click { at, .. } | CuaAction::MoveTo { at } | CuaAction::Scroll { at, .. } => at,
        CuaAction::Drag { to, .. } => to,
        _ => return cursor,
    };
    match target {
        Target::Point(at) => Some(CursorAt {
            x: at.x.0,
            y: at.y.0,
        }),
        Target::Node(_) | Target::Centre => cursor,
    }
}

/// `call_user` with `text`: the model asks the person.
fn call_user<S: CoordSpace>(args: &Args) -> Verdict<CuaAction<S>> {
    Ok(CuaAction::Ask {
        question: bounded(Summary::new(string(args, "text")?))?,
        choices: Vec::new(),
    })
}

/// A drag that presses where the cursor is and releases at `coordinate`. Without a known cursor
/// there is no start, so the drag is dropped.
fn drag_from_cursor<S: CoordSpace>(
    context: &ToolContext,
    ctx: &Ctx,
    args: &Args,
) -> Verdict<CuaAction<S>> {
    let from = context.cursor.ok_or(DropReason::MissingArgument)?;
    Ok(CuaAction::Drag {
        from: Target::Point(ctx.point(from.x, from.y)?),
        to: Target::Point(point(ctx, args, &[""])?),
        button: Button::Left,
    })
}

/// A `scroll` whose `direction` names the axis: `vertical` when it says nothing.
fn scroll_by_direction<S: CoordSpace>(
    verbs: &Verbs,
    ctx: &Ctx,
    args: &Args,
) -> Verdict<CuaAction<S>> {
    let axis = match args.get("direction") {
        None => Axis::Vertical,
        Some(Value::String(word)) => match word.trim().to_ascii_lowercase().as_str() {
            "vertical" => Axis::Vertical,
            "horizontal" => Axis::Horizontal,
            _ => return Err(DropReason::BadArgument),
        },
        Some(_) => return Err(DropReason::BadArgument),
    };
    scroll(verbs, ctx, args, axis)
}
