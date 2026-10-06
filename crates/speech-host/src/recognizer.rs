//! The seam between the protocol loop and a speech engine.

use model_provider::ProviderError;
use speech_provider::{AudioChunk, SpeechModelInfo, SttEnd, SttRequest, TranscriptEvent};

/// What `finish` yields: the events the last audio and the flush produced, then the summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub events: Vec<TranscriptEvent>,
    pub end: SttEnd,
}

/// One streaming recognizer, one utterance at a time. The serve loop guarantees the order:
/// `begin`, any number of `accept`, then `finish` or `reset`; `reset` also follows an error and a
/// dropped connection. Audio is held in memory only, for the utterance.
pub trait Recognizer {
    /// The models this engine serves, for the `Hello` reply.
    fn models(&self) -> Vec<SpeechModelInfo>;

    /// Starts an utterance; an unserved model or an unsupported format is an error.
    fn begin(&mut self, request: &SttRequest) -> Result<(), ProviderError>;

    /// Feeds one chunk and returns the events it made (possibly none).
    fn accept(&mut self, chunk: &AudioChunk) -> Result<Vec<TranscriptEvent>, ProviderError>;

    /// No more audio: flush and summarise.
    fn finish(&mut self) -> Result<Finished, ProviderError>;

    /// Drops the utterance and everything held for it.
    fn reset(&mut self);
}
