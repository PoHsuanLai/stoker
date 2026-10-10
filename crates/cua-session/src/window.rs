//! Parsed actions into window space: a point that falls outside the frame is dropped, never
//! clamped.

use cua_action::{CoordSpace, CuaAction, Length, Point, WindowSpace};
use cua_parse::{DropReason, Dropped, InSpace, VerbText};
use vision_prep::{FrameMap, MapError};

/// The verb a dropped action is reported under.
fn verb_of<S: CoordSpace>(action: &CuaAction<S>) -> &'static str {
    match action {
        CuaAction::Click { .. } => "click",
        CuaAction::MoveTo { .. } => "move",
        CuaAction::Drag { .. } => "drag",
        CuaAction::Type { .. } => "type",
        CuaAction::Key { .. } => "key",
        CuaAction::Scroll { .. } => "scroll",
        CuaAction::Wait { .. } => "wait",
        CuaAction::Zoom { .. } => "zoom",
        CuaAction::Observe => "observe",
        CuaAction::Finish { .. } => "finish",
        CuaAction::Ask { .. } => "ask",
        _ => "act",
    }
}

fn map_all<S: CoordSpace>(
    actions: Vec<CuaAction<S>>,
    point: impl Fn(Point<S>) -> Result<Point<WindowSpace>, MapError>,
    length: impl Fn(Length<S>) -> Result<Length<WindowSpace>, MapError>,
) -> (Vec<CuaAction<WindowSpace>>, Vec<Dropped>) {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for action in actions {
        let verb = verb_of(&action);
        match action.map_points(&point, &length) {
            Ok(mapped) => kept.push(mapped),
            Err(_) => dropped.push(Dropped {
                verb: VerbText::new(verb).unwrap_or_else(|_| unreachable!("a verb name is short")),
                reason: DropReason::OutOfFrame,
            }),
        }
    }
    (kept, dropped)
}

/// The actions in window space, and what mapping dropped.
pub(crate) fn to_window(
    actions: InSpace,
    map: &FrameMap,
) -> (Vec<CuaAction<WindowSpace>>, Vec<Dropped>) {
    match actions {
        InSpace::Image(list) => map_all(
            list,
            |p| map.image_to_window(p),
            |l| map.length_to_window(l),
        ),
        InSpace::Grid(_, list) => {
            map_all(list, |p| map.grid_to_window(p), |l| map.length_to_window(l))
        }
    }
}
