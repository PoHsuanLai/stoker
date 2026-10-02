//! Where an utterance starts and stops: a pure machine over the detector's verdicts.

use serde::{Deserialize, Serialize};
use speech_provider::{AudioMs, SampleIndex, Voiced};

/// The endpointing numbers. `Default` holds the proposal; the daemon reads the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointParams {
    /// Audio kept before the first speech frame.
    pub lead: AudioMs,
    /// Silence after speech that ends the utterance (dictation only: a hold ends on release).
    pub silence_end: AudioMs,
    /// Speech shorter than this is not speech.
    pub min_speech: AudioMs,
}

impl Default for EndpointParams {
    fn default() -> Self {
        Self {
            lead: AudioMs(300),
            silence_end: AudioMs(30_000),
            min_speech: AudioMs(150),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Endpoint {
    /// No speech yet.
    Waiting,
    InSpeech {
        since: SampleIndex,
    },
    /// Speech has stopped; `since` is where the silence began.
    Trailing {
        since: SampleIndex,
    },
    Ended(EndWhy),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndWhy {
    /// Speech, then `silence_end` of silence.
    Silence,
    /// Only silence (or less than `min_speech`) before the end.
    NoSpeech,
}

/// The next state after one frame's verdict at position `at`.
pub fn endpoint(
    state: Endpoint,
    voiced: Voiced,
    at: SampleIndex,
    params: &EndpointParams,
) -> Endpoint {
    let _ = (state, voiced, at, params);
    todo!("endpoint: Waiting to InSpeech on speech, Trailing on silence, Ended after silence_end")
}
