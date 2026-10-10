//! The provider-neutral computer-use action vocabulary.
//!
//! Every point carries the coordinate space it lives in, so mixing a window point with an image
//! point does not compile. Models, parsers and the run machine all speak `CuaAction<S>`; only
//! the space parameter changes as a point travels from the model's grid to the leased window.
//!
//! ```
//! use cua_action::{ActionClass, Button, ClickCount, Coord, CuaAction, Point, Target, WindowSpace};
//! use std::collections::BTreeSet;
//!
//! let at = Target::Point(Point::<WindowSpace>::new(Coord(10), Coord(20)));
//! let click = CuaAction::Click { at, button: Button::Left, count: ClickCount::One, mods: BTreeSet::new() };
//! assert_eq!(click.class(), ActionClass::Pointer);
//!
//! let json = serde_json::to_string(&click)?;
//! let back: CuaAction<WindowSpace> = serde_json::from_str(&json)?;
//! assert_eq!(back, click);
//! # Ok::<(), serde_json::Error>(())
//! ```

mod action;
mod dialect;
mod gate;
mod geometry;
mod keys;
mod space;
mod target;
mod text;

pub use action::{ActionClass, CuaAction, Extracted, FinishOutcome, ScrollBy};
pub use dialect::{CuaDialect, ModelSpace, TextDialect, ToolDialect, WireDialect};
pub use gate::{RunMode, WindowClass, WindowTrust};
pub use geometry::{Coord, DeviceSize, GridMax, Length, PixelFormat, Point, Rect, Scale120, Size};
pub use keys::{
    Button, Chord, ChordError, ClickCount, Modifier, Notches, Repeat, RepeatError, ScrollDir,
    WaitError, WaitMs,
};
pub use space::{CoordSpace, GridSpace, ImageSpace, WindowSpace};
pub use target::{NodeId, Target};
pub use text::{Choice, ExtractedText, FieldName, Summary, TextError, TypedText};
