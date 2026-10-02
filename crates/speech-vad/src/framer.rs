//! Cuts audio chunks into 512-sample frames, carrying the remainder to the next chunk.

use speech_provider::{AudioChunk, Frame512, SampleIndex};

/// One frame and where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramedAt {
    pub at: SampleIndex,
    pub frame: Frame512,
}

/// Why a chunk cannot be framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FramerError {
    #[error("the chunk is not 16 kHz S16 mono")]
    WrongFormat,
}

/// Splits chunks of 16 kHz S16 mono audio into frames.
#[derive(Debug, Clone, Default)]
pub struct Framer {
    carry: Vec<i16>,
    next: SampleIndex,
}

impl Framer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Samples held back, fewer than a frame.
    pub fn pending(&self) -> usize {
        self.carry.len()
    }

    /// The whole frames the chunk completes, in order; the rest is kept for the next chunk.
    pub fn push(&mut self, chunk: &AudioChunk) -> Result<Vec<FramedAt>, FramerError> {
        let _ = (&mut self.carry, &mut self.next, chunk);
        todo!("Framer::push: decode S16, join the carry, cut 512-sample frames")
    }
}
