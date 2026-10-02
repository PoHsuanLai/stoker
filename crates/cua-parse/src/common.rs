//! What every dialect parser shares: the space a point may live in, the batch that keeps the
//! actions and what was dropped, and the small constructors and bounds around them.
//!
//! A drop reason is the one reason a verb did not become an action. `DropReason` has no variant
//! for "an argument that is present but unusable" (an empty `type` text, a control character, a
//! key chord of two plain keys, an unknown direction), so those read as `MissingArgument`: no
//! usable argument was given.

use std::collections::BTreeSet;

use cua_action::{
    Button, ClickCount, Coord, CoordSpace, CuaAction, GridMax, Length, ModelSpace, Point, Target,
    TextError,
};

use crate::{DropReason, Dropped, InSpace, ParseError, ParseLimits, Parsed, VerbText};

/// The most dropped verbs one parse reports: input is bounded, the report is too.
const MAX_DROPPED: usize = 64;

/// Where a dialect's points live, as far as a parser can check: a grid has a top, an image's
/// size is the frame map's business.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    grid_max: Option<u32>,
}

impl Ctx {
    pub(crate) fn new(space: ModelSpace) -> Ctx {
        Ctx {
            grid_max: match space {
                ModelSpace::Image => None,
                ModelSpace::Grid(GridMax(max)) => Some(u32::from(max)),
            },
        }
    }

    /// A point of the dialect's space. A grid point past the grid's top is refused, never
    /// clamped.
    pub(crate) fn point<S: CoordSpace>(&self, x: u32, y: u32) -> Result<Point<S>, DropReason> {
        match self.grid_max {
            Some(max) if x > max || y > max => Err(DropReason::OutOfFrame),
            _ => Ok(Point::new(Coord(x), Coord(y))),
        }
    }
}

/// A dialect that can parse into any space.
pub(crate) trait Dialect {
    fn collect<S: CoordSpace>(
        &self,
        ctx: &Ctx,
        limits: ParseLimits,
    ) -> Result<Collected<S>, ParseError>;
}

/// Runs a dialect in the space the model speaks.
pub(crate) fn in_space(
    dialect: &impl Dialect,
    space: ModelSpace,
    limits: ParseLimits,
) -> Result<Parsed, ParseError> {
    let ctx = Ctx::new(space);
    match space {
        ModelSpace::Image => dialect
            .collect(&ctx, limits)
            .map(|c| c.parsed(InSpace::Image)),
        ModelSpace::Grid(max) => dialect
            .collect(&ctx, limits)
            .map(|c| c.parsed(|actions| InSpace::Grid(max, actions))),
    }
}

/// A parse in one space, before it is wrapped in `InSpace`.
#[derive(Debug)]
pub(crate) struct Collected<S: CoordSpace> {
    thought: Option<String>,
    actions: Vec<CuaAction<S>>,
    dropped: Vec<Dropped>,
}

impl<S: CoordSpace> Collected<S> {
    fn parsed(self, wrap: impl FnOnce(Vec<CuaAction<S>>) -> InSpace) -> Parsed {
        Parsed {
            thought: self.thought,
            actions: wrap(self.actions),
            dropped: self.dropped,
        }
    }
}

/// The actions kept so far, up to the batch limit, and every verb that was not kept.
#[derive(Debug)]
pub(crate) struct Batch<S: CoordSpace> {
    limit: usize,
    actions: Vec<CuaAction<S>>,
    dropped: Vec<Dropped>,
}

impl<S: CoordSpace> Batch<S> {
    pub(crate) fn new(limits: ParseLimits) -> Self {
        Batch {
            limit: usize::from(limits.max_actions.0),
            actions: Vec::new(),
            dropped: Vec::new(),
        }
    }

    /// Keeps a verb's action while there is room; otherwise records why the verb is gone.
    pub(crate) fn push(&mut self, verb: &str, result: Result<CuaAction<S>, DropReason>) {
        match result {
            Ok(action) if self.actions.len() < self.limit => self.actions.push(action),
            Ok(_) => self.drop(verb, DropReason::OverBatchLimit),
            Err(reason) => self.drop(verb, reason),
        }
    }

    fn drop(&mut self, verb: &str, reason: DropReason) {
        if self.dropped.len() < MAX_DROPPED {
            self.dropped.push(Dropped {
                verb: VerbText::lossy(verb),
                reason,
            });
        }
    }

    /// A reply with no verb at all is `NoAction`; a reply whose verbs were all refused is a
    /// parse with no actions, so the caller can tell the model what was refused.
    pub(crate) fn finish(self, thought: Option<String>) -> Result<Collected<S>, ParseError> {
        if self.actions.is_empty() && self.dropped.is_empty() {
            return Err(ParseError::NoAction);
        }
        Ok(Collected {
            thought,
            actions: self.actions,
            dropped: self.dropped,
        })
    }
}

/// A bounded text refused for its content: too long stays `TooLong`, the rest reads as a
/// missing argument.
pub(crate) fn bounded<T>(result: Result<T, TextError>) -> Result<T, DropReason> {
    result.map_err(|e| match e {
        TextError::TooLong { .. } => DropReason::TooLong,
        TextError::Empty | TextError::ControlChar { .. } => DropReason::MissingArgument,
    })
}

pub(crate) fn click<S: CoordSpace>(
    at: Point<S>,
    button: Button,
    count: ClickCount,
) -> CuaAction<S> {
    CuaAction::Click {
        at: Target::Point(at),
        button,
        count,
        mods: BTreeSet::new(),
    }
}

pub(crate) fn scroll_length<S: CoordSpace>(pixels: u32) -> Length<S> {
    Length::new(Coord(pixels))
}
