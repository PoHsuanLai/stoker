//! The order a conversation must be in before it is encoded.
//!
//! Strict servers (vLLM chat templates, Anthropic later) reject or mis-render an orphaned tool
//! result; local templates sometimes fail silently. The check runs in the encode paths and where
//! the planner windows its history.

use std::collections::BTreeSet;

use crate::{Message, Part, Role, ToolCallId};

/// What is wrong with a conversation's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum SequenceFault {
    #[error("two assistant messages follow each other")]
    ConsecutiveAssistant,
    #[error("a tool call has no result before the next turn")]
    UnansweredCall,
    #[error("a tool result answers no call")]
    OrphanResult,
}

/// `Ok` when no assistant message follows another, every tool call is answered before the next
/// non-tool message (and before the conversation ends), and every tool result answers an earlier,
/// still unanswered call. A tool call and its result are matched by id, so a second result for
/// one call is an orphan.
pub fn check(messages: &[Message]) -> Result<(), SequenceFault> {
    let mut pending: BTreeSet<&ToolCallId> = BTreeSet::new();
    let mut previous: Option<Role> = None;
    for message in messages {
        for part in &message.parts {
            if let Part::ToolResult(result) = part
                && !pending.remove(&result.id)
            {
                return Err(SequenceFault::OrphanResult);
            }
        }
        if message.role == Role::Assistant && previous == Some(Role::Assistant) {
            return Err(SequenceFault::ConsecutiveAssistant);
        }
        if message.role != Role::Tool && !pending.is_empty() {
            return Err(SequenceFault::UnansweredCall);
        }
        pending.extend(message.parts.iter().filter_map(|part| match part {
            Part::ToolCall(call) => Some(&call.id),
            _ => None,
        }));
        previous = Some(message.role);
    }
    if pending.is_empty() {
        Ok(())
    } else {
        Err(SequenceFault::UnansweredCall)
    }
}
