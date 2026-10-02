//! Speech as traits, below every engine and account, local or cloud.
//!
//! Audio is mono only (PipeWire converts at the edge). [`SpeechToText`] pulls audio from an
//! [`AudioSource`] and pushes [`TranscriptEvent`]s into a [`TranscriptSink`]; [`TextToSpeech`]
//! pushes [`AudioChunk`]s into an [`AudioSink`]. Cancellation is dropping the future, as in
//! `model-provider`. [`VoiceActivity`] is synchronous: CPU only, one 32 ms frame at a time.
//! `host_wire` is the framed JSON between inferd and the `speech-host` engine process.
//!
//! What a person said, what a model heard and every sample are personal: `PcmBytes`, `HeardText`
//! and `SpokenText` write `Debug` by hand and print a length, never the content.

mod audio;
mod caps;
mod host_wire;
mod stt;
#[cfg(feature = "testing")]
mod testing;
mod text;
mod tts;
mod vad;

pub use audio::{AudioChunk, AudioFormat, AudioMs, PcmBytes, PcmFormat, SampleIndex, SampleRate};
pub use caps::{LangSet, SpeechCaps, SpeechDir, SpeechIo, SpeechModelInfo};
pub use host_wire::{
    FrameError, HostIn, HostOut, HostVocab, MAX_FRAME_BYTES, decode_frame, encode_frame,
    frame_length,
};
pub use stt::{
    AudioPull, AudioSource, SpeechToText, SttEnd, SttMode, SttRequest, TranscriptEvent,
    TranscriptSink,
};
#[cfg(feature = "testing")]
pub use testing::{ScriptedStt, ScriptedTts, ScriptedVad, TtsScript};
pub use text::{
    HeardText, Lang, LangChoice, LangError, SpokenText, SpokenTextError, VoiceId, VoiceIdError,
};
pub use tts::{AudioSink, TextToSpeech, TtsEnd, TtsRequest};
pub use vad::{FRAME_SAMPLES, Frame512, FrameLenError, SpeechProb, VoiceActivity, Voiced};
