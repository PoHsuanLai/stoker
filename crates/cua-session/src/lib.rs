//! The model side of one computer-use step, as a pure value.
//!
//! `request` assembles the prompt for a model from the goal, the history and a prepared frame;
//! `absorb` takes the model's reply, parses it (one repair is allowed), maps its points into
//! window space and pushes the step into the history. Nothing here waits or does I/O.

mod history;
mod model;
mod prompt;
mod reply;
mod session;
mod transcript;
mod window;

pub use model::{
    CuaProfile, CuaTaskText, FrameBudget, MaskedRegions, ObservationIn, RepairBudget, StepIndex,
    StepLines, StepOutcome, TurnSettings, TurnTranscript,
};
pub use session::CuaSession;
pub use transcript::TranscriptSink;
