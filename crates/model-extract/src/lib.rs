//! Typed outputs from a model, as a machine that does no I/O.
//!
//! [`choose`] picks how to ask: constrained decoding where the engine supports the shape, a
//! synthetic `final_result` tool call where it does not, the schema in the prompt as the last
//! resort, and the reply is always validated. [`ExtractSession`] builds the request, absorbs the
//! reply, repairs at most as many times as its budget says, and fails with a reason; it never
//! hands an unchecked string on. This generalises `cua-session::absorb` (the same one-repair rule,
//! the same total function).
//!
//! Where it runs: `inferd` runs an `ExtractSession` for `ReplyShape::Json` and `ReplyShape::Choice`
//! and owns the retry, so every client (readerd, memoryd's consolidator, the action reviewer, the
//! policy writer) gets a reply that already passes `Shape::check`, or `ModelError::Unparseable`.
//!
//! A repair prompt names the field and the expected shape and never echoes the model's output: it
//! may hold untrusted text, and the reader is quarantined.

mod mode;
mod repair;
mod session;
mod shaped;

pub use mode::{ExtractMode, ToolsPresent, choose};
pub use session::{
    ExtractFailure, ExtractSession, Extracted, FINAL_RESULT_TOOL, RepairBudget, RepairsLeft,
};
pub use shaped::ShapedSession;
