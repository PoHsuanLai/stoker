//! Speech to text: the request, the events, the seam.

use model_provider::{Flow, ModelName, ProviderError};
use serde::{Deserialize, Serialize};

use crate::{
    AudioChunk, AudioFormat, AudioMs, HeardText, Lang, LangChoice, SampleIndex, SpeechModelInfo,
};

/// How the audio arrives and how the text leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum SttMode {
    /// Partial and final events while audio is still arriving; `chunk` is the engine's step.
    Streaming { chunk: AudioMs },
    /// One transcript when the audio ends.
    Batch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SttRequest {
    pub model: ModelName,
    pub mode: SttMode,
    pub lang: LangChoice,
    pub format: AudioFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum TranscriptEvent {
    /// Replaces the unstable tail since `from`.
    Partial { text: HeardText, from: SampleIndex },
    /// A stable segment, never revised.
    Final {
        text: HeardText,
        from: SampleIndex,
        to: SampleIndex,
    },
    /// The detected language, once.
    Lang(Lang),
}

/// How a transcription ended: the whole text, how much audio it heard, the model that served it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SttEnd {
    pub text: HeardText,
    pub audio: AudioMs,
    pub served: ModelName,
}

/// What a source answers when asked for more audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioPull {
    Chunk(AudioChunk),
    /// No more audio: the hold ended.
    End,
}

/// Where audio comes from, a chunk at a time.
pub trait AudioSource: Send {
    fn next(&mut self) -> impl Future<Output = AudioPull> + Send;
}

/// Where transcript events go. `Flow::Stop` ends the transcription early.
pub trait TranscriptSink: Send {
    fn event(&mut self, event: TranscriptEvent) -> Flow;
}

/// One endpoint that turns audio into text.
///
/// Cancellation is drop, as in `model-provider`: dropping the future closes the engine stream.
pub trait SpeechToText: Send + Sync {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send;

    fn transcribe<A: AudioSource, K: TranscriptSink>(
        &self,
        request: &SttRequest,
        audio: &mut A,
        sink: &mut K,
    ) -> impl Future<Output = Result<SttEnd, ProviderError>> + Send;
}
