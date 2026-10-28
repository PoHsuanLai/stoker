//! Vendor computer-use encodings: the tool declaration a vendor wants, how its calls decode into
//! actions, and how step results encode back.
//!
//! ```
//! use cua_action::{Coord, Size, WireDialect};
//! use cua_vendors::WireCodec;
//!
//! // A dialect declares its tool for an image of the given size.
//! let dialect = WireDialect::OpenAiComputer;
//! assert_eq!(dialect.tools(Size::new(Coord(1280), Coord(800))).len(), 1);
//! ```

mod anthropic;
mod args;
mod codec;
mod gemini;
mod openai;
mod results;
mod safety;
mod step_result;

pub use codec::{WireCodec, WireError};
pub use step_result::StepResult;
