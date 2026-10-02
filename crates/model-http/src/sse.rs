//! A decoder for server-sent events, fed bytes in any chunking.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventName(pub String);

/// One event: its optional name and its data lines joined with `\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<EventName>,
    pub data: String,
}

/// A stream that is not valid SSE (not UTF-8, or a line over the limit). Never a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SseError {
    #[error("the stream is not valid UTF-8")]
    NotUtf8,
    #[error("a line is longer than the decoder accepts")]
    LineTooLong,
}

/// Holds the bytes of an incomplete line between `feed` calls. The same bytes give the same
/// events whatever the chunking.
#[derive(Debug, Clone, Default)]
pub struct SseDecoder {
    pending: Vec<u8>,
    event: Option<EventName>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one chunk; returns the events it completed.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, SseError> {
        let _ = (bytes, &self.pending, &self.event, &self.data);
        todo!("SseDecoder::feed: lines, fields, blank line dispatches")
    }

    /// The stream ended: an event left half-built is dropped, as the SSE standard says.
    pub fn finish(self) -> Vec<SseEvent> {
        let _ = (self.pending, self.event, self.data);
        todo!("SseDecoder::finish")
    }
}
