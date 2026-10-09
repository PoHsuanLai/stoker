//! Parsers for the action dialects of local computer-use models.
//!
//! Strict and total: no `eval`, no panic on any input, verbs on an allow-list, numbers parsed as
//! `u32` within bounds. An unknown verb is dropped and reported, never guessed.

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
