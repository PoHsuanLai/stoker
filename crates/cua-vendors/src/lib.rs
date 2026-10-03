//! Vendor computer-use encodings: the tool declaration a vendor wants, how its calls decode into
//! actions, and how step results encode back.

mod anthropic;
mod args;
mod codec;
mod gemini;
mod openai;
mod results;
mod safety;
mod step_result;

pub use codec::{WireCodec, WireCodecs, WireError, codec};
pub use step_result::StepResult;
