//! The order a conversation must be in before it is encoded.
//!
//! Strict servers (vLLM chat templates, Anthropic later) reject or mis-render an orphaned tool
//! result; local templates sometimes fail silently. The check runs in the encode paths and where
//! the planner windows its history.

use crate::Message;

/// What is wrong with a conversation's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum SequenceFault {
    #[error("two assistant messages follow each other")]
    ConsecutiveAssistant,
    #[error("a tool call has no result before the next turn")]
    UnansweredCall,
    #[error("a tool result answers no call")]
    OrphanResult,
}

/// `Ok` when no assistant message follows another, every tool call is answered before the next
/// non-tool message, and every tool result answers an earlier call.
pub fn check(messages: &[Message]) -> Result<(), SequenceFault> {
    let _ = messages;
    todo!("sequence::check: port of rig's validate_canonical")
}
