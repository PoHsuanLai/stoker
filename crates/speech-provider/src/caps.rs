//! What one concrete speech model can do behind one backend.

use std::collections::BTreeSet;

use model_provider::{ModelName, Support};
use serde::{Deserialize, Serialize};

use crate::{AudioFormat, AudioMs, Lang, VoiceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechDir {
    /// Speech to text.
    In,
    /// Text to speech.
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum LangSet {
    Any,
    Listed(BTreeSet<Lang>),
}

/// The direction-specific half of [`SpeechCaps`]. In a catalog file it sits flat in the
/// `speech` table, with the direction written as `dir = "in"` or `dir = "out"`; this is the one
/// internally tagged enum, because `dir` is the key the spec fixed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "dir", rename_all = "snake_case")]
pub enum SpeechIo {
    In {
        input: AudioFormat,
    },
    Out {
        output: AudioFormat,
        voices: Vec<VoiceId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechCaps {
    pub streaming: Support,
    pub partials: Support,
    pub punctuation: Support,
    pub timestamps: Support,
    pub langs: LangSet,
    /// The longest audio one request takes (in) or makes (out).
    #[serde(rename = "max_audio_ms")]
    pub max_audio: AudioMs,
    #[serde(flatten)]
    pub io: SpeechIo,
}

impl SpeechCaps {
    pub fn dir(&self) -> SpeechDir {
        match self.io {
            SpeechIo::In { .. } => SpeechDir::In,
            SpeechIo::Out { .. } => SpeechDir::Out,
        }
    }
}

/// One served speech model and what it can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeechModelInfo {
    pub name: ModelName,
    pub caps: SpeechCaps,
}
