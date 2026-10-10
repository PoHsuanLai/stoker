//! Parsers for the action dialects of local computer-use models.
//!
//! Strict and total: no `eval`, no panic on any input, verbs on an allow-list, numbers parsed as
//! `u32` within bounds. An unknown verb is dropped and reported, never guessed.
//!
//! ```
//! use cua_action::{ActionClass, ModelSpace, TextDialect};
//! use cua_parse::{InSpace, ParseLimits, parse_text};
//!
//! let reply = "Thought: Open the search box.\nAction: click(start_box='(459,203)')";
//! let parsed = parse_text(TextDialect::UiTars15, ModelSpace::Image, reply, ParseLimits::default())?;
//! let InSpace::Image(actions) = parsed.actions else { panic!("image space was asked for") };
//! assert_eq!(actions.len(), 1);
//! assert_eq!(actions[0].class(), ActionClass::Pointer);
//! assert!(parsed.dropped.is_empty());
//! # Ok::<(), cua_parse::ParseError>(())
//! ```

mod chord;
mod common;
mod limits;
mod outcome;
mod parse;
mod scan;
mod tools;
mod ui_tars;

pub use chord::{from_text as chord_from_text, from_words as chord_from_words};
pub use common::{Batch, bounded};
pub use limits::{ActionCount, ByteLen, ByteOffset, ParseLimits};
pub use outcome::{DropReason, Dropped, InSpace, ParseError, Parsed, VerbText, VerbTextError};
pub use parse::{parse_text, parse_tool_calls};
pub use tools::SCROLL_PIXELS_RULE;
