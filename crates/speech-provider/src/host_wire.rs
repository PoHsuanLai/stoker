//! inferd to `speech-host`, over `$XDG_RUNTIME_DIR/inferd/<engine>.sock`: a 4-byte big-endian
//! length, then that many bytes of JSON, at most 1 MiB. porter's framing shape, declared again
//! here because stoker has no porter dependency.

use model_provider::ProviderError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{AudioChunk, SpeechModelInfo, SttEnd, SttRequest, TranscriptEvent};

/// The largest frame body, in bytes.
pub const MAX_FRAME_BYTES: usize = 1 << 20;

/// The version of this vocabulary; both sides say it first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostVocab(pub u32);

impl HostVocab {
    pub const CURRENT: HostVocab = HostVocab(1);
}

/// What inferd sends the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum HostIn {
    Hello {
        vocab: HostVocab,
    },
    Begin(SttRequest),
    Audio(AudioChunk),
    /// No more audio.
    End,
    /// Drop the utterance without a transcript.
    Cancel,
}

/// What the host sends back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum HostOut {
    Hello {
        vocab: HostVocab,
        models: Vec<SpeechModelInfo>,
    },
    Event(TranscriptEvent),
    Done(SttEnd),
    Failed(ProviderError),
}

/// A frame that cannot be written or read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("a frame of {len} bytes is over the {MAX_FRAME_BYTES} byte cap")]
    TooLarge { len: usize },
    #[error("the frame body is not the expected JSON: {0}")]
    Json(String),
}

/// The length a frame header announces; over the cap is refused before any body is read.
pub fn frame_length(header: [u8; 4]) -> Result<usize, FrameError> {
    let len = u32::from_be_bytes(header) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge { len });
    }
    Ok(len)
}

/// The bytes to write for `message`: the header, then the JSON body.
pub fn encode_frame<T: Serialize>(message: &T) -> Result<Vec<u8>, FrameError> {
    let body = serde_json::to_vec(message).map_err(|e| FrameError::Json(e.to_string()))?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge { len: body.len() });
    }
    // The cap is below `u32::MAX`, so the length always fits.
    let header = u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes();
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// Reads one frame's body (without its header).
pub fn decode_frame<T: DeserializeOwned>(body: &[u8]) -> Result<T, FrameError> {
    if body.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge { len: body.len() });
    }
    serde_json::from_slice(body).map_err(|e| FrameError::Json(e.to_string()))
}
