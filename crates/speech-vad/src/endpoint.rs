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

/// Samples in a millisecond of 16 kHz audio.
const SAMPLES_PER_MS: u64 = 16;

fn samples(ms: AudioMs) -> u64 {
    u64::from(ms.0) * SAMPLES_PER_MS
}

/// The next state after one frame's verdict at position `at` (the frame's first sample, counted
/// from the start of the utterance).
///
/// - `Waiting` becomes `InSpeech` at the first speech frame; with only silence for
///   `silence_end` from the start it is `Ended(NoSpeech)`.
/// - `InSpeech` becomes `Trailing` at the first silent frame, if the speech lasted `min_speech`;
///   a shorter burst was not speech and the machine is `Waiting` again.
/// - `Trailing` becomes `Ended(Silence)` once the silence has lasted `silence_end`; speech
///   resuming before that is `InSpeech` again, and speech that has been accepted once stays
///   accepted (the resumed state's `since` is set back by `min_speech`, so the next silence
///   does not count it a burst).
/// - `Ended` stays ended.
///
/// `lead` is the caller's: it keeps that much audio before `InSpeech`'s `since`.
pub fn endpoint(
    state: Endpoint,
    voiced: Voiced,
    at: SampleIndex,
    params: &EndpointParams,
) -> Endpoint {
    let silent_for = |since: SampleIndex| at.0.saturating_sub(since.0);
    match (state, voiced) {
        (Endpoint::Ended(why), _) => Endpoint::Ended(why),
        (Endpoint::Waiting, Voiced::Speech) => Endpoint::InSpeech { since: at },
        (Endpoint::Waiting, Voiced::Silence) if at.0 >= samples(params.silence_end) => {
            Endpoint::Ended(EndWhy::NoSpeech)
        }
        (Endpoint::Waiting, Voiced::Silence) => Endpoint::Waiting,
        (Endpoint::InSpeech { since }, Voiced::Speech) => Endpoint::InSpeech { since },
        (Endpoint::InSpeech { since }, Voiced::Silence) => {
            if silent_for(since) >= samples(params.min_speech) {
                Endpoint::Trailing { since: at }
            } else {
                Endpoint::Waiting
            }
        }
        (Endpoint::Trailing { .. }, Voiced::Speech) => Endpoint::InSpeech {
            since: SampleIndex(at.0.saturating_sub(samples(params.min_speech))),
        },
        (Endpoint::Trailing { since }, Voiced::Silence) => {
            if silent_for(since) >= samples(params.silence_end) {
                Endpoint::Ended(EndWhy::Silence)
            } else {
                Endpoint::Trailing { since }
            }
        }
    }
}
