//! Audio: formats, positions, durations and the bytes of one chunk.

use core::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

/// Samples per second: 16 000 for speech-to-text input; a synthesiser states its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SampleRate(pub u32);

/// How one mono sample is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PcmFormat {
    S16Le,
    F32Le,
}

impl PcmFormat {
    /// Bytes in one sample.
    pub const fn width(self) -> u32 {
        match self {
            PcmFormat::S16Le => 2,
            PcmFormat::F32Le => 4,
        }
    }
}

/// Mono PCM at a rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AudioFormat {
    pub rate: SampleRate,
    pub pcm: PcmFormat,
}

/// A position in samples from the start of the utterance.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SampleIndex(pub u64);

/// A length of audio in milliseconds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct AudioMs(pub u32);

/// The bytes of mono PCM. Serialises as base64 text; `Debug` prints the length only; the bytes
/// are zeroed when the value is dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct PcmBytes(Vec<u8>);

impl PcmBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for PcmBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

// Samples are what the person said: Debug shows the length only.
impl fmt::Debug for PcmBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PcmBytes(<{} bytes>)", self.0.len())
    }
}

impl Serialize for PcmBytes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for PcmBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD
            .decode(text)
            .map(PcmBytes)
            .map_err(serde::de::Error::custom)
    }
}

/// A run of samples and where it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioChunk {
    pub format: AudioFormat,
    pub at: SampleIndex,
    pub pcm: PcmBytes,
}

impl AudioChunk {
    /// Whole samples in the chunk; a trailing partial sample is not counted.
    pub fn samples(&self) -> u32 {
        let whole = self.pcm.len() as u64 / u64::from(self.format.pcm.width());
        u32::try_from(whole).unwrap_or(u32::MAX)
    }

    /// The length of the chunk, rounded down to a millisecond; a rate of zero gives zero.
    pub fn duration(&self) -> AudioMs {
        let rate = u64::from(self.format.rate.0);
        let ms = (u64::from(self.samples()) * 1000)
            .checked_div(rate)
            .unwrap_or(0);
        AudioMs(u32::try_from(ms).unwrap_or(u32::MAX))
    }
}
