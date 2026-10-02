//! Text to speech: the request and the seam.

use model_provider::{Flow, ModelName, ProviderError};
use serde::{Deserialize, Serialize};

use crate::{AudioChunk, AudioFormat, AudioMs, Lang, SpeechModelInfo, SpokenText, VoiceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsRequest {
    pub model: ModelName,
    pub text: SpokenText,
    pub voice: VoiceId,
    pub lang: Lang,
    /// The format the caller wants; an engine that cannot make it answers `BadRequest`.
    pub format: AudioFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsEnd {
    pub audio: AudioMs,
    pub served: ModelName,
}

/// Where synthesised audio goes. `Flow::Stop` is barge-in: the person spoke over it.
pub trait AudioSink: Send {
    fn chunk(&mut self, chunk: AudioChunk) -> Flow;
}

/// One endpoint that turns text into audio. Cancellation is drop.
pub trait TextToSpeech: Send + Sync {
    fn describe(&self) -> impl Future<Output = Result<Vec<SpeechModelInfo>, ProviderError>> + Send;

    fn speak<K: AudioSink>(
        &self,
        request: &TtsRequest,
        sink: &mut K,
    ) -> impl Future<Output = Result<TtsEnd, ProviderError>> + Send;
}
