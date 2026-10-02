//! Whether recorded events could be a stream a provider produced.

use model_provider::TurnEvent;

/// What is wrong with a recorded event stream (port of rig's `Transcript::push` checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum StreamFault {
    #[error("a tool-call delta or end names a call that never started")]
    UnknownCall,
    #[error("a tool call started or finished twice")]
    EndedTwice,
    #[error("a tool call started and never finished")]
    Unclosed,
    #[error("an event follows the call it belongs to has finished")]
    AfterDone,
}

/// `Ok` when every `ToolCallStarted` is followed by its deltas and one `ToolCallDone`, and no
/// event names a call out of order. Run when a cassette loads.
pub fn check_sequence(events: &[TurnEvent]) -> Result<(), StreamFault> {
    let _ = events;
    todo!("check_sequence: the per-index state of every tool call")
}
