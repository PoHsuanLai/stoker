//! Gathering a streamed turn into the transcript `absorb` reads.

use model_provider::{Flow, ToolCall, TurnEnd, TurnEvent, TurnSink};

use crate::TurnTranscript;

/// A [`TurnSink`] that keeps what a turn said: its text, its thoughts and its finished tool
/// calls, in order. `finish` makes the transcript once the turn has ended.
#[derive(Debug, Default)]
pub struct TranscriptSink {
    text: String,
    thought: String,
    calls: Vec<ToolCall>,
}

impl TranscriptSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finish(self, end: TurnEnd) -> TurnTranscript {
        TurnTranscript {
            text: self.text,
            thought: self.thought,
            calls: self.calls,
            end,
        }
    }
}

impl TurnSink for TranscriptSink {
    fn event(&mut self, event: TurnEvent) -> Flow {
        match event {
            TurnEvent::TextDelta(text) => self.text.push_str(&text),
            TurnEvent::ThoughtDelta(text) => self.thought.push_str(&text),
            TurnEvent::ToolCallDone(call) => self.calls.push(call),
            TurnEvent::ThoughtSealed(_)
            | TurnEvent::ToolCallStarted { .. }
            | TurnEvent::ToolCallDelta { .. }
            | TurnEvent::Safety(_)
            | TurnEvent::Usage(_) => {}
        }
        Flow::Continue
    }
}
