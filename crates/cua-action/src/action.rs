//! The action enum and the two questions the rest of the program asks of it: what class is it,
//! and what does it become in another coordinate space.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    Button, Choice, Chord, ClickCount, CoordSpace, ExtractedText, FieldName, Length, Modifier,
    Notches, Point, Rect, Repeat, ScrollDir, Size, Summary, Target, TypedText, WaitMs,
};

/// One thing a model asks the computer to do, in space `S`.
///
/// Verbs outside this enum (`open_app`, `navigate`, mouse down/up, hold-key) do not exist: a
/// parser drops them, so a model cannot act through a verb the schema lacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case", bound = "")]
pub enum CuaAction<S: CoordSpace> {
    Click {
        at: Target<S>,
        button: Button,
        count: ClickCount,
        mods: BTreeSet<Modifier>,
    },
    MoveTo {
        at: Target<S>,
    },
    Drag {
        from: Target<S>,
        to: Target<S>,
        button: Button,
    },
    Type {
        text: TypedText,
    },
    Key {
        chord: Chord,
        repeat: Repeat,
    },
    Scroll {
        at: Target<S>,
        dir: ScrollDir,
        by: ScrollBy<S>,
    },
    Wait {
        for_ms: WaitMs,
    },
    /// An observation request: look closer at a region. No side effect.
    Zoom {
        region: Rect<S>,
    },
    /// Take a screenshot: the next observation. No side effect.
    Observe,
    Finish {
        outcome: FinishOutcome,
        summary: Summary,
        extracted: Vec<Extracted>,
    },
    Ask {
        question: Summary,
        choices: Vec<Choice>,
    },
}

/// How far one scroll goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case", bound = "")]
pub enum ScrollBy<S: CoordSpace> {
    Notches(Notches),
    Distance(Length<S>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishOutcome {
    Done,
    Failed,
    Infeasible,
}

/// A value the run read off the screen; labelled `Untrusted(Screen)` by the run machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extracted {
    pub name: FieldName,
    pub value: ExtractedText,
}

/// What a policy needs to know about an action before it knows anything else. One match, here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionClass {
    /// Looks, never acts: `Observe`, `Zoom`, `Wait`.
    Observe,
    /// Moves or clicks the pointer: `Click`, `MoveTo`, `Drag`, `Scroll`.
    Pointer,
    /// Types: `Type`, `Key`.
    Keyboard,
    /// Ends or interrupts the run: `Finish`, `Ask`.
    Conclude,
}

impl<S: CoordSpace> CuaAction<S> {
    pub fn class(&self) -> ActionClass {
        match self {
            CuaAction::Observe | CuaAction::Zoom { .. } | CuaAction::Wait { .. } => {
                ActionClass::Observe
            }
            CuaAction::Click { .. }
            | CuaAction::MoveTo { .. }
            | CuaAction::Drag { .. }
            | CuaAction::Scroll { .. } => ActionClass::Pointer,
            CuaAction::Type { .. } | CuaAction::Key { .. } => ActionClass::Keyboard,
            CuaAction::Finish { .. } | CuaAction::Ask { .. } => ActionClass::Conclude,
        }
    }

    /// Re-expresses every point and length in space `T`. Nodes pass through; a rectangle's
    /// origin maps as a point and its extent as two lengths. The first error stops the map.
    pub fn map_points<T: CoordSpace, E>(
        self,
        f: impl Fn(Point<S>) -> Result<Point<T>, E>,
        g: impl Fn(Length<S>) -> Result<Length<T>, E>,
    ) -> Result<CuaAction<T>, E> {
        let target = |t: Target<S>| match t {
            Target::Point(p) => f(p).map(Target::Point),
            Target::Node(n) => Ok(Target::Node(n)),
        };
        Ok(match self {
            CuaAction::Click {
                at,
                button,
                count,
                mods,
            } => CuaAction::Click {
                at: target(at)?,
                button,
                count,
                mods,
            },
            CuaAction::MoveTo { at } => CuaAction::MoveTo { at: target(at)? },
            CuaAction::Drag { from, to, button } => CuaAction::Drag {
                from: target(from)?,
                to: target(to)?,
                button,
            },
            CuaAction::Type { text } => CuaAction::Type { text },
            CuaAction::Key { chord, repeat } => CuaAction::Key { chord, repeat },
            CuaAction::Scroll { at, dir, by } => {
                let by = match by {
                    ScrollBy::Notches(n) => ScrollBy::Notches(n),
                    ScrollBy::Distance(len) => ScrollBy::Distance(g(len)?),
                };
                CuaAction::Scroll {
                    at: target(at)?,
                    dir,
                    by,
                }
            }
            CuaAction::Wait { for_ms } => CuaAction::Wait { for_ms },
            CuaAction::Zoom { region } => {
                let w = g(Length::new(region.size.w))?.coord();
                let h = g(Length::new(region.size.h))?.coord();
                CuaAction::Zoom {
                    region: Rect {
                        origin: f(region.origin)?,
                        size: Size::new(w, h),
                    },
                }
            }
            CuaAction::Observe => CuaAction::Observe,
            CuaAction::Finish {
                outcome,
                summary,
                extracted,
            } => CuaAction::Finish {
                outcome,
                summary,
                extracted,
            },
            CuaAction::Ask { question, choices } => CuaAction::Ask { question, choices },
        })
    }
}
