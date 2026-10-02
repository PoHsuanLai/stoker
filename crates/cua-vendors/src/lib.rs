//! Vendor computer-use encodings: the tool declaration a vendor wants, how its calls decode into
//! actions, and how step results encode back.

mod codec;
mod step_result;

pub use codec::{WireCodec, WireCodecs, WireError, codec};
pub use step_result::StepResult;
