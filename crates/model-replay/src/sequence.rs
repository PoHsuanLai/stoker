//! Whether recorded events could be a stream a provider produced.

use std::collections::BTreeMap;

use model_provider::{CallIndex, ToolCallId, TurnEvent};

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

/// Where one call index stands.
enum Call<'a> {
    Open(&'a ToolCallId),
    Closed(&'a ToolCallId),
}

/// `Ok` when every `ToolCallStarted` is followed by its deltas and one `ToolCallDone`, and no
/// event names a call out of order. Run when a cassette loads.
///
/// An index may start a second call once its first is done (servers reuse indices); a delta for
/// an index whose call is done and not restarted is `AfterDone`.
pub fn check_sequence(events: &[TurnEvent]) -> Result<(), StreamFault> {
    let mut calls: BTreeMap<CallIndex, Call<'_>> = BTreeMap::new();
    for event in events {
        match event {
            TurnEvent::ToolCallStarted { index, id, .. } => {
                if let Some(Call::Open(_)) = calls.get(index) {
                    return Err(StreamFault::EndedTwice);
                }
                calls.insert(*index, Call::Open(id));
            }
            TurnEvent::ToolCallDelta { index, .. } => match calls.get(index) {
                None => return Err(StreamFault::UnknownCall),
                Some(Call::Closed(_)) => return Err(StreamFault::AfterDone),
                Some(Call::Open(_)) => {}
            },
            TurnEvent::ToolCallDone(call) => {
                let open = calls.iter().find_map(|(index, c)| {
                    matches!(c, Call::Open(id) if **id == call.id).then_some(*index)
                });
                match open {
                    Some(index) => {
                        calls.insert(index, Call::Closed(&call.id));
                    }
                    None if calls
                        .values()
                        .any(|c| matches!(c, Call::Closed(id) if *id == &call.id)) =>
                    {
                        return Err(StreamFault::EndedTwice);
                    }
                    None => return Err(StreamFault::UnknownCall),
                }
            }
            _ => {}
        }
    }
    match calls.values().any(|c| matches!(c, Call::Open(_))) {
        true => Err(StreamFault::Unclosed),
        false => Ok(()),
    }
}
