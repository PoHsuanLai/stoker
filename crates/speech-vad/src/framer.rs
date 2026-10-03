//! Cuts audio chunks into 512-sample frames, carrying the remainder to the next chunk.

use speech_provider::{
    AudioChunk, AudioFormat, FRAME_SAMPLES, Frame512, PcmFormat, SampleIndex, SampleRate,
};

/// The only format the detectors take.
const INPUT: AudioFormat = AudioFormat {
    rate: SampleRate(16_000),
    pcm: PcmFormat::S16Le,
};

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
    /// Whole samples not yet in a frame.
    carry: Vec<i16>,
    /// The low byte of a sample whose high byte is in the next chunk.
    spare: Option<u8>,
    /// Where the first carried sample is (or, with nothing carried, where the next one is).
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

    /// The whole frames the chunk completes, in order; the rest is kept for the next chunk. Any
    /// way of cutting the same bytes into chunks gives the same frames, a cut inside a sample
    /// included. A chunk that starts where nothing is carried restarts the count at its own
    /// position, so a gap in the audio is a gap in the numbers and not a shift.
    pub fn push(&mut self, chunk: &AudioChunk) -> Result<Vec<FramedAt>, FramerError> {
        if chunk.format != INPUT {
            return Err(FramerError::WrongFormat);
        }
        if self.carry.is_empty() && self.spare.is_none() {
            self.next = chunk.at;
        }
        let mut bytes = chunk.pcm.as_slice();
        // A sample cut in two by the chunk boundary is joined first.
        if let (Some(low), Some((high, rest))) = (self.spare, bytes.split_first()) {
            self.carry.push(i16::from_le_bytes([low, *high]));
            self.spare = None;
            bytes = rest;
        }
        let (pairs, rest) = bytes.as_chunks::<2>();
        self.carry
            .extend(pairs.iter().map(|pair| i16::from_le_bytes(*pair)));
        self.spare = self.spare.or(rest.first().copied());
        let mut frames = Vec::new();
        while self.carry.len() >= FRAME_SAMPLES {
            let samples: Vec<i16> = self.carry.drain(..FRAME_SAMPLES).collect();
            let Ok(frame) = Frame512::new(&samples) else {
                break;
            };
            frames.push(FramedAt {
                at: self.next,
                frame,
            });
            self.next = SampleIndex(self.next.0 + FRAME_SAMPLES as u64);
        }
        Ok(frames)
    }
}
